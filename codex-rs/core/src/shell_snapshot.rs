use std::collections::HashMap;
use std::io::ErrorKind;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use std::time::SystemTime;

use crate::StateDbHandle;
use crate::rollout::list::find_thread_path_by_id_str;
use crate::shell::Shell;
use crate::shell::ShellType;
use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use codex_exec_server::Environment;
use codex_network_proxy::CREDENTIAL_BROKER_ACTIVE_ENV_KEY;
use codex_network_proxy::NetworkProxy;
use codex_network_proxy::brokered_credential_dummy_env_keys;
use codex_network_proxy::brokered_credential_env_keys;
use codex_network_proxy::credential_broker_provider_context_env_keys;
use codex_network_proxy::is_credential_broker_provider_env_key;
use codex_otel::SessionTelemetry;
use codex_protocol::ThreadId;
use codex_protocol::config_types::ShellEnvironmentPolicy;
use codex_protocol::shell_environment::create_env_from_vars;
use codex_shell_command::shell_snapshot::CapturedSnapshot;
use codex_shell_command::shell_snapshot::PreparedSnapshot;
use codex_shell_command::shell_snapshot::SnapshotCaptureOptions;
use codex_shell_command::shell_snapshot::SnapshotCredentialEnvironment;
use codex_shell_command::shell_snapshot::SnapshotStartup;
use codex_shell_command::shell_snapshot::prepare_snapshot_credentials;
use codex_shell_command::shell_snapshot::snapshot_capture_script;
use codex_utils_absolute_path::AbsolutePathBuf;
use codex_utils_path_uri::PathUri;
use tokio::fs;
use tokio::process::Command;
use tokio::sync::watch;
use tokio::time::timeout;
use tracing::Instrument;
use tracing::info_span;

#[path = "shell_snapshot_sandbox.rs"]
mod sandbox;

pub(crate) use sandbox::ShellSnapshotSandbox;
pub(crate) use sandbox::snapshot_read_permissions;

#[derive(Clone)]
pub(crate) struct ShellSnapshot {
    config: Option<Arc<ShellSnapshotConfig>>,
    credential_broker: Option<watch::Sender<SnapshotCredentialBrokerState>>,
}

struct ShellSnapshotConfig {
    codex_home: AbsolutePathBuf,
    session_id: ThreadId,
    session_telemetry: SessionTelemetry,
    state_db: Option<StateDbHandle>,
    credential_broker: Option<watch::Receiver<SnapshotCredentialBrokerState>>,
    prefer_executor_snapshots: bool,
}

#[derive(Clone, PartialEq)]
pub(crate) enum SnapshotCredentialBrokerState {
    Starting,
    Inactive,
    Unavailable,
    Ready(NetworkProxy),
}

pub(crate) struct ShellSnapshotFile {
    path: AbsolutePathBuf,
    credentials: Option<SnapshotCredentials>,
}

struct SnapshotCredentials {
    network_proxy: NetworkProxy,
    shell_environment_policy: ShellEnvironmentPolicy,
    credential_env: HashMap<String, String>,
}

struct SnapshotCredentialBroker {
    network_proxy: NetworkProxy,
    shell_environment_policy: ShellEnvironmentPolicy,
    allow_login_shell: bool,
}

const SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(10);
const SNAPSHOT_RETENTION: Duration = Duration::from_secs(60 * 60 * 24 * 3); // 3 days retention.
const SNAPSHOT_DIR: &str = "shell_snapshots";

impl ShellSnapshot {
    pub(crate) fn new(
        codex_home: AbsolutePathBuf,
        session_id: ThreadId,
        session_telemetry: SessionTelemetry,
        state_db: Option<StateDbHandle>,
        credential_broker: Option<watch::Sender<SnapshotCredentialBrokerState>>,
        prefer_executor_snapshots: bool,
    ) -> Self {
        Self {
            config: Some(Arc::new(ShellSnapshotConfig {
                codex_home,
                session_id,
                session_telemetry,
                state_db,
                credential_broker: credential_broker.as_ref().map(watch::Sender::subscribe),
                prefer_executor_snapshots,
            })),
            credential_broker,
        }
    }

    pub(crate) fn disabled() -> Self {
        Self {
            config: None,
            credential_broker: None,
        }
    }

    pub(crate) fn set_credential_broker(&self, state: SnapshotCredentialBrokerState) -> bool {
        self.credential_broker.as_ref().is_some_and(|sender| {
            let previous = sender.send_replace(state.clone());
            previous != state
        })
    }

    pub(crate) fn should_rebuild_inherited(&self) -> bool {
        self.credential_broker.as_ref().is_some_and(|sender| {
            !matches!(*sender.borrow(), SnapshotCredentialBrokerState::Inactive)
        })
    }

    pub(crate) async fn build(
        self,
        environment: Arc<Environment>,
        cwd: PathUri,
        shell: Option<Shell>,
        allow_login_shell: bool,
        shell_environment_policy: ShellEnvironmentPolicy,
        sandbox: Option<ShellSnapshotSandbox>,
    ) -> Option<Arc<ShellSnapshotFile>> {
        let config = Arc::clone(self.config.as_ref()?);
        if environment.is_remote() {
            return None;
        }

        let shell = shell?;
        // TODO(anp): Migrate shell snapshot creation to accept PathUri and defer native
        // conversion to the spawned shell process.
        let cwd = cwd.to_abs_path().ok()?;
        drop(self);
        Self::build_for_cwd(
            config,
            cwd,
            shell,
            allow_login_shell,
            shell_environment_policy,
            sandbox,
        )
        .await
    }

    async fn build_for_cwd(
        config: Arc<ShellSnapshotConfig>,
        cwd: AbsolutePathBuf,
        shell: Shell,
        allow_login_shell: bool,
        shell_environment_policy: ShellEnvironmentPolicy,
        sandbox: Option<ShellSnapshotSandbox>,
    ) -> Option<Arc<ShellSnapshotFile>> {
        let snapshot_span = info_span!("shell_snapshot", thread_id = %config.session_id);
        async {
            let credential_broker = if let Some(receiver) = config.credential_broker.as_ref() {
                let mut receiver = receiver.clone();
                if matches!(&*receiver.borrow(), SnapshotCredentialBrokerState::Starting)
                    && receiver.changed().await.is_err()
                {
                    return None;
                }
                let state = receiver.borrow().clone();
                match state {
                    SnapshotCredentialBrokerState::Starting
                    | SnapshotCredentialBrokerState::Unavailable => return None,
                    SnapshotCredentialBrokerState::Inactive if config.prefer_executor_snapshots => {
                        return None;
                    }
                    SnapshotCredentialBrokerState::Inactive => None,
                    SnapshotCredentialBrokerState::Ready(network_proxy) if sandbox.is_some() => {
                        Some(SnapshotCredentialBroker {
                            network_proxy,
                            shell_environment_policy,
                            allow_login_shell,
                        })
                    }
                    SnapshotCredentialBrokerState::Ready(_) => return None,
                }
            } else {
                None
            };
            let timer = config
                .session_telemetry
                .start_timer("codex.shell_snapshot.duration_ms", &[("version", "v1")]);
            let snapshot = ShellSnapshot::try_create(
                &config.codex_home,
                config.session_id,
                &cwd,
                &shell,
                config.state_db.clone(),
                credential_broker,
                sandbox.as_ref(),
            )
            .await;
            let success_tag = if snapshot.is_ok() { "true" } else { "false" };
            let _ = timer.map(|timer| timer.record(&[("success", success_tag)]));
            let mut counter_tags = vec![("version", "v1"), ("success", success_tag)];
            if let Some(failure_reason) = snapshot.as_ref().err() {
                counter_tags.push(("failure_reason", *failure_reason));
            }
            config
                .session_telemetry
                .counter("codex.shell_snapshot", /*inc*/ 1, &counter_tags);
            snapshot.ok().map(Arc::new)
        }
        .instrument(snapshot_span)
        .await
    }

    async fn try_create(
        codex_home: &AbsolutePathBuf,
        session_id: ThreadId,
        session_cwd: &AbsolutePathBuf,
        shell: &Shell,
        state_db: Option<StateDbHandle>,
        credential_broker: Option<SnapshotCredentialBroker>,
        sandbox: Option<&ShellSnapshotSandbox>,
    ) -> std::result::Result<ShellSnapshotFile, &'static str> {
        // File to store the snapshot
        let extension = match shell.shell_type {
            ShellType::PowerShell => "ps1",
            _ => "sh",
        };
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let path = codex_home
            .join(SNAPSHOT_DIR)
            .join(format!("{session_id}.{nonce}.{extension}"));
        let temp_path = codex_home
            .join(SNAPSHOT_DIR)
            .join(format!("{session_id}.tmp-{nonce}"));

        // Clean the (unlikely) leaked snapshot files.
        let codex_home = codex_home.clone();
        let cleanup_session_id = session_id;
        tokio::spawn(async move {
            if let Err(err) =
                cleanup_stale_snapshots(&codex_home, cleanup_session_id, state_db).await
            {
                tracing::warn!("Failed to clean up shell snapshots: {err:?}");
            }
        });

        // Make the new snapshot.
        let credentials = write_shell_snapshot(
            shell,
            &temp_path,
            session_cwd,
            credential_broker.as_ref(),
            sandbox,
        )
        .await
        .map_err(|err| {
            tracing::warn!(
                "Failed to create shell snapshot for {}: {err:?}",
                shell.name()
            );
            "write_failed"
        })?;
        tracing::info!(
            "Shell snapshot successfully created: {}",
            temp_path.display()
        );

        if let Err(err) = validate_snapshot(
            shell,
            &temp_path,
            session_cwd,
            credential_broker.as_ref(),
            sandbox,
        )
        .await
        {
            tracing::error!("Shell snapshot validation failed: {err:?}");
            remove_snapshot_file(&temp_path).await;
            return Err("validation_failed");
        }

        if let Err(err) = fs::rename(&temp_path, &path).await {
            tracing::warn!("Failed to finalize shell snapshot: {err:?}");
            remove_snapshot_file(&temp_path).await;
            return Err("write_failed");
        }

        Ok(ShellSnapshotFile { path, credentials })
    }
}

impl ShellSnapshotFile {
    pub(crate) fn is_brokered_for(
        &self,
        shell_environment_policy: &ShellEnvironmentPolicy,
    ) -> bool {
        self.credentials.as_ref().is_some_and(|credentials| {
            &credentials.shell_environment_policy == shell_environment_policy
        })
    }

    pub(crate) fn path(&self) -> AbsolutePathBuf {
        self.path.clone()
    }

    pub(crate) fn restore_credentials(
        &self,
        env: &mut HashMap<String, String>,
        shell_environment_policy: &ShellEnvironmentPolicy,
    ) {
        let Some(credentials) = self.credentials.as_ref() else {
            return;
        };

        // This snapshot belongs to one command and was captured under this exact policy.
        if &credentials.shell_environment_policy != shell_environment_policy {
            return;
        }
        let mut snapshot_env = credentials.credential_env.clone();
        snapshot_env.insert(
            CREDENTIAL_BROKER_ACTIVE_ENV_KEY.to_string(),
            "1".to_string(),
        );
        credentials
            .network_proxy
            .restore_brokered_credentials(&mut snapshot_env, &mut []);
        snapshot_env.remove(CREDENTIAL_BROKER_ACTIVE_ENV_KEY);
        env.extend(snapshot_env);
    }
}

impl Drop for ShellSnapshotFile {
    fn drop(&mut self) {
        if let Err(err) = std::fs::remove_file(&self.path) {
            tracing::warn!(
                "Failed to delete shell snapshot at {:?}: {err:?}",
                self.path
            );
        }
    }
}

async fn write_shell_snapshot(
    shell: &Shell,
    output_path: &AbsolutePathBuf,
    cwd: &AbsolutePathBuf,
    credential_broker: Option<&SnapshotCredentialBroker>,
    sandbox: Option<&ShellSnapshotSandbox>,
) -> Result<Option<SnapshotCredentials>> {
    let shell_type = shell.shell_type;
    if shell_type == ShellType::PowerShell || shell_type == ShellType::Cmd {
        bail!("Shell snapshot not supported yet for {shell_type:?}");
    }
    let (snapshot, credentials) = capture_snapshot(shell, cwd, credential_broker, sandbox).await?;

    if let Some(parent) = output_path.parent() {
        let parent_display = parent.display();
        fs::create_dir_all(&parent)
            .await
            .with_context(|| format!("Failed to create snapshot parent {parent_display}"))?;
    }

    let snapshot_path = output_path.display();
    fs::write(output_path, snapshot)
        .await
        .with_context(|| format!("Failed to write snapshot to {snapshot_path}"))?;

    Ok(credentials)
}

async fn capture_snapshot(
    shell: &Shell,
    cwd: &AbsolutePathBuf,
    credential_broker: Option<&SnapshotCredentialBroker>,
    sandbox: Option<&ShellSnapshotSandbox>,
) -> Result<(String, Option<SnapshotCredentials>)> {
    let shell_type = shell.shell_type;
    let shell_startup = if credential_broker.is_some_and(|broker| !broker.allow_login_shell) {
        SnapshotStartup::NonInteractive
    } else {
        SnapshotStartup::Interactive
    };
    let script = snapshot_capture_script(
        shell_type,
        SnapshotCaptureOptions {
            startup: shell_startup,
            declarations: true,
            environment: credential_broker.is_some(),
        },
    )
    .ok_or_else(|| anyhow!("Shell snapshotting is not yet supported for {shell_type:?}"))?;
    let shell_mode = if credential_broker.is_none_or(|broker| broker.allow_login_shell) {
        SnapshotShellMode::Login
    } else {
        SnapshotShellMode::NonLogin
    };
    let raw_snapshot = run_script_with_timeout(
        shell,
        &script,
        SNAPSHOT_TIMEOUT,
        shell_mode,
        cwd,
        credential_broker,
        sandbox,
    )
    .await?;
    let capture = CapturedSnapshot::parse(shell_type, raw_snapshot.as_bytes())
        .ok_or_else(|| anyhow!("invalid shell snapshot capture"))?;
    let Some(credential_broker) = credential_broker else {
        return Ok((capture.render_script(), None));
    };

    let original_env = std::str::from_utf8(capture.environment)?
        .split('\0')
        .filter_map(|entry| entry.split_once('='))
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect::<HashMap<_, _>>();
    let policy = &credential_broker.shell_environment_policy;
    let inherited_env = create_env_from_vars(std::env::vars(), policy, /*thread_id*/ None);
    let mut restored_env = original_env.clone();
    credential_broker
        .network_proxy
        .restore_brokered_credentials(&mut restored_env, &mut []);
    let mut discovery_env = restored_env.clone();
    replace_provider_context_with_inherited(
        &mut discovery_env,
        &inherited_env,
        credential_broker_provider_context_env_keys(),
    );
    for (key, value) in &policy.r#set {
        if !discovery_env.contains_key(key)
            || credential_broker_provider_context_env_keys()
                .any(|context_key| context_key.eq_ignore_ascii_case(key))
        {
            discovery_env.insert(key.clone(), value.clone());
        }
    }
    credential_broker
        .network_proxy
        .apply_to_env(&mut discovery_env);
    let brokered_keys = brokered_credential_dummy_env_keys(&discovery_env);
    let mut env = create_env_from_vars(
        restored_env
            .iter()
            .map(|(key, value)| (key.clone(), value.clone())),
        policy,
        /*thread_id*/ None,
    );
    replace_provider_context_with_inherited(
        &mut env,
        &inherited_env,
        credential_broker_provider_context_env_keys(),
    );
    credential_broker.network_proxy.apply_to_env(&mut env);
    let allowed_brokered_keys = brokered_credential_dummy_env_keys(&env);
    let mut snapshot_env = env.clone();
    for key in credential_broker_provider_context_env_keys() {
        if original_env.get(key) != env.get(key) {
            snapshot_env.remove(key);
        }
    }
    let PreparedSnapshot {
        script: snapshot, ..
    } = prepare_snapshot_credentials(
        &capture,
        SnapshotCredentialEnvironment {
            original: &original_env,
            restored: &restored_env,
            configured: &policy.r#set,
            discovered: &discovery_env,
            allowed: &snapshot_env,
            is_allowed_unset: &|key| {
                create_env_from_vars(
                    std::iter::once((key.to_string(), String::new())),
                    policy,
                    /*thread_id*/ None,
                )
                .contains_key(key)
            },
            brokered_keys: &brokered_keys,
            brokered_alias_keys: &[],
            allowed_brokered_keys: &allowed_brokered_keys,
        },
        |value| {
            credential_broker
                .network_proxy
                .virtualize_brokered_text(value, &env)
        },
    )
    .ok_or_else(|| anyhow!("shell snapshot contains a credential outside supported exports"))?;

    let credential_env = brokered_credential_env_keys(&env)
        .map(str::to_string)
        .chain(allowed_brokered_keys)
        .filter_map(|key| env.get(&key).map(|value| (key, value.clone())))
        .collect();
    Ok((
        snapshot,
        Some(SnapshotCredentials {
            network_proxy: credential_broker.network_proxy.clone(),
            shell_environment_policy: policy.clone(),
            credential_env,
        }),
    ))
}

fn replace_provider_context_with_inherited(
    env: &mut HashMap<String, String>,
    inherited_env: &HashMap<String, String>,
    context_keys: impl Iterator<Item = &'static str>,
) {
    for key in context_keys {
        if let Some(value) = inherited_env.get(key) {
            env.insert(key.to_string(), value.clone());
        }
    }
}

#[derive(Clone, Copy)]
enum SnapshotShellMode<'a> {
    Login,
    NonLogin,
    Validation(&'a AbsolutePathBuf),
}

async fn validate_snapshot(
    shell: &Shell,
    snapshot_path: &AbsolutePathBuf,
    cwd: &AbsolutePathBuf,
    credential_broker: Option<&SnapshotCredentialBroker>,
    sandbox: Option<&ShellSnapshotSandbox>,
) -> Result<()> {
    let snapshot_path_display = snapshot_path.display();
    let script = format!("set -e; . \"{snapshot_path_display}\"");
    run_script_with_timeout(
        shell,
        &script,
        SNAPSHOT_TIMEOUT,
        SnapshotShellMode::Validation(snapshot_path),
        cwd,
        credential_broker,
        sandbox,
    )
    .await
    .map(|_| ())
}

async fn run_script_with_timeout(
    shell: &Shell,
    script: &str,
    snapshot_timeout: Duration,
    shell_mode: SnapshotShellMode<'_>,
    cwd: &AbsolutePathBuf,
    credential_broker: Option<&SnapshotCredentialBroker>,
    sandbox: Option<&ShellSnapshotSandbox>,
) -> Result<String> {
    let suppress_startup_files =
        credential_broker.is_some() && matches!(shell_mode, SnapshotShellMode::Validation(_));
    let mut args = shell.derive_exec_args(script, matches!(shell_mode, SnapshotShellMode::Login));
    if suppress_startup_files && shell.shell_type == ShellType::Zsh {
        args[1] = "-fc".to_string();
    }
    let shell_name = shell.name();
    let mut prepared_env = None;
    if let Some(credential_broker) = credential_broker {
        let policy = &credential_broker.shell_environment_policy;
        let mut inherited_policy = policy.clone();
        inherited_policy.r#set.clear();
        let mut env =
            create_env_from_vars(std::env::vars(), &inherited_policy, /*thread_id*/ None);
        env.extend(
            policy
                .r#set
                .iter()
                .filter(|(key, _)| {
                    (is_credential_broker_provider_env_key(key)
                        || matches!(
                            (shell.shell_type, key.as_str()),
                            (ShellType::Zsh, "ZDOTDIR") | (ShellType::Bash, "BASH_ENV")
                        ))
                        && (policy.include_only.is_empty()
                            || policy
                                .include_only
                                .iter()
                                .any(|pattern| pattern.matches(key)))
                })
                .map(|(key, value)| (key.clone(), value.clone())),
        );
        if suppress_startup_files {
            env.remove("BASH_ENV");
        }
        credential_broker.network_proxy.apply_to_env(&mut env);
        prepared_env = Some(env);
    }
    if let Some(sandbox) = sandbox {
        let snapshot_read_path = match shell_mode {
            SnapshotShellMode::Validation(path) => Some(path),
            SnapshotShellMode::Login | SnapshotShellMode::NonLogin => None,
        };
        return sandbox
            .run(
                args,
                cwd,
                prepared_env.unwrap_or_else(|| std::env::vars().collect()),
                snapshot_timeout,
                shell_name,
                snapshot_read_path,
            )
            .await;
    }

    // Handler is kept as guard to control the drop. The `mut` pattern is required because .args()
    // returns a ref of handler.
    let mut handler = Command::new(&args[0]);
    handler.args(&args[1..]);
    handler.stdin(Stdio::null());
    handler.current_dir(cwd);
    if let Some(env) = prepared_env {
        handler.env_clear();
        handler.envs(env);
    }
    codex_protocol::shell_environment::scrub_non_inheritable_env_vars(handler.as_std_mut());
    #[cfg(unix)]
    unsafe {
        handler.pre_exec(|| {
            codex_utils_pty::process_group::detach_from_tty()?;
            Ok(())
        });
    }
    handler.kill_on_drop(true);
    let output = timeout(snapshot_timeout, handler.output())
        .await
        .map_err(|_| anyhow!("Snapshot command timed out for {shell_name}"))?
        .with_context(|| format!("Failed to execute {shell_name}"))?;

    if !output.status.success() {
        let status = output.status;
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Snapshot command exited with status {status}: {stderr}");
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Removes shell snapshots that either lack a matching session rollout file or
/// whose rollouts have not been updated within the retention window.
/// The active session id is exempt from cleanup.
pub async fn cleanup_stale_snapshots(
    codex_home: &AbsolutePathBuf,
    active_session_id: ThreadId,
    state_db: Option<StateDbHandle>,
) -> Result<()> {
    let snapshot_dir = codex_home.join(SNAPSHOT_DIR);

    let mut entries = match fs::read_dir(&snapshot_dir).await {
        Ok(entries) => entries,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err.into()),
    };

    let now = SystemTime::now();
    let active_session_id = active_session_id.to_string();

    while let Some(entry) = entries.next_entry().await? {
        if !entry.file_type().await?.is_file() {
            continue;
        }

        let path = entry.path();

        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        let Some(session_id) = snapshot_session_id_from_file_name(&file_name) else {
            remove_snapshot_file(&path).await;
            continue;
        };
        if session_id == active_session_id {
            continue;
        }

        let rollout_path =
            find_thread_path_by_id_str(codex_home, session_id, state_db.as_deref()).await?;
        let Some(rollout_path) = rollout_path else {
            remove_snapshot_file(&path).await;
            continue;
        };

        let modified = match fs::metadata(&rollout_path).await.and_then(|m| m.modified()) {
            Ok(modified) => modified,
            Err(err) => {
                tracing::warn!(
                    "Failed to check rollout age for snapshot {}: {err:?}",
                    path.display()
                );
                continue;
            }
        };

        if now
            .duration_since(modified)
            .ok()
            .is_some_and(|age| age >= SNAPSHOT_RETENTION)
        {
            remove_snapshot_file(&path).await;
        }
    }

    Ok(())
}

async fn remove_snapshot_file(path: &Path) {
    if let Err(err) = fs::remove_file(path).await {
        tracing::warn!("Failed to delete shell snapshot at {:?}: {err:?}", path);
    }
}

fn snapshot_session_id_from_file_name(file_name: &str) -> Option<&str> {
    let (stem, extension) = file_name.rsplit_once('.')?;
    match extension {
        "sh" | "ps1" => Some(
            stem.split_once('.')
                .map_or(stem, |(session_id, _generation)| session_id),
        ),
        _ if extension.starts_with("tmp-") => Some(stem),
        _ => None,
    }
}

#[cfg(test)]
#[path = "shell_snapshot_tests.rs"]
mod tests;
