#![allow(clippy::unwrap_used)]

//! Port of upstream `codex-rs/core/tests/suite/model_tools.rs` that verifies
//! each model family exposes the expected tool set in this fork.

mod common;

use common::{
    load_default_config_for_test,
    load_sse_fixture_with_id,
    mount_sse_once,
    skip_if_no_network,
    wait_for_event,
};

use code_core::built_in_model_providers;
use code_core::model_family::find_family_for_model;
use code_core::protocol::{EventMsg, InputItem, Op};
use code_core::{CodexAuth, ConversationManager, ModelProviderInfo};
use serde_json::Value;
use tempfile::TempDir;
use wiremock::MockServer;

fn sse_completed(id: &str) -> String {
    load_sse_fixture_with_id("tests/fixtures/completed_template.json", id)
}

#[allow(clippy::expect_used)]
fn tool_identifiers(body: &Value) -> Vec<String> {
    body["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| {
            tool.get("name")
                .and_then(|v| v.as_str())
                .or_else(|| tool.get("type").and_then(|v| v.as_str()))
                .map(str::to_string)
                .expect("tool should have either name or type")
        })
        .collect()
}

#[allow(clippy::expect_used)]
async fn collect_tool_identifiers_for_model(model: &str) -> Vec<String> {
    let server = MockServer::start().await;
    let sse = sse_completed(model);
    let resp_mock = mount_sse_once(&server, sse).await;

    let model_provider = ModelProviderInfo {
        base_url: Some(format!("{}/v1", server.uri())),
        ..built_in_model_providers()["openai"].clone()
    };

    let cwd = TempDir::new().unwrap();
    let code_home = TempDir::new().unwrap();
    let mut config = load_default_config_for_test(&code_home);
    config.cwd = cwd.path().to_path_buf();
    config.model_provider = model_provider;
    config.model = model.to_string();
    config.model_family =
        find_family_for_model(model).unwrap_or_else(|| panic!("unknown model family for {model}"));

    // Disable optional tools so expectations stay stable across environments.
    config.include_apply_patch_tool = false;
    config.include_view_image_tool = false;
    config.tools_web_search_request = false;
    config.include_plan_tool = true;

    let conversation_manager =
        ConversationManager::with_auth(CodexAuth::from_api_key("Test API Key"));
    let codex = conversation_manager
        .new_conversation(config)
        .await
        .expect("create new conversation")
        .conversation;

    codex
        .submit(Op::UserInput {
            items: vec![InputItem::Text {
                text: "hello tools".into(),
            }],
            final_output_json_schema: None,
        })
        .await
        .unwrap();
    wait_for_event(&codex, |ev| matches!(ev, EventMsg::TaskComplete(_))).await;

    let body = resp_mock.single_body_json();
    tool_identifiers(&body)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn model_selects_expected_tools() {
    if skip_if_no_network() {
        return;
    }
    if std::env::var_os("CARGO_BIN_EXE_code-linux-sandbox").is_none() {
        eprintln!("Skipping tool selection test; code-linux-sandbox binary missing");
        return;
    }

    use pretty_assertions::assert_eq;

    let codex_tools = collect_tool_identifiers_for_model("codex-mini-latest").await;
    assert_eq!(
        codex_tools,
        vec![
            "local_shell".to_string(),
            "list_mcp_resources".to_string(),
            "list_mcp_resource_templates".to_string(),
            "read_mcp_resource".to_string(),
            "update_plan".to_string()
        ],
        "codex-mini-latest should expose the local shell tool",
    );

    let o3_tools = collect_tool_identifiers_for_model("o3").await;
    assert_eq!(
        o3_tools,
        vec![
            "shell".to_string(),
            "list_mcp_resources".to_string(),
            "list_mcp_resource_templates".to_string(),
            "read_mcp_resource".to_string(),
            "update_plan".to_string()
        ],
        "o3 should expose the generic shell tool",
    );

    let gpt5_codex_tools = collect_tool_identifiers_for_model("gpt-5-codex").await;
    assert_eq!(
        gpt5_codex_tools,
        vec![
            "shell".to_string(),
            "list_mcp_resources".to_string(),
            "list_mcp_resource_templates".to_string(),
            "read_mcp_resource".to_string(),
            "update_plan".to_string(),
            "apply_patch".to_string()
        ],
        "gpt-5-codex should expose the apply_patch tool",
    );

    let gpt51_codex_tools = collect_tool_identifiers_for_model("gpt-5.1-codex").await;
    assert_eq!(
        gpt51_codex_tools,
        vec![
            "shell".to_string(),
            "list_mcp_resources".to_string(),
            "list_mcp_resource_templates".to_string(),
            "read_mcp_resource".to_string(),
            "update_plan".to_string(),
            "apply_patch".to_string()
        ],
        "gpt-5.1-codex should expose the apply_patch tool",
    );

    let gpt51_tools = collect_tool_identifiers_for_model("gpt-5.1").await;
    assert_eq!(
        gpt51_tools,
        vec![
            "shell".to_string(),
            "list_mcp_resources".to_string(),
            "list_mcp_resource_templates".to_string(),
            "read_mcp_resource".to_string(),
            "update_plan".to_string(),
            "apply_patch".to_string()
        ],
        "gpt-5.1 should expose the apply_patch tool",
    );
}
