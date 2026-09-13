//! Stops sandbox work under the setup lock before removing its resources and protections.

use std::path::Path;

use anyhow::Result;
use anyhow::anyhow;

use crate::setup::OFFLINE_USERNAME;
use crate::setup::ONLINE_USERNAME;

mod firewall;
mod principals;
mod processes;

/// Removes sandbox resources created for one authenticated packaged installation.
/// Keep a supplied home and its ancestors pinned until `clean_up_desktop` starts.
/// That callback removes user-owned desktop files while the sandbox accounts remain disabled.
pub fn clean_up_packaged_windows_sandbox(
    codex_home: Option<&Path>,
    clean_up_desktop: impl FnOnce() -> Result<()>,
) -> Result<()> {
    prepare_packaged_windows_sandbox_cleanup()?.finish(codex_home, clean_up_desktop)
}

/// Holds the setup lock after sandbox accounts are disabled, their processes
/// have exited, and their logon tokens have been released.
///
/// Keep this guard on the thread that acquired the setup lock. Callers must not
/// re-enable the accounts or create new sandbox logons before `finish`.
///
/// Dropping the guard only releases the lock. It does not re-enable accounts,
/// remove protections, or undo work performed between the two phases.
#[must_use = "finish cleanup or leave the sandbox accounts disabled with their protections intact"]
pub struct PreparedWindowsSandboxCleanup {
    _setup_lock: crate::setup_mutex::SandboxSetupLock,
}

/// Disables sandbox accounts, stops their processes, and waits for their logon
/// tokens to be released.
///
/// No resources or protections are removed here. On failure, restoration of the
/// original account flags is attempted before the setup lock is released; any
/// restoration failure is included in the returned error.
pub fn prepare_packaged_windows_sandbox_cleanup() -> Result<PreparedWindowsSandboxCleanup> {
    let setup_lock = crate::setup_mutex::acquire_sandbox_setup_lock(/*timeout_ms*/ 5_000)?;
    let mut errors = Vec::new();
    let mut users = principals::DisabledSandboxUsers::default();
    if let Err(error) = users.disable().and_then(|()| processes::stop(&users)) {
        errors.push(format!("{error:#}"));
        // No network protections have been removed, so failed preparation can restore these flags.
        if let Err(error) = users.restore() {
            errors.push(format!("{error:#}"));
        }
        return Err(anyhow!(errors.join("; ")));
    }

    Ok(PreparedWindowsSandboxCleanup {
        _setup_lock: setup_lock,
    })
}

impl PreparedWindowsSandboxCleanup {
    /// Removes resources while retaining the setup lock and disabled accounts.
    ///
    /// Keep a supplied home and its ancestors pinned until `clean_up_desktop`
    /// starts. Independent cleanup steps continue after an error, as in
    /// `clean_up_packaged_windows_sandbox`.
    pub fn finish(
        self,
        codex_home: Option<&Path>,
        clean_up_desktop: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        let _prepared = self;
        let mut errors = Vec::new();

        if let Some(codex_home) = codex_home {
            for directory in [
                crate::setup::sandbox_dir(codex_home),
                crate::setup::sandbox_secrets_dir(codex_home),
                crate::setup::sandbox_bin_dir(codex_home),
            ] {
                match std::fs::remove_dir_all(&directory) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => {
                        errors.push(format!("remove {}: {error}", directory.display()));
                    }
                }
            }
        }

        if let Err(error) = clean_up_desktop() {
            errors.push(format!("{error:#}"));
        }

        for result in [
            crate::wfp::remove_wfp_filters(),
            firewall::cleanup_firewall_rules(),
            principals::remove_sandbox_principal("CodexSandboxUsers"),
            crate::hide_users::unhide_sandbox_users(&[OFFLINE_USERNAME, ONLINE_USERNAME]),
            // Keep accounts disabled and setup locked until shared cleanup and account deletion finish.
            principals::remove_sandbox_principal(OFFLINE_USERNAME),
            principals::remove_sandbox_principal(ONLINE_USERNAME),
        ] {
            if let Err(error) = result {
                errors.push(format!("{error:#}"));
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(anyhow!(errors.join("; ")))
        }
    }
}
