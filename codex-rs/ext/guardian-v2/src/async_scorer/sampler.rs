//! Builds tool-less risk requests and publishes the first classifier output.
//! Both transports share request identity, retry, cancellation, and output handling.

mod connection_pool;

use connection_pool::ConnectionPool;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

use codex_api::ApiError;
use codex_api::Reasoning;
use codex_api::ReasoningContext;
use codex_api::ResponseEvent;
use codex_api::ResponsesApiRequest;
use codex_api::ResponsesEndpoint;
use codex_api::TransportError;
use codex_extension_api::ExtensionMetrics;
use codex_http_client::HttpClientFactory;
use codex_login::AgentIdentityAuthPolicy;
use codex_login::UnauthorizedRecovery;
use codex_model_provider::SharedModelProvider;
use codex_protocol::ResponseItemId;
use codex_protocol::error::CodexErr;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::openai_models::ReasoningEffort;
use codex_protocol::protocol::SessionSource;
use codex_protocol::protocol::TokenUsage;
use http::StatusCode;
use serde_json::json;
use thiserror::Error;
use tokio::sync::oneshot;
use uuid::Uuid;

pub(crate) const MODEL: &str = "gpt-5.6-luna";
pub(crate) const CLASSIFICATION_TOKEN_USAGE_METRIC: &str =
    "codex.guardian_v2.classification.token_usage";
const MAX_OUTPUT_BYTES: usize = 8 * 1024;
pub(super) const INITIAL_WEBSOCKET_CONNECTIONS: usize = if cfg!(test) { 2 } else { 8 };
const MAX_CONCURRENT_REQUESTS: usize = 16;
const MAX_SAMPLING_RETRIES: usize = 2;
const RESPONSES_LITE_METADATA_KEY: &str =
    "ws_request_header_x_openai_internal_codex_responses_lite";
const TURN_METADATA_KEY: &str = "x-codex-turn-metadata";

/// Host-owned provider, authentication, and attribution for one Luna connection.
pub struct LunaSamplerConfig {
    /// Provider and credentials selected for the owning thread.
    pub provider: SharedModelProvider,
    /// Effective proxy, custom-CA, and cookie configuration.
    pub http_client_factory: HttpClientFactory,
    /// Agent-identity policy selected for the owning thread.
    pub agent_identity_policy: AgentIdentityAuthPolicy,
    /// Host-resolved source used to scope agent-identity authentication.
    pub session_source: SessionSource,
    /// Owning runtime session identifier.
    pub session_id: String,
    /// Owning thread identifier.
    pub thread_id: String,
    /// Optional host-resolved request originator.
    pub originator: Option<String>,
    /// Whether this thread may use the unmetered Guardian classifier endpoint.
    pub free_guardian: bool,
    /// Optional inference service tier.
    pub service_tier: Option<String>,
    /// Luna model's host-resolved encrypted-compaction compatibility hash.
    pub luna_compaction_hash: Option<String>,
    /// Complete input allowance resolved for the classifier model.
    pub max_input_tokens: usize,
    /// Host-provided metrics capability with the owning session's attribution.
    pub metrics: Option<Arc<dyn ExtensionMetrics>>,
}

/// One tool-less Luna classification request.
pub struct LunaSamplingRequest {
    /// ID of the response handling the classified tool.
    pub parent_response_id: Option<String>,
    /// Trusted instructions describing the requested classification.
    pub instructions: String,
    /// Composed evidence messages, with roles, annotations and content order intact.
    pub input: Vec<ResponseItem>,
    /// Opaque parent compaction to reuse only for compatible model configurations.
    pub parent_compaction: Option<ResponseItem>,
    /// Host-selected compatibility hash for the supplied parent checkpoint.
    pub parent_compaction_hash: Option<String>,
    /// Reasoning budget explicitly selected for this request.
    pub reasoning_effort: ReasoningEffort,
    /// Owning turn that initiated this classification, not the classifier turn.
    pub parent_turn_id: String,
    /// Trusted causal root of the owning turn, absent when unknown or ambiguous.
    pub root_turn_id: Option<String>,
}

/// Failures returned while connecting or sampling the Luna model.
#[derive(Debug, Error)]
pub enum LunaSamplerError {
    /// The thread's provider or scoped credentials could not be resolved.
    #[error("could not resolve the Luna model provider: {0}")]
    Provider(#[source] CodexErr),
    /// The Responses request could not be opened or streamed.
    #[error("Luna Responses request failed: {0}")]
    Api(#[source] ApiError),
    /// The provider's WebSocket connect deadline elapsed.
    #[error("Luna Responses WebSocket connection timed out")]
    ConnectionTimeout,
    /// The response did not contain an assistant text value.
    #[error("Luna response did not contain assistant output")]
    MissingOutput,
    /// The response exceeded the bounded output limit.
    #[error("Luna response exceeded the output limit")]
    OutputTooLarge,
    /// A newer classification replaced this request when the pool was full.
    #[error("Luna request was superseded by a newer classification")]
    Superseded,
    /// The supplied parent checkpoint cannot be consumed by this Luna configuration.
    #[error("parent compaction is incompatible with Luna")]
    IncompatibleCompaction,
    /// The complete classifier input exceeded the model allowance.
    #[error("Luna input exceeds the complete request budget")]
    InputTooLarge,
}

struct ActiveRequest {
    supersede: oneshot::Sender<()>,
    scored: Arc<AtomicBool>,
}

fn record_token_usage(metrics: Option<&dyn ExtensionMetrics>, token_usage: Option<&TokenUsage>) {
    let (Some(metrics), Some(token_usage)) = (metrics, token_usage) else {
        return;
    };

    for (token_type, value) in [
        ("total", token_usage.total_tokens.max(0)),
        ("input", token_usage.input_tokens.max(0)),
        ("cached_input", token_usage.cached_input()),
        (
            "cache_write_input",
            token_usage.cache_write_input_tokens.max(0),
        ),
        ("non_cached_input", token_usage.non_cached_input()),
        ("output", token_usage.output_tokens.max(0)),
        (
            "reasoning_output",
            token_usage.reasoning_output_tokens.max(0),
        ),
    ] {
        metrics.histogram(
            CLASSIFICATION_TOKEN_USAGE_METRIC,
            value,
            &[("token_type", token_type)],
        );
    }
}

/// Runs bounded Luna classifications over pooled WebSockets or HTTP.
pub struct LunaSampler {
    config: Arc<LunaSamplerConfig>,
    connections: Arc<ConnectionPool>,
    active_requests: Mutex<VecDeque<ActiveRequest>>,
}

impl LunaSampler {
    /// A checkpoint is reusable only when both models declare the same nonempty hash.
    pub(super) fn supports_parent_compaction(&self, parent_hash: Option<&str>) -> bool {
        parent_hash
            .zip(self.config.luna_compaction_hash.as_deref())
            .is_some_and(|(parent_hash, luna_hash)| {
                !parent_hash.is_empty() && parent_hash == luna_hash
            })
    }

    pub(super) fn new(config: LunaSamplerConfig) -> Self {
        let config = Arc::new(config);
        Self {
            connections: ConnectionPool::new(Arc::clone(&config)),
            config,
            active_requests: Mutex::new(VecDeque::with_capacity(MAX_CONCURRENT_REQUESTS)),
        }
    }

    pub(super) async fn prewarm(&self) {
        if let Some(refill) = self.connections.replenish() {
            let _ = refill.await;
        }
    }

    async fn retry_after_failure(
        &self,
        error: &LunaSamplerError,
        auth_recovery: &mut Option<UnauthorizedRecovery>,
        retries: &mut usize,
    ) -> bool {
        let retryable = match error {
            LunaSamplerError::ConnectionTimeout
            | LunaSamplerError::Api(
                ApiError::Retryable { .. }
                | ApiError::RateLimitExceeded { .. }
                | ApiError::Stream(_)
                | ApiError::ServerOverloaded,
            )
            | LunaSamplerError::Api(ApiError::Transport(
                TransportError::RetryLimit
                | TransportError::Timeout
                | TransportError::Connection(_)
                | TransportError::Network(_),
            )) => true,
            LunaSamplerError::Api(ApiError::Transport(TransportError::Http { status, .. }))
            | LunaSamplerError::Api(ApiError::Api { status, .. }) => {
                if *status == StatusCode::UNAUTHORIZED {
                    let Some(recovery) = auth_recovery.as_mut() else {
                        return false;
                    };
                    if !recovery.has_next() || recovery.next().await.is_err() {
                        return false;
                    }
                    self.connections.clear();
                    return true;
                } else {
                    status.is_server_error() || *status == StatusCode::TOO_MANY_REQUESTS
                }
            }
            LunaSamplerError::Provider(_)
            | LunaSamplerError::MissingOutput
            | LunaSamplerError::OutputTooLarge
            | LunaSamplerError::Superseded
            | LunaSamplerError::IncompatibleCompaction
            | LunaSamplerError::InputTooLarge
            | LunaSamplerError::Api(
                ApiError::Transport(TransportError::Build(_))
                | ApiError::ContextWindowExceeded
                | ApiError::QuotaExceeded
                | ApiError::UsageNotIncluded
                | ApiError::RateLimit(_)
                | ApiError::InvalidRequest { .. }
                | ApiError::MisalignmentPolicyViolation { .. }
                | ApiError::CyberPolicy { .. },
            ) => false,
        };
        if retryable && *retries < MAX_SAMPLING_RETRIES {
            *retries += 1;
            return true;
        }
        false
    }

    /// Sends one tool-less classification request using an available transport.
    pub async fn sample(&self, request: LunaSamplingRequest) -> Result<String, LunaSamplerError> {
        if request.parent_compaction.is_some()
            && !self.supports_parent_compaction(request.parent_compaction_hash.as_deref())
        {
            return Err(LunaSamplerError::IncompatibleCompaction);
        }
        // A classification is its own inference turn; retries keep that identity.
        let turn_id = Uuid::now_v7().to_string();
        let parent_response_id = request.parent_response_id;
        let parent_turn_id = request.parent_turn_id;
        let root_turn_id = request.root_turn_id;
        let mut input = vec![
            ResponseItem::AdditionalTools {
                id: None,
                role: "developer".to_owned(),
                tools: Vec::new(),
            },
            ResponseItem::Message {
                id: None,
                role: "developer".to_owned(),
                content: vec![ContentItem::InputText {
                    text: request.instructions,
                }],
                phase: None,
                internal_chat_message_metadata_passthrough: None,
            },
        ];
        if let Some(parent_compaction) = request.parent_compaction {
            input.push(parent_compaction);
        }
        let mut evidence = request.input;
        for item in &mut evidence {
            if let ResponseItem::Message { content, .. } = item {
                for content in content {
                    if let ContentItem::InputImage { detail, .. } = content {
                        *detail = None;
                    }
                }
            }
        }
        input.extend(evidence);
        // Assign IDs once so retries reuse the same input item identities.
        for item in &mut input {
            if item.id().is_none()
                && let Some(prefix) = item.id_prefix()
            {
                item.set_id(Some(ResponseItemId::new(prefix)));
            }
        }
        let total_tokens = input
            .iter()
            .map(codex_guardian_context::estimate_input_tokens)
            .fold(0usize, usize::saturating_add);
        if let Some(metrics) = self.config.metrics.as_deref() {
            for (component, tokens) in [
                ("existing_context", 0),
                ("new_input", total_tokens),
                ("total", total_tokens),
            ] {
                metrics.histogram_with_boundaries(
                    codex_guardian_context::REQUEST_TOKENS_METRIC,
                    i64::try_from(tokens).unwrap_or(i64::MAX),
                    codex_guardian_context::REQUEST_TOKENS_BOUNDARIES,
                    &[("target", "async"), ("component", component)],
                );
            }
        }
        // Oversized classifications defer to sync with the existing failure score.
        if total_tokens > self.config.max_input_tokens.saturating_sub(/*rhs*/ 256) {
            return Err(LunaSamplerError::InputTooLarge);
        }
        let mut request = ResponsesApiRequest {
            model: MODEL.to_owned(),
            instructions: String::new(),
            input,
            tools: None,
            tool_choice: "none".to_owned(),
            parallel_tool_calls: false,
            reasoning: Some(Reasoning {
                effort: Some(request.reasoning_effort),
                summary: None,
                context: Some(ReasoningContext::AllTurns),
            }),
            store: false,
            stream: true,
            stream_options: None,
            include: Vec::new(),
            service_tier: None,
            prompt_cache_key: Some(format!("guardian-v2:{}", self.config.thread_id)),
            text: None,
            client_metadata: None,
            access_programs: None,
        };
        let (supersede, mut superseded) = oneshot::channel();
        let scored = Arc::new(AtomicBool::new(false));
        {
            let mut active_requests = self
                .active_requests
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            active_requests.retain(|request| !request.supersede.is_closed());
            if active_requests.len() == MAX_CONCURRENT_REQUESTS {
                let oldest_scored = active_requests
                    .iter()
                    .position(|request| request.scored.load(Ordering::Relaxed))
                    .unwrap_or(0);
                if let Some(oldest) = active_requests.remove(oldest_scored) {
                    let _ = oldest.supersede.send(());
                }
            }
            active_requests.push_back(ActiveRequest {
                supersede,
                scored: Arc::clone(&scored),
            });
        }
        let mut retries = 0;
        let mut auth_recovery = self
            .config
            .provider
            .auth_manager()
            .map(|manager| manager.unauthorized_recovery());
        'retry: loop {
            let lease = match tokio::select! {
                biased;
                _ = &mut superseded => return Err(LunaSamplerError::Superseded),
                lease = self.connections.lease() => lease,
            } {
                Ok(lease) => lease,
                Err(error) => {
                    if self
                        .retry_after_failure(&error, &mut auth_recovery, &mut retries)
                        .await
                    {
                        continue;
                    }
                    return Err(error);
                }
            };
            request.service_tier = if lease.endpoint == ResponsesEndpoint::GuardianClassifier {
                None
            } else {
                self.config.service_tier.clone()
            };
            let thread_id = &lease.thread_id;
            let mut turn_metadata = json!({
                "session_id": self.config.session_id,
                "thread_id": thread_id,
                "guardian_classifier_source_thread_id": self.config.thread_id,
                "turn_id": turn_id,
                "parent_turn_id": parent_turn_id,
                "thread_source": "guardian_classifier",
                "turn_trigger": "guardian_classifier",
            });
            let mut client_metadata = HashMap::from([
                ("session_id".to_owned(), self.config.session_id.clone()),
                ("thread_id".to_owned(), thread_id.clone()),
                ("turn_id".to_owned(), turn_id.clone()),
                ("parent_turn_id".to_owned(), parent_turn_id.clone()),
                ("x-openai-subagent".to_owned(), "guardian".to_owned()),
                // Classifier requests do not advance their own context window.
                ("x-codex-window-id".to_owned(), format!("{thread_id}:0")),
                (RESPONSES_LITE_METADATA_KEY.to_owned(), "true".to_owned()),
            ]);
            if let Some(root_turn_id) = &root_turn_id {
                client_metadata.insert("root_turn_id".to_owned(), root_turn_id.clone());
                turn_metadata["root_turn_id"] = json!(root_turn_id);
            }
            client_metadata.insert(TURN_METADATA_KEY.to_owned(), turn_metadata.to_string());
            if lease.endpoint == ResponsesEndpoint::GuardianClassifier
                && let Some(parent_response_id) = &parent_response_id
            {
                client_metadata.insert("parent_response_id".to_owned(), parent_response_id.clone());
            }
            request.client_metadata = Some(client_metadata);
            let mut stream = match tokio::select! {
                biased;
                _ = &mut superseded => return Err(LunaSamplerError::Superseded),
                stream = lease.stream_request(&request) => stream,
            } {
                Ok(stream) => stream,
                Err(error) => {
                    let error = LunaSamplerError::Api(error);
                    if self
                        .retry_after_failure(&error, &mut auth_recovery, &mut retries)
                        .await
                    {
                        continue;
                    }
                    return Err(error);
                }
            };

            let mut output = String::new();
            while let Some(event) = tokio::select! {
                biased;
                _ = &mut superseded => {
                    return if scored.load(Ordering::Relaxed) && !output.is_empty() {
                        Ok(output)
                    } else {
                        Err(LunaSamplerError::Superseded)
                    };
                }
                event = stream.rx_event.recv() => event,
            } {
                let event = match event {
                    Ok(event) => event,
                    Err(error) => {
                        let error = LunaSamplerError::Api(error);
                        if self
                            .retry_after_failure(&error, &mut auth_recovery, &mut retries)
                            .await
                        {
                            continue 'retry;
                        }
                        return Err(error);
                    }
                };
                match event {
                    ResponseEvent::OutputTextDelta(delta) => {
                        if delta.is_empty() {
                            continue;
                        }
                        if delta.len() > MAX_OUTPUT_BYTES {
                            return Err(LunaSamplerError::OutputTooLarge);
                        }
                        // The first output token is the complete classification.
                        // Later output cannot revise that decision; drain it only
                        // to preserve connection reuse and token accounting.
                        scored.store(true, Ordering::Relaxed);
                        let mut remaining_events = stream.rx_event;
                        let metrics = self.config.metrics.clone();
                        tokio::spawn(async move {
                            while let Some(event) = tokio::select! {
                                biased;
                                _ = &mut superseded => None,
                                event = remaining_events.recv() => event,
                            } {
                                match event {
                                    Ok(ResponseEvent::Completed { token_usage, .. }) => {
                                        record_token_usage(
                                            metrics.as_deref(),
                                            token_usage.as_ref(),
                                        );
                                        lease.reuse();
                                        break;
                                    }
                                    Err(_) => break,
                                    _ => {}
                                }
                            }
                        });
                        return Ok(delta);
                    }
                    ResponseEvent::OutputItemDone(ResponseItem::Message {
                        role, content, ..
                    }) if role == "assistant" => {
                        for item in content {
                            if let ContentItem::OutputText { text } = item {
                                output.push_str(&text);
                            }
                        }
                    }
                    ResponseEvent::Completed { token_usage, .. } => {
                        record_token_usage(self.config.metrics.as_deref(), token_usage.as_ref());
                        lease.reuse();
                        if !output.is_empty() {
                            return Ok(output);
                        }
                        return Err(LunaSamplerError::MissingOutput);
                    }
                    _ => {}
                }
                if output.len() > MAX_OUTPUT_BYTES {
                    return Err(LunaSamplerError::OutputTooLarge);
                }
                if !output.is_empty() {
                    scored.store(true, Ordering::Relaxed);
                }
            }
            return Err(LunaSamplerError::MissingOutput);
        }
    }
}

#[cfg(test)]
#[path = "sampler_tests.rs"]
pub(super) mod tests;
