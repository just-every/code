use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use code_app_server_protocol::AuthMode;
use code_protocol::openai_models::ApplyPatchToolType as ProtocolApplyPatchToolType;
use code_protocol::openai_models::ModelInfo;
use code_protocol::openai_models::ModelsResponse;
use code_protocol::openai_models::ReasoningEffort as ProtocolReasoningEffort;
use code_protocol::openai_models::TruncationMode as ProtocolTruncationMode;
use reqwest::header;
use reqwest::Method;
use reqwest::Url;
use tokio::sync::RwLock;

use crate::auth::AuthManager;
use crate::model_family::{derive_default_model_family, find_family_for_model, ModelFamily};
use crate::model_provider_info::ModelProviderInfo;
use crate::tool_apply_patch::ApplyPatchToolType;
use crate::CodexAuth;

mod cache;

const MODEL_CACHE_FILE: &str = "models_cache.json";
const DEFAULT_MODEL_CACHE_TTL: Duration = Duration::from_secs(300);
const REMOTE_MODELS_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const CODEX_AUTO_BALANCED_MODEL: &str = "codex-auto-balanced";

#[derive(Debug, Default, Clone)]
struct RemoteModelsState {
    loaded_from_disk: bool,
    fetched_at: Option<chrono::DateTime<Utc>>,
    etag: Option<String>,
    models: Vec<ModelInfo>,
}

/// Coordinates remote `/models` discovery and cached metadata on disk.
///
/// Any error (disk, auth, network, parse) results in an empty remote model list
/// so callers can safely fall back to built-in behaviour.
#[derive(Debug)]
pub struct RemoteModelsManager {
    state: RwLock<RemoteModelsState>,
    auth_manager: Arc<AuthManager>,
    provider: ModelProviderInfo,
    code_home: PathBuf,
    cache_ttl: Duration,
    client: reqwest::Client,
}

impl RemoteModelsManager {
    pub fn new(auth_manager: Arc<AuthManager>, provider: ModelProviderInfo, code_home: PathBuf) -> Self {
        Self {
            state: RwLock::new(RemoteModelsState::default()),
            auth_manager,
            provider,
            code_home,
            cache_ttl: DEFAULT_MODEL_CACHE_TTL,
            client: crate::default_client::create_client(crate::default_client::DEFAULT_ORIGINATOR),
        }
    }

    #[cfg(test)]
    pub fn with_cache_ttl(mut self, ttl: Duration) -> Self {
        self.cache_ttl = ttl;
        self
    }

    /// Returns an in-memory snapshot of known remote models.
    ///
    /// This loads from disk once (best-effort) but does not block on network.
    pub async fn remote_models_snapshot(&self) -> Vec<ModelInfo> {
        self.ensure_loaded_from_disk().await;
        self.state.read().await.models.clone()
    }

    /// Returns the remote default model slug when available.
    ///
    /// When the user did not explicitly choose a model, Code may adopt this
    /// server-provided default without persisting it.
    pub async fn default_model_slug(&self, auth_mode: Option<AuthMode>) -> Option<String> {
        self.ensure_loaded_from_disk().await;

        if auth_mode != Some(AuthMode::ChatGPT) {
            return None;
        }

        let state = self.state.read().await;
        state
            .models
            .iter()
            .any(|m| m.slug == CODEX_AUTO_BALANCED_MODEL)
            .then(|| CODEX_AUTO_BALANCED_MODEL.to_string())
    }

    /// Best-effort refresh of remote models.
    ///
    /// Never errors: on failures the in-memory snapshot remains unchanged.
    pub async fn refresh_remote_models(&self) {
        self.refresh_remote_models_with_cache().await;
    }

    pub async fn refresh_remote_models_with_cache(&self) {
        self.ensure_loaded_from_disk().await;

        let (stale_etag, should_fetch) = {
            let state = self.state.read().await;
            let is_fresh = state
                .fetched_at
                .map(|t| cache::is_fresh(t, self.cache_ttl))
                .unwrap_or(false);
            (state.etag.clone(), !is_fresh)
        };

        if !should_fetch {
            return;
        }

        self.refresh_remote_models_inner(stale_etag).await;
    }

    pub async fn refresh_remote_models_no_cache(&self) {
        self.ensure_loaded_from_disk().await;
        let stale_etag = self.state.read().await.etag.clone();
        self.refresh_remote_models_inner(stale_etag).await;
    }

    pub async fn refresh_if_new_etag(&self, etag: String) {
        let current_etag = self.get_etag().await;
        if current_etag.clone().is_some() && current_etag.as_deref() == Some(etag.as_str()) {
            return;
        }
        self.refresh_remote_models_no_cache().await;
    }

    async fn get_etag(&self) -> Option<String> {
        self.state.read().await.etag.clone()
    }

    async fn refresh_remote_models_inner(&self, stale_etag: Option<String>) {
        let auth = self.auth_manager.auth();
        let auth_mode = auth.as_ref().map(|a| a.mode);
        if auth_mode != Some(AuthMode::ChatGPT) {
            // Only the ChatGPT backend exposes the Codex `/models` schema.
            return;
        }

        let url = match self.models_url(&auth) {
            Ok(url) => url,
            Err(err) => {
                tracing::debug!("remote /models URL construction failed: {err}");
                return;
            }
        };

        let mut request = match self
            .provider
            .create_request_builder_for_url(&self.client, &auth, Method::GET, url)
            .await
        {
            Ok(request) => request,
            Err(err) => {
                tracing::debug!("remote /models auth/header setup failed: {err}");
                return;
            }
        };

        request = request.timeout(REMOTE_MODELS_REQUEST_TIMEOUT);

        if let Some(etag) = stale_etag.as_deref() {
            request = request.header(header::IF_NONE_MATCH, etag);
        }

        if let Some(auth) = auth.as_ref()
            && auth.mode == AuthMode::ChatGPT
            && let Some(account_id) = auth.get_account_id()
        {
            request = request.header("chatgpt-account-id", account_id);
        }

        let response = match request.send().await {
            Ok(response) => response,
            Err(err) => {
                tracing::debug!("remote /models request failed: {err}");
                return;
            }
        };

        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            let mut state = self.state.write().await;
            state.fetched_at = Some(Utc::now());
            if let Err(err) = cache::save_cache(&self.cache_path(), &cache::ModelsCache {
                fetched_at: state.fetched_at.unwrap_or_else(Utc::now),
                etag: state.etag.clone(),
                models: state.models.clone(),
            }) {
                tracing::debug!("failed to persist /models cache on 304: {err}");
            }
            return;
        }

        if !response.status().is_success() {
            tracing::debug!("remote /models request failed with status {}", response.status());
            return;
        }

        let header_etag = response
            .headers()
            .get(header::ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);

        let body = match response.text().await {
            Ok(body) => body,
            Err(err) => {
                tracing::debug!("remote /models response body read failed: {err}");
                return;
            }
        };

        let parsed = match serde_json::from_str::<ModelsResponse>(&body) {
            Ok(parsed) => parsed,
            Err(err) => {
                tracing::debug!("remote /models response parse failed: {err}");
                return;
            }
        };

        let etag = header_etag.filter(|value| !value.trim().is_empty());

        let fetched_at = Utc::now();
        {
            let mut state = self.state.write().await;
            state.models = parsed.models;
            state.etag = etag.clone();
            state.fetched_at = Some(fetched_at);
        }

        if let Err(err) = cache::save_cache(&self.cache_path(), &cache::ModelsCache {
            fetched_at,
            etag,
            models: self.state.read().await.models.clone(),
        }) {
            tracing::debug!("failed to write /models cache: {err}");
        }
    }

    pub async fn apply_remote_overrides(&self, model: &str, family: ModelFamily) -> ModelFamily {
        self.ensure_loaded_from_disk().await;

        let info = {
            let state = self.state.read().await;
            state
                .models
                .iter()
                .find(|info| info.slug.eq_ignore_ascii_case(model))
                .cloned()
        };
        let Some(info) = info else {
            return family;
        };

        apply_model_info_overrides(&info, family)
    }

    pub async fn construct_model_family(&self, model: &str) -> ModelFamily {
        let base = find_family_for_model(model).unwrap_or_else(|| derive_default_model_family(model));
        self.apply_remote_overrides(model, base).await
    }

    async fn ensure_loaded_from_disk(&self) {
        let loaded = { self.state.read().await.loaded_from_disk };
        if loaded {
            return;
        }

        let cache_path = self.cache_path();
        let cache = match cache::load_cache(&cache_path) {
            Ok(cache) => cache,
            Err(err) => {
                tracing::debug!("failed to load /models cache: {err}");
                None
            }
        };

        let mut state = self.state.write().await;
        state.loaded_from_disk = true;
        if let Some(cache) = cache {
            state.fetched_at = Some(cache.fetched_at);
            state.etag = cache.etag;
            state.models = cache.models;
        }
    }

    fn models_url(&self, auth: &Option<CodexAuth>) -> crate::error::Result<Url> {
        let base_url = self.provider.base_url.clone().unwrap_or_else(|| {
            if matches!(
                auth,
                Some(CodexAuth {
                    mode: AuthMode::ChatGPT,
                    ..
                })
            ) {
                "https://chatgpt.com/backend-api/codex".to_string()
            } else {
                "https://api.openai.com/v1".to_string()
            }
        });

        let mut url = Url::parse(&base_url).map_err(|err| {
            crate::error::CodexErr::ServerError(format!("invalid models base_url {base_url}: {err}"))
        })?;
        let base_path = url.path().trim_end_matches('/');
        url.set_path(&format!("{base_path}/models"));

        {
            let mut pairs = url.query_pairs_mut();
            if let Some(params) = self.provider.query_params.as_ref() {
                for (k, v) in params {
                    pairs.append_pair(k, v);
                }
            }
            pairs.append_pair("client_version", &format_client_version_to_whole());
        }

        Ok(url)
    }

    fn cache_path(&self) -> PathBuf {
        self.code_home.join(MODEL_CACHE_FILE)
    }
}

pub fn apply_model_info_overrides(info: &ModelInfo, mut family: ModelFamily) -> ModelFamily {
    let trimmed = info.base_instructions.trim();
    if !trimmed.is_empty() {
        family.base_instructions = info.base_instructions.clone();
    }

    if let Some(context_window) = info
        .context_window
        .and_then(|value| (value > 0).then(|| value as u64))
    {
        family.context_window = Some(context_window);
    }

    if let Some(tool_type) = info.apply_patch_tool_type.as_ref() {
        family.apply_patch_tool_type = Some(map_apply_patch_tool_type(tool_type));
    }

    if let Some(limit) = info.auto_compact_token_limit() {
        family.set_auto_compact_token_limit(Some(limit));
    }

    family.set_truncation_policy(map_truncation_policy(&info.truncation_policy));

    family.supports_reasoning_summaries = info.supports_reasoning_summaries;
    family.supports_parallel_tool_calls = info.supports_parallel_tool_calls;
    if let Some(effort) = info.default_reasoning_level {
        family.default_reasoning_effort = Some(map_reasoning_effort(effort));
    }
    family
}

fn map_apply_patch_tool_type(tool_type: &ProtocolApplyPatchToolType) -> ApplyPatchToolType {
    match tool_type {
        ProtocolApplyPatchToolType::Freeform => ApplyPatchToolType::Freeform,
        ProtocolApplyPatchToolType::Function => ApplyPatchToolType::Function,
    }
}

fn map_reasoning_effort(effort: ProtocolReasoningEffort) -> crate::config_types::ReasoningEffort {
    use crate::config_types::ReasoningEffort as LocalEffort;

    match effort {
        ProtocolReasoningEffort::None => LocalEffort::None,
        ProtocolReasoningEffort::Minimal => LocalEffort::Minimal,
        ProtocolReasoningEffort::Low => LocalEffort::Low,
        ProtocolReasoningEffort::Medium => LocalEffort::Medium,
        ProtocolReasoningEffort::High => LocalEffort::High,
        ProtocolReasoningEffort::XHigh => LocalEffort::XHigh,
    }
}

fn map_truncation_policy(
    policy: &code_protocol::openai_models::TruncationPolicyConfig,
) -> code_protocol::protocol::TruncationPolicy {
    let limit = usize::try_from(policy.limit).unwrap_or(usize::MAX);
    match policy.mode {
        ProtocolTruncationMode::Bytes => code_protocol::protocol::TruncationPolicy::Bytes(limit),
        ProtocolTruncationMode::Tokens => code_protocol::protocol::TruncationPolicy::Tokens(limit),
    }
}

/// Convert the build's version triple into a whole semver string.
fn format_client_version_to_whole() -> String {
    format!(
        "{}.{}.{}",
        env!("CARGO_PKG_VERSION_MAJOR"),
        env!("CARGO_PKG_VERSION_MINOR"),
        env!("CARGO_PKG_VERSION_PATCH")
    )
}
