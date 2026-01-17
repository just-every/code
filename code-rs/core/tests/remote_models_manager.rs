use chrono::Utc;
use code_core::AuthManager;
use code_core::CodexAuth;
use code_core::ModelProviderInfo;
use code_core::WireApi;
use code_core::remote_models::RemoteModelsManager;
use code_protocol::openai_models::ModelInfo;
use code_protocol::openai_models::ModelsResponse;
use pretty_assertions::assert_eq;
use tempfile::tempdir;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn skip_if_no_network() -> bool {
    std::env::var(code_core::spawn::CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR).is_ok()
}

fn remote_model(slug: &str, display: &str, priority: i32) -> ModelInfo {
    serde_json::from_value(serde_json::json!({
        "slug": slug,
        "display_name": display,
        "description": format!("{display} desc"),
        "default_reasoning_level": "medium",
        "supported_reasoning_levels": [
            {"effort": "low", "description": "low"},
            {"effort": "medium", "description": "medium"}
        ],
        "shell_type": "shell_command",
        "visibility": "list",
        "supported_in_api": true,
        "priority": priority,
        "upgrade": null,
        "base_instructions": "",
        "supports_reasoning_summaries": false,
        "support_verbosity": false,
        "default_verbosity": null,
        "apply_patch_tool_type": null,
        "truncation_policy": {"mode": "bytes", "limit": 10_000},
        "supports_parallel_tool_calls": false,
        "context_window": null,
        "experimental_supported_tools": [],
    }))
    .expect("valid model")
}

fn provider_for(base_url: String) -> ModelProviderInfo {
    ModelProviderInfo {
        name: "mock".into(),
        base_url: Some(base_url),
        env_key: None,
        env_key_instructions: None,
        experimental_bearer_token: None,
        wire_api: WireApi::Responses,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: Some(0),
        stream_max_retries: Some(0),
        stream_idle_timeout_ms: Some(5_000),
        requires_openai_auth: false,
        openrouter: None,
    }
}

fn auth_manager_chatgpt() -> std::sync::Arc<AuthManager> {
    AuthManager::from_auth_for_testing(CodexAuth::create_dummy_chatgpt_auth_for_testing())
}

#[tokio::test]
async fn refresh_remote_models_uses_cache_when_fresh() {
    if skip_if_no_network() {
        return;
    }

    let server = MockServer::start().await;
    let response = ModelsResponse {
        models: vec![remote_model("cached", "Cached", 1)],
    };

    Mock::given(method("GET"))
        .and(path("/models"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(&response)
                .insert_header("ETag", "etag-1"),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;

    let code_home = tempdir().expect("temp dir");
    let provider = provider_for(server.uri());
    let manager = RemoteModelsManager::new(
        auth_manager_chatgpt(),
        provider,
        code_home.path().to_path_buf(),
    );

    manager.refresh_remote_models().await;
    let models = manager.remote_models_snapshot().await;
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].slug, "cached");

    let requests = server.received_requests().await.expect("requests");
    assert_eq!(requests.len(), 1);
    assert!(
        requests[0].url.as_str().contains("client_version="),
        "expected client_version query param"
    );

    // Second refresh should hit the fresh in-memory snapshot and avoid the network.
    manager.refresh_remote_models().await;
    let requests = server.received_requests().await.expect("requests");
    assert_eq!(requests.len(), 1);
}

#[tokio::test]
async fn refresh_remote_models_refetches_when_cache_stale() {
    if skip_if_no_network() {
        return;
    }

    let server = MockServer::start().await;
    let initial = ModelsResponse {
        models: vec![remote_model("stale", "Stale", 1)],
    };

    Mock::given(method("GET"))
        .and(path("/models"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(&initial)
                .insert_header("ETag", "etag-stale"),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;

    let code_home = tempdir().expect("temp dir");
    let provider = provider_for(server.uri());

    let manager = RemoteModelsManager::new(
        auth_manager_chatgpt(),
        provider.clone(),
        code_home.path().to_path_buf(),
    );
    manager.refresh_remote_models().await;
    assert_eq!(manager.remote_models_snapshot().await[0].slug, "stale");

    // Rewrite the cache to be stale.
    let cache_path = code_home.path().join("models_cache.json");
    let contents = std::fs::read_to_string(&cache_path).expect("cache file exists");
    let mut json: serde_json::Value = serde_json::from_str(&contents).expect("cache json");
    let old = (Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
    json["fetched_at"] = serde_json::Value::String(old);
    std::fs::write(&cache_path, serde_json::to_vec_pretty(&json).unwrap()).expect("rewrite");

    let updated = ModelsResponse {
        models: vec![remote_model("fresh", "Fresh", 0)],
    };

    server.reset().await;
    Mock::given(method("GET"))
        .and(path("/models"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(&updated)
                .insert_header("ETag", "etag-fresh"),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;

    // New manager should load the stale cache and refetch.
    let manager = RemoteModelsManager::new(
        auth_manager_chatgpt(),
        provider,
        code_home.path().to_path_buf(),
    );
    manager.refresh_remote_models().await;
    let models = manager.remote_models_snapshot().await;
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].slug, "fresh");
    assert_eq!(
        server.received_requests().await.expect("requests").len(),
        1
    );
}

#[tokio::test]
async fn refresh_remote_models_sends_if_none_match_and_handles_304() {
    if skip_if_no_network() {
        return;
    }

    let server = MockServer::start().await;
    let initial = ModelsResponse {
        models: vec![remote_model("cached", "Cached", 1)],
    };

    Mock::given(method("GET"))
        .and(path("/models"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(&initial)
                .insert_header("ETag", "etag-304"),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;

    let code_home = tempdir().expect("temp dir");
    let provider = provider_for(server.uri());
    let manager = RemoteModelsManager::new(
        auth_manager_chatgpt(),
        provider.clone(),
        code_home.path().to_path_buf(),
    );
    manager.refresh_remote_models().await;

    // Rewrite cache to be stale.
    let cache_path = code_home.path().join("models_cache.json");
    let contents = std::fs::read_to_string(&cache_path).expect("cache file exists");
    let mut json: serde_json::Value = serde_json::from_str(&contents).expect("cache json");
    let old = (Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
    json["fetched_at"] = serde_json::Value::String(old);
    std::fs::write(&cache_path, serde_json::to_vec_pretty(&json).unwrap()).expect("rewrite");

    server.reset().await;
    Mock::given(method("GET"))
        .and(path("/models"))
        .and(header("if-none-match", "etag-304"))
        .respond_with(ResponseTemplate::new(304))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    let manager = RemoteModelsManager::new(
        auth_manager_chatgpt(),
        provider,
        code_home.path().to_path_buf(),
    );
    manager.refresh_remote_models().await;
    let models = manager.remote_models_snapshot().await;
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].slug, "cached");
}

#[tokio::test]
async fn construct_model_family_applies_remote_overrides() {
    if skip_if_no_network() {
        return;
    }

    let server = MockServer::start().await;
    let info: ModelInfo = serde_json::from_value(serde_json::json!({
        "slug": "gpt-5.2",
        "display_name": "gpt-5.2",
        "description": null,
        "default_reasoning_level": "high",
        "supported_reasoning_levels": [],
        "shell_type": "shell_command",
        "visibility": "list",
        "supported_in_api": true,
        "priority": 0,
        "upgrade": null,
        "base_instructions": "REMOTE INSTRUCTIONS",
        "supports_reasoning_summaries": true,
        "support_verbosity": false,
        "default_verbosity": null,
        "apply_patch_tool_type": "function",
        "truncation_policy": {"mode": "bytes", "limit": 10_000},
        "supports_parallel_tool_calls": false,
        "context_window": 12345,
        "experimental_supported_tools": [],
    }))
    .expect("model info");

    let response = ModelsResponse {
        models: vec![info],
    };

    Mock::given(method("GET"))
        .and(path("/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    let code_home = tempdir().expect("temp dir");
    let provider = provider_for(server.uri());
    let manager = RemoteModelsManager::new(
        auth_manager_chatgpt(),
        provider,
        code_home.path().to_path_buf(),
    );

    manager.refresh_remote_models().await;
    let family = manager.construct_model_family("gpt-5.2").await;
    assert_eq!(family.context_window, Some(12345));
    assert_eq!(family.base_instructions, "REMOTE INSTRUCTIONS");
    assert_eq!(
        family.apply_patch_tool_type,
        Some(code_core::ApplyPatchToolType::Function)
    );
    assert_eq!(family.supports_reasoning_summaries, true);
    assert_eq!(
        family.default_reasoning_effort,
        Some(code_core::config_types::ReasoningEffort::High)
    );
}
