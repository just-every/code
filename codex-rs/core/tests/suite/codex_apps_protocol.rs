//! Verifies the hosted Apps protocol choice in standalone Codex sessions.

use codex_features::Feature;
use codex_login::CodexAuth;
use codex_mcp::CODEX_APPS_MCP_SERVER_NAME;
use core_test_support::apps_test_server::AppsTestServer;
use core_test_support::responses;
use core_test_support::skip_if_no_network;
use core_test_support::test_codex::test_codex;
use core_test_support::wait_for_mcp_server;
use pretty_assertions::assert_eq;
use serde_json::Value;
use test_case::test_case;

#[derive(Clone, Copy)]
enum HostedProtocolSetting {
    Default,
    Enabled,
}

#[test_case(HostedProtocolSetting::Default, &["initialize", "notifications/initialized", "tools/list"]; "defaults_to_legacy")]
#[test_case(HostedProtocolSetting::Enabled, &["server/discover", "initialize", "notifications/initialized", "tools/list"]; "opt_in_discovers_and_falls_back_to_legacy")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn standalone_codex_apps_respects_protocol_setting(
    setting: HostedProtocolSetting,
    expected_methods: &[&str],
) -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    let apps_base_url = AppsTestServer::mount(&server).await?.chatgpt_base_url;
    let fixture = test_codex()
        .with_auth(CodexAuth::create_dummy_chatgpt_auth_for_testing())
        .with_config(move |config| {
            config
                .features
                .enable(Feature::Apps)
                .expect("test config should allow Apps override");
            if matches!(setting, HostedProtocolSetting::Enabled) {
                config
                    .features
                    .enable(Feature::CodexAppsMcp20260728)
                    .expect("test config should allow Apps protocol override");
            }
            config.chatgpt_base_url = apps_base_url;
        })
        .build_with_auto_env(&server)
        .await?;

    wait_for_mcp_server(&fixture.codex, CODEX_APPS_MCP_SERVER_NAME).await?;
    let methods = server
        .received_requests()
        .await
        .expect("mock server should capture Apps MCP startup requests")
        .into_iter()
        .filter(|request| request.url.path() == "/api/codex/ps/mcp")
        .filter_map(|request| {
            let body: Value = serde_json::from_slice(&request.body).ok()?;
            body.get("method")?.as_str().map(str::to_string)
        })
        .collect::<Vec<_>>();
    assert_eq!(methods, expected_methods);
    Ok(())
}
