use std::collections::BTreeMap;
use std::io::BufRead;
use std::path::Path;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::AuthManager;
use crate::RefreshTokenError;
use crate::account_usage;
use crate::auth;
use crate::auth_accounts;
use bytes::Bytes;
use code_app_server_protocol::AuthMode;
use code_protocol::models::ResponseItem;
use eventsource_stream::Eventsource;
use futures::prelude::*;
use httpdate::parse_http_date;
use regex_lite::Regex;
use reqwest::StatusCode;
use reqwest::header::HeaderMap;
use reqwest::header::HeaderValue;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_util::io::ReaderStream;
use tokio_stream::wrappers::ReceiverStream;
use tracing::debug;
use tracing::trace;
use tracing::warn;
use uuid::Uuid;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

const AUTH_REQUIRED_MESSAGE: &str = "Authentication required. Run `code login` to continue.";

use crate::agent_defaults::{default_agent_configs, enabled_agent_model_specs};
use crate::chat_completions::AggregateStreamExt;
use crate::chat_completions::stream_chat_completions;
use crate::client_common::Prompt;
use crate::client_common::ResponseEvent;
use crate::client_common::ResponseStream;
use crate::client_common::ResponsesApiRequest;
use crate::client_common::create_reasoning_param_for_request;
use crate::config::Config;
use crate::config_types::ReasoningEffort as ReasoningEffortConfig;
use crate::config_types::ReasoningSummary as ReasoningSummaryConfig;
use crate::config_types::TextVerbosity as TextVerbosityConfig;
use crate::debug_logger::DebugLogger;
use crate::default_client::create_client;
use crate::error::{CodexErr, RetryAfter};
use crate::error::Result;
use crate::error::RetryLimitReachedError;
use crate::error::UnexpectedResponseError;
use crate::error::UsageLimitReachedError;
use crate::flags::CODEX_RS_SSE_FIXTURE;
use crate::model_family::{find_family_for_model, ModelFamily};
use crate::model_provider_info::ModelProviderInfo;
use crate::model_provider_info::WireApi;
use crate::openai_tools::create_tools_json_for_responses_api;
use crate::openai_tools::ConfigShellToolType;
use crate::openai_tools::ToolsConfig;
use crate::protocol::RateLimitSnapshotEvent;
use crate::protocol::SandboxPolicy;
use crate::protocol::TokenUsage;
use crate::reasoning::clamp_reasoning_effort_for_model;
use crate::slash_commands::get_enabled_agents;
use crate::util::backoff;
use code_otel::otel_event_manager::{OtelEventManager, TurnLatencyPayload};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::RwLock;

const RESPONSES_BETA_HEADER_V1: &str = "responses=v1";
const RESPONSES_BETA_HEADER_EXPERIMENTAL: &str = "responses=experimental";

const CODE_OPENAI_SUBAGENT_ENV: &str = "CODE_OPENAI_SUBAGENT";

#[derive(Default, Debug)]
struct StreamCheckpoint {
    /// Highest sequence_number observed across attempts. Used to drop replayed deltas.
    last_sequence: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: Error,
}

#[derive(Debug, Deserialize)]
struct Error {
    r#type: Option<String>,
    #[allow(dead_code)]
    code: Option<String>,
    /// Optional parameter that triggered the error (e.g. "reasoning.summary").
    #[allow(dead_code)]
    param: Option<String>,
    message: Option<String>,

    // Optional fields available on "usage_limit_reached" and "usage_not_included" errors
    plan_type: Option<String>,
    resets_in_seconds: Option<u64>,
}

#[derive(Serialize)]
struct CompactHistoryRequest<'a> {
    model: &'a str,
    #[serde(borrow)]
    input: &'a [ResponseItem],
    instructions: String,
}

#[derive(Debug, Deserialize)]
struct CompactHistoryResponse {
    output: Vec<ResponseItem>,
}

fn rate_limit_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)(?:please\s+try\s+again|try\s+again|please\s+retry|retry|try)\s+(?:in|after)\s*(\d+(?:\.\d+)?)\s*(ms|milliseconds?|s|sec|secs|seconds?)"
        )
            .expect("valid rate limit regex")
    })
}

fn try_parse_retry_after(err: &Error, now: DateTime<Utc>) -> Option<RetryAfter> {
    if let Some(seconds) = err.resets_in_seconds {
        return Some(RetryAfter::from_duration(Duration::from_secs(seconds), now));
    }

    let message = err.message.as_deref()?;
    let re = rate_limit_regex();
    let captures = re.captures(message)?;
    let value = captures.get(1)?.as_str().trim().parse::<f64>().ok()?;
    if value.is_sign_negative() {
        return None;
    }
    let unit = captures.get(2)?.as_str().trim().to_ascii_lowercase();

    if unit.starts_with("ms") {
        Some(RetryAfter::from_duration(Duration::from_millis(value.round() as u64), now))
    } else if unit.starts_with("sec") || unit == "s" || unit.starts_with("second") {
        Some(RetryAfter::from_duration(Duration::from_secs_f64(value), now))
    } else {
        None
    }
}

fn is_quota_exceeded_error(error: &Error) -> bool {
    matches!(
        error.code.as_deref().or_else(|| error.r#type.as_deref()),
        Some("insufficient_quota")
    )
}

fn is_quota_exceeded_http_error(status: StatusCode, error: &Error) -> bool {
    status.is_client_error() && is_quota_exceeded_error(error)
}

fn is_reasoning_summary_rejected(error: &Error) -> bool {
    let param_matches = matches!(error.param.as_deref(), Some("reasoning.summary"));
    let code_matches = matches!(error.code.as_deref(), Some("unsupported_value"));

    let message_matches = error
        .message
        .as_deref()
        .map(|msg| {
            let msg = msg.to_ascii_lowercase();
            msg.contains("organization must be verified") && msg.contains("reasoning summar")
        })
        .unwrap_or(false);

    // Only treat as rejection if it's specifically an "unsupported_value" error
    // for the reasoning.summary parameter, or if the message explicitly says
    // the organization must be verified for reasoning summaries.
    (param_matches && code_matches) || (code_matches && message_matches)
}

fn map_unauthorized_outcome(
    had_auth: bool,
    refresh_error: Option<&RefreshTokenError>,
) -> Option<CodexErr> {
    if let Some(err) = refresh_error {
        if err.is_permanent() {
            return Some(CodexErr::AuthRefreshPermanent(err.message.clone()));
        }
        return None;
    }

    if !had_auth {
        return Some(CodexErr::AuthRefreshPermanent(
            AUTH_REQUIRED_MESSAGE.to_string(),
        ));
    }

    None
}

#[derive(Debug)]
pub struct ModelClient {
    config: Arc<Config>,
    auth_manager: Option<Arc<AuthManager>>,
    otel_event_manager: Option<OtelEventManager>,
    client: reqwest::Client,
    provider: ModelProviderInfo,
    session_id: Uuid,
    effort: ReasoningEffortConfig,
    summary: ReasoningSummaryConfig,
    reasoning_summary_disabled: AtomicBool,
    verbosity: TextVerbosityConfig,
    debug_logger: Arc<Mutex<DebugLogger>>,
}

impl Clone for ModelClient {
    fn clone(&self) -> Self {
        Self {
            config: Arc::clone(&self.config),
            auth_manager: self.auth_manager.clone(),
            otel_event_manager: self.otel_event_manager.clone(),
            client: self.client.clone(),
            provider: self.provider.clone(),
            session_id: self.session_id,
            effort: self.effort,
            summary: self.summary,
            reasoning_summary_disabled: AtomicBool::new(
                self.reasoning_summary_disabled.load(Ordering::Relaxed),
            ),
            verbosity: self.verbosity,
            debug_logger: Arc::clone(&self.debug_logger),
        }
    }
}

impl ModelClient {
    pub fn new(
        config: Arc<Config>,
        auth_manager: Option<Arc<AuthManager>>,
        otel_event_manager: Option<OtelEventManager>,
        provider: ModelProviderInfo,
        effort: ReasoningEffortConfig,
        summary: ReasoningSummaryConfig,
        verbosity: TextVerbosityConfig,
        session_id: Uuid,
        debug_logger: Arc<Mutex<DebugLogger>>,
    ) -> Self {
        let effective_verbosity = clamp_text_verbosity_for_model(config.model.as_str(), verbosity);
        let clamped_effort = clamp_reasoning_effort_for_model(config.model.as_str(), effort);
        let client = create_client(&config.responses_originator_header);

        Self {
            config,
            auth_manager,
            otel_event_manager,
            client,
            provider,
            session_id,
            effort: clamped_effort,
            summary,
            reasoning_summary_disabled: AtomicBool::new(false),
            verbosity: effective_verbosity,
            debug_logger,
        }
    }

    /// Get the reasoning effort configuration
    pub fn get_reasoning_effort(&self) -> ReasoningEffortConfig {
        self.effort
    }

    /// Get the reasoning summary configuration
    pub fn get_reasoning_summary(&self) -> ReasoningSummaryConfig {
        if self.reasoning_summary_disabled.load(Ordering::Relaxed) {
            ReasoningSummaryConfig::None
        } else {
            self.summary
        }
    }

    fn current_reasoning_param(
        &self,
        family: &ModelFamily,
        effort: ReasoningEffortConfig,
    ) -> Option<crate::client_common::Reasoning> {
        if self.reasoning_summary_disabled.load(Ordering::Relaxed) {
            return None;
        }

        create_reasoning_param_for_request(
            family,
            Some(effort),
            self.summary,
        )
    }

    fn disable_reasoning_summary(&self) {
        if !self.reasoning_summary_disabled.swap(true, Ordering::Relaxed) {
            tracing::warn!("disabling reasoning summaries after API rejection");
        }
    }

    /// Get the text verbosity configuration
    #[allow(dead_code)]
    pub fn get_text_verbosity(&self) -> TextVerbosityConfig {
        self.verbosity
    }

    pub fn get_otel_event_manager(&self) -> Option<OtelEventManager> {
        self.otel_event_manager.clone()
    }

    pub fn log_turn_latency_debug(&self, payload: &TurnLatencyPayload) {
        if let Ok(logger) = self.debug_logger.lock() {
            let _ = logger.log_turn_latency(payload);
        }
    }

    pub fn code_home(&self) -> &Path {
        &self.config.code_home
    }

    pub fn debug_enabled(&self) -> bool {
        self.config.debug
    }

    pub fn auto_switch_accounts_on_rate_limit(&self) -> bool {
        self.config.auto_switch_accounts_on_rate_limit
    }

    pub fn api_key_fallback_on_all_accounts_limited(&self) -> bool {
        self.config.api_key_fallback_on_all_accounts_limited
    }

    pub fn build_tools_config_with_sandbox(
        &self,
        sandbox_policy: SandboxPolicy,
    ) -> ToolsConfig {
        self.build_tools_config_with_sandbox_for_family(sandbox_policy, &self.config.model_family)
    }

    pub fn build_tools_config_with_sandbox_for_family(
        &self,
        sandbox_policy: SandboxPolicy,
        model_family: &ModelFamily,
    ) -> ToolsConfig {
        let mut tools_config = ToolsConfig::new(
            model_family,
            self.config.approval_policy,
            sandbox_policy.clone(),
            self.config.include_plan_tool,
            self.config.include_apply_patch_tool,
            self.config.tools_web_search_request,
            self.config.use_experimental_streamable_shell_tool,
            self.config.include_view_image_tool,
        );
        tools_config.web_search_allowed_domains = self.config.tools_web_search_allowed_domains.clone();

        let mut agent_models: Vec<String> = if self.config.agents.is_empty() {
            default_agent_configs()
                .into_iter()
                .filter(|cfg| cfg.enabled)
                .map(|cfg| cfg.name)
                .collect()
        } else {
            get_enabled_agents(&self.config.agents)
        };
        if agent_models.is_empty() {
            agent_models = enabled_agent_model_specs()
                .into_iter()
                .map(|spec| spec.slug.to_string())
                .collect();
        }
        agent_models.sort_by(|a, b| a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase()));
        agent_models.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        tools_config.set_agent_models(agent_models);

        let base_shell_type = tools_config.shell_type.clone();
        let base_uses_native_shell = matches!(
            &base_shell_type,
            ConfigShellToolType::LocalShell | ConfigShellToolType::StreamableShell
        );

        tools_config.shell_type = match sandbox_policy.clone() {
            SandboxPolicy::ReadOnly => {
                if base_uses_native_shell {
                    base_shell_type.clone()
                } else {
                    ConfigShellToolType::ShellWithRequest {
                        sandbox_policy: SandboxPolicy::ReadOnly,
                    }
                }
            }
            sp @ SandboxPolicy::WorkspaceWrite { .. } => {
                if base_uses_native_shell {
                    base_shell_type.clone()
                } else {
                    ConfigShellToolType::ShellWithRequest { sandbox_policy: sp }
                }
            }
            SandboxPolicy::DangerFullAccess => base_shell_type,
        };

        tools_config
    }

    pub fn build_tools_config(&self) -> ToolsConfig {
        self.build_tools_config_with_sandbox(self.config.sandbox_policy.clone())
    }

    pub fn get_auto_compact_token_limit(&self) -> Option<i64> {
        self.config
            .model_auto_compact_token_limit
            .or_else(|| self.config.model_family.auto_compact_token_limit())
    }

    pub fn default_model_slug(&self) -> &str {
        self.config.model.as_str()
    }

    pub fn default_model_family(&self) -> &ModelFamily {
        &self.config.model_family
    }

    /// Dispatches to either the Responses or Chat implementation depending on
    /// the provider config.  Public callers always invoke `stream()` – the
    /// specialised helpers are private to avoid accidental misuse.
    pub async fn stream(&self, prompt: &Prompt) -> Result<ResponseStream> {
        let env_log_tag = std::env::var("CODE_DEBUG_LOG_TAG").ok();
        let log_tag = env_log_tag
            .as_deref()
            .or(prompt.log_tag.as_deref());
        match self.provider.wire_api {
            WireApi::Responses => self.stream_responses(prompt, log_tag).await,
            WireApi::ResponsesWebsocket => self.stream_responses_websocket(prompt, log_tag).await,
            WireApi::Chat => {
                let effective_family = prompt
                    .model_family_override
                    .as_ref()
                    .unwrap_or(&self.config.model_family);
                let model_slug = prompt
                    .model_override
                    .as_deref()
                    .unwrap_or(self.config.model.as_str());
                // Create the raw streaming connection first.
                let response_stream = stream_chat_completions(
                    prompt,
                    effective_family,
                    model_slug,
                    &self.client,
                    &self.provider,
                    &self.debug_logger,
                    self.auth_manager.clone(),
                    self.otel_event_manager.clone(),
                    log_tag,
                )
                .await?;

                // Wrap it with the aggregation adapter so callers see *only*
                // the final assistant message per turn (matching the
                // behaviour of the Responses API).
                let mut aggregated = if self.config.show_raw_agent_reasoning {
                    crate::chat_completions::AggregatedChatStream::streaming_mode(response_stream)
                } else {
                    response_stream.aggregate()
                };

                // Bridge the aggregated stream back into a standard
                // `ResponseStream` by forwarding events through a channel.
                let (tx, rx) = mpsc::channel::<Result<ResponseEvent>>(16);

                tokio::spawn(async move {
                    use futures::StreamExt;
                    while let Some(ev) = aggregated.next().await {
                        // Exit early if receiver hung up.
                        if tx.send(ev).await.is_err() {
                            break;
                        }
                    }
                });

                Ok(ResponseStream { rx_event: rx })
            }
        }
    }

    async fn stream_responses_websocket(
        &self,
        prompt: &Prompt,
        log_tag: Option<&str>,
    ) -> Result<ResponseStream> {
        let auth_manager = self.auth_manager.clone();
        let auth_mode = auth_manager
            .as_ref()
            .and_then(|m| m.auth())
            .as_ref()
            .map(|a| a.mode);

        // Use non-stored turns on all paths for stability.
        let store = false;

        let request_model = prompt
            .model_override
            .as_deref()
            .unwrap_or(self.config.model.as_str());
        let effective_effort = clamp_reasoning_effort_for_model(request_model, self.effort);
        let request_family = prompt
            .model_family_override
            .clone()
            .or_else(|| find_family_for_model(request_model))
            .unwrap_or_else(|| self.config.model_family.clone());

        let full_instructions = prompt.get_full_instructions(&request_family);
        let mut tools_json = create_tools_json_for_responses_api(&prompt.tools)?;
        if matches!(effective_effort, ReasoningEffortConfig::Minimal) {
            tools_json.retain(|tool| {
                tool.get("type")
                    .and_then(|value| value.as_str())
                    .map(|tool_type| tool_type != "web_search")
                    .unwrap_or(true)
            });
        }

        let input_with_instructions = prompt.get_formatted_input();

        let want_format = prompt.text_format.clone().or_else(|| {
            prompt.output_schema.as_ref().map(|schema| crate::client_common::TextFormat {
                r#type: "json_schema".to_string(),
                name: Some("code_output_schema".to_string()),
                strict: Some(true),
                schema: Some(schema.clone()),
            })
        });

        let effective_verbosity = clamp_text_verbosity_for_model(request_model, self.verbosity);
        let verbosity = match &request_family.family {
            family if family == "gpt-5" || family == "gpt-5.1" => Some(effective_verbosity),
            _ => None,
        };

        let text_template = match (auth_mode, want_format, verbosity) {
            (Some(AuthMode::ChatGPT), None, _) => None,
            (_, Some(fmt), _) => Some(crate::client_common::Text {
                verbosity: effective_verbosity.into(),
                format: Some(fmt),
            }),
            (_, None, Some(_)) => Some(crate::client_common::Text {
                verbosity: effective_verbosity.into(),
                format: None,
            }),
            (_, None, None) => None,
        };

        let model_slug = request_model;
        let session_id = prompt.session_id_override.unwrap_or(self.session_id);
        let session_id_str = session_id.to_string();
        let mut attempt = 0;
        let max_retries = self.provider.request_max_retries();
        let mut request_id = String::new();

        loop {
            attempt += 1;

            let reasoning = self.current_reasoning_param(&request_family, effective_effort);
            let include: Vec<String> = if !store && reasoning.is_some() {
                vec!["reasoning.encrypted_content".to_string()]
            } else {
                Vec::new()
            };

            let payload = ResponsesApiRequest {
                model: model_slug,
                instructions: &full_instructions,
                input: &input_with_instructions,
                tools: &tools_json,
                tool_choice: "auto",
                parallel_tool_calls: request_family.supports_parallel_tool_calls,
                reasoning,
                text: text_template.clone(),
                store: self.provider.is_azure_responses_endpoint(),
                stream: true,
                include,
                prompt_cache_key: Some(session_id_str.clone()),
            };

            let mut payload_json = serde_json::to_value(&payload)?;
            if let Some(model_value) = payload_json.get_mut("model") {
                *model_value = serde_json::Value::String(model_slug.to_string());
            }
            if self.provider.is_azure_responses_endpoint() {
                attach_item_ids(&mut payload_json, &input_with_instructions);
            }
            if let Some(openrouter_cfg) = self.provider.openrouter_config() {
                if let Some(obj) = payload_json.as_object_mut() {
                    if let Some(provider) = &openrouter_cfg.provider {
                        obj.insert("provider".to_string(), serde_json::to_value(provider)?);
                    }
                    if let Some(route) = &openrouter_cfg.route {
                        obj.insert("route".to_string(), route.clone());
                    }
                    for (key, value) in &openrouter_cfg.extra {
                        obj.entry(key.clone()).or_insert(value.clone());
                    }
                }
            }

            let auth = auth_manager.as_ref().and_then(|m| m.auth());
            let endpoint = self.provider.get_full_url(&auth);

            let url = reqwest::Url::parse(&endpoint).map_err(|err| {
                CodexErr::Stream(
                    format!("[ws] invalid URL: {err}"),
                    None,
                    Some(request_id.clone()),
                )
            })?;
            let mut req_builder = self
                .provider
                .create_request_builder_for_url(&self.client, &auth, reqwest::Method::GET, url)
                .await?;

            let has_beta_header = req_builder
                .try_clone()
                .and_then(|builder| builder.build().ok())
                .map_or(false, |req| req.headers().contains_key("OpenAI-Beta"));

            if !has_beta_header {
                let beta_value = if self.provider.is_public_openai_responses_endpoint() {
                    RESPONSES_BETA_HEADER_V1
                } else {
                    RESPONSES_BETA_HEADER_EXPERIMENTAL
                };
                req_builder = req_builder.header("OpenAI-Beta", beta_value);
            }

            req_builder = attach_openai_subagent_header(req_builder);
            req_builder = attach_codex_beta_features_header(req_builder, &self.config);
            req_builder = req_builder
                .header("conversation_id", session_id_str.clone())
                .header("session_id", session_id_str.clone());

            if let Some(auth) = auth.as_ref()
                && auth.mode == AuthMode::ChatGPT
                && let Some(account_id) = auth.get_account_id()
            {
                req_builder = req_builder.header("chatgpt-account-id", account_id);
            }

            let header_snapshot = req_builder
                .try_clone()
                .and_then(|builder| builder.build().ok())
                .map(|req| header_map_to_json(req.headers()));

            if request_id.is_empty() {
                if let Ok(logger) = self.debug_logger.lock() {
                    request_id = logger
                        .start_request_log(&endpoint, &payload_json, header_snapshot.as_ref(), log_tag)
                        .unwrap_or_default();
                }
            }

            let ws_headers = req_builder
                .try_clone()
                .and_then(|builder| builder.build().ok())
                .map(|req| req.headers().clone())
                .unwrap_or_else(HeaderMap::new);

            let mut ws_request = endpoint
                .clone()
                .into_client_request()
                .map_err(|err| {
                    CodexErr::Stream(
                        format!("[ws] failed to build request: {err}"),
                        None,
                        Some(request_id.clone()),
                    )
                })?;
            ws_request.headers_mut().extend(ws_headers);

            // Wrap the normal /responses request payload in the WebSocket envelope.
            let mut ws_payload = serde_json::Map::new();
            ws_payload.insert(
                "type".to_string(),
                serde_json::Value::String("response.create".to_string()),
            );
            if let Some(obj) = payload_json.as_object() {
                for (k, v) in obj {
                    ws_payload.insert(k.clone(), v.clone());
                }
            }
            let ws_payload_text = serde_json::to_string(&serde_json::Value::Object(ws_payload))?;

            let connect = tokio_tungstenite::connect_async(ws_request).await;
            match connect {
                Ok((mut ws_stream, response)) => {
                    let (tx_event, rx_event) = mpsc::channel::<Result<ResponseEvent>>(1600);

                    if let Some(snapshot) = parse_rate_limit_snapshot(response.headers()) {
                        debug!(
                            "rate limit headers:\n{}",
                            format_rate_limit_headers(response.headers())
                        );
                        if tx_event
                            .send(Ok(ResponseEvent::RateLimits(snapshot)))
                            .await
                            .is_err()
                        {
                            debug!("receiver dropped rate limit snapshot event");
                        }
                    }

                    let models_etag = response
                        .headers()
                        .get("X-Models-Etag")
                        .and_then(|value| value.to_str().ok())
                        .map(ToString::to_string);
                    if let Some(etag) = models_etag {
                        if tx_event
                            .send(Ok(ResponseEvent::ModelsEtag(etag)))
                            .await
                            .is_err()
                        {
                            debug!("receiver dropped models etag event");
                        }
                    }

                    ws_stream
                        .send(Message::Text(ws_payload_text))
                        .await
                        .map_err(|err| {
                            CodexErr::Stream(
                                format!("[ws] failed to send websocket request: {err}"),
                                None,
                                Some(request_id.clone()),
                            )
                        })?;

                    let (tx_bytes, rx_bytes) = mpsc::channel::<Result<Bytes>>(1600);
                    let request_id_for_ws = request_id.clone();
                    tokio::spawn(async move {
                        loop {
                            let Some(next) = ws_stream.next().await else {
                                break;
                            };
                            match next {
                                Ok(Message::Text(text)) => {
                                    let chunk = format!("data: {text}\n\n");
                                    if tx_bytes.send(Ok(Bytes::from(chunk))).await.is_err() {
                                        break;
                                    }
                                }
                                Ok(Message::Ping(payload)) => {
                                    if ws_stream.send(Message::Pong(payload)).await.is_err() {
                                        break;
                                    }
                                }
                                Ok(Message::Pong(_)) => {}
                                Ok(Message::Close(_)) => break,
                                Ok(Message::Binary(_)) => {
                                    let _ = tx_bytes
                                        .send(Err(CodexErr::Stream(
                                            "[ws] unexpected binary websocket event".to_string(),
                                            None,
                                            Some(request_id_for_ws.clone()),
                                        )))
                                        .await;
                                    break;
                                }
                                Ok(_) => {}
                                Err(err) => {
                                    let _ = tx_bytes
                                        .send(Err(CodexErr::Stream(
                                            format!("[ws] websocket error: {err}"),
                                            None,
                                            Some(request_id_for_ws.clone()),
                                        )))
                                        .await;
                                    break;
                                }
                            }
                        }
                    });

                    let stream = ReceiverStream::new(rx_bytes);
                    let debug_logger = Arc::clone(&self.debug_logger);
                    let request_id_clone = request_id.clone();
                    let otel_event_manager = self.otel_event_manager.clone();
                    tokio::spawn(process_sse(
                        stream,
                        tx_event,
                        self.provider.stream_idle_timeout(),
                        debug_logger,
                        request_id_clone,
                        otel_event_manager,
                        Arc::new(RwLock::new(StreamCheckpoint::default())),
                    ));

                    return Ok(ResponseStream { rx_event });
                }
                Err(err) => {
                    let err = CodexErr::Stream(
                        format!("[ws] failed to connect: {err}"),
                        None,
                        Some(request_id.clone()),
                    );
                    if (attempt as u64) < max_retries {
                        tokio::time::sleep(backoff(attempt as u64)).await;
                        continue;
                    }
                    return Err(err);
                }
            }
        }
    }

    /// Implementation for the OpenAI *Responses* experimental API.
    async fn stream_responses(&self, prompt: &Prompt, log_tag: Option<&str>) -> Result<ResponseStream> {
        if let Some(path) = &*CODEX_RS_SSE_FIXTURE {
            // short circuit for tests
            warn!(path, "Streaming from fixture");
            return stream_from_fixture(path, self.provider.clone(), self.otel_event_manager.clone())
                .await;
        }

        let auth_manager = self.auth_manager.clone();

        let auth_mode = auth_manager
            .as_ref()
            .and_then(|m| m.auth())
            .as_ref()
            .map(|a| a.mode);

        // Use non-stored turns on all paths for stability.
        let store = false;

        let request_model = prompt
            .model_override
            .as_deref()
            .unwrap_or(self.config.model.as_str());
        let effective_effort = clamp_reasoning_effort_for_model(request_model, self.effort);
        let request_family = prompt
            .model_family_override
            .clone()
            .or_else(|| find_family_for_model(request_model))
            .unwrap_or_else(|| self.config.model_family.clone());

        let full_instructions = prompt.get_full_instructions(&request_family);
        let mut tools_json = create_tools_json_for_responses_api(&prompt.tools)?;
        if matches!(effective_effort, ReasoningEffortConfig::Minimal) {
            tools_json.retain(|tool| {
                tool.get("type")
                    .and_then(|value| value.as_str())
                    .map(|tool_type| tool_type != "web_search")
                    .unwrap_or(true)
                });
        }

        let input_with_instructions = prompt.get_formatted_input();

        // Build `text` parameter with conditional verbosity and optional format.
        // - Omit entirely for ChatGPT auth unless a `text.format` or output schema is present.
        // - Only include `text.verbosity` for GPT-5 family models; warn and ignore otherwise.
        // - When a structured `format` is present, still include `verbosity` so GPT-5 can honor it.
        let want_format = prompt.text_format.clone().or_else(|| {
            prompt.output_schema.as_ref().map(|schema| crate::client_common::TextFormat {
                r#type: "json_schema".to_string(),
                name: Some("code_output_schema".to_string()),
                strict: Some(true),
                schema: Some(schema.clone()),
            })
        });

        let effective_verbosity = clamp_text_verbosity_for_model(request_model, self.verbosity);

        let verbosity = match &request_family.family {
            family if family == "gpt-5" || family == "gpt-5.1" => Some(effective_verbosity),
            _ => None,
        };

        let text_template = match (auth_mode, want_format, verbosity) {
            (Some(AuthMode::ChatGPT), None, _) => None,
            (_, Some(fmt), _) => Some(crate::client_common::Text {
                verbosity: effective_verbosity.into(),
                format: Some(fmt),
            }),
            (_, None, Some(_)) => Some(crate::client_common::Text {
                verbosity: effective_verbosity.into(),
                format: None,
            }),
            (_, None, None) => None,
        };

        // In general, we want to explicitly send `store: false` when using the Responses API,
        // but in practice, the Azure Responses API rejects `store: false`:
        //
        // - If store = false and id is sent an error is thrown that ID is not found
        // - If store = false and id is not sent an error is thrown that ID is required
        //
        // For Azure, we send `store: true` and preserve reasoning item IDs.
        let azure_workaround = self.provider.is_azure_responses_endpoint();

        let model_slug = request_model;

        let session_id = prompt
            .session_id_override
            .unwrap_or(self.session_id);
        let session_id_str = session_id.to_string();

        let mut attempt = 0;
        let max_retries = self.provider.request_max_retries();
        let mut request_id = String::new();
        let mut rate_limit_switch_state = crate::account_switching::RateLimitSwitchState::default();

        // Compute endpoint with the latest available auth (may be None at this point).
        let endpoint = self
            .provider
            .get_full_url(&auth_manager.as_ref().and_then(|m| m.auth()));

        loop {
            attempt += 1;

            let reasoning = self.current_reasoning_param(&request_family, effective_effort);
            // Request encrypted COT if we are not storing responses,
            // otherwise reasoning items will be referenced by ID
            let include: Vec<String> = if !store && reasoning.is_some() {
                vec!["reasoning.encrypted_content".to_string()]
            } else {
                Vec::new()
            };

            let text = text_template.clone();

            let payload = ResponsesApiRequest {
                model: model_slug,
                instructions: &full_instructions,
                input: &input_with_instructions,
                tools: &tools_json,
                tool_choice: "auto",
                parallel_tool_calls: request_family.supports_parallel_tool_calls,
                reasoning,
                text,
                store: azure_workaround,
                stream: true,
                include,
                // Use a stable per-process cache key (session id). With store=false this is inert.
                prompt_cache_key: Some(session_id_str.clone()),
            };

            let mut payload_json = serde_json::to_value(&payload)?;
            if let Some(model_value) = payload_json.get_mut("model") {
                *model_value = serde_json::Value::String(model_slug.to_string());
            }
            if azure_workaround {
                attach_item_ids(&mut payload_json, &input_with_instructions);
            }
            if let Some(openrouter_cfg) = self.provider.openrouter_config() {
                if let Some(obj) = payload_json.as_object_mut() {
                    if let Some(provider) = &openrouter_cfg.provider {
                        obj.insert(
                            "provider".to_string(),
                            serde_json::to_value(provider)?
                        );
                    }
                    if let Some(route) = &openrouter_cfg.route {
                        obj.insert("route".to_string(), route.clone());
                    }
                    for (key, value) in &openrouter_cfg.extra {
                        obj.entry(key.clone()).or_insert(value.clone());
                    }
                }
            }
            let payload_body = serde_json::to_string(&payload_json)?;

            let mut auth_refresh_error: Option<RefreshTokenError> = None;

            // Always fetch the latest auth in case a prior attempt refreshed the token.
            let auth = auth_manager.as_ref().and_then(|m| m.auth());

            trace!(
                "POST to {}: {}",
                self.provider.get_full_url(&auth),
                payload_body.as_str()
            );

            let mut req_builder = self
                .provider
                .create_request_builder(&self.client, &auth)
                .await?;

            let has_beta_header = req_builder
                .try_clone()
                .and_then(|builder| builder.build().ok())
                .map_or(false, |req| req.headers().contains_key("OpenAI-Beta"));

            if !has_beta_header {
                let beta_value = if self.provider.is_public_openai_responses_endpoint() {
                    RESPONSES_BETA_HEADER_V1
                } else {
                    RESPONSES_BETA_HEADER_EXPERIMENTAL
                };
                req_builder = req_builder.header("OpenAI-Beta", beta_value);
            }

            req_builder = attach_openai_subagent_header(req_builder);
            req_builder = attach_codex_beta_features_header(req_builder, &self.config);

            req_builder = req_builder
                // Send `conversation_id`/`session_id` so the server can hit the prompt-cache.
                .header("conversation_id", session_id_str.clone())
                .header("session_id", session_id_str.clone())
                .header(reqwest::header::ACCEPT, "text/event-stream")
                .json(&payload_json);

            if let Some(auth) = auth.as_ref()
                && auth.mode == AuthMode::ChatGPT
                && let Some(account_id) = auth.get_account_id()
            {
                req_builder = req_builder.header("chatgpt-account-id", account_id);
            }

            if request_id.is_empty() {
                let endpoint_for_log = self.provider.get_full_url(&auth);
                let header_snapshot = req_builder
                    .try_clone()
                    .and_then(|builder| builder.build().ok())
                    .map(|req| header_map_to_json(req.headers()));

                if let Ok(logger) = self.debug_logger.lock() {
                    request_id = logger
                        .start_request_log(
                            &endpoint_for_log,
                            &payload_json,
                            header_snapshot.as_ref(),
                            log_tag,
                        )
                        .unwrap_or_default();
                }
            }

            let res = if let Some(otel) = self.otel_event_manager.as_ref() {
                otel.log_request(attempt, || req_builder.send()).await
            } else {
                req_builder.send().await
            };
            if let Ok(resp) = &res {
                trace!(
                    "Response status: {}, request-id: {}",
                    resp.status(),
                    resp.headers()
                        .get("x-request-id")
                        .map(|v| v.to_str().unwrap_or_default())
                        .unwrap_or_default()
                );
            }

            match res {
                Ok(resp) if resp.status().is_success() => {
                    // Log successful response initiation
                    if let Ok(logger) = self.debug_logger.lock() {
                        let _ = logger.append_response_event(
                            &request_id,
                            "stream_initiated",
                            &serde_json::json!({
                                "status": "success",
                                "status_code": resp.status().as_u16(),
                                "x_request_id": resp.headers()
                                    .get("x-request-id")
                                    .and_then(|v| v.to_str().ok())
                                    .unwrap_or_default()
                            }),
                        );
                    }
                    let (tx_event, rx_event) = mpsc::channel::<Result<ResponseEvent>>(1600);

                    if let Some(snapshot) = parse_rate_limit_snapshot(resp.headers()) {
                        debug!(
                            "rate limit headers:\n{}",
                            format_rate_limit_headers(resp.headers())
                        );

                        if tx_event
                            .send(Ok(ResponseEvent::RateLimits(snapshot)))
                            .await
                            .is_err()
                        {
                            debug!("receiver dropped rate limit snapshot event");
                        }
                    }

                    let models_etag = resp
                        .headers()
                        .get("X-Models-Etag")
                        .and_then(|value| value.to_str().ok())
                        .map(ToString::to_string);
                    if let Some(etag) = models_etag {
                        if tx_event
                            .send(Ok(ResponseEvent::ModelsEtag(etag)))
                            .await
                            .is_err()
                        {
                            debug!("receiver dropped models etag event");
                        }
                    }

                    // spawn task to process SSE
                    let stream = resp.bytes_stream().map_err(CodexErr::Reqwest);
                    let debug_logger = Arc::clone(&self.debug_logger);
                    let request_id_clone = request_id.clone();
                    let otel_event_manager = self.otel_event_manager.clone();
                    tokio::spawn(process_sse(
                        stream,
                        tx_event,
                        self.provider.stream_idle_timeout(),
                        debug_logger,
                        request_id_clone,
                        otel_event_manager,
                        Arc::new(RwLock::new(StreamCheckpoint::default())),
                    ));

                    return Ok(ResponseStream { rx_event });
                }
                Ok(res) => {
                    let status = res.status();
                    // Capture x-request-id up-front in case we consume the response body later.
                    let x_request_id = res
                        .headers()
                        .get("x-request-id")
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_string());
                    let now = Utc::now();

                    // Pull out Retry‑After header if present.
                    let retry_after_hint = res
                        .headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|raw| parse_retry_after_header(raw, now));

                    if status == StatusCode::UNAUTHORIZED {
                        if let Some(manager) = auth_manager.as_ref() {
                            match manager.refresh_token_classified().await {
                                Ok(Some(_)) => {}
                                Ok(None) => {
                                    auth_refresh_error = Some(RefreshTokenError::permanent(
                                        AUTH_REQUIRED_MESSAGE,
                                    ));
                                }
                                Err(err) => {
                                    auth_refresh_error = Some(err);
                                }
                            }
                        } else {
                            auth_refresh_error = Some(RefreshTokenError::permanent(
                                "Authentication manager unavailable; please log in again.",
                            ));
                        }
                    }

                    // Read the response body once for diagnostics across error branches.
                    let body_text = res.text().await.unwrap_or_default();
                    let body = serde_json::from_str::<ErrorResponse>(&body_text).ok();

                    if status == StatusCode::TOO_MANY_REQUESTS
                        && self.config.auto_switch_accounts_on_rate_limit
                        && auth_manager.is_some()
                        && auth::read_code_api_key_from_env().is_none()
                    {
                        let current_account_id = auth
                            .as_ref()
                            .and_then(|current| current.get_account_id())
                            .or_else(|| {
                                auth_accounts::get_active_account_id(self.code_home())
                                    .ok()
                                    .flatten()
                            });
                        if let Some(current_account_id) = current_account_id {
                            let mut retry_after_delay = retry_after_hint.clone();
                            if retry_after_delay.is_none() {
                                if let Some(ErrorResponse { ref error }) = body {
                                    retry_after_delay = try_parse_retry_after(error, now);
                                }
                            }

                            let current_auth_mode = auth
                                .as_ref()
                                .map(|a| a.mode)
                                .unwrap_or(AuthMode::ApiKey);

                            let switch_reason = match body
                                .as_ref()
                                .and_then(|err| err.error.r#type.as_deref())
                            {
                                Some("usage_limit_reached") => "usage_limit_reached",
                                Some("usage_not_included") => "usage_not_included",
                                _ => "http_429",
                            };

                            let (blocked_until, should_record_usage_limit) = match body.as_ref() {
                                Some(ErrorResponse { error })
                                    if error.r#type.as_deref() == Some("usage_limit_reached") =>
                                {
                                    (
                                        error
                                            .resets_in_seconds
                                            .map(|seconds| now + ChronoDuration::seconds(seconds as i64)),
                                        true,
                                    )
                                }
                                _ => (retry_after_delay.as_ref().map(|info| info.resume_at), false),
                            };

                            rate_limit_switch_state.mark_limited(
                                &current_account_id,
                                current_auth_mode,
                                blocked_until,
                            );

                            if let Ok(Some(next_account_id)) =
                                crate::account_switching::select_next_account_id(
                                    self.code_home(),
                                    &rate_limit_switch_state,
                                    self.config.api_key_fallback_on_all_accounts_limited,
                                    now,
                                    Some(current_account_id.as_str()),
                                )
                            {
                                if should_record_usage_limit {
                                    let plan_type = body
                                        .as_ref()
                                        .and_then(|err| err.error.plan_type.as_deref())
                                        .map(|s| s.to_string());
                                    let resets_in_seconds =
                                        body.as_ref().and_then(|err| err.error.resets_in_seconds);
                                    let code_home = self.code_home().to_path_buf();
                                    let account_id = current_account_id.clone();
                                    tokio::task::spawn_blocking(move || {
                                        let observed_at = Utc::now();
                                        if let Err(err) = account_usage::record_usage_limit_hint(
                                            &code_home,
                                            &account_id,
                                            plan_type.as_deref(),
                                            resets_in_seconds,
                                            observed_at,
                                        ) {
                                            tracing::warn!("Failed to persist usage limit hint: {err}");
                                        }
                                    });
                                }

                                tracing::info!(
                                    from_account_id = %current_account_id,
                                    to_account_id = %next_account_id,
                                    reason = switch_reason,
                                    "rate limit hit; auto-switching active account"
                                );

                                if let Ok(logger) = self.debug_logger.lock() {
                                    let _ = logger.append_response_event(
                                        &request_id,
                                        "account_switch",
                                        &serde_json::json!({
                                            "reason": switch_reason,
                                            "from_account_id": current_account_id.clone(),
                                            "to_account_id": next_account_id.clone(),
                                            "status": status.as_u16(),
                                        }),
                                    );
                                }

                                if let Err(err) =
                                    auth::activate_account(self.code_home(), &next_account_id)
                                {
                                    tracing::warn!(
                                        from_account_id = %current_account_id,
                                        to_account_id = %next_account_id,
                                        error = %err,
                                        "failed to activate account after rate limit"
                                    );
                                } else {
                                    if let Some(manager) = auth_manager.as_ref() {
                                        manager.reload();
                                    }
                                    attempt = 0;
                                    continue;
                                }
                            }
                        }
                    }

                    if status == StatusCode::BAD_REQUEST {
                        if let Some(ErrorResponse { ref error }) = body {
                            if !self.reasoning_summary_disabled.load(Ordering::Relaxed)
                                && is_reasoning_summary_rejected(error)
                            {
                                self.disable_reasoning_summary();

                                if let Ok(logger) = self.debug_logger.lock() {
                                    let _ = logger.append_response_event(
                                        &request_id,
                                        "reasoning_summary_disabled",
                                        &serde_json::json!({
                                            "status": status.as_u16(),
                                            "message": error.message.clone(),
                                            "code": error.code.clone(),
                                            "param": error.param.clone(),
                                        }),
                                    );
                                }

                                // Retry immediately with reasoning summaries removed.
                                attempt = 0;
                                continue;
                            }
                        }
                    }

                    // The OpenAI Responses endpoint returns structured JSON bodies even for 4xx/5xx
                    // errors. When we bubble early with only the HTTP status the caller sees an opaque
                    // "unexpected status 400 Bad Request" which makes debugging nearly impossible.
                    // Instead, read (and include) the response text so higher layers and users see the
                    // exact error message (e.g. "Unknown parameter: 'input[0].metadata'"). The body is
                    // small and this branch only runs on error paths so the extra allocation is
                    // negligible.
                    if !(status == StatusCode::TOO_MANY_REQUESTS
                        || status == StatusCode::UNAUTHORIZED
                        || status.is_server_error())
                    {
                        // Log error response
                        if let Ok(logger) = self.debug_logger.lock() {
                            let _ = logger.append_response_event(
                                &request_id,
                                "error",
                                &serde_json::json!({
                                    "status": status.as_u16(),
                                    "body": body_text
                                }),
                            );
                            let _ = logger.end_request_log(&request_id);
                        }
                        return Err(CodexErr::UnexpectedStatus(UnexpectedResponseError {
                            status,
                            body: body_text,
                            request_id: None,
                        }));
                    }

                    if let Some(ErrorResponse { ref error }) = body {
                        if is_quota_exceeded_http_error(status, error) {
                            return Err(CodexErr::QuotaExceeded);
                        }
                    }

                    if status == StatusCode::UNAUTHORIZED {
                        if let Some(error) =
                            map_unauthorized_outcome(auth.is_some(), auth_refresh_error.as_ref())
                        {
                            return Err(error);
                        }
                    }

                    if status == StatusCode::TOO_MANY_REQUESTS {
                        if let Some(ErrorResponse { ref error }) = body {
                            if error.r#type.as_deref() == Some("usage_limit_reached") {
                                // Prefer the plan_type provided in the error message if present
                                // because it's more up to date than the one encoded in the auth
                                // token.
                                let plan_type = error
                                    .plan_type
                                    .clone()
                                    .or_else(|| auth.and_then(|a| a.get_plan_type()));
                                let resets_in_seconds = error.resets_in_seconds;
                                return Err(CodexErr::UsageLimitReached(UsageLimitReachedError {
                                    plan_type,
                                    resets_in_seconds,
                                }));
                            } else if error.r#type.as_deref() == Some("usage_not_included") {
                                return Err(CodexErr::UsageNotIncluded);
                            }
                        }
                    }

                    if attempt > max_retries {
                        // On final attempt, surface rich diagnostics for server errors.
                        // On final attempt, surface rich diagnostics for server errors.
                        if status.is_server_error() {
                            let (message, body_excerpt) =
                                match serde_json::from_str::<ErrorResponse>(&body_text) {
                                    Ok(ErrorResponse { error }) => {
                                        let msg = error
                                            .message
                                            .unwrap_or_else(|| "server error".to_string());
                                        (msg, None)
                                    }
                                    Err(_) => {
                                        let mut excerpt = body_text;
                                        const MAX: usize = 600;
                                        if excerpt.len() > MAX {
                                            excerpt.truncate(MAX);
                                        }
                                        (
                                            "server error".to_string(),
                                            if excerpt.is_empty() {
                                                None
                                            } else {
                                                Some(excerpt)
                                            },
                                        )
                                    }
                                };

                            // Build a single-line, actionable message for the UI and logs.
                            let mut msg = format!("server error {status}: {message}");
                            if let Some(id) = &x_request_id {
                                msg.push_str(&format!(" (request-id: {id})"));
                            }
                            if let Some(excerpt) = &body_excerpt {
                                msg.push_str(&format!(" | body: {excerpt}"));
                            }

                            // Log detailed context to the debug logger and close the request log.
                            if let Ok(logger) = self.debug_logger.lock() {
                                let _ = logger.append_response_event(
                                    &request_id,
                                    "server_error_on_retry_limit",
                                    &serde_json::json!({
                                        "status": status.as_u16(),
                                        "x_request_id": x_request_id,
                                        "message": message,
                                        "body_excerpt": body_excerpt,
                                    }),
                                );
                                let _ = logger.end_request_log(&request_id);
                            }

                            return Err(CodexErr::ServerError(msg));
                        }

                        return Err(CodexErr::RetryLimit(RetryLimitReachedError {
                            status,
                            request_id: None,
                            retryable: status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS,
                        }));
                    }

                    let mut retry_after_delay = retry_after_hint;
                    if retry_after_delay.is_none() {
                        if let Some(ErrorResponse { ref error }) = body {
                            retry_after_delay = try_parse_retry_after(error, now);
                        }
                    }

                    let delay = retry_after_delay
                        .as_ref()
                        .map(|info| info.delay)
                        .unwrap_or_else(|| backoff(attempt));
                    tokio::time::sleep(delay).await;
                }
                Err(e) => {
                    let is_connectivity = e.is_connect() || e.is_timeout() || e.is_request();
                    if attempt > max_retries {
                        // Log network error before surfacing.
                        if let Ok(logger) = self.debug_logger.lock() {
                            let _ = logger.log_error(&endpoint, &format!("Network error: {}", e), log_tag);
                        }
                        if is_connectivity {
                            let req_id = (!request_id.is_empty()).then(|| request_id.clone());
                            return Err(CodexErr::Stream(
                                format!("[transport] network unavailable: {e}"),
                                None,
                                req_id,
                            ));
                        }
                        return Err(e.into());
                    }
                    let delay = backoff(attempt);
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    pub fn get_provider(&self) -> ModelProviderInfo {
        self.provider.clone()
    }

    /// Returns the currently configured model slug.
    #[allow(dead_code)]
    pub fn get_model(&self) -> String {
        self.config.model.clone()
    }

    pub fn model_explicit(&self) -> bool {
        self.config.model_explicit
    }

    /// Returns the currently configured model family.
    #[allow(dead_code)]
    pub fn get_model_family(&self) -> ModelFamily {
        self.config.model_family.clone()
    }

    #[allow(dead_code)]
    pub fn get_model_context_window(&self) -> Option<u64> {
        self.config.model_context_window
    }

    #[allow(dead_code)]
    pub fn get_auth_manager(&self) -> Option<Arc<AuthManager>> {
        self.auth_manager.clone()
    }

    pub async fn compact_conversation_history(&self, prompt: &Prompt) -> Result<Vec<ResponseItem>> {
        if prompt.input.is_empty() {
            return Ok(Vec::new());
        }

        let auth_manager = self.auth_manager.clone();
        let mut rate_limit_switch_state = crate::account_switching::RateLimitSwitchState::default();

        let model_slug = prompt
            .model_override
            .as_deref()
            .unwrap_or(self.config.model.as_str());
        let family = prompt
            .model_family_override
            .clone()
            .or_else(|| find_family_for_model(model_slug))
            .unwrap_or_else(|| self.config.model_family.clone());
        let instructions = prompt.get_full_instructions(&family).into_owned();
        let payload = CompactHistoryRequest {
            model: model_slug,
            input: &prompt.input,
            instructions: instructions.clone(),
        };
        let payload_json = serde_json::json!({
            "model": payload.model,
            "input": payload.input,
            "instructions": instructions,
        });
        let mut request_id = String::new();

        loop {
            let auth = auth_manager.as_ref().and_then(|m| m.auth());
            let mut request = self
                .provider
                .create_compact_request_builder(&self.client, &auth)
                .await?;

            // Ensure Responses API beta header is present for compact calls. Mirror the
            // streaming path: use the public "responses=v1" header for the public OpenAI
            // endpoint and fall back to "responses=experimental" for other providers.
            let has_beta_header = request
                .try_clone()
                .and_then(|builder| builder.build().ok())
                .map_or(false, |req| req.headers().contains_key("OpenAI-Beta"));

            if !has_beta_header {
                let beta_value = if self.provider.is_public_openai_responses_endpoint() {
                    RESPONSES_BETA_HEADER_V1
                } else {
                    RESPONSES_BETA_HEADER_EXPERIMENTAL
                };
                request = request.header("OpenAI-Beta", beta_value);
            }

            request = attach_openai_subagent_header(request);
            request = attach_codex_beta_features_header(request, &self.config);

            if let Some(auth) = auth.as_ref()
                && auth.mode == AuthMode::ChatGPT
                && let Some(account_id) = auth.get_account_id()
            {
                request = request.header("chatgpt-account-id", account_id);
            }

            request = request.json(&payload);

            let header_snapshot = request
                .try_clone()
                .and_then(|builder| builder.build().ok())
                .map(|req| header_map_to_json(req.headers()));

            if request_id.is_empty() {
                if let Ok(logger) = self.debug_logger.lock() {
                    let endpoint = self
                        .provider
                        .get_compact_url(&auth)
                        .unwrap_or_else(|| self.provider.get_full_url(&auth));
                    request_id = logger
                        .start_request_log(
                            &endpoint,
                            &payload_json,
                            header_snapshot.as_ref(),
                            Some("compact_remote"),
                        )
                        .unwrap_or_default();
                }
            }

            let response = request.send().await?;
            let status = response.status();
            let body = response.text().await?;

            if status == StatusCode::TOO_MANY_REQUESTS
                && self.config.auto_switch_accounts_on_rate_limit
                && auth_manager.is_some()
                && auth::read_code_api_key_from_env().is_none()
            {
                let now = Utc::now();
                let current_account_id = auth
                    .as_ref()
                    .and_then(|current| current.get_account_id())
                    .or_else(|| {
                        auth_accounts::get_active_account_id(self.code_home())
                            .ok()
                            .flatten()
                    });
                if let Some(current_account_id) = current_account_id {
                    let current_auth_mode = auth
                        .as_ref()
                        .map(|a| a.mode)
                        .unwrap_or(AuthMode::ApiKey);
                    rate_limit_switch_state.mark_limited(
                        &current_account_id,
                        current_auth_mode,
                        None,
                    );
                    if let Ok(Some(next_account_id)) =
                        crate::account_switching::select_next_account_id(
                            self.code_home(),
                            &rate_limit_switch_state,
                            self.config.api_key_fallback_on_all_accounts_limited,
                            now,
                            Some(current_account_id.as_str()),
                        )
                    {
                        tracing::info!(
                            from_account_id = %current_account_id,
                            to_account_id = %next_account_id,
                            "rate limit hit during compact; auto-switching active account"
                        );
                        if let Err(err) = auth::activate_account(self.code_home(), &next_account_id) {
                            tracing::warn!(
                                from_account_id = %current_account_id,
                                to_account_id = %next_account_id,
                                error = %err,
                                "failed to activate account after rate limit during compact"
                            );
                        } else {
                            if let Some(manager) = auth_manager.as_ref() {
                                manager.reload();
                            }
                            continue;
                        }
                    }
                }
            }

            if let Ok(logger) = self.debug_logger.lock() {
                let response_body: serde_json::Value = serde_json::from_str(&body)
                    .unwrap_or_else(|_| serde_json::json!({ "raw": body }));
                let _ = logger.append_response_event(
                    &request_id,
                    "compact_response",
                    &serde_json::json!({
                        "status_code": status.as_u16(),
                        "body": response_body,
                    }),
                );
                let _ = logger.end_request_log(&request_id);
            }

            if !status.is_success() {
                return Err(CodexErr::UnexpectedStatus(UnexpectedResponseError {
                    status,
                    body,
                    request_id: None,
                }));
            }

            let CompactHistoryResponse { output } = serde_json::from_str(&body)?;
            return Ok(output);
        }
    }
}

fn attach_codex_beta_features_header(
    builder: reqwest::RequestBuilder,
    config: &Config,
) -> reqwest::RequestBuilder {
    let Some(value) = codex_beta_features_header_value(config) else {
        return builder;
    };

    let has_header = builder
        .try_clone()
        .and_then(|builder| builder.build().ok())
        .map_or(false, |req| req.headers().contains_key("x-codex-beta-features"));
    if has_header {
        return builder;
    }

    builder.header("x-codex-beta-features", value)
}

fn codex_beta_features_header_value(config: &Config) -> Option<HeaderValue> {
    let mut enabled: Vec<&'static str> = Vec::new();

    if config.skills_enabled {
        enabled.push("skills");
    }
    if config.tools_web_search_request {
        enabled.push("web_search_request");
    }

    let value = enabled.join(",");
    if value.is_empty() {
        return None;
    }

    HeaderValue::from_str(value.as_str()).ok()
}

fn attach_openai_subagent_header(builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    let Some(value) = openai_subagent_header_value() else {
        return builder;
    };

    let has_header = builder
        .try_clone()
        .and_then(|builder| builder.build().ok())
        .map_or(false, |req| req.headers().contains_key("x-openai-subagent"));
    if has_header {
        return builder;
    }

    builder.header("x-openai-subagent", value)
}

fn openai_subagent_header_value() -> Option<HeaderValue> {
    let subagent = std::env::var(CODE_OPENAI_SUBAGENT_ENV).ok()?;
    let subagent = subagent.trim();
    if subagent.is_empty() {
        return None;
    }
    HeaderValue::from_str(subagent).ok()
}

fn clamp_text_verbosity_for_model(
    model: &str,
    requested: TextVerbosityConfig,
) -> TextVerbosityConfig {
    let allowed = supported_text_verbosity_for_model(model);
    if allowed.iter().any(|v| v == &requested) {
        return requested;
    }

    if let Some(medium) = allowed.iter().find(|v| matches!(v, TextVerbosityConfig::Medium)) {
        tracing::debug!(
            model,
            requested = ?requested,
            fallback = ?medium,
            "text verbosity clamped to supported value for model",
        );
        return *medium;
    }

    let fallback = *allowed.first().unwrap_or(&TextVerbosityConfig::Medium);
    tracing::debug!(
        model,
        requested = ?requested,
        fallback = ?fallback,
        "text verbosity clamped to first supported value for model",
    );
    fallback
}

fn supported_text_verbosity_for_model(model: &str) -> &'static [TextVerbosityConfig] {
    if model.eq_ignore_ascii_case("gpt-5.1-codex-max") {
        return &[TextVerbosityConfig::Medium];
    }

    const ALL: &[TextVerbosityConfig] = &[TextVerbosityConfig::Low, TextVerbosityConfig::Medium, TextVerbosityConfig::High];
    ALL
}

#[derive(Debug, Deserialize, Serialize)]
struct SseEvent {
    #[serde(rename = "type")]
    kind: String,
    response: Option<Value>,
    item: Option<Value>,
    delta: Option<String>,
    // Present on delta events from the Responses API; used to correlate
    // streaming chunks with the final OutputItemDone.
    item_id: Option<String>,
    // Optional ordering metadata from the Responses API; used to filter
    // duplicates and out‑of‑order reasoning deltas.
    sequence_number: Option<u64>,
    output_index: Option<u32>,
    content_index: Option<u32>,
    summary_index: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct ResponseCompleted {
    id: String,
    usage: Option<ResponseCompletedUsage>,
}

#[derive(Debug, Deserialize)]
struct ResponseCompletedUsage {
    input_tokens: u64,
    input_tokens_details: Option<ResponseCompletedInputTokensDetails>,
    output_tokens: u64,
    output_tokens_details: Option<ResponseCompletedOutputTokensDetails>,
    total_tokens: u64,
}

impl From<ResponseCompletedUsage> for TokenUsage {
    fn from(val: ResponseCompletedUsage) -> Self {
        TokenUsage {
            input_tokens: val.input_tokens,
            cached_input_tokens: val
                .input_tokens_details
                .map(|d| d.cached_tokens)
                .unwrap_or(0),
            output_tokens: val.output_tokens,
            reasoning_output_tokens: val
                .output_tokens_details
                .map(|d| d.reasoning_tokens)
                .unwrap_or(0),
            total_tokens: val.total_tokens,
        }
    }
}

#[derive(Debug, Deserialize)]
struct ResponseCompletedInputTokensDetails {
    cached_tokens: u64,
}

#[derive(Debug, Deserialize)]
struct ResponseCompletedOutputTokensDetails {
    reasoning_tokens: u64,
}

fn attach_item_ids(payload_json: &mut Value, original_items: &[ResponseItem]) {
    let Some(input_value) = payload_json.get_mut("input") else {
        return;
    };
    let serde_json::Value::Array(items) = input_value else {
        return;
    };

    for (value, item) in items.iter_mut().zip(original_items.iter()) {
        if let ResponseItem::Reasoning { id, .. }
        | ResponseItem::Message { id: Some(id), .. }
        | ResponseItem::WebSearchCall { id: Some(id), .. }
        | ResponseItem::FunctionCall { id: Some(id), .. }
        | ResponseItem::LocalShellCall { id: Some(id), .. }
        | ResponseItem::CustomToolCall { id: Some(id), .. } = item
        {
            if id.is_empty() {
                continue;
            }

            if let Some(obj) = value.as_object_mut() {
                obj.insert("id".to_string(), Value::String(id.clone()));
            }
        }
    }
}

fn parse_rate_limit_snapshot(headers: &HeaderMap) -> Option<RateLimitSnapshotEvent> {
    let primary_used_percent = parse_header_f64(headers, "x-codex-primary-used-percent")?;
    let secondary_used_percent = parse_header_f64(headers, "x-codex-secondary-used-percent")?;
    let primary_to_secondary_ratio_percent =
        parse_header_f64(headers, "x-codex-primary-over-secondary-limit-percent")?;
    let primary_window_minutes = parse_header_u64(headers, "x-codex-primary-window-minutes")?;
    let secondary_window_minutes = parse_header_u64(headers, "x-codex-secondary-window-minutes")?;
    let primary_reset_after_seconds =
        parse_header_u64(headers, "x-codex-primary-reset-after-seconds");
    let secondary_reset_after_seconds =
        parse_header_u64(headers, "x-codex-secondary-reset-after-seconds");

    Some(RateLimitSnapshotEvent {
        primary_used_percent,
        secondary_used_percent,
        primary_to_secondary_ratio_percent,
        primary_window_minutes,
        secondary_window_minutes,
        primary_reset_after_seconds,
        secondary_reset_after_seconds,
    })
}

fn format_rate_limit_headers(headers: &HeaderMap) -> String {
    let mut pairs: Vec<String> = headers
        .iter()
        .map(|(name, value)| {
            let value_str = value.to_str().unwrap_or("<invalid>");
            format!("{}: {}", name, value_str)
        })
        .collect();
    pairs.sort();
    pairs.join("\n")
}

fn parse_header_f64(headers: &HeaderMap, name: &str) -> Option<f64> {
    parse_header_str(headers, name)?
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
}

fn parse_header_u64(headers: &HeaderMap, name: &str) -> Option<u64> {
    parse_header_str(headers, name)?.parse::<u64>().ok()
}

fn parse_header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name)?.to_str().ok()
}

fn parse_retry_after_header(value: &str, now: DateTime<Utc>) -> Option<RetryAfter> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let normalized = trimmed
        .trim_matches(|c: char| matches!(c, '"' | '\'' | '<' | '>'))
        .trim();
    if normalized.is_empty() {
        return None;
    }

    if let Ok(secs) = normalized.parse::<u64>() {
        return Some(RetryAfter::from_duration(Duration::from_secs(secs), now));
    }
    if let Ok(float_secs) = normalized.parse::<f64>() {
        if !float_secs.is_sign_negative() {
            return Some(RetryAfter::from_duration(Duration::from_secs_f64(float_secs), now));
        }
    }
    if let Ok(system_time) = parse_http_date(normalized) {
        let resume_at: DateTime<Utc> = system_time.into();
        return Some(RetryAfter::from_resume_at(resume_at, now));
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(normalized) {
        return Some(RetryAfter::from_resume_at(dt.with_timezone(&Utc), now));
    }
    if let Ok(dt) = DateTime::parse_from_rfc2822(normalized) {
        return Some(RetryAfter::from_resume_at(dt.with_timezone(&Utc), now));
    }
    if let Ok(dt) = DateTime::parse_from_str(normalized, "%a, %d %b %Y %H:%M:%S %z") {
        return Some(RetryAfter::from_resume_at(dt.with_timezone(&Utc), now));
    }

    None
}

fn header_map_to_json(headers: &HeaderMap) -> Value {
    let mut ordered: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (name, value) in headers.iter() {
        let entry = ordered.entry(name.as_str().to_string()).or_default();
        entry.push(value.to_str().unwrap_or_default().to_string());
    }

    serde_json::to_value(ordered).unwrap_or(Value::Null)
}

async fn process_sse<S>(
    stream: S,
    tx_event: mpsc::Sender<Result<ResponseEvent>>,
    idle_timeout: Duration,
    debug_logger: Arc<Mutex<DebugLogger>>,
    request_id: String,
    otel_event_manager: Option<OtelEventManager>,
    checkpoint: Arc<RwLock<StreamCheckpoint>>,
) where
    S: Stream<Item = Result<Bytes>> + Unpin,
{
    let mut stream = stream.eventsource();

    // If the stream stays completely silent for an extended period treat it as disconnected.
    // The response id returned from the "complete" message.
    let mut response_completed: Option<ResponseCompleted> = None;
    let mut response_error: Option<CodexErr> = None;
    // Track the current item_id to include with delta events
    let mut current_item_id: Option<String> = None;

    // Monotonic sequence guards to drop duplicate/out‑of‑order deltas.
    // Keys are item_id strings.
    use std::collections::HashMap;
    // Track last sequence_number per (item_id, output_index[, content_index])
    // Default indices to 0 when absent for robustness across providers.
    let mut last_seq_reasoning_summary: HashMap<(String, u32, u32), u64> = HashMap::new();
    let mut last_seq_reasoning_content: HashMap<(String, u32, u32), u64> = HashMap::new();
    // Best-effort duplicate text guard when sequence_number is unavailable.
    let mut last_text_reasoning_summary: HashMap<(String, u32, u32), String> = HashMap::new();
    let mut last_text_reasoning_content: HashMap<(String, u32, u32), String> = HashMap::new();
    let mut global_last_seq: Option<u64> = checkpoint.read().ok().and_then(|c| c.last_sequence);

    loop {
        let next_event = if let Some(manager) = otel_event_manager.as_ref() {
            manager
                .log_sse_event(|| timeout(idle_timeout, stream.next()))
                .await
        } else {
            timeout(idle_timeout, stream.next()).await
        };

        let sse = match next_event {
            Ok(Some(Ok(sse))) => sse,
            Ok(Some(Err(e))) => {
                debug!("SSE Error: {e:#}");
                let event = CodexErr::Stream(
                    format!("[transport] {e}"),
                    None,
                    Some(request_id.clone()),
                );
                let _ = tx_event.send(Err(event)).await;
                return;
            }
            Ok(None) => {
                match response_completed {
                    Some(ResponseCompleted {
                        id: response_id,
                        usage,
                    }) => {
                        if let (Some(usage), Some(manager)) = (&usage, otel_event_manager.as_ref()) {
                            manager.sse_event_completed(
                                usage.input_tokens,
                                usage.output_tokens,
                                usage
                                    .input_tokens_details
                                    .as_ref()
                                    .map(|d| d.cached_tokens),
                                usage
                                    .output_tokens_details
                                    .as_ref()
                                    .map(|d| d.reasoning_tokens),
                                usage.total_tokens,
                            );
                        }
                        let event = ResponseEvent::Completed {
                            response_id,
                            token_usage: usage.map(Into::into),
                        };
                        let _ = tx_event.send(Ok(event)).await;
                    }
                    None => {
                        let error = response_error.unwrap_or(CodexErr::Stream(
                            "stream closed before response.completed".into(),
                            None,
                            Some(request_id.clone()),
                        ));
                        if let Some(manager) = otel_event_manager.as_ref() {
                            manager.see_event_completed_failed(&error);
                        }
                        let _ = tx_event.send(Err(error)).await;
                    }
                }
                // Mark the request log as complete
                if let Ok(logger) = debug_logger.lock() {
                    let _ = logger.end_request_log(&request_id);
                }
                return;
            }
            Err(_) => {
                let _ = tx_event
                    .send(Err(CodexErr::Stream(
                        "[idle] timeout waiting for SSE".into(),
                        None,
                        Some(request_id.clone()),
                    )))
                    .await;
                return;
            }
        };

        let raw = sse.data.clone();
        trace!("SSE event: {}", raw);

        // Log the raw SSE event data
        if let Ok(logger) = debug_logger.lock() {
            if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(&sse.data) {
                let _ = logger.append_response_event(&request_id, "sse_event", &json_value);
            }
        }

        let event: SseEvent = match serde_json::from_str(&sse.data) {
            Ok(event) => event,
            Err(e) => {
                // Log parse error with data excerpt, and record it in the debug logger as well.
                let mut excerpt = sse.data.clone();
                const MAX: usize = 600;
                if excerpt.len() > MAX {
                    excerpt.truncate(MAX);
                }
                debug!("Failed to parse SSE event: {e}, data: {excerpt}");
                if let Ok(logger) = debug_logger.lock() {
                    let _ = logger.append_response_event(
                        &request_id,
                        "sse_parse_error",
                        &serde_json::json!({
                            "error": e.to_string(),
                            "data_excerpt": excerpt,
                        }),
                    );
                }
                continue;
            }
        };

        if let Some(seq) = event.sequence_number {
            if let Some(last) = global_last_seq {
                if seq <= last {
                    continue;
                }
            }
            global_last_seq = Some(seq);
            if let Ok(mut guard) = checkpoint.write() {
                guard.last_sequence = Some(seq);
            }
        }

        match event.kind.as_str() {
            // Individual output item finalised. Forward immediately so the
            // rest of the agent can stream assistant text/functions *live*
            // instead of waiting for the final `response.completed` envelope.
            //
            // IMPORTANT: We used to ignore these events and forward the
            // duplicated `output` array embedded in the `response.completed`
            // payload.  That produced two concrete issues:
            //   1. No real‑time streaming – the user only saw output after the
            //      entire turn had finished, which broke the "typing" UX and
            //      made long‑running turns look stalled.
            //   2. Duplicate `function_call_output` items – both the
            //      individual *and* the completed array were forwarded, which
            //      confused the backend and triggered 400
            //      "previous_response_not_found" errors because the duplicated
            //      IDs did not match the incremental turn chain.
            //
            // The fix is to forward the incremental events *as they come* and
            // drop the duplicated list inside `response.completed`.
            "response.output_item.done" => {
                let Some(item_val) = event.item else { continue };
                // Special-case: web_search_call completion -> synthesize a completion event
                if item_val
                    .get("type")
                    .and_then(|v| v.as_str())
                    .is_some_and(|s| s == "web_search_call")
                {
                    let call_id = item_val
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let query = item_val
                        .get("action")
                        .and_then(|a| a.get("query"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let ev = ResponseEvent::WebSearchCallCompleted { call_id, query };
                    if tx_event.send(Ok(ev)).await.is_err() {
                        return;
                    }
                }
                let Ok(item) = serde_json::from_value::<ResponseItem>(item_val.clone()) else {
                    debug!("failed to parse ResponseItem from output_item.done");
                    continue;
                };

                // Extract item_id if present
                if let Some(id) = item_val.get("id").and_then(|v| v.as_str()) {
                    current_item_id = Some(id.to_string());
                } else {
                    // Check within the parsed item structure
                    match &item {
                        ResponseItem::Message { id, .. }
                        | ResponseItem::FunctionCall { id, .. }
                        | ResponseItem::LocalShellCall { id, .. } => {
                            if let Some(item_id) = id {
                                current_item_id = Some(item_id.clone());
                            }
                        }
                        ResponseItem::Reasoning { id, .. } => {
                            current_item_id = Some(id.clone());
                        }
                        _ => {}
                    }
                }

                let event = ResponseEvent::OutputItemDone { item, sequence_number: event.sequence_number, output_index: event.output_index };
                if tx_event.send(Ok(event)).await.is_err() {
                    return;
                }
            }
            "response.output_text.delta" => {
                if let Some(delta) = event.delta {
                    // Prefer the explicit item_id from the SSE event; fall back to last seen.
                    if let Some(ref id) = event.item_id {
                        current_item_id = Some(id.clone());
                    }
                    tracing::debug!("sse.delta output_text id={:?} len={}", current_item_id, delta.len());
                    let ev = ResponseEvent::OutputTextDelta {
                        delta,
                        item_id: event.item_id.or_else(|| current_item_id.clone()),
                        sequence_number: event.sequence_number,
                        output_index: event.output_index,
                    };
                    if tx_event.send(Ok(ev)).await.is_err() {
                        return;
                    }
                }
            }
            "response.reasoning_summary_text.delta" => {
                if let Some(delta) = event.delta {
                    if let Some(ref id) = event.item_id {
                        current_item_id = Some(id.clone());
                    }
                    // Compose key using item_id + output_index
                    let out_idx: u32 = event.output_index.unwrap_or(0);
                    let sum_idx: u32 = event.summary_index.unwrap_or(0);
                    if let Some(ref id) = current_item_id {
                        // Drop duplicates/out‑of‑order by sequence_number when available
                        if let Some(sn) = event.sequence_number {
                            let last = last_seq_reasoning_summary.entry((id.clone(), out_idx, sum_idx)).or_insert(0);
                            if *last >= sn { continue; }
                            *last = sn;
                        } else {
                            // Best-effort: drop exact duplicate text for same key when seq is missing
                            let key = (id.clone(), out_idx, sum_idx);
                            if last_text_reasoning_summary.get(&key).map_or(false, |prev| prev == &delta) {
                                continue;
                            }
                            last_text_reasoning_summary.insert(key, delta.clone());
                        }
                    }
                    tracing::debug!(
                        "sse.delta reasoning_summary id={:?} out_idx={} sum_idx={} len={} seq={:?}",
                        current_item_id, out_idx, sum_idx,
                        delta.len(),
                        event.sequence_number
                    );
                    let ev = ResponseEvent::ReasoningSummaryDelta {
                        delta,
                        item_id: event.item_id.or_else(|| current_item_id.clone()),
                        sequence_number: event.sequence_number,
                        output_index: event.output_index,
                        summary_index: event.summary_index,
                    };
                    if tx_event.send(Ok(ev)).await.is_err() {
                        return;
                    }
                }
            }
            "response.reasoning_text.delta" => {
                if let Some(delta) = event.delta {
                    if let Some(ref id) = event.item_id {
                        current_item_id = Some(id.clone());
                    }
                    // Compose key using item_id + output_index + content_index
                    let out_idx: u32 = event.output_index.unwrap_or(0);
                    let content_idx: u32 = event.content_index.unwrap_or(0);
                    if let Some(ref id) = current_item_id {
                        // Drop duplicates/out‑of‑order by sequence_number when available
                        if let Some(sn) = event.sequence_number {
                            let last = last_seq_reasoning_content.entry((id.clone(), out_idx, content_idx)).or_insert(0);
                            if *last >= sn { continue; }
                            *last = sn;
                        } else {
                            // Best-effort: drop exact duplicate text for same key when seq is missing
                            let key = (id.clone(), out_idx, content_idx);
                            if last_text_reasoning_content.get(&key).map_or(false, |prev| prev == &delta) {
                                continue;
                            }
                            last_text_reasoning_content.insert(key, delta.clone());
                        }
                    }
                    tracing::debug!(
                        "sse.delta reasoning_content id={:?} out_idx={} content_idx={} len={} seq={:?}",
                        current_item_id, out_idx, content_idx,
                        delta.len(),
                        event.sequence_number
                    );
                    let ev = ResponseEvent::ReasoningContentDelta {
                        delta,
                        item_id: event.item_id.or_else(|| current_item_id.clone()),
                        sequence_number: event.sequence_number,
                        output_index: event.output_index,
                        content_index: event.content_index,
                    };
                    if tx_event.send(Ok(ev)).await.is_err() {
                        return;
                    }
                }
            }
            "response.created" => {
                if event.response.is_some() {
                    let _ = tx_event.send(Ok(ResponseEvent::Created {})).await;
                }
            }
            "response.failed" => {
                if let Some(resp_val) = event.response {
                    response_error = Some(CodexErr::Stream(
                        "response.failed event received".to_string(),
                        None,
                        Some(request_id.clone()),
                    ));

                    let error = resp_val.get("error");

                    if let Some(error) = error {
                        match serde_json::from_value::<Error>(error.clone()) {
                            Ok(error) => {
                                if is_quota_exceeded_error(&error) {
                                    response_error = Some(CodexErr::QuotaExceeded);
                                } else {
                                    let retry_after = try_parse_retry_after(&error, Utc::now());
                                    let message = error.message.unwrap_or_default();
                                    response_error = Some(CodexErr::Stream(
                                        message,
                                        retry_after,
                                        Some(request_id.clone()),
                                    ));
                                }
                            }
                            Err(e) => {
                                debug!("failed to parse ErrorResponse: {e}");
                            }
                        }
                    }
                }
            }
            // Final response completed – includes array of output items & id
            "response.completed" => {
                if let Some(resp_val) = event.response {
                    match serde_json::from_value::<ResponseCompleted>(resp_val) {
                        Ok(r) => {
                            response_completed = Some(r);
                        }
                        Err(e) => {
                            debug!("failed to parse ResponseCompleted: {e}");
                            continue;
                        }
                    };
                };
            }
            "response.content_part.done"
            | "response.function_call_arguments.delta"
            | "response.custom_tool_call_input.delta"
            | "response.custom_tool_call_input.done" // also emitted as response.output_item.done
            | "response.in_progress"
            | "response.output_item.added"
            | "response.output_text.done" => {
                if event.kind == "response.output_item.added" {
                    if let Some(item) = event.item.as_ref() {
                        // Detect web_search_call begin and forward a synthetic event upstream.
                        if let Some(ty) = item.get("type").and_then(|v| v.as_str()) {
                            if ty == "web_search_call" {
                                let call_id = item
                                    .get("id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                let ev = ResponseEvent::WebSearchCallBegin { call_id };
                                if tx_event.send(Ok(ev)).await.is_err() {
                                    return;
                                }
                            }
                        }
                    }
                }
            }
            "response.reasoning_summary_part.added" => {
                // Boundary between reasoning summary sections (e.g., titles).
                let event = ResponseEvent::ReasoningSummaryPartAdded;
                if tx_event.send(Ok(event)).await.is_err() {
                    return;
                }
            }
            "response.reasoning_summary_text.done" => {}
            _ => {}
        }
    }
}

/// used in tests to stream from a text SSE file
async fn stream_from_fixture(
    path: impl AsRef<Path>,
    provider: ModelProviderInfo,
    otel_event_manager: Option<OtelEventManager>,
) -> Result<ResponseStream> {
    let (tx_event, rx_event) = mpsc::channel::<Result<ResponseEvent>>(1600);
    let f = std::fs::File::open(path.as_ref())?;
    let lines = std::io::BufReader::new(f).lines();

    // insert \n\n after each line for proper SSE parsing
    let mut content = String::new();
    for line in lines {
        content.push_str(&line?);
        content.push_str("\n\n");
    }

    let rdr = std::io::Cursor::new(content);
    let stream = ReaderStream::new(rdr).map_err(CodexErr::Io);
    // Create a dummy debug logger for testing
    let debug_logger = Arc::new(Mutex::new(DebugLogger::new(false).unwrap()));
    tokio::spawn(process_sse(
        stream,
        tx_event,
        provider.stream_idle_timeout(),
        debug_logger,
        String::new(), // Empty request_id for test fixture
        otel_event_manager,
        Arc::new(RwLock::new(StreamCheckpoint::default())),
    ));
    Ok(ResponseStream { rx_event })
}

// Note: legacy helpers for parsing Retry-After headers and rate-limit messages
// were removed during merge cleanup. If needed in the future, pick them from
// upstream and integrate with our error handling path.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_provider_info::{ModelProviderInfo, WireApi};
    use std::collections::HashMap;
    use serde_json::json;
    use tokio::sync::mpsc;
    use tokio_test::io::Builder as IoBuilder;
    use tokio_util::io::ReaderStream;
    use chrono::{Duration as ChronoDuration, TimeZone, Utc};

    // ────────────────────────────
    // Helpers
    // ────────────────────────────

    #[test]
    fn unauthorized_outcome_returns_permanent_error_for_permanent_refresh_failure() {
        let err = RefreshTokenError::permanent("token revoked");
        let outcome = map_unauthorized_outcome(true, Some(&err))
            .expect("should produce CodexErr");
        match outcome {
            CodexErr::AuthRefreshPermanent(msg) => {
                assert!(
                    msg.contains("token revoked"),
                    "unexpected message: {}",
                    msg
                );
            }
            other => panic!("unexpected outcome: {:?}", other),
        }
    }

    #[test]
    fn unauthorized_outcome_requires_login_without_auth() {
        let outcome = map_unauthorized_outcome(false, None)
            .expect("should require login");
        match outcome {
            CodexErr::AuthRefreshPermanent(msg) => {
                assert_eq!(msg, AUTH_REQUIRED_MESSAGE);
            }
            other => panic!("unexpected outcome: {:?}", other),
        }
    }

    #[test]
    fn unauthorized_outcome_allows_retry_for_transient_refresh_error() {
        let err = RefreshTokenError::transient("server busy");
        assert!(map_unauthorized_outcome(true, Some(&err)).is_none());
    }

    #[tokio::test]
    async fn responses_request_uses_beta_header_for_public_openai() {
        let provider = ModelProviderInfo {
            name: "openai".to_string(),
            base_url: Some("https://api.openai.com/v1".to_string()),
            env_key: None,
            env_key_instructions: None,
            experimental_bearer_token: None,
            wire_api: WireApi::Responses,
            query_params: None,
            http_headers: None,
            env_http_headers: None,
            request_max_retries: Some(0),
            stream_max_retries: None,
            stream_idle_timeout_ms: None,
            requires_openai_auth: false,
            openrouter: None,
        };

        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("client");

        let mut builder = provider
            .create_request_builder(&client, &None)
            .await
            .expect("builder");
        let has_beta = builder
            .try_clone()
            .and_then(|b| b.build().ok())
            .map_or(false, |req| req.headers().contains_key("OpenAI-Beta"));
        if !has_beta {
            builder = builder.header("OpenAI-Beta", RESPONSES_BETA_HEADER_V1);
        }
        let request = builder
            .try_clone()
            .expect("clone request builder")
            .build()
            .expect("build request");

        let header_value = request
            .headers()
            .get("OpenAI-Beta")
            .expect("OpenAI-Beta header present");
        assert_eq!(header_value, RESPONSES_BETA_HEADER_V1);
    }

    #[tokio::test]
    async fn responses_request_uses_experimental_for_backend() {
        let provider = ModelProviderInfo {
            name: "backend".to_string(),
            base_url: Some("https://chatgpt.com/backend-api/codex".to_string()),
            env_key: None,
            env_key_instructions: None,
            experimental_bearer_token: None,
            wire_api: WireApi::Responses,
            query_params: None,
            http_headers: None,
            env_http_headers: None,
            request_max_retries: Some(0),
            stream_max_retries: None,
            stream_idle_timeout_ms: None,
            requires_openai_auth: false,
            openrouter: None,
        };

        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("client");

        let mut builder = provider
            .create_request_builder(&client, &None)
            .await
            .expect("builder");
        let has_beta = builder
            .try_clone()
            .and_then(|b| b.build().ok())
            .map_or(false, |req| req.headers().contains_key("OpenAI-Beta"));
        if !has_beta {
            builder = builder.header("OpenAI-Beta", RESPONSES_BETA_HEADER_EXPERIMENTAL);
        }
        let request = builder
            .try_clone()
            .expect("clone request builder")
            .build()
            .expect("build request");

        let header_value = request
            .headers()
            .get("OpenAI-Beta")
            .expect("OpenAI-Beta header present");
        assert_eq!(header_value, RESPONSES_BETA_HEADER_EXPERIMENTAL);
    }

    #[tokio::test]
    async fn responses_request_respects_preexisting_beta_header() {
        let mut headers = HashMap::new();
        headers.insert("OpenAI-Beta".to_string(), "custom".to_string());
        let provider = ModelProviderInfo {
            name: "custom".to_string(),
            base_url: Some("https://api.openai.com/v1".to_string()),
            env_key: None,
            env_key_instructions: None,
            experimental_bearer_token: None,
            wire_api: WireApi::Responses,
            query_params: None,
            http_headers: Some(headers),
            env_http_headers: None,
            request_max_retries: Some(0),
            stream_max_retries: None,
            stream_idle_timeout_ms: None,
            requires_openai_auth: false,
            openrouter: None,
        };

        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("client");

        let request = provider
            .create_request_builder(&client, &None)
            .await
            .expect("builder")
            .try_clone()
            .expect("clone request builder")
            .build()
            .expect("build request");

        let header_value = request
            .headers()
            .get("OpenAI-Beta")
            .expect("OpenAI-Beta header present");
        assert_eq!(header_value, "custom");
    }

    /// Runs the SSE parser on pre-chunked byte slices and returns every event
    /// (including any final `Err` from a stream-closure check).
    async fn collect_events(
        chunks: &[&[u8]],
        provider: ModelProviderInfo,
    ) -> Vec<Result<ResponseEvent>> {
        let mut builder = IoBuilder::new();
        for chunk in chunks {
            builder.read(chunk);
        }

        let reader = builder.build();
        let stream = ReaderStream::new(reader).map_err(CodexErr::Io);
        let (tx, mut rx) = mpsc::channel::<Result<ResponseEvent>>(16);
        let debug_logger = Arc::new(Mutex::new(DebugLogger::new(false).unwrap()));
        let checkpoint = Arc::new(RwLock::new(StreamCheckpoint::default()));
        tokio::spawn(process_sse(
            stream,
            tx,
            provider.stream_idle_timeout(),
            debug_logger,
            String::new(),
            None,
            checkpoint,
        ));

        let mut events = Vec::new();
        while let Some(ev) = rx.recv().await {
            events.push(ev);
        }
        events
    }

    /// Builds an in-memory SSE stream from JSON fixtures and returns only the
    /// successfully parsed events (panics on internal channel errors).
    async fn run_sse(
        events: Vec<serde_json::Value>,
        provider: ModelProviderInfo,
    ) -> Vec<ResponseEvent> {
        let mut body = String::new();
        for e in events {
            let kind = e
                .get("type")
                .and_then(|v| v.as_str())
                .expect("fixture event missing type");
            if e.as_object().map(|o| o.len() == 1).unwrap_or(false) {
                body.push_str(&format!("event: {kind}\n\n"));
            } else {
                body.push_str(&format!("event: {kind}\ndata: {e}\n\n"));
            }
        }

        let (tx, mut rx) = mpsc::channel::<Result<ResponseEvent>>(8);
        let stream = ReaderStream::new(std::io::Cursor::new(body)).map_err(CodexErr::Io);
        let debug_logger = Arc::new(Mutex::new(DebugLogger::new(false).unwrap()));
        let checkpoint = Arc::new(RwLock::new(StreamCheckpoint::default()));
        tokio::spawn(process_sse(
            stream,
            tx,
            provider.stream_idle_timeout(),
            debug_logger,
            String::new(),
            None,
            checkpoint,
        ));

        let mut out = Vec::new();
        while let Some(ev) = rx.recv().await {
            out.push(ev.expect("channel closed"));
        }
        out
    }

    // ────────────────────────────
    // Tests from `implement-test-for-responses-api-sse-parser`
    // ────────────────────────────

    #[tokio::test]
    async fn parses_items_and_completed() {
        let item1 = json!({
            "type": "response.output_item.done",
            "item": {
                "type": "message",
                "role": "assistant",
                "content": [{"type": "output_text", "text": "Hello"}]
            }
        })
        .to_string();

        let item2 = json!({
            "type": "response.output_item.done",
            "item": {
                "type": "message",
                "role": "assistant",
                "content": [{"type": "output_text", "text": "World"}]
            }
        })
        .to_string();

        let completed = json!({
            "type": "response.completed",
            "response": { "id": "resp1" }
        })
        .to_string();

        let sse1 = format!("event: response.output_item.done\ndata: {item1}\n\n");
        let sse2 = format!("event: response.output_item.done\ndata: {item2}\n\n");
        let sse3 = format!("event: response.completed\ndata: {completed}\n\n");

        let provider = ModelProviderInfo {
            name: "test".to_string(),
            base_url: Some("https://test.com".to_string()),
            env_key: Some("TEST_API_KEY".to_string()),
            env_key_instructions: None,
            experimental_bearer_token: None,
            wire_api: WireApi::Responses,
            query_params: None,
            http_headers: None,
            env_http_headers: None,
            request_max_retries: Some(0),
            stream_max_retries: Some(0),
            stream_idle_timeout_ms: Some(1000),
            requires_openai_auth: false,
            openrouter: None,
        };

        let events = collect_events(
            &[sse1.as_bytes(), sse2.as_bytes(), sse3.as_bytes()],
            provider,
        )
        .await;

        assert_eq!(events.len(), 3);

        matches!(
            &events[0],
            Ok(ResponseEvent::OutputItemDone {
                item: ResponseItem::Message { role, .. },
                ..
            }) if role == "assistant"
        );

        matches!(
            &events[1],
            Ok(ResponseEvent::OutputItemDone {
                item: ResponseItem::Message { role, .. },
                ..
            }) if role == "assistant"
        );

        match &events[2] {
            Ok(ResponseEvent::Completed {
                response_id,
                token_usage,
            }) => {
                assert_eq!(response_id, "resp1");
                assert!(token_usage.is_none());
            }
            other => panic!("unexpected third event: {other:?}"),
        }
    }

    #[tokio::test]
    async fn error_when_missing_completed() {
        let item1 = json!({
            "type": "response.output_item.done",
            "item": {
                "type": "message",
                "role": "assistant",
                "content": [{"type": "output_text", "text": "Hello"}]
            }
        })
        .to_string();

        let sse1 = format!("event: response.output_item.done\ndata: {item1}\n\n");
        let provider = ModelProviderInfo {
            name: "test".to_string(),
            base_url: Some("https://test.com".to_string()),
            env_key: Some("TEST_API_KEY".to_string()),
            env_key_instructions: None,
            experimental_bearer_token: None,
            wire_api: WireApi::Responses,
            query_params: None,
            http_headers: None,
            env_http_headers: None,
            request_max_retries: Some(0),
            stream_max_retries: Some(0),
            stream_idle_timeout_ms: Some(1000),
            requires_openai_auth: false,
            openrouter: None,
        };

        let events = collect_events(&[sse1.as_bytes()], provider).await;

        assert_eq!(events.len(), 2);

        matches!(
            events[0],
            Ok(ResponseEvent::OutputItemDone { .. })
        );

        match &events[1] {
            Err(CodexErr::Stream(msg, _, _)) => {
                assert_eq!(msg, "stream closed before response.completed")
            }
            other => panic!("unexpected second event: {other:?}"),
        }
    }

    #[tokio::test]
    async fn error_when_error_event() {
        let raw_error = r#"{"type":"response.failed","sequence_number":3,"response":{"id":"resp_689bcf18d7f08194bf3440ba62fe05d803fee0cdac429894","object":"response","created_at":1755041560,"status":"failed","background":false,"error":{"code":"rate_limit_exceeded","message":"Rate limit reached for gpt-5.1 in organization org-AAA on tokens per min (TPM): Limit 30000, Used 22999, Requested 12528. Please try again in 11.054s. Visit https://platform.openai.com/account/rate-limits to learn more."}, "usage":null,"user":null,"metadata":{}}}"#;

        let sse1 = format!("event: response.failed\ndata: {raw_error}\n\n");
        let provider = ModelProviderInfo {
            name: "test".to_string(),
            base_url: Some("https://test.com".to_string()),
            env_key: Some("TEST_API_KEY".to_string()),
            env_key_instructions: None,
            experimental_bearer_token: None,
            wire_api: WireApi::Responses,
            query_params: None,
            http_headers: None,
            env_http_headers: None,
            request_max_retries: Some(0),
            stream_max_retries: Some(0),
            stream_idle_timeout_ms: Some(1000),
            requires_openai_auth: false,
            openrouter: None,
        };

        let events = collect_events(&[sse1.as_bytes()], provider).await;

        assert_eq!(events.len(), 1);

        match &events[0] {
            Err(CodexErr::Stream(msg, Some(retry), _)) => {
                assert_eq!(
                    msg,
                    "Rate limit reached for gpt-5.1 in organization org-AAA on tokens per min (TPM): Limit 30000, Used 22999, Requested 12528. Please try again in 11.054s. Visit https://platform.openai.com/account/rate-limits to learn more."
                );
                assert_eq!(retry.delay, Duration::from_secs_f64(11.054));
            }
            other => panic!("unexpected second event: {other:?}"),
        }
    }

    // ────────────────────────────
    // Table-driven test from `main`
    // ────────────────────────────

    /// Verifies that the adapter produces the right `ResponseEvent` for a
    /// variety of incoming `type` values.
    #[tokio::test]
    async fn table_driven_event_kinds() {
        struct TestCase {
            name: &'static str,
            event: serde_json::Value,
            expect_first: fn(&ResponseEvent) -> bool,
            expected_len: usize,
        }

        fn is_created(ev: &ResponseEvent) -> bool {
            matches!(ev, ResponseEvent::Created)
        }
        fn is_output(ev: &ResponseEvent) -> bool {
            matches!(ev, ResponseEvent::OutputItemDone { .. })
        }
        fn is_completed(ev: &ResponseEvent) -> bool {
            matches!(ev, ResponseEvent::Completed { .. })
        }

        let completed = json!({
            "type": "response.completed",
            "response": {
                "id": "c",
                "usage": {
                    "input_tokens": 0,
                    "input_tokens_details": null,
                    "output_tokens": 0,
                    "output_tokens_details": null,
                    "total_tokens": 0
                },
                "output": []
            }
        });

        let cases = vec![
            TestCase {
                name: "created",
                event: json!({"type": "response.created", "response": {}}),
                expect_first: is_created,
                expected_len: 2,
            },
            TestCase {
                name: "output_item.done",
                event: json!({
                    "type": "response.output_item.done",
                    "item": {
                        "type": "message",
                        "role": "assistant",
                        "content": [
                            {"type": "output_text", "text": "hi"}
                        ]
                    }
                }),
                expect_first: is_output,
                expected_len: 2,
            },
            TestCase {
                name: "unknown",
                event: json!({"type": "response.new_tool_event"}),
                expect_first: is_completed,
                expected_len: 1,
            },
        ];

        for case in cases {
            let mut evs = vec![case.event];
            evs.push(completed.clone());

            let provider = ModelProviderInfo {
                name: "test".to_string(),
                base_url: Some("https://test.com".to_string()),
                env_key: Some("TEST_API_KEY".to_string()),
                env_key_instructions: None,
                experimental_bearer_token: None,
                wire_api: WireApi::Responses,
                query_params: None,
                http_headers: None,
                env_http_headers: None,
                request_max_retries: Some(0),
                stream_max_retries: Some(0),
                stream_idle_timeout_ms: Some(1000),
                requires_openai_auth: false,
                openrouter: None,
            };

            let out = run_sse(evs, provider).await;
            assert_eq!(out.len(), case.expected_len, "case {}", case.name);
            assert!(
                (case.expect_first)(&out[0]),
                "first event mismatch in case {}",
                case.name
            );
        }
    }

    fn fixed_now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2025, 11, 7, 12, 0, 0).unwrap()
    }

    #[test]
    fn test_try_parse_retry_after_ms() {
        let now = fixed_now();
        let err = Error {
            r#type: None,
            message: Some("Rate limit reached for gpt-5.1 in organization org- on tokens per min (TPM): Limit 1, Used 1, Requested 19304. Please try again in 28ms. Visit https://platform.openai.com/account/rate-limits to learn more.".to_string()),
            code: Some("rate_limit_exceeded".to_string()),
            param: None,
            plan_type: None,
            resets_in_seconds: None,
        };

        let retry_after = try_parse_retry_after(&err, now).expect("retry");
        assert_eq!(retry_after.delay, Duration::from_millis(28));
        assert!(retry_after.resume_at >= now);
    }

    #[test]
    fn test_try_parse_retry_after_seconds() {
        let now = fixed_now();
        let err = Error {
            r#type: None,
            message: Some("Rate limit reached for gpt-5.1 in organization <ORG> on tokens per min (TPM): Limit 30000, Used 6899, Requested 24050. Please try again in 1.898s. Visit https://platform.openai.com/account/rate-limits to learn more.".to_string()),
            code: Some("rate_limit_exceeded".to_string()),
            param: None,
            plan_type: None,
            resets_in_seconds: None,
        };
        let retry_after = try_parse_retry_after(&err, now).expect("retry");
        assert_eq!(retry_after.delay, Duration::from_secs_f64(1.898));
    }

    #[test]
    fn test_try_parse_retry_after_azure() {
        let now = fixed_now();
        let err = Error {
            r#type: None,
            message: Some("Rate limit exceeded. Retry after 35 seconds.".to_string()),
            code: Some("rate_limit_exceeded".to_string()),
            param: None,
            plan_type: None,
            resets_in_seconds: None,
        };
        let retry_after = try_parse_retry_after(&err, now).expect("retry");
        assert_eq!(retry_after.delay, Duration::from_secs(35));
    }

    #[test]
    fn test_try_parse_retry_after_none_when_missing() {
        let now = fixed_now();
        let err = Error {
            r#type: None,
            message: Some("Some other error".to_string()),
            code: None,
            param: None,
            plan_type: None,
            resets_in_seconds: None,
        };

        assert!(try_parse_retry_after(&err, now).is_none());
    }

    #[test]
    fn parse_retry_after_header_parses_seconds() {
        let now = fixed_now();
        let retry = parse_retry_after_header("42", now).expect("header");
        assert_eq!(retry.delay, Duration::from_secs(42));
        assert_eq!(retry.resume_at, now + ChronoDuration::seconds(42));
    }

    #[test]
    fn parse_retry_after_header_parses_rfc7231_date() {
        let now = Utc.with_ymd_and_hms(1994, 11, 15, 8, 0, 0).unwrap();
        let retry = parse_retry_after_header("Tue, 15 Nov 1994 08:12:31 GMT", now).expect("header");
        assert_eq!(
            retry.resume_at,
            Utc.with_ymd_and_hms(1994, 11, 15, 8, 12, 31).unwrap()
        );
    }

    #[test]
    fn parse_retry_after_header_clamps_past_date() {
        let now = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
        let retry = parse_retry_after_header("Tue, 15 Nov 1994 08:12:31 GMT", now).expect("header");
        assert_eq!(retry.delay, Duration::ZERO);
        assert_eq!(retry.resume_at, now);
    }

    #[test]
    fn parse_retry_after_header_strips_wrappers() {
        let now = fixed_now();
        let retry = parse_retry_after_header(" \"17\" ", now).expect("header");
        assert_eq!(retry.delay, Duration::from_secs(17));
    }

    #[test]
    fn retry_after_prefers_header_over_body_hint() {
        let now = fixed_now();
        let header_retry = parse_retry_after_header("5", now);
        let mut chosen = header_retry.clone();
        if chosen.is_none() {
            let err = Error {
                r#type: None,
                message: Some(
                    "Rate limit reached for gpt-5.1. Please try again in 30 seconds.".to_string(),
                ),
                code: Some("rate_limit_exceeded".to_string()),
                param: None,
                plan_type: None,
                resets_in_seconds: None,
            };
            chosen = try_parse_retry_after(&err, now);
        }
        let retry = chosen.expect("retry");
        assert_eq!(retry.delay, Duration::from_secs(5));
    }

    #[test]
    fn parse_retry_after_header_handles_timezones() {
        let now = Utc.with_ymd_and_hms(2025, 3, 9, 5, 0, 0).unwrap();
        let retry = parse_retry_after_header("Sun, 09 Mar 2025 01:30:00 -0500", now).expect("header");
        assert_eq!(
            retry.resume_at,
            Utc.with_ymd_and_hms(2025, 3, 9, 6, 30, 0).unwrap()
        );
    }

    #[test]
    fn quota_error_detected_for_common_statuses() {
        let error = Error {
            r#type: Some("invalid_request_error".to_string()),
            message: Some("You exceeded your current quota".to_string()),
            code: Some("insufficient_quota".to_string()),
            param: None,
            plan_type: None,
            resets_in_seconds: None,
        };

        for status in [
            StatusCode::BAD_REQUEST,
            StatusCode::FORBIDDEN,
            StatusCode::TOO_MANY_REQUESTS,
        ] {
            assert!(is_quota_exceeded_http_error(status, &error), "status {status} should be fatal");
        }

        assert!(
            !is_quota_exceeded_http_error(StatusCode::INTERNAL_SERVER_ERROR, &error),
            "server errors should not map to quota handling"
        );
    }

    #[test]
    fn malformed_quota_body_is_ignored() {
        let error = Error {
            r#type: Some("invalid_request_error".to_string()),
            message: Some("missing code".to_string()),
            code: None,
            param: None,
            plan_type: None,
            resets_in_seconds: None,
        };

        assert!(!is_quota_exceeded_http_error(StatusCode::BAD_REQUEST, &error));
    }

    #[test]
    fn reasoning_summary_rejection_is_detected() {
        let error_with_param = Error {
            r#type: Some("invalid_request_error".to_string()),
            message: Some("Your organization must be verified to generate reasoning summaries.".to_string()),
            code: Some("unsupported_value".to_string()),
            param: Some("reasoning.summary".to_string()),
            plan_type: None,
            resets_in_seconds: None,
        };

        assert!(is_reasoning_summary_rejected(&error_with_param));

        let error_by_message = Error {
            r#type: Some("invalid_request_error".to_string()),
            message: Some("Your organization must be verified to generate reasoning summaries. If you just verified, it can take up to 15 minutes for access to propagate.".to_string()),
            code: Some("unsupported_value".to_string()),
            param: None,
            plan_type: None,
            resets_in_seconds: None,
        };

        assert!(is_reasoning_summary_rejected(&error_by_message));

        // An error with param="reasoning.summary" but a different error code
        // (e.g., rate_limit_exceeded) should NOT be treated as a rejection.
        let rate_limit_error = Error {
            r#type: Some("rate_limit_error".to_string()),
            message: Some("Rate limit reached for reasoning.summary requests.".to_string()),
            code: Some("rate_limit_exceeded".to_string()),
            param: Some("reasoning.summary".to_string()),
            plan_type: None,
            resets_in_seconds: None,
        };

        assert!(!is_reasoning_summary_rejected(&rate_limit_error));
    }

    #[tokio::test]
    async fn quota_exceeded_error_is_fatal() {
        let raw_error = r#"{"type":"response.failed","sequence_number":3,"response":{"id":"resp_quota","object":"response","created_at":1759771626,"status":"failed","background":false,"error":{"code":"insufficient_quota","message":"You exceeded your current quota, please check your plan and billing details."},"incomplete_details":null}}"#;

        let sse1 = format!("event: response.failed\ndata: {raw_error}\n\n");
        let provider = ModelProviderInfo {
            name: "test".to_string(),
            base_url: Some("https://test.com".to_string()),
            env_key: Some("TEST_API_KEY".to_string()),
            env_key_instructions: None,
            experimental_bearer_token: None,
            wire_api: WireApi::Responses,
            query_params: None,
            http_headers: None,
            env_http_headers: None,
            request_max_retries: Some(0),
            stream_max_retries: Some(0),
            stream_idle_timeout_ms: Some(1000),
            requires_openai_auth: false,
            openrouter: None,
        };

        let events = collect_events(&[sse1.as_bytes()], provider).await;

        assert_eq!(events.len(), 1);
        match &events[0] {
            Err(CodexErr::QuotaExceeded) => {}
            other => panic!("unexpected quota event: {other:?}"),
        }
    }
}
