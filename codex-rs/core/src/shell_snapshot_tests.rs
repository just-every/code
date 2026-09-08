use super::*;
#[cfg(unix)]
use crate::config::NetworkProxySpec;
#[cfg(unix)]
use codex_network_proxy::NetworkProxyConfig;
#[cfg(unix)]
use codex_network_proxy::brokered_credential_binding_env_keys;
#[cfg(unix)]
use codex_protocol::config_types::EnvironmentVariablePattern;
#[cfg(unix)]
use codex_protocol::models::PermissionProfile;
use core_test_support::PathBufExt;
use core_test_support::PathExt;
use pretty_assertions::assert_eq;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
#[cfg(unix)]
use std::process::Command;
#[cfg(target_os = "linux")]
use std::process::Command as StdCommand;

use tempfile::tempdir;

#[cfg(unix)]
struct BlockingStdinPipe {
    original: i32,
    write_end: i32,
}

#[cfg(unix)]
impl BlockingStdinPipe {
    fn install() -> Result<Self> {
        let mut fds = [0i32; 2];
        if unsafe { libc::pipe(fds.as_mut_ptr()) } == -1 {
            return Err(std::io::Error::last_os_error()).context("create stdin pipe");
        }

        let original = unsafe { libc::dup(libc::STDIN_FILENO) };
        if original == -1 {
            let err = std::io::Error::last_os_error();
            unsafe {
                libc::close(fds[0]);
                libc::close(fds[1]);
            }
            return Err(err).context("dup stdin");
        }

        if unsafe { libc::dup2(fds[0], libc::STDIN_FILENO) } == -1 {
            let err = std::io::Error::last_os_error();
            unsafe {
                libc::close(fds[0]);
                libc::close(fds[1]);
                libc::close(original);
            }
            return Err(err).context("replace stdin");
        }

        unsafe {
            libc::close(fds[0]);
        }

        Ok(Self {
            original,
            write_end: fds[1],
        })
    }
}

#[cfg(unix)]
impl Drop for BlockingStdinPipe {
    fn drop(&mut self) {
        unsafe {
            libc::dup2(self.original, libc::STDIN_FILENO);
            libc::close(self.original);
            libc::close(self.write_end);
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn assert_posix_snapshot_sections(snapshot: &str) {
    assert!(snapshot.contains("# Snapshot file"));
    assert!(snapshot.contains("aliases "));
    assert!(snapshot.contains("exports "));
    assert!(
        snapshot.contains("PATH"),
        "snapshot should capture a PATH export"
    );
    assert!(snapshot.contains("setopts "));
}

async fn get_snapshot(shell_type: ShellType) -> Result<String> {
    let dir = tempdir()?;
    let path = dir.path().join("snapshot.sh");
    let shell = crate::shell::get_shell(shell_type)
        .with_context(|| format!("No available shell for {shell_type:?}"))?;
    write_shell_snapshot(
        &shell,
        &path.abs(),
        &dir.path().abs(),
        /*credential_broker*/ None,
        /*sandbox*/ None,
    )
    .await?;
    let content = fs::read_to_string(&path).await?;
    Ok(content)
}

#[test]
fn snapshot_file_name_parser_supports_legacy_and_suffixed_names() {
    let session_id = "019cf82b-6a62-7700-bbbd-46909794ef89";

    assert_eq!(
        snapshot_session_id_from_file_name(&format!("{session_id}.sh")),
        Some(session_id)
    );
    assert_eq!(
        snapshot_session_id_from_file_name(&format!("{session_id}.123.sh")),
        Some(session_id)
    );
    assert_eq!(
        snapshot_session_id_from_file_name(&format!("{session_id}.tmp-123")),
        Some(session_id)
    );
    assert_eq!(
        snapshot_session_id_from_file_name("not-a-snapshot.txt"),
        None
    );
}

#[cfg(unix)]
#[tokio::test]
async fn inactive_profiles_keep_snapshots_but_active_brokers_require_sandbox() -> Result<()> {
    let dir = tempdir()?;
    std::fs::create_dir(dir.path().join(".codex"))?;
    std::fs::write(
        dir.path().join(".codex/startup.sh"),
        "printf started > startup-ran\n",
    )?;
    let session_id = ThreadId::new();
    let session_telemetry = SessionTelemetry::new(
        session_id,
        "test",
        "test",
        /*account_id*/ None,
        /*account_email*/ None,
        /*auth_mode*/ None,
        "test".to_string(),
        /*log_user_prompts*/ false,
        "test".to_string(),
        codex_protocol::protocol::SessionSource::Cli,
    );
    let shell = Shell {
        shell_type: ShellType::Bash,
        shell_path: PathBuf::from("/bin/bash"),
    };
    let permission_profile = PermissionProfile::workspace_write();
    let mut network_config = NetworkProxyConfig::default();
    network_config.set_credential_broker_enabled(/*enabled*/ true);
    let network_spec = NetworkProxySpec::from_config_and_constraints(
        network_config,
        /*requirements*/ None,
        &permission_profile,
    )?;
    let started_proxy = network_spec
        .start_proxy(
            &permission_profile,
            /*policy_decider*/ None,
            /*blocked_request_observer*/ None,
            /*enable_network_approval_flow*/ false,
            Default::default(),
        )
        .await?;
    for (state, prefer_executor_snapshots, expect_snapshot) in [
        (SnapshotCredentialBrokerState::Unavailable, false, false),
        (SnapshotCredentialBrokerState::Inactive, false, true),
        (SnapshotCredentialBrokerState::Inactive, true, false),
        (
            SnapshotCredentialBrokerState::Ready(started_proxy.proxy()),
            false,
            false,
        ),
    ] {
        let (_sender, receiver) = watch::channel(state);
        let config = Arc::new(ShellSnapshotConfig {
            codex_home: dir.path().abs(),
            session_id,
            session_telemetry: session_telemetry.clone(),
            state_db: None,
            credential_broker: Some(receiver),
            prefer_executor_snapshots,
        });
        let snapshot = tokio::time::timeout(
            SNAPSHOT_TIMEOUT,
            ShellSnapshot::build_for_cwd(
                config,
                dir.path().abs(),
                shell.clone(),
                /*allow_login_shell*/ false,
                ShellEnvironmentPolicy {
                    r#set: HashMap::from([(
                        "BASH_ENV".to_string(),
                        "./.codex/startup.sh".to_string(),
                    )]),
                    ..ShellEnvironmentPolicy::default()
                },
                /*sandbox*/ None,
            ),
        )
        .await?;

        assert_eq!(snapshot.is_some(), expect_snapshot);
    }
    assert!(!dir.path().join("startup-ran").exists());
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn snapshot_discovers_and_redacts_shell_initialized_credentials() -> Result<()> {
    let dir = tempdir()?;
    let startup = dir.path().join("startup.sh");
    std::fs::write(
        &startup,
        "export GH_TOKEN='ghp_shell_only_secret'\n\
         export AUTH_HEADER=\"Bearer $GH_TOKEN\"\n\
         declare -rx GITHUB_TOKEN='ghp_readonly_secret'\n\
         declare -rx HOMEBREW_GITHUB_API_TOKEN=\"$GITHUB_TOKEN\"\n\
         export GH_ENTERPRISE_TOKEN='ghp_enterprise_secret'\n\
         export GH_HOST='attacker.example'\n\
         export OPENAI_API_KEY='sk-proj-snapshot-secret'\n\
         export AUTH_BUNDLE=\"GitHub $GH_TOKEN\n\
         OpenAI $OPENAI_API_KEY\"\n\
         export OPENAI_BASE_URL='https://api.snapshot.example/v1'\n\
         export IDENTITY_SEEN=\"${OPENAI_IDENTITY_TOKEN_FILE-missing}\"\n\
         export EXCLUDED_PARENT_HOME=\"${HOME-missing}\"\n\
         export STARTUP_PATH_OVERRIDE_SEEN=\"${PATH%%:*}\"\n",
    )?;

    let mut network_config = NetworkProxyConfig::default();
    network_config.set_credential_broker_enabled(/*enabled*/ true);
    let permission_profile = PermissionProfile::workspace_write();
    let network_spec = NetworkProxySpec::from_config_and_constraints(
        network_config,
        /*requirements*/ None,
        &permission_profile,
    )?;
    let started_proxy = network_spec
        .start_proxy(
            &permission_profile,
            /*policy_decider*/ None,
            /*blocked_request_observer*/ None,
            /*enable_network_approval_flow*/ false,
            Default::default(),
        )
        .await?;
    let network_proxy = started_proxy.proxy();

    let shell = Shell {
        shell_type: ShellType::Bash,
        shell_path: PathBuf::from("/bin/bash"),
    };
    let trusted_startup = ("BASH_ENV".to_string(), startup.display().to_string());
    let shell_environment_policy = ShellEnvironmentPolicy {
        exclude: vec![EnvironmentVariablePattern::new_case_insensitive("HOME")],
        r#set: HashMap::from([
            trusted_startup.clone(),
            ("GH_HOST".to_string(), "github.example.com".to_string()),
            (
                "OPENAI_IDENTITY_TOKEN_FILE".to_string(),
                "identity-token-secret".to_string(),
            ),
            (
                "PATH".to_string(),
                format!("/repository-controlled:{}", std::env::var("PATH")?),
            ),
        ]),
        ..ShellEnvironmentPolicy::default()
    };
    let credential_broker = SnapshotCredentialBroker {
        network_proxy: network_proxy.clone(),
        shell_environment_policy: shell_environment_policy.clone(),
        allow_login_shell: false,
    };
    let path = dir.path().join("snapshot.sh").abs();
    let credentials = write_shell_snapshot(
        &shell,
        &path,
        &dir.path().abs(),
        Some(&credential_broker),
        /*sandbox*/ None,
    )
    .await?;
    let snapshot = fs::read_to_string(&path).await?;

    for secret in [
        "ghp_shell_only_secret",
        "ghp_readonly_secret",
        "ghp_enterprise_secret",
        "sk-proj-snapshot-secret",
        "identity-token-secret",
    ] {
        assert!(!snapshot.contains(secret), "snapshot exposed {secret}");
    }
    assert!(!snapshot.contains("attacker.example"));
    assert!(snapshot.contains("api.snapshot.example"));
    assert!(snapshot.contains("IDENTITY_SEEN=\"missing\""));
    assert!(snapshot.contains("EXCLUDED_PARENT_HOME=\"missing\""));
    assert!(!snapshot.contains("STARTUP_PATH_OVERRIDE_SEEN=\"/repository-controlled\""));
    assert!(snapshot.contains("declare -rx HOMEBREW_GITHUB_API_TOKEN=\"${GITHUB_TOKEN-}\""));

    validate_snapshot(
        &shell,
        &path,
        &dir.path().abs(),
        Some(&credential_broker),
        /*sandbox*/ None,
    )
    .await?;

    let validation_path = dir.path().join("validation.sh").abs();
    fs::write(
        &validation_path,
        "test \"${HOME-missing}\" = missing && \
         test \"${CODEX_NETWORK_PROXY_CREDENTIAL_BROKER_ACTIVE-}\" = 1\n",
    )
    .await?;
    validate_snapshot(
        &shell,
        &validation_path,
        &dir.path().abs(),
        Some(&credential_broker),
        /*sandbox*/ None,
    )
    .await?;

    let filtered_startup_broker = SnapshotCredentialBroker {
        network_proxy: network_proxy.clone(),
        shell_environment_policy: ShellEnvironmentPolicy {
            r#set: HashMap::from([trusted_startup.clone()]),
            include_only: vec![
                EnvironmentVariablePattern::new_case_insensitive("PATH"),
                EnvironmentVariablePattern::new_case_insensitive("EXCLUDED_PARENT_HOME"),
            ],
            ..ShellEnvironmentPolicy::default()
        },
        allow_login_shell: false,
    };
    let filtered_startup_path = dir.path().join("filtered-startup-snapshot.sh").abs();
    write_shell_snapshot(
        &shell,
        &filtered_startup_path,
        &dir.path().abs(),
        Some(&filtered_startup_broker),
        /*sandbox*/ None,
    )
    .await?;
    assert!(
        !fs::read_to_string(&filtered_startup_path)
            .await?
            .contains("EXCLUDED_PARENT_HOME")
    );

    let inherited_secret = "ghp_inherited_enterprise_secret";
    let inherited_context = HashMap::from([(
        "GH_HOST".to_string(),
        "github.inherited.example".to_string(),
    )]);
    let mut inherited_discovery_env = HashMap::from([
        (
            "GH_ENTERPRISE_TOKEN".to_string(),
            inherited_secret.to_string(),
        ),
        ("GH_HOST".to_string(), "attacker.example".to_string()),
    ]);
    replace_provider_context_with_inherited(
        &mut inherited_discovery_env,
        &inherited_context,
        credential_broker_provider_context_env_keys(),
    );
    assert_eq!(
        inherited_discovery_env.get("GH_HOST").map(String::as_str),
        Some("github.inherited.example")
    );
    network_proxy.apply_to_env(&mut inherited_discovery_env);
    let mut inherited_allowed_env = HashMap::from([(
        "GH_ENTERPRISE_TOKEN".to_string(),
        inherited_secret.to_string(),
    )]);
    replace_provider_context_with_inherited(
        &mut inherited_allowed_env,
        &inherited_context,
        brokered_credential_binding_env_keys(&inherited_discovery_env),
    );
    network_proxy.apply_to_env(&mut inherited_allowed_env);
    assert_eq!(
        inherited_allowed_env.get("GH_HOST").map(String::as_str),
        Some("github.inherited.example")
    );
    assert_ne!(
        inherited_allowed_env
            .get("GH_ENTERPRISE_TOKEN")
            .map(String::as_str),
        Some(inherited_secret)
    );
    let mut inherited_snapshot = format!("export GH_ENTERPRISE_TOKEN={inherited_secret}\n");
    assert!(
        network_proxy.virtualize_brokered_text(&mut inherited_snapshot, &inherited_allowed_env)
    );
    assert!(!inherited_snapshot.contains(inherited_secret));

    let snapshot_file = ShellSnapshotFile { path, credentials };
    let mut env = HashMap::from([("GH_HOST".to_string(), "github.example.com".to_string())]);
    snapshot_file.restore_credentials(&mut env, &shell_environment_policy);
    for (key, value) in [
        ("GH_TOKEN", "ghp_shell_only_secret"),
        ("GITHUB_TOKEN", "ghp_readonly_secret"),
        ("GH_ENTERPRISE_TOKEN", "ghp_enterprise_secret"),
        ("OPENAI_API_KEY", "sk-proj-snapshot-secret"),
    ] {
        assert_eq!(env.get(key).map(String::as_str), Some(value), "{key}");
    }
    assert_eq!(
        env.get("OPENAI_BASE_URL").map(String::as_str),
        Some("https://api.snapshot.example/v1")
    );

    let unbrokered_replay = Command::new("/bin/bash")
        .arg("-c")
        .arg(". \"$1\" && printf '%s\\n%s' \"$AUTH_HEADER\" \"$AUTH_BUNDLE\"")
        .arg("snapshot")
        .arg(snapshot_file.path().as_path())
        .env_clear()
        .envs(&env)
        .output()?;
    assert!(unbrokered_replay.status.success());
    assert_eq!(
        String::from_utf8(unbrokered_replay.stdout)?,
        "Bearer ghp_shell_only_secret\nGitHub ghp_shell_only_secret\nOpenAI sk-proj-snapshot-secret"
    );

    network_proxy.apply_to_env(&mut env);
    assert_ne!(env["GH_TOKEN"], "ghp_shell_only_secret");
    assert_ne!(env["GITHUB_TOKEN"], "ghp_readonly_secret");
    assert_ne!(env["GH_ENTERPRISE_TOKEN"], "ghp_enterprise_secret");
    assert_ne!(env["OPENAI_API_KEY"], "sk-proj-snapshot-secret");
    let replay = Command::new("/bin/bash")
        .arg("-c")
        .arg(
            ". \"$1\" && printf '%s\\n%s\\n%s' \"$HOMEBREW_GITHUB_API_TOKEN\" \"$AUTH_HEADER\" \"$AUTH_BUNDLE\"",
        )
        .arg("snapshot")
        .arg(snapshot_file.path().as_path())
        .env_clear()
        .envs(&env)
        .output()?;
    assert!(
        replay.status.success(),
        "snapshot replay failed: {replay:?}"
    );
    assert_eq!(
        String::from_utf8(replay.stdout)?,
        format!(
            "{}\nBearer {}\nGitHub {}\nOpenAI {}",
            env["GITHUB_TOKEN"], env["GH_TOKEN"], env["GH_TOKEN"], env["OPENAI_API_KEY"]
        )
    );

    let filtered_policy = ShellEnvironmentPolicy {
        include_only: vec![EnvironmentVariablePattern::new_case_insensitive("GH_HOST")],
        ..ShellEnvironmentPolicy::default()
    };
    let mut filtered_env = HashMap::new();
    snapshot_file.restore_credentials(&mut filtered_env, &filtered_policy);
    assert!(filtered_env.is_empty());

    let partially_filtered_credential_broker = SnapshotCredentialBroker {
        network_proxy: network_proxy.clone(),
        shell_environment_policy: ShellEnvironmentPolicy {
            r#set: HashMap::from([trusted_startup.clone()]),
            include_only: vec![
                EnvironmentVariablePattern::new_case_insensitive("BASH_ENV"),
                EnvironmentVariablePattern::new_case_insensitive("GH_TOKEN"),
                EnvironmentVariablePattern::new_case_insensitive("AUTH_HEADER"),
                EnvironmentVariablePattern::new_case_insensitive("AUTH_BUNDLE"),
            ],
            ..ShellEnvironmentPolicy::default()
        },
        allow_login_shell: false,
    };
    for (github_token, openai_api_key) in [
        ("ghp_shell_only_secret", "sk-proj-snapshot-secret"),
        (env["GH_TOKEN"].as_str(), env["OPENAI_API_KEY"].as_str()),
    ] {
        std::fs::write(
            &startup,
            format!(
                "export GH_TOKEN='{github_token}'\n\
                 export GITHUB_TOKEN=\"$GH_TOKEN\"\n\
                 export AUTH_HEADER=\"Bearer $GH_TOKEN\"\n\
                 export OPENAI_API_KEY='{openai_api_key}'\n\
                 export AUTH_BUNDLE=\"GitHub $GH_TOKEN\n\
                 OpenAI $OPENAI_API_KEY\"\n\
                 unset OPENAI_API_KEY\n"
            ),
        )?;
        let partially_filtered_path = dir.path().join("partially-filtered-snapshot.sh").abs();
        write_shell_snapshot(
            &shell,
            &partially_filtered_path,
            &dir.path().abs(),
            Some(&partially_filtered_credential_broker),
            /*sandbox*/ None,
        )
        .await?;
        let filtered_replay = Command::new("/bin/bash")
            .arg("-c")
            .arg(". \"$1\" && printf '%s\\n%s' \"$AUTH_HEADER\" \"${AUTH_BUNDLE-unset}\"")
            .arg("snapshot")
            .arg(partially_filtered_path.as_path())
            .env_clear()
            .env("GH_TOKEN", "ghp_filtered_dummy")
            .output()?;
        assert!(filtered_replay.status.success());
        assert_eq!(
            String::from_utf8(filtered_replay.stdout)?,
            "Bearer ghp_filtered_dummy\nunset"
        );
    }

    std::fs::write(
        &startup,
        "export GH_TOKEN='ghp_hidden_alias_secret'\n\
         export HIDDEN_HEADER=\"Bearer $GH_TOKEN\"\n\
         unset GH_TOKEN GITHUB_ENTERPRISE_TOKEN\n",
    )?;
    let mut previously_brokered_env = HashMap::from([(
        "GH_TOKEN".to_string(),
        "ghp_hidden_alias_secret".to_string(),
    )]);
    network_proxy.apply_to_env(&mut previously_brokered_env);
    let inherited_credential_broker = SnapshotCredentialBroker {
        network_proxy,
        shell_environment_policy: ShellEnvironmentPolicy {
            r#set: HashMap::from([trusted_startup]),
            ..ShellEnvironmentPolicy::default()
        },
        allow_login_shell: false,
    };
    let inherited_path = dir.path().join("inherited-snapshot.sh").abs();
    write_shell_snapshot(
        &shell,
        &inherited_path,
        &dir.path().abs(),
        Some(&inherited_credential_broker),
        /*sandbox*/ None,
    )
    .await?;
    assert!(
        !fs::read_to_string(inherited_path)
            .await?
            .contains("ghp_hidden_alias_secret")
    );

    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn try_create_creates_and_deletes_snapshot_file() -> Result<()> {
    let dir = tempdir()?;
    let shell = Shell {
        shell_type: ShellType::Bash,
        shell_path: PathBuf::from("/bin/bash"),
    };

    let snapshot = ShellSnapshot::try_create(
        &dir.path().abs(),
        ThreadId::new(),
        &dir.path().abs(),
        &shell,
        /*state_db*/ None,
        /*credential_broker*/ None,
        /*sandbox*/ None,
    )
    .await
    .expect("snapshot should be created");
    let path = snapshot.path.clone();
    assert!(path.exists());

    drop(snapshot);

    assert!(!path.exists());

    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn try_create_uses_distinct_generation_paths() -> Result<()> {
    let dir = tempdir()?;
    let session_id = ThreadId::new();
    let shell = Shell {
        shell_type: ShellType::Bash,
        shell_path: PathBuf::from("/bin/bash"),
    };

    let initial_snapshot = ShellSnapshot::try_create(
        &dir.path().abs(),
        session_id,
        &dir.path().abs(),
        &shell,
        /*state_db*/ None,
        /*credential_broker*/ None,
        /*sandbox*/ None,
    )
    .await
    .expect("initial snapshot should be created");
    let refreshed_snapshot = ShellSnapshot::try_create(
        &dir.path().abs(),
        session_id,
        &dir.path().abs(),
        &shell,
        /*state_db*/ None,
        /*credential_broker*/ None,
        /*sandbox*/ None,
    )
    .await
    .expect("refreshed snapshot should be created");
    let initial_path = initial_snapshot.path.clone();
    let refreshed_path = refreshed_snapshot.path.clone();
    assert_ne!(initial_path, refreshed_path);
    assert_eq!(initial_path.exists(), true);
    assert_eq!(refreshed_path.exists(), true);

    drop(initial_snapshot);

    assert_eq!(initial_path.exists(), false);
    assert_eq!(refreshed_path.exists(), true);

    drop(refreshed_snapshot);

    assert_eq!(refreshed_path.exists(), false);

    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn snapshot_shell_does_not_inherit_stdin() -> Result<()> {
    let _stdin_guard = BlockingStdinPipe::install()?;

    let dir = tempdir()?;
    let home = dir.path().abs();
    let read_status_path = home.join("stdin-read-status");
    let read_status_display = read_status_path.display();
    // Persist the startup `read` exit status so the test can assert whether
    // bash saw EOF on stdin after the snapshot process exits.
    let bashrc = format!("read -t 1 -r ignored\nprintf '%s' \"$?\" > \"{read_status_display}\"\n");
    fs::write(home.join(".bashrc"), bashrc).await?;

    let shell = Shell {
        shell_type: ShellType::Bash,
        shell_path: PathBuf::from("/bin/bash"),
    };

    let home_display = home.display();
    let script = format!(
        "HOME=\"{home_display}\"; export HOME; {}",
        snapshot_capture_script(
            ShellType::Bash,
            SnapshotCaptureOptions {
                startup: SnapshotStartup::Interactive,
                declarations: true,
                environment: false,
            }
        )
        .expect("bash supports snapshots")
    );
    let output = run_script_with_timeout(
        &shell,
        &script,
        Duration::from_secs(2),
        SnapshotShellMode::Login,
        &home,
        /*credential_broker*/ None,
        /*sandbox*/ None,
    )
    .await
    .context("run snapshot command")?;
    let read_status = fs::read_to_string(&read_status_path)
        .await
        .context("read stdin probe status")?;

    assert_eq!(
        read_status, "1",
        "expected shell startup read to see EOF on stdin; status={read_status:?}"
    );

    assert!(
        output.contains("# Snapshot file"),
        "expected snapshot marker in output; output={output:?}"
    );

    Ok(())
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn timed_out_snapshot_shell_is_terminated() -> Result<()> {
    use std::process::Stdio;
    use tokio::time::Duration as TokioDuration;
    use tokio::time::Instant;
    use tokio::time::sleep;

    let dir = tempdir()?;
    let pid_path = dir.path().join("pid");
    let script = format!("echo $$ > \"{}\"; sleep 30", pid_path.display());

    let shell = Shell {
        shell_type: ShellType::Sh,
        shell_path: PathBuf::from("/bin/sh"),
    };

    let err = run_script_with_timeout(
        &shell,
        &script,
        Duration::from_secs(1),
        SnapshotShellMode::Login,
        &dir.path().abs(),
        /*credential_broker*/ None,
        /*sandbox*/ None,
    )
    .await
    .expect_err("snapshot shell should time out");
    assert!(
        err.to_string().contains("timed out"),
        "expected timeout error, got {err:?}"
    );

    let pid = fs::read_to_string(&pid_path)
        .await
        .expect("snapshot shell writes its pid before timing out")
        .trim()
        .parse::<i32>()?;

    let deadline = Instant::now() + TokioDuration::from_secs(1);
    loop {
        let kill_status = StdCommand::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .stderr(Stdio::null())
            .stdout(Stdio::null())
            .status()?;
        if !kill_status.success() {
            break;
        }
        if Instant::now() >= deadline {
            panic!("timed out snapshot shell is still alive after grace period");
        }
        sleep(TokioDuration::from_millis(50)).await;
    }

    Ok(())
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn macos_zsh_snapshot_includes_sections() -> Result<()> {
    let snapshot = get_snapshot(ShellType::Zsh).await?;
    assert_posix_snapshot_sections(&snapshot);
    Ok(())
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn linux_bash_snapshot_includes_sections() -> Result<()> {
    let snapshot = get_snapshot(ShellType::Bash).await?;
    assert_posix_snapshot_sections(&snapshot);
    Ok(())
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn linux_sh_snapshot_includes_sections() -> Result<()> {
    let snapshot = get_snapshot(ShellType::Sh).await?;
    assert_posix_snapshot_sections(&snapshot);
    Ok(())
}

#[cfg(target_os = "windows")]
#[ignore]
#[tokio::test]
async fn windows_powershell_snapshot_includes_sections() -> Result<()> {
    let snapshot = get_snapshot(ShellType::PowerShell).await?;
    assert!(snapshot.contains("# Snapshot file"));
    assert!(snapshot.contains("aliases "));
    assert!(snapshot.contains("exports "));
    Ok(())
}

async fn write_rollout_stub(codex_home: &Path, session_id: ThreadId) -> Result<PathBuf> {
    let dir = codex_home
        .join("sessions")
        .join("2025")
        .join("01")
        .join("01");
    fs::create_dir_all(&dir).await?;
    let path = dir.join(format!("rollout-2025-01-01T00-00-00-{session_id}.jsonl"));
    fs::write(&path, "").await?;
    Ok(path)
}

#[tokio::test]
async fn cleanup_stale_snapshots_removes_orphans_and_keeps_live() -> Result<()> {
    let dir = tempdir()?;
    let codex_home = dir.path().abs();
    let snapshot_dir = codex_home.join(SNAPSHOT_DIR);
    fs::create_dir_all(&snapshot_dir).await?;

    let live_session = ThreadId::new();
    let orphan_session = ThreadId::new();
    let live_snapshot = snapshot_dir.join(format!("{live_session}.123.sh"));
    let orphan_snapshot = snapshot_dir.join(format!("{orphan_session}.456.sh"));
    let invalid_snapshot = snapshot_dir.join("not-a-snapshot.txt");

    write_rollout_stub(&codex_home, live_session).await?;
    fs::write(&live_snapshot, "live").await?;
    fs::write(&orphan_snapshot, "orphan").await?;
    fs::write(&invalid_snapshot, "invalid").await?;

    cleanup_stale_snapshots(&codex_home, ThreadId::new(), /*state_db*/ None).await?;

    assert_eq!(live_snapshot.exists(), true);
    assert_eq!(orphan_snapshot.exists(), false);
    assert_eq!(invalid_snapshot.exists(), false);
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn cleanup_stale_snapshots_removes_stale_rollouts() -> Result<()> {
    let dir = tempdir()?;
    let codex_home = dir.path().abs();
    let snapshot_dir = codex_home.join(SNAPSHOT_DIR);
    fs::create_dir_all(&snapshot_dir).await?;

    let stale_session = ThreadId::new();
    let stale_snapshot = snapshot_dir.join(format!("{stale_session}.123.sh"));
    let rollout_path = write_rollout_stub(&codex_home, stale_session).await?;
    fs::write(&stale_snapshot, "stale").await?;

    set_file_mtime(&rollout_path, SNAPSHOT_RETENTION + Duration::from_secs(60))?;

    cleanup_stale_snapshots(&codex_home, ThreadId::new(), /*state_db*/ None).await?;

    assert_eq!(stale_snapshot.exists(), false);
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn cleanup_stale_snapshots_skips_active_session() -> Result<()> {
    let dir = tempdir()?;
    let codex_home = dir.path().abs();
    let snapshot_dir = codex_home.join(SNAPSHOT_DIR);
    fs::create_dir_all(&snapshot_dir).await?;

    let active_session = ThreadId::new();
    let active_snapshot = snapshot_dir.join(format!("{active_session}.123.sh"));
    let rollout_path = write_rollout_stub(&codex_home, active_session).await?;
    fs::write(&active_snapshot, "active").await?;

    set_file_mtime(&rollout_path, SNAPSHOT_RETENTION + Duration::from_secs(60))?;

    cleanup_stale_snapshots(&codex_home, active_session, /*state_db*/ None).await?;

    assert_eq!(active_snapshot.exists(), true);
    Ok(())
}

#[cfg(unix)]
fn set_file_mtime(path: &Path, age: Duration) -> Result<()> {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_secs()
        .saturating_sub(age.as_secs());
    let tv_sec = now
        .try_into()
        .map_err(|_| anyhow!("Snapshot mtime is out of range for libc::timespec"))?;
    let ts = libc::timespec { tv_sec, tv_nsec: 0 };
    let times = [ts, ts];
    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes())?;
    let result = unsafe { libc::utimensat(libc::AT_FDCWD, c_path.as_ptr(), times.as_ptr(), 0) };
    if result != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}
