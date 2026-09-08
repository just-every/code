//! Translate canonical permissions into native MXC grants without expanding
//! access. Denies and read-only carveouts remain separate kernel policy lists.

use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;

use codex_protocol::protocol::FileSystemPath;
use codex_protocol::protocol::FileSystemSpecialPath;
use codex_utils_path_uri::PathUri;
use thiserror::Error;
use wxc_common::cmdline::CommandLineContext;
use wxc_common::cmdline::CommandLineError;
use wxc_common::cmdline::cmdline_from_argv_for_context;
use wxc_common::models::ContainerPolicy;
use wxc_common::models::ExecutionRequest;
use wxc_common::models::FallbackPolicy;
use wxc_common::models::NetworkAction;
use wxc_common::models::NetworkEgressPolicy;
use wxc_common::models::NetworkIngressPolicy;
use wxc_common::models::NetworkPolicy;

use crate::MxcCommand;

// Use the same Windows case/URI identity as the canonical permission model,
// while retaining native spellings for the OS API.
type NativePaths = HashMap<PathUri, PathBuf>;

/// Invalid inputs or unsupported permissions encountered during policy translation.
#[derive(Debug, Error)]
pub enum PolicyError {
    #[error("MXC command must not be empty")]
    EmptyCommand,
    #[error("MXC requires an absolute policy working directory")]
    RelativePolicyCwd,
    #[error("MXC requires an absolute command working directory")]
    RelativeCommandCwd,
    #[error("MXC symbolic filesystem roots are not implemented")]
    SymbolicRoots,
    #[error("MXC does not support filesystem deny glob patterns")]
    DenyGlobs,
    #[error("MXC requires a Unicode command working directory")]
    NonUnicodeCommandCwd,
    #[error("MXC requires Unicode filesystem policy paths")]
    NonUnicodePolicyPath,
    #[error(transparent)]
    Path(#[from] std::io::Error),
    #[error(transparent)]
    CommandLine(#[from] CommandLineError),
}

pub fn build_request(
    command: &MxcCommand,
    command_cwd: &Path,
    env: Vec<String>,
) -> Result<ExecutionRequest, PolicyError> {
    if command.command.is_empty() {
        return Err(PolicyError::EmptyCommand);
    }
    let permissions = &command.permissions;
    let cwd = &command.sandbox_policy_cwd;
    if !cwd.is_absolute() {
        return Err(PolicyError::RelativePolicyCwd);
    }
    if !command_cwd.is_absolute() {
        return Err(PolicyError::RelativeCommandCwd);
    }
    let fs = permissions.file_system_sandbox_policy();
    if fs.has_full_disk_write_access()
        || fs.entries.iter().any(|entry| match &entry.path {
            FileSystemPath::Path { .. } | FileSystemPath::GlobPattern { .. } => false,
            FileSystemPath::Special { value } => {
                !matches!(value, FileSystemSpecialPath::Unknown { .. })
            }
        })
    {
        return Err(PolicyError::SymbolicRoots);
    }
    if !fs.get_unreadable_globs_with_cwd(cwd).is_empty() {
        return Err(PolicyError::DenyGlobs);
    }

    let roots = fs.get_writable_roots_with_cwd_preserving_mutable_paths(cwd);
    let mut write = collect_paths(roots.iter().map(|root| root.root.to_path_buf()))?;
    let mut read = collect_paths(
        fs.get_readable_roots_with_cwd(cwd)
            .into_iter()
            .filter(|path| !fs.can_write_local_path_with_cwd(path.as_path(), cwd))
            .map(|path| path.to_path_buf()),
    )?;
    let deny = collect_paths(
        fs.get_unreadable_roots_with_cwd(cwd)
            .into_iter()
            .map(|path| path.to_path_buf()),
    )?;
    let carveouts = collect_paths(roots.into_iter().flat_map(|root| {
        let protected = root
            .protected_metadata_names
            .into_iter()
            .map(|name| root.root.join(name).to_path_buf());
        root.read_only_subpaths
            .into_iter()
            .map(|path| path.to_path_buf())
            .chain(protected)
            .collect::<Vec<_>>()
    }))?;
    // Write restrictions must not grant otherwise forbidden reads.
    read.extend(
        carveouts
            .iter()
            .filter(|(_, path)| fs.can_read_local_path_with_cwd(path.as_path(), cwd))
            .map(|(key, path)| (key.clone(), path.clone())),
    );
    write.retain(|key, _| !carveouts.contains_key(key) && !deny.contains_key(key));
    read.retain(|key, _| !deny.contains_key(key));
    // Resolve equal-path write overrides before reaching MXC so the native
    // API receives one effective access mode for each path identity.
    read.retain(|key, _| !write.contains_key(key) && !deny.contains_key(key));
    let network_enabled = permissions.network_sandbox_policy().is_enabled();
    let egress_default = if network_enabled {
        NetworkAction::Allow
    } else {
        NetworkAction::Deny
    };
    let ingress_default = if network_enabled {
        NetworkAction::Allow
    } else {
        NetworkAction::Deny
    };
    let egress = NetworkEgressPolicy {
        default: egress_default,
        ..Default::default()
    };
    Ok(ExecutionRequest {
        script_code: cmdline_from_argv_for_context(
            &command.command,
            CommandLineContext::WindowsCreateProcess,
        )?,
        working_directory: command_cwd
            .to_str()
            .ok_or(PolicyError::NonUnicodeCommandCwd)?
            .to_owned(),
        env,
        policy: ContainerPolicy {
            capabilities: vec!["registryRead".to_owned()],
            readwrite_paths: unicode_paths(write)?,
            readonly_paths: unicode_paths(read)?,
            denied_paths: unicode_paths(deny)?,
            fallback: FallbackPolicy {
                allow_dacl_mutation: false,
            },
            default_network_policy: if egress_default == NetworkAction::Allow {
                NetworkPolicy::Allow
            } else {
                NetworkPolicy::Block
            },
            allow_local_network: ingress_default == NetworkAction::Allow,
            network_egress: Some(egress),
            network_ingress: Some(NetworkIngressPolicy {
                default: ingress_default,
                host_loopback: if network_enabled {
                    NetworkAction::Allow
                } else {
                    NetworkAction::Deny
                },
            }),
            network_specified: true,
            network_mode_specified: true,
            ..Default::default()
        },
        ..Default::default()
    })
}

fn collect_paths(paths: impl IntoIterator<Item = PathBuf>) -> Result<NativePaths, PolicyError> {
    paths
        .into_iter()
        .map(|path| Ok((PathUri::from_host_native_path(&path)?, path)))
        .collect()
}

fn unicode_paths(paths: NativePaths) -> Result<Vec<String>, PolicyError> {
    let mut paths = paths
        .into_values()
        .map(|path| {
            path.into_os_string()
                .into_string()
                .map_err(|_| PolicyError::NonUnicodePolicyPath)
        })
        .collect::<Result<Vec<_>, _>>()?;
    paths.sort();
    Ok(paths)
}
