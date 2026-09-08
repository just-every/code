use crate::exec::ExecCapturePolicy;
use crate::exec::ExecExpiration;
use crate::sandboxing::ExecOptions;
use crate::sandboxing::ExecRequest;
use crate::sandboxing::execute_env;
use crate::tools::sandboxing::SandboxAttempt;
use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use codex_network_proxy::NetworkProxy;
use codex_protocol::config_types::WindowsSandboxLevel;
use codex_protocol::models::AdditionalPermissionProfile;
use codex_protocol::models::FileSystemPermissions;
use codex_protocol::models::PermissionProfile;
use codex_sandboxing::SandboxCommand;
use codex_sandboxing::SandboxManager;
use codex_sandboxing::SandboxTransformRequest;
use codex_sandboxing::SandboxType;
use codex_sandboxing::policy_transforms::merge_permission_profiles;
use codex_utils_absolute_path::AbsolutePathBuf;
use codex_utils_path_uri::PathUri;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Clone)]
pub(crate) struct ShellSnapshotSandbox {
    sandbox: SandboxType,
    sandbox_requested: bool,
    permissions: PermissionProfile,
    enforce_managed_network: bool,
    sandbox_policy_cwd: PathUri,
    workspace_roots: Vec<PathUri>,
    codex_linux_sandbox_exe: Option<PathBuf>,
    use_legacy_landlock: bool,
    windows_sandbox_level: WindowsSandboxLevel,
    windows_sandbox_private_desktop: bool,
    network: Option<NetworkProxy>,
    environment_id: String,
    additional_permissions: Option<AdditionalPermissionProfile>,
}

pub(crate) fn snapshot_read_permissions(
    snapshot_path: &AbsolutePathBuf,
    permissions: &PermissionProfile,
    sandbox_cwd: &PathUri,
) -> Option<AdditionalPermissionProfile> {
    if let Ok(cwd) = sandbox_cwd.to_abs_path()
        && permissions
            .file_system_sandbox_policy()
            .can_read_local_path_with_cwd(snapshot_path.as_path(), cwd.as_path())
    {
        return None;
    }
    Some(AdditionalPermissionProfile {
        network: None,
        file_system: Some(FileSystemPermissions::from_read_write_roots(
            Some(vec![snapshot_path.clone()]),
            /*write*/ None,
        )),
    })
}

impl ShellSnapshotSandbox {
    pub(crate) fn new(
        attempt: &SandboxAttempt<'_>,
        network: Option<&NetworkProxy>,
        environment_id: &str,
        additional_permissions: Option<&AdditionalPermissionProfile>,
    ) -> Self {
        Self {
            sandbox: attempt.sandbox,
            sandbox_requested: attempt.sandbox_requested,
            permissions: attempt.permissions.clone(),
            enforce_managed_network: attempt.enforce_managed_network,
            sandbox_policy_cwd: attempt.sandbox_cwd.clone(),
            workspace_roots: attempt.workspace_roots.to_vec(),
            codex_linux_sandbox_exe: attempt.codex_linux_sandbox_exe.cloned(),
            use_legacy_landlock: attempt.use_legacy_landlock,
            windows_sandbox_level: attempt.windows_sandbox_level,
            windows_sandbox_private_desktop: attempt.windows_sandbox_private_desktop,
            network: attempt.network_proxy(network).cloned(),
            environment_id: environment_id.to_string(),
            additional_permissions: additional_permissions.cloned(),
        }
    }

    pub(crate) async fn run(
        &self,
        args: Vec<String>,
        cwd: &AbsolutePathBuf,
        mut env: HashMap<String, String>,
        snapshot_timeout: Duration,
        shell_name: &str,
        snapshot_read_path: Option<&AbsolutePathBuf>,
    ) -> Result<String> {
        if self.sandbox_requested && self.sandbox == SandboxType::None {
            bail!("shell snapshot sandbox cannot be enforced on this host");
        }

        let managed_network = if let Some(network) = self.network.as_ref() {
            let prepared = network
                .prepare_for_optional_environment(env, Some(&self.environment_id))
                .context("failed to prepare managed network for shell snapshot")?;
            env = prepared.env;
            Some(prepared.sandbox_context)
        } else {
            None
        };
        let (program, args) = args
            .split_first()
            .ok_or_else(|| anyhow!("shell snapshot command is empty"))?;
        let snapshot_permissions = snapshot_read_path.and_then(|path| {
            snapshot_read_permissions(
                path,
                &self
                    .permissions
                    .clone()
                    .materialize_project_roots_with_path_uris(&self.workspace_roots),
                &self.sandbox_policy_cwd,
            )
        });
        let additional_permissions = merge_permission_profiles(
            self.additional_permissions.as_ref(),
            snapshot_permissions.as_ref(),
        );
        let manager = SandboxManager::new();
        let request = manager
            .transform(SandboxTransformRequest {
                command: SandboxCommand {
                    program: program.clone().into(),
                    args: args.to_vec(),
                    cwd: PathUri::from_abs_path(cwd),
                    env,
                    managed_network,
                    additional_permissions,
                },
                permissions: &self.permissions,
                sandbox: self.sandbox,
                enforce_managed_network: self.enforce_managed_network,
                environment_id: Some(&self.environment_id),
                network: self.network.as_ref(),
                sandbox_policy_cwd: &self.sandbox_policy_cwd,
                codex_linux_sandbox_exe: self.codex_linux_sandbox_exe.as_deref(),
                use_legacy_landlock: self.use_legacy_landlock,
                windows_sandbox_level: self.windows_sandbox_level,
                windows_sandbox_private_desktop: self.windows_sandbox_private_desktop,
            })
            .context("failed to prepare shell snapshot sandbox")?;
        let workspace_roots = self
            .workspace_roots
            .iter()
            .map(PathUri::to_abs_path)
            .collect::<std::io::Result<Vec<_>>>()
            .context("failed to resolve shell snapshot workspace roots")?;
        let request = ExecRequest::from_sandbox_exec_request(
            request,
            ExecOptions {
                expiration: ExecExpiration::Timeout(snapshot_timeout),
                capture_policy: ExecCapturePolicy::FullBuffer,
            },
            workspace_roots,
        )
        .context("failed to prepare shell snapshot execution")?;
        let output = tokio::time::timeout(
            snapshot_timeout,
            execute_env(request, /*stdout_stream*/ None),
        )
        .await
        .map_err(|_| anyhow!("Snapshot command timed out for {shell_name}"))?
        .with_context(|| format!("Failed to execute sandboxed {shell_name}"))?;

        if output.exit_code != 0 {
            bail!(
                "Snapshot command exited with status {}: {}",
                output.exit_code,
                output.stderr.text
            );
        }

        Ok(output.stdout.text)
    }
}
