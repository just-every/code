//! Covers policy presence and the environment-only configuration projection.

use super::*;
use codex_config::RequirementSource;
use pretty_assertions::assert_eq;

#[test]
fn environment_policy_presence_keeps_selected_and_managed_denials() {
    let mut configured_proxy = NetworkProxyConfig::default();
    configured_proxy.set_denied_domains(vec!["blocked.example".to_string()]);
    let expected = EnvironmentNetworkPolicy::from_config(
        &configured_proxy,
        /*managed_allowed_domains_only*/ false,
    );
    for (has_selected_policy, requirements, has_policy) in [
        (false, None, false),
        (true, None, true),
        (
            false,
            Some(NetworkConstraints {
                enabled: Some(false),
                ..Default::default()
            }),
            true,
        ),
    ] {
        let actual = PreparedNetworkConfig {
            configured_proxy: configured_proxy.clone(),
        }
        .build_environment_policy(
            requirements.map(|value| Sourced::new(value, RequirementSource::Unknown)),
            &PermissionProfile::read_only(),
            has_selected_policy,
        )
        .unwrap();
        assert_eq!(actual, has_policy.then(|| expected.clone()));
    }
}

#[test]
fn attachment_projection_preserves_policy_and_drops_listener_addresses() {
    for raw in [
        "",
        r#"
enabled = false
allow_upstream_proxy = false
dangerously_allow_all_unix_sockets = false
allow_local_binding = false
[domains]
'allowed.example' = 'allow'
'blocked.example' = 'deny'
[unix_sockets]
'/tmp/allowed.sock' = 'allow'
'/tmp/blocked.sock' = 'deny'
"#,
    ] {
        let expected: NetworkToml = toml::from_str(raw).unwrap();
        let network = NetworkToml {
            proxy_url: Some("not a listener URL".to_string()),
            socks_url: Some("socks5://127.0.0.1:1080".to_string()),
            ..expected.clone()
        };
        assert_eq!(
            project_environment_profile_network(Some(network)),
            Ok(Some(expected))
        );
    }
    assert_eq!(
        project_environment_profile_network(/*network*/ None),
        Ok(None)
    );
}

#[test]
fn attachment_projection_rejects_unsupported_restrictions_and_invalid_policy() {
    for raw in [
        "mode = 'full'",
        "mode = 'limited'",
        r#"
[mitm.actions.redact]
strip_request_headers = ['Authorization']
[mitm.hooks.api]
host = 'api.example'
methods = ['GET']
path_prefixes = ['/']
action = ['redact']
"#,
        "[unix_sockets]\n'relative.sock' = 'allow'",
        "[unix_sockets]\n'relative.sock' = 'deny'",
        "[domains]\n'[' = 'allow'",
    ] {
        let network: NetworkToml = toml::from_str(raw).unwrap();
        assert_eq!(
            project_environment_profile_network(Some(network)),
            Err(EnvironmentNetworkConfigError),
            "{raw}"
        );
    }
}
