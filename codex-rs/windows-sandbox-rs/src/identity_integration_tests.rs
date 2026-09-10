//! Exercises credential orchestration with real marker files and DPAPI credentials.
//! Setup and account lookup callbacks isolate machine state. This does not exercise
//! the public wrapper, native account queries, payload serialization, singleflight,
//! or helper launches.

use super::require_logon_sandbox_creds_with_setup;
use crate::WindowsSandboxProxySettingsMode;
use crate::resolved_permissions::ResolvedWindowsSandboxPermissions;
use crate::setup::SETUP_VERSION;
use crate::setup::SandboxUserRecord;
use crate::setup::SandboxUsersFile;
use crate::setup::SetupMarker;
use crate::setup::sandbox_users_path;
use crate::setup::setup_marker_path;
use anyhow::Result;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use codex_protocol::models::PermissionProfile;
use pretty_assertions::assert_eq;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use windows_sys::Win32::NetworkManagement::NetManagement::UF_NORMAL_ACCOUNT;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SetupLaunch {
    Full,
    Refresh,
}

#[test]
fn credential_setup_reconciles_effective_firewall_policy() -> Result<()> {
    let permissions = ResolvedWindowsSandboxPermissions::try_from_permission_profile(
        &PermissionProfile::read_only(),
    )?;
    for (stored_binding, desired_binding, ports, expected_launch) in [
        (true, true, "8080", SetupLaunch::Refresh),
        (true, true, "", SetupLaunch::Refresh),
        (true, true, "8080,3129", SetupLaunch::Refresh),
        (false, false, "8080", SetupLaunch::Full),
        (false, true, "3128", SetupLaunch::Full),
        (true, false, "3128", SetupLaunch::Full),
    ] {
        let launches = RefCell::new(Vec::new());
        let home = tempfile::tempdir()?;
        let marker = SetupMarker {
            version: SETUP_VERSION,
            offline_username: "offline".into(),
            online_username: "online".into(),
            created_at: None,
            proxy_ports: vec![3128],
            allow_local_binding: stored_binding,
        };
        let user = SandboxUserRecord {
            username: marker.offline_username.clone(),
            password: BASE64.encode(crate::dpapi::protect(b"test-password")?),
        };
        let users = SandboxUsersFile {
            version: SETUP_VERSION,
            offline: user.clone(),
            online: SandboxUserRecord {
                username: marker.online_username.clone(),
                ..user
            },
        };
        let marker_bytes = serde_json::to_vec(&marker)?;
        for (path, bytes) in [
            (setup_marker_path(home.path()), marker_bytes.clone()),
            (sandbox_users_path(home.path()), serde_json::to_vec(&users)?),
        ] {
            fs::create_dir_all(path.parent().unwrap())?;
            fs::write(path, bytes)?;
        }
        let env = HashMap::from([
            ("CODEX_WINDOWS_SANDBOX_PROXY_PORTS".into(), ports.into()),
            (
                "CODEX_NETWORK_ALLOW_LOCAL_BINDING".into(),
                u8::from(desired_binding).to_string(),
            ),
        ]);
        let result = require_logon_sandbox_creds_with_setup(
            &permissions,
            home.path(),
            &env,
            home.path(),
            Some(&[]),
            /*read_roots_include_platform_defaults*/ false,
            Some(&[]),
            &[],
            &[],
            /*proxy_enforced*/ true,
            WindowsSandboxProxySettingsMode::Reconcile,
            |_, _| {
                launches.borrow_mut().push(SetupLaunch::Full);
                Err(anyhow::anyhow!("full setup intercepted"))
            },
            |_, _, _| {
                launches.borrow_mut().push(SetupLaunch::Refresh);
                Ok(())
            },
            |_| Ok(Some(UF_NORMAL_ACCOUNT)),
        );
        assert_eq!(launches.into_inner(), [expected_launch]);
        assert_eq!(
            result
                .map(|creds| (creds.username, creds.password))
                .map_err(|err| err.to_string()),
            match expected_launch {
                SetupLaunch::Refresh => Ok(("offline".into(), "test-password".into())),
                SetupLaunch::Full => Err("full setup intercepted".into()),
            }
        );
        assert_eq!(fs::read(setup_marker_path(home.path()))?, marker_bytes);
    }
    Ok(())
}
