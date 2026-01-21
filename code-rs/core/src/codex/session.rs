use super::*;
use serde_json::Value;
use super::streaming::{
    AgentTask,
    TRUNCATION_MARKER,
    TimelineReplayContext,
    debug_history,
    ensure_user_dir,
    parse_env_delta_from_response,
    parse_env_snapshot_from_response,
    process_rollout_env_item,
    truncate_middle_bytes,
    write_agent_file,
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ApprovedCommandPattern {
    argv: Vec<String>,
    kind: ApprovedCommandMatchKind,
    semantic_prefix: Option<Vec<String>>,
}

impl ApprovedCommandPattern {
    pub(crate) fn new(
        argv: Vec<String>,
        kind: ApprovedCommandMatchKind,
        semantic_prefix: Option<Vec<String>>,
    ) -> Self {
        let semantic_prefix = if matches!(kind, ApprovedCommandMatchKind::Prefix) {
            semantic_prefix.or_else(|| Some(argv.clone()))
        } else {
            None
        };
        Self {
            argv,
            kind,
            semantic_prefix,
        }
    }

    pub(crate) fn matches(&self, command: &[String]) -> bool {
        match self.kind {
            ApprovedCommandMatchKind::Exact => command == self.argv.as_slice(),
            ApprovedCommandMatchKind::Prefix => {
                if command.starts_with(&self.argv) {
                    return true;
                }
                if let (Some(pattern), Some(candidate)) = (
                    self.semantic_prefix.as_ref(),
                    semantic_tokens(command),
                ) {
                    return candidate.starts_with(pattern);
                }
                false
            }
        }
    }

    pub fn argv(&self) -> &[String] { &self.argv }

    pub fn kind(&self) -> ApprovedCommandMatchKind { self.kind }
}

fn semantic_tokens(command: &[String]) -> Option<Vec<String>> {
    if command.is_empty() {
        return None;
    }
    if let Some(tokens) = shell_script_tokens(command) {
        return Some(tokens);
    }
    Some(command.to_vec())
}

fn shell_script_tokens(command: &[String]) -> Option<Vec<String>> {
    if command.len() == 3 && is_shell_wrapper(&command[0], &command[1]) {
        if let Some(tokens) = shlex_split(&command[2]) {
            return Some(tokens);
        }
        return Some(vec![command[2].clone()]);
    }
    None
}

fn is_shell_wrapper(shell: &str, flag: &str) -> bool {
    let file_name = Path::new(shell)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(shell)
        .to_ascii_lowercase();
    matches!(
        file_name.as_str(),
        "bash" | "sh" | "zsh" | "ksh" | "fish" | "dash"
    ) && matches!(flag, "-lc" | "-c")
}

#[derive(Clone)]
pub(super) struct RunningExecMeta {
    pub(super) sub_id: String,
    pub(super) order_meta: crate::protocol::OrderMeta,
    pub(super) cancel_flag: Arc<AtomicBool>,
    pub(super) end_emitted: Arc<AtomicBool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WaitInterruptReason {
    UserMessage,
    SessionAborted,
}

#[derive(Clone, Default)]
pub(super) struct EnvironmentContextStreamRegistry {
    env_stream_id: Option<String>,
    browser_stream_id: Option<String>,
}

impl EnvironmentContextStreamRegistry {
    pub(super) fn env_stream_id(&mut self, session_id: Uuid) -> String {
        self.env_stream_id
            .get_or_insert_with(|| format!("env-context-{}", session_id))
            .clone()
    }

    pub(super) fn browser_stream_id(&mut self, session_id: Uuid) -> String {
        self.browser_stream_id
            .get_or_insert_with(|| format!("browser-context-{}", session_id))
            .clone()
    }
}

#[derive(Default)]
pub(super) struct State {
    pub(super) approved_commands: HashSet<ApprovedCommandPattern>,
    pub(super) current_task: Option<AgentTask>,
    pub(super) pending_approvals: HashMap<String, oneshot::Sender<ReviewDecision>>,
    pub(super) pending_tool_user_input: VecDeque<oneshot::Sender<Vec<InputItem>>>,
    pub(super) pending_input: Vec<ResponseInputItem>,
    pub(super) pending_user_input: Vec<QueuedUserInput>,
    pub(super) history: ConversationHistory,
    /// Tracks which completed agents (by id) have already been returned to the
    /// model for a given batch when using `agent` with `action="wait"` and
    /// `return_all=false`.
    /// This enables sequential waiting behavior across multiple calls.
    pub(super) seen_completed_agents_by_batch: HashMap<String, HashSet<String>>,
    /// Tracks agent batches that already triggered a wake-up after completion.
    pub(super) agent_completion_wake_batches: HashSet<String>,
    /// Scratchpad that buffers streamed items/deltas for the current HTTP attempt
    /// so we can seed retries without losing progress.
    pub(super) turn_scratchpad: Option<TurnScratchpad>,
    /// Per-submission monotonic event sequence (resets at TaskStarted)
    pub(super) event_seq_by_sub_id: HashMap<String, u64>,
    /// Per-submission sequence used when synthesizing background OrderMeta.
    pub(super) background_seq_by_sub_id: HashMap<String, u64>,
    /// 1-based ordinal of the current HTTP request attempt in this session.
    pub(super) request_ordinal: u64,
    pub(super) dry_run_guard: DryRunGuardState,
    /// Background execs by call_id
    pub(super) background_execs: std::collections::HashMap<String, BackgroundExecState>,
    /// Active foreground exec calls keyed by call_id (ExecCommandBegin/End lifecycle)
    pub(super) running_execs: HashMap<String, RunningExecMeta>,
    pub(super) next_internal_sub_id: u64,
    pub(super) token_usage_info: Option<TokenUsageInfo>,
    pub(super) latest_rate_limits: Option<RateLimitSnapshotEvent>,
    pub(super) pending_manual_compacts: VecDeque<String>,
    pub(super) wait_interrupt_epoch: u64,
    pub(super) wait_interrupt_reason: Option<WaitInterruptReason>,
    pub(super) context_timeline: ContextTimeline,
    pub(super) environment_context_tracker: EnvironmentContextTracker,
    pub(super) environment_context_seq: u64,
    pub(super) last_environment_snapshot: Option<EnvironmentContextSnapshot>,
    pub(super) context_stream_ids: EnvironmentContextStreamRegistry,
    pub(super) last_turn_started_at: Option<Instant>,
    pub(super) last_turn_completed_at: Option<Instant>,
    pub(super) last_turn_prompt_counts: Option<TurnPromptCounts>,
}

#[derive(Clone, Copy, Default)]
pub(super) struct TurnPromptCounts {
    pub(super) input_items: usize,
    pub(super) status_items: usize,
}

#[derive(Clone, Copy, Default)]
pub(super) struct TurnQueueMetrics {
    pub(super) pending_input_count: usize,
    pub(super) pending_user_input_count: usize,
    pub(super) pending_background_execs: usize,
    pub(super) running_exec_count: usize,
    pub(super) pending_manual_compacts: usize,
    pub(super) scratchpad_active: bool,
}

pub(super) fn capture_turn_queue_metrics(state: &State) -> TurnQueueMetrics {
    TurnQueueMetrics {
        pending_input_count: state.pending_input.len(),
        pending_user_input_count: state.pending_user_input.len(),
        pending_background_execs: state.background_execs.len(),
        running_exec_count: state.running_execs.len(),
        pending_manual_compacts: state.pending_manual_compacts.len(),
        scratchpad_active: state.turn_scratchpad.is_some(),
    }
}

pub(super) fn duration_to_millis(duration: Duration) -> u64 {
    let ms = duration.as_millis();
    if ms > u128::from(u64::MAX) {
        u64::MAX
    } else {
        ms as u64
    }
}

#[derive(Clone)]
pub(crate) struct QueuedUserInput {
    pub(super) submission_id: String,
    pub(super) response_item: ResponseInputItem,
    pub(super) core_items: Vec<InputItem>,
}

/// Buffers partial turn progress produced during a single HTTP streaming attempt.
/// This is not recorded to persistent history. It is only used to seed retries
/// when the SSE stream disconnects mid‑turn.
#[derive(Default, Clone, Debug)]
pub(super) struct TurnScratchpad {
    /// Output items that reached `response.output_item.done` during this attempt
    pub(super) items: Vec<ResponseItem>,
    /// Tool outputs we produced locally in reaction to output items
    pub(super) responses: Vec<ResponseInputItem>,
    /// Last assistant text fragment received via deltas (not yet finalized)
    pub(super) partial_assistant_text: String,
    /// Last reasoning summary fragment received via deltas (not yet finalized)
    pub(super) partial_reasoning_summary: String,
}

#[derive(Clone)]
pub(super) struct AccountUsageContext {
    pub(super) code_home: PathBuf,
    pub(super) account_id: String,
    pub(super) plan: Option<String>,
}

pub(super) fn account_usage_context(sess: &Session) -> Option<AccountUsageContext> {
    let code_home = sess.client.code_home().to_path_buf();
    let account_id = auth_accounts::get_active_account_id(&code_home).ok().flatten()?;
    let plan = auth_accounts::find_account(&code_home, &account_id)
        .ok()
        .flatten()
        .and_then(|account| {
            account
                .tokens
                .as_ref()
                .and_then(|tokens| tokens.id_token.get_chatgpt_plan_type())
        });
    Some(AccountUsageContext {
        code_home,
        account_id,
        plan,
    })
}

pub(super) fn spawn_usage_task<F>(task: F)
where
    F: FnOnce() + Send + 'static,
{
    let _ = tokio::task::spawn_blocking(task);
}

pub(super) fn format_retry_eta(retry_after: &RetryAfter) -> Option<String> {
    let resume_at = retry_after.resume_at;
    let local = resume_at.with_timezone(&Local);
    let now = Local::now();
    let formatted = if local.date_naive() == now.date_naive() {
        local.format("%-I:%M %p %Z").to_string()
    } else {
        local.format("%b %-d, %Y %-I:%M %p %Z").to_string()
    };
    Some(formatted)
}

pub(super) fn is_connectivity_error(err: &CodexErr) -> bool {
    match err {
        CodexErr::Reqwest(e) => e.is_connect() || e.is_timeout() || e.is_request(),
        CodexErr::Stream(msg, _, _) => {
            let lower = msg.to_ascii_lowercase();
            msg.starts_with("[transport]")
                || lower.contains("network")
                || lower.contains("connection")
                || lower.contains("connectivity")
                || lower.contains("timeout")
                || lower.contains("transport")
        }
        _ => false,
    }
}

#[derive(Debug)]
pub(super) struct BackgroundExecState {
    pub(super) notify: std::sync::Arc<tokio::sync::Notify>,
    pub(super) result_cell: std::sync::Arc<std::sync::Mutex<Option<ExecToolCallOutput>>>,
    pub(super) tail_buf: Option<std::sync::Arc<std::sync::Mutex<Vec<u8>>>>,
    pub(super) cmd_display: String,
    pub(super) suppress_event: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub(super) task_handle: Option<tokio::task::JoinHandle<()>>,
    pub(super) order_meta_for_end: crate::protocol::OrderMeta,
    pub(super) sub_id: String,
}

/// Context for an initialized model agent
///
/// A session has at most 1 running agent at a time, and can be interrupted by user input.
pub(crate) struct Session {
    pub(super) id: Uuid,
    pub(super) client: ModelClient,
    pub(super) remote_models_manager: Option<Arc<RemoteModelsManager>>,
    pub(super) tx_event: Sender<Event>,

    /// The session's current working directory. All relative paths provided by
    /// the model as well as sandbox policies are resolved against this path
    /// instead of `std::env::current_dir()`.
    pub(super) cwd: PathBuf,
    pub(super) base_instructions: Option<String>,
    pub(super) user_instructions: Option<String>,
    pub(super) demo_developer_message: Option<String>,
    pub(super) compact_prompt_override: Option<String>,
    pub(super) approval_policy: AskForApproval,
    pub(super) sandbox_policy: SandboxPolicy,
    pub(super) shell_environment_policy: ShellEnvironmentPolicy,
    pub(super) _writable_roots: Vec<PathBuf>,
    pub(super) disable_response_storage: bool,
    pub(super) tools_config: ToolsConfig,

    /// Manager for external MCP servers/tools.
    pub(super) mcp_connection_manager: McpConnectionManager,
    pub(super) client_tools: Option<ClientTools>,
    #[allow(dead_code)]
    pub(super) session_manager: ExecSessionManager,

    /// Configuration for available agent models
    pub(super) agents: Vec<crate::config_types::AgentConfig>,

    /// Default reasoning effort for spawned agents and model calls in this session
    pub(super) model_reasoning_effort: ReasoningEffortConfig,

    /// External notifier command (will be passed as args to exec()). When
    /// `None` this feature is disabled.
    pub(super) notify: Option<Vec<String>>,

    /// Optional rollout recorder for persisting the conversation transcript so
    /// sessions can be replayed or inspected later.
    pub(super) rollout: Mutex<Option<RolloutRecorder>>,
    pub(super) state: Mutex<State>,
    pub(super) code_linux_sandbox_exe: Option<PathBuf>,
    pub(super) user_shell: shell::Shell,
    pub(super) show_raw_agent_reasoning: bool,
    /// Pending browser screenshots to include in the next model request
    #[allow(dead_code)]
    pub(super) pending_browser_screenshots: Mutex<Vec<PathBuf>>,
    /// Track the last system status to detect changes
    pub(super) last_system_status: Mutex<Option<String>>,
    /// Track the last screenshot path and hash to detect changes
    pub(super) last_screenshot_info: Mutex<Option<(PathBuf, Vec<u8>, Vec<u8>)>>, // (path, phash, dhash)
    pub(super) time_budget: Mutex<Option<RunTimeBudget>>,
    pub(super) confirm_guard: ConfirmGuardRuntime,
    pub(super) project_hooks: ProjectHooks,
    pub(super) project_commands: Vec<ProjectCommand>,
    pub(super) tool_output_max_bytes: usize,
    pub(super) hook_guard: AtomicBool,
    pub(super) github: Arc<RwLock<crate::config_types::GithubConfig>>,
    pub(super) validation: Arc<RwLock<crate::config_types::ValidationConfig>>,
    pub(super) self_handle: Weak<Session>,
    pub(super) active_review: Mutex<Option<ReviewRequest>>,
    pub(super) next_turn_text_format: Mutex<Option<TextFormat>>,
    pub(super) env_ctx_v2: bool,
    pub(super) retention_config: crate::config_types::RetentionConfig,
    pub(super) model_descriptions: Option<String>,
}
pub(super) struct HookGuard<'a> {
    flag: &'a AtomicBool,
}

impl<'a> HookGuard<'a> {
    pub(super) fn try_acquire(flag: &'a AtomicBool) -> Option<Self> {
        flag
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()
            .map(|_| Self { flag })
    }
}

impl Drop for HookGuard<'_> {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::SeqCst);
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ToolCallCtx {
    pub sub_id: String,
    pub call_id: String,
    pub seq_hint: Option<u64>,
    pub output_index: Option<u32>,
}

impl ToolCallCtx {
    pub fn new(sub_id: String, call_id: String, seq_hint: Option<u64>, output_index: Option<u32>) -> Self {
        Self { sub_id, call_id, seq_hint, output_index }
    }

    pub fn order_meta(&self, req_ordinal: u64) -> crate::protocol::OrderMeta {
        crate::protocol::OrderMeta { request_ordinal: req_ordinal, output_index: self.output_index, sequence_number: self.seq_hint }
    }
}

impl Session {
    #[allow(dead_code)]
    pub(crate) fn get_writable_roots(&self) -> &[PathBuf] {
        &self._writable_roots
    }

    pub(crate) fn get_approval_policy(&self) -> AskForApproval {
        self.approval_policy
    }

    fn next_background_sequence(&self, sub_id: &str) -> u64 {
        let mut state = self.state.lock().unwrap();
        let entry = state
            .background_seq_by_sub_id
            .entry(sub_id.to_string())
            .or_insert(0);
        let current = *entry;
        *entry = entry.saturating_add(1);
        current
    }

    pub(crate) fn next_background_order(
        &self,
        sub_id: &str,
        req_ordinal: u64,
        output_index: Option<u32>,
    ) -> crate::protocol::OrderMeta {
        let normalized_req = if req_ordinal == 0 { 1 } else { req_ordinal };
        let sequence = self.next_background_sequence(sub_id);
        let stored_output_index = output_index.unwrap_or(i32::MAX as u32);
        crate::protocol::OrderMeta {
            request_ordinal: normalized_req,
            output_index: Some(stored_output_index),
            sequence_number: Some(sequence),
        }
    }

    pub(crate) fn background_order_for_ctx(
        &self,
        ctx: &ToolCallCtx,
        req_ordinal: u64,
    ) -> crate::protocol::OrderMeta {
        let base_output = ctx.output_index.unwrap_or(i32::MAX as u32);
        self.next_background_order(&ctx.sub_id, req_ordinal, Some(base_output))
    }

    pub(crate) fn get_cwd(&self) -> &Path {
        &self.cwd
    }

    pub(super) async fn apply_remote_model_overrides(&self, prompt: &mut Prompt) {
        let configured_model = self.client.get_model();

        if prompt.model_override.is_none() {
            if !self.client.model_explicit() {
                let auth_mode = self
                    .client
                    .get_auth_manager()
                    .as_ref()
                    .and_then(|mgr| mgr.auth())
                    .map(|auth| auth.mode);

                let default_model_slug = if auth_mode == Some(code_app_server_protocol::AuthMode::ChatGPT) {
                    crate::config::GPT_5_CODEX_MEDIUM_MODEL
                } else {
                    crate::config::OPENAI_DEFAULT_MODEL
                };

                if let Some(remote) = self.remote_models_manager.as_ref()
                    && configured_model.eq_ignore_ascii_case(default_model_slug)
                    && let Some(default_model) = remote.default_model_slug(auth_mode).await
                {
                    prompt.model_override = Some(default_model);
                }
            }

            if prompt.model_override.is_none() {
                prompt.model_override = Some(configured_model.clone());
            }
        }

        if prompt.model_family_override.is_none() {
            let model_slug = prompt
                .model_override
                .as_deref()
                .unwrap_or(configured_model.as_str());
            let base_family = find_family_for_model(model_slug)
                .unwrap_or_else(|| derive_default_model_family(model_slug));

            let family = if let Some(remote) = self.remote_models_manager.as_ref() {
                remote.apply_remote_overrides(model_slug, base_family).await
            } else {
                base_family
            };
            prompt.model_family_override = Some(family);
        }
    }

    pub(crate) async fn record_bridge_event(&self, text: String) {
        let message = ResponseItem::Message {
            id: None,
            role: "developer".to_string(),
            content: vec![ContentItem::InputText { text }],
        };
        self.record_conversation_items(&[message]).await;
    }

    pub(crate) fn get_sandbox_policy(&self) -> &SandboxPolicy {
        &self.sandbox_policy
    }

    pub(crate) fn session_uuid(&self) -> Uuid {
        self.id
    }

    pub(crate) fn get_github_config(&self) -> Arc<RwLock<crate::config_types::GithubConfig>> {
        Arc::clone(&self.github)
    }

    pub(crate) fn validation_config(&self) -> Arc<RwLock<crate::config_types::ValidationConfig>> {
        Arc::clone(&self.validation)
    }

    pub(crate) fn client_tools(&self) -> Option<&ClientTools> {
        self.client_tools.as_ref()
    }

    pub(super) fn set_active_review(&self, review_request: ReviewRequest) {
        let mut guard = self.active_review.lock().unwrap();
        *guard = Some(review_request);
    }

    pub(super) fn take_active_review(&self) -> Option<ReviewRequest> {
        self.active_review.lock().unwrap().take()
    }

    pub(crate) fn mcp_connection_manager(&self) -> &McpConnectionManager {
        &self.mcp_connection_manager
    }

    pub(crate) async fn shutdown_mcp_clients(&self) {
        self.mcp_connection_manager.shutdown_all().await;
    }

    pub(crate) fn update_validation_tool(&self, name: &str, enable: bool) {
        if name == "actionlint" {
            if let Ok(mut github) = self.github.write() {
                github.actionlint_on_patch = enable;
            }
            return;
        }

        if let Ok(mut cfg) = self.validation.write() {
            let tools = &mut cfg.tools;
            match name {
                "shellcheck" => tools.shellcheck = Some(enable),
                "markdownlint" => tools.markdownlint = Some(enable),
                "hadolint" => tools.hadolint = Some(enable),
                "yamllint" => tools.yamllint = Some(enable),
                "cargo-check" => tools.cargo_check = Some(enable),
                "shfmt" => tools.shfmt = Some(enable),
                "prettier" => tools.prettier = Some(enable),
                _ => {}
            }
        }
    }

    pub(crate) fn update_validation_group(&self, group: ValidationGroup, enable: bool) {
        if let Ok(mut cfg) = self.validation.write() {
            match group {
                ValidationGroup::Functional => cfg.groups.functional = enable,
                ValidationGroup::Stylistic => cfg.groups.stylistic = enable,
            }
        }
    }

    pub(super) fn resolve_path(&self, path: Option<String>) -> PathBuf {
        path.as_ref()
            .map(PathBuf::from)
            .map_or_else(|| self.cwd.clone(), |p| self.cwd.join(p))
    }

    pub(crate) async fn maybe_parse_apply_patch_verified(
        &self,
        argv: &[String],
        cwd: &Path,
    ) -> MaybeApplyPatchVerified {
        // Upstream parser no longer needs a filesystem; it is pure and sync.
        let _ = self.client_tools.as_ref();
        code_apply_patch::maybe_parse_apply_patch_verified(argv, cwd)
    }

    // ────────────────────────────
    // Scratchpad helpers
    // ────────────────────────────
    pub(super) fn begin_attempt_scratchpad(&self) {
        let mut state = self.state.lock().unwrap();
        state.turn_scratchpad = Some(TurnScratchpad::default());
    }

    /// Bump the per-session HTTP request attempt ordinal so `OrderMeta`
    /// reflects the correct provider request index for this attempt.
    pub(super) fn begin_http_attempt(&self) {
        let mut state = self.state.lock().unwrap();
        state.request_ordinal = state.request_ordinal.saturating_add(1);
    }

    pub(super) fn turn_latency_request_scheduled(&self, attempt_req: u64, prompt: &Prompt) {
        let now = Instant::now();
        let gap_and_metrics = {
            let mut state = self.state.lock().unwrap();
            let gap = state
                .last_turn_completed_at
                .map(|prev| now.saturating_duration_since(prev));
            state.last_turn_started_at = Some(now);
            state.last_turn_prompt_counts = Some(TurnPromptCounts {
                input_items: prompt.input.len(),
                status_items: prompt.status_items.len(),
            });
            let metrics = capture_turn_queue_metrics(&state);
            (gap, metrics)
        };

        let pending_browser_screenshots = self.pending_browser_screenshots.lock().unwrap().len();
        let (gap, metrics) = gap_and_metrics;
        let payload = TurnLatencyPayload {
            phase: TurnLatencyPhase::RequestScheduled,
            attempt: attempt_req,
            gap_ms: gap.map(duration_to_millis),
            duration_ms: None,
            pending_input_count: metrics.pending_input_count as u64,
            pending_user_input_count: metrics.pending_user_input_count as u64,
            pending_background_execs: metrics.pending_background_execs as u64,
            running_exec_count: metrics.running_exec_count as u64,
            pending_manual_compacts: metrics.pending_manual_compacts as u64,
            pending_browser_screenshots: pending_browser_screenshots as u64,
            scratchpad_active: metrics.scratchpad_active,
            prompt_input_count: Some(prompt.input.len() as u64),
            prompt_status_count: Some(prompt.status_items.len() as u64),
            output_item_count: None,
            token_usage_input_tokens: None,
            token_usage_cached_input_tokens: None,
            token_usage_output_tokens: None,
            token_usage_reasoning_output_tokens: None,
            token_usage_total_tokens: None,
            note: None,
        };
        self.emit_turn_latency(payload);
    }

    pub(super) fn turn_latency_request_completed(
        &self,
        attempt_req: u64,
        output_item_count: usize,
        token_usage: Option<&TokenUsage>,
    ) {
        let now = Instant::now();
        let (duration, prompt_counts, metrics) = {
            let mut state = self.state.lock().unwrap();
            let duration = state
                .last_turn_started_at
                .map(|start| now.saturating_duration_since(start));
            state.last_turn_started_at = None;
            state.last_turn_completed_at = Some(now);
            let prompt_counts = state.last_turn_prompt_counts.take();
            let metrics = capture_turn_queue_metrics(&state);
            (duration, prompt_counts, metrics)
        };

        let pending_browser_screenshots = self.pending_browser_screenshots.lock().unwrap().len();
        let (token_usage_input_tokens, token_usage_cached_input_tokens, token_usage_output_tokens, token_usage_reasoning_output_tokens, token_usage_total_tokens) =
            match token_usage {
                Some(usage) => (
                    Some(usage.input_tokens),
                    Some(usage.cached_input_tokens),
                    Some(usage.output_tokens),
                    Some(usage.reasoning_output_tokens),
                    Some(usage.total_tokens),
                ),
                None => (None, None, None, None, None),
            };
        let payload = TurnLatencyPayload {
            phase: TurnLatencyPhase::RequestCompleted,
            attempt: attempt_req,
            gap_ms: None,
            duration_ms: duration.map(duration_to_millis),
            pending_input_count: metrics.pending_input_count as u64,
            pending_user_input_count: metrics.pending_user_input_count as u64,
            pending_background_execs: metrics.pending_background_execs as u64,
            running_exec_count: metrics.running_exec_count as u64,
            pending_manual_compacts: metrics.pending_manual_compacts as u64,
            pending_browser_screenshots: pending_browser_screenshots as u64,
            scratchpad_active: metrics.scratchpad_active,
            prompt_input_count: prompt_counts.map(|counts| counts.input_items as u64),
            prompt_status_count: prompt_counts.map(|counts| counts.status_items as u64),
            output_item_count: Some(output_item_count as u64),
            token_usage_input_tokens,
            token_usage_cached_input_tokens,
            token_usage_output_tokens,
            token_usage_reasoning_output_tokens,
            token_usage_total_tokens,
            note: None,
        };
        self.emit_turn_latency(payload);
    }

    pub(super) fn turn_latency_request_failed(&self, attempt_req: u64, note: Option<String>) {
        let now = Instant::now();
        let (duration, prompt_counts, metrics) = {
            let mut state = self.state.lock().unwrap();
            let duration = state
                .last_turn_started_at
                .map(|start| now.saturating_duration_since(start));
            state.last_turn_started_at = None;
            let prompt_counts = state.last_turn_prompt_counts.take();
            let metrics = capture_turn_queue_metrics(&state);
            (duration, prompt_counts, metrics)
        };

        let pending_browser_screenshots = self.pending_browser_screenshots.lock().unwrap().len();
        let payload = TurnLatencyPayload {
            phase: TurnLatencyPhase::RequestFailed,
            attempt: attempt_req,
            gap_ms: None,
            duration_ms: duration.map(duration_to_millis),
            pending_input_count: metrics.pending_input_count as u64,
            pending_user_input_count: metrics.pending_user_input_count as u64,
            pending_background_execs: metrics.pending_background_execs as u64,
            running_exec_count: metrics.running_exec_count as u64,
            pending_manual_compacts: metrics.pending_manual_compacts as u64,
            pending_browser_screenshots: pending_browser_screenshots as u64,
            scratchpad_active: metrics.scratchpad_active,
            prompt_input_count: prompt_counts.map(|counts| counts.input_items as u64),
            prompt_status_count: prompt_counts.map(|counts| counts.status_items as u64),
            output_item_count: None,
            token_usage_input_tokens: None,
            token_usage_cached_input_tokens: None,
            token_usage_output_tokens: None,
            token_usage_reasoning_output_tokens: None,
            token_usage_total_tokens: None,
            note,
        };
        self.emit_turn_latency(payload);
    }

    fn emit_turn_latency(&self, payload: TurnLatencyPayload) {
        if let Some(otel) = self.client.get_otel_event_manager() {
            otel.turn_latency_event(payload.clone());
        }
        self.client.log_turn_latency_debug(&payload);
    }

    pub(super) fn scratchpad_push(
        &self,
        item: &ResponseItem,
        response: &Option<ResponseInputItem>,
        sub_id: &str,
    ) {
        let mut state = self.state.lock().unwrap();
        if let Some(sp) = &mut state.turn_scratchpad {
            sp.items.push(item.clone());
            if let Some(r) = response {
                let mut truncated = r.clone();
                self.enforce_user_message_limits(sub_id, &mut truncated);
                sp.responses.push(truncated);
            }
        }
    }

    pub(super) fn scratchpad_add_text_delta(&self, delta: &str) {
        let mut state = self.state.lock().unwrap();
        if let Some(sp) = &mut state.turn_scratchpad {
            sp.partial_assistant_text.push_str(delta);
            // Keep memory bounded (ensure UTF-8 char boundary when trimming)
            if sp.partial_assistant_text.len() > 4000 {
                let mut drain_up_to = sp.partial_assistant_text.len() - 4000;
                while !sp.partial_assistant_text.is_char_boundary(drain_up_to) {
                    drain_up_to -= 1;
                }
                sp.partial_assistant_text.drain(..drain_up_to);
            }
        }
    }

    pub(super) fn scratchpad_add_reasoning_delta(&self, delta: &str) {
        let mut state = self.state.lock().unwrap();
        if let Some(sp) = &mut state.turn_scratchpad {
            sp.partial_reasoning_summary.push_str(delta);
            if sp.partial_reasoning_summary.len() > 4000 {
                let mut drain_up_to = sp.partial_reasoning_summary.len() - 4000;
                while !sp.partial_reasoning_summary.is_char_boundary(drain_up_to) {
                    drain_up_to -= 1;
                }
                sp.partial_reasoning_summary.drain(..drain_up_to);
            }
        }
    }

    pub(super) fn scratchpad_clear_partial_message(&self) {
        let mut state = self.state.lock().unwrap();
        if let Some(sp) = &mut state.turn_scratchpad {
            sp.partial_assistant_text.clear();
        }
    }

    pub(super) fn take_scratchpad(&self) -> Option<TurnScratchpad> {
        let mut state = self.state.lock().unwrap();
        state.turn_scratchpad.take()
    }

    pub(super) fn clear_scratchpad(&self) {
        let mut state = self.state.lock().unwrap();
        state.turn_scratchpad = None;
    }
}
impl Session {
    pub(super) fn set_task(&self, agent: AgentTask) {
        let mut state = self.state.lock().unwrap();
        if let Some(current_task) = state.current_task.take() {
            current_task.abort(TurnAbortReason::Replaced);
        }
        state.current_task = Some(agent);
    }

    pub async fn start_pending_only_turn_if_idle(self: &Arc<Self>) -> bool {
        let should_start = {
            let state = self.state.lock().unwrap();
            state.current_task.is_none()
        };

        if !should_start {
            return false;
        }

        self.cleanup_old_status_items().await;
        let turn_context = self.make_turn_context();
        let sub_id = self.next_internal_sub_id();
        let sentinel_input = vec![InputItem::Text {
            text: PENDING_ONLY_SENTINEL.to_string(),
        }];
        let agent = AgentTask::spawn(Arc::clone(self), turn_context, sub_id, sentinel_input);
        self.set_task(agent);
        true
    }

    pub fn replace_history(&self, items: Vec<ResponseItem>) {
        let mut state = self.state.lock().unwrap();
        state.history.replace(items);
    }

    pub fn remove_task(&self, sub_id: &str) {
        let mut state = self.state.lock().unwrap();
        if let Some(agent) = &state.current_task {
            if agent.sub_id == sub_id {
                state.current_task.take();
            }
        }
    }

    pub fn has_running_task(&self) -> bool {
        self.state.lock().unwrap().current_task.is_some()
    }

    pub fn queue_user_input(&self, queued: QueuedUserInput) {
        let mut state = self.state.lock().unwrap();
        state.pending_user_input.push(queued);
    }

    pub(super) fn notify_wait_interrupted(&self, reason: WaitInterruptReason) {
        let mut state = self.state.lock().unwrap();
        state.wait_interrupt_epoch = state.wait_interrupt_epoch.saturating_add(1);
        state.wait_interrupt_reason = Some(reason);
    }

    pub(super) fn wait_interrupt_snapshot(&self) -> (u64, Option<WaitInterruptReason>) {
        let state = self.state.lock().unwrap();
        (state.wait_interrupt_epoch, state.wait_interrupt_reason)
    }

    pub(super) fn enforce_user_message_limits(
        &self,
        sub_id: &str,
        response_item: &mut ResponseInputItem,
    ) {
        let ResponseInputItem::Message { role, content } = response_item else {
            return;
        };
        if role != "user" {
            return;
        }

        let mut aggregated = String::new();
        let mut text_segments: Vec<(usize, usize)> = Vec::new();
        for item in content.iter() {
            if let ContentItem::InputText { text } = item {
                let start = aggregated.len();
                aggregated.push_str(text);
                let end = aggregated.len();
                text_segments.push((start, end));
            }
        }

        if text_segments.is_empty() {
            return;
        }

        let (_, was_truncated, prefix_end, suffix_start) =
            truncate_middle_bytes(&aggregated, self.tool_output_max_bytes);
        if !was_truncated {
            return;
        }

        let cwd = self.get_cwd().to_path_buf();
        let safe_sub_id = crate::fs_sanitize::safe_path_component(sub_id, "sub");
        let uuid = Uuid::new_v4();
        let filename = format!("user-message-{safe_sub_id}-{uuid}.txt");
        let file_note = match ensure_user_dir(&cwd)
            .and_then(|dir| write_agent_file(&dir, &filename, &aggregated))
        {
            Ok(path) => format!("\n\n[Full output saved to: {}]", path.display()),
            Err(e) => format!("\n\n[Full output was too large and truncation applied; failed to save file: {e}]")
        };

        let original = std::mem::take(content);
        let mut new_content = Vec::with_capacity(original.len());
        let mut segment_iter = text_segments.into_iter();
        let mut marker_inserted = false;
        let mut last_text_idx: Option<usize> = None;

        for item in original.into_iter() {
            match item {
                ContentItem::InputText { text } => {
                    if let Some((seg_start, seg_end)) = segment_iter.next() {
                        let mut new_text = String::new();

                        if seg_start < prefix_end {
                            let slice_end = seg_end.min(prefix_end) - seg_start;
                            if let Some(prefix_slice) = text.get(..slice_end) {
                                new_text.push_str(prefix_slice);
                            }
                        }

                        if !marker_inserted && seg_end > prefix_end && seg_start < suffix_start {
                            new_text.push_str(TRUNCATION_MARKER);
                            marker_inserted = true;
                        }

                        if seg_end > suffix_start {
                            let slice_start = seg_start.max(suffix_start) - seg_start;
                            if let Some(suffix_slice) = text.get(slice_start..) {
                                new_text.push_str(suffix_slice);
                            }
                        }

                        new_content.push(ContentItem::InputText { text: new_text });
                        last_text_idx = Some(new_content.len() - 1);
                    }
                }
                other => new_content.push(other),
            }
        }

        if !marker_inserted {
            if let Some(idx) = last_text_idx {
                if let ContentItem::InputText { text } = &mut new_content[idx] {
                    text.push_str(TRUNCATION_MARKER);
                }
            } else {
                new_content.push(ContentItem::InputText {
                    text: TRUNCATION_MARKER.to_string(),
                });
                last_text_idx = Some(new_content.len() - 1);
            }
        }

        if let Some(idx) = last_text_idx {
            if let ContentItem::InputText { text } = &mut new_content[idx] {
                text.push_str(&file_note);
            }
        } else {
            new_content.push(ContentItem::InputText { text: file_note });
        }

        *content = new_content;
    }

    pub fn pop_next_queued_user_input(&self) -> Option<QueuedUserInput> {
        let mut state = self.state.lock().unwrap();
        if state.pending_user_input.is_empty() {
            None
        } else {
            Some(state.pending_user_input.remove(0))
        }
    }

    /// Enqueue a response item that should be surfaced to the model at the start of the
    /// next turn. Returns `true` if no agent is currently running and a new turn should be
    /// scheduled immediately.
    pub fn enqueue_out_of_turn_item(&self, item: ResponseInputItem) -> bool {
        let mut state = self.state.lock().unwrap();
        let should_start_turn = state.current_task.is_none();
        state.pending_input.push(item);
        should_start_turn
    }

    pub(crate) fn next_internal_sub_id(&self) -> String {
        let mut state = self.state.lock().unwrap();
        let id = state.next_internal_sub_id;
        state.next_internal_sub_id = state.next_internal_sub_id.saturating_add(1);
        format!("auto-compact-{id}")
    }

    /// Sends the given event to the client and swallows the send error, if
    /// any, logging it as an error.
    pub(super) fn make_turn_context(&self) -> Arc<TurnContext> {
        self.make_turn_context_with_schema(None)
    }

    pub(super) fn make_turn_context_with_schema(
        &self,
        final_output_json_schema: Option<Value>,
    ) -> Arc<TurnContext> {
        Arc::new(TurnContext {
            client: self.client.clone(),
            cwd: self.cwd.clone(),
            base_instructions: self.base_instructions.clone(),
            user_instructions: self.user_instructions.clone(),
            demo_developer_message: self.demo_developer_message.clone(),
            compact_prompt_override: self.compact_prompt_override.clone(),
            approval_policy: self.approval_policy,
            sandbox_policy: self.sandbox_policy.clone(),
            shell_environment_policy: self.shell_environment_policy.clone(),
            is_review_mode: false,
            text_format_override: self.next_turn_text_format.lock().unwrap().take(),
            final_output_json_schema,
        })
    }

    pub(super) fn compact_prompt_text(&self) -> String {
        crate::codex::compact::resolve_compact_prompt_text(
            self.compact_prompt_override.as_deref(),
        )
    }

    pub async fn request_command_approval(
        &self,
        sub_id: String,
        call_id: String,
        command: Vec<String>,
        cwd: PathBuf,
        reason: Option<String>,
    ) -> oneshot::Receiver<ReviewDecision> {
        let (tx_approve, rx_approve) = oneshot::channel();
        let event = self.make_event(
            &sub_id,
            EventMsg::ExecApprovalRequest(ExecApprovalRequestEvent {
                call_id: call_id.clone(),
                command,
                cwd,
                reason,
            }),
        );
        let _ = self.tx_event.send(event).await;
        {
            let mut state = self.state.lock().unwrap();
            // Track pending approval by call_id (unique per request) rather than sub_id
            // so parallel approvals in the same turn do not clobber each other.
            state.pending_approvals.insert(call_id, tx_approve);
        }
        rx_approve
    }

    pub async fn request_patch_approval(
        &self,
        sub_id: String,
        call_id: String,
        action: &ApplyPatchAction,
        reason: Option<String>,
        grant_root: Option<PathBuf>,
    ) -> oneshot::Receiver<ReviewDecision> {
        let (tx_approve, rx_approve) = oneshot::channel();
        let event = self.make_event(
            &sub_id,
            EventMsg::ApplyPatchApprovalRequest(ApplyPatchApprovalRequestEvent {
                call_id: call_id.clone(),
                changes: convert_apply_patch_to_protocol(action),
                reason,
                grant_root,
            }),
        );
        let _ = self.tx_event.send(event).await;
        {
            let mut state = self.state.lock().unwrap();
            // Track pending approval by call_id to avoid collisions.
            state.pending_approvals.insert(call_id, tx_approve);
        }
        rx_approve
    }

    pub fn notify_approval(&self, call_id: &str, decision: ReviewDecision) {
        let mut state = self.state.lock().unwrap();
        if let Some(tx_approve) = state.pending_approvals.remove(call_id) {
            let _ = tx_approve.send(decision);
        } else {
            // If we cannot find a pending approval for this call id, surface a warning
            // to aid debugging of stuck approvals.
            tracing::warn!("no pending approval found for call_id={}", call_id);
        }
    }

    pub async fn request_tool_user_input(&self) -> Option<Vec<InputItem>> {
        let (tx, rx) = oneshot::channel();
        {
            let mut state = self.state.lock().unwrap();
            state.pending_tool_user_input.push_back(tx);
        }
        rx.await.ok()
    }

    pub fn notify_tool_user_input(&self, items: Vec<InputItem>) -> Option<Vec<InputItem>> {
        let pending = {
            let mut state = self.state.lock().unwrap();
            state.pending_tool_user_input.pop_front()
        };
        if let Some(tx) = pending {
            let _ = tx.send(items);
            None
        } else {
            Some(items)
        }
    }

    pub fn add_approved_command(&self, pattern: ApprovedCommandPattern) {
        let mut state = self.state.lock().unwrap();
        state.approved_commands.insert(pattern);
    }

    /// Records items to both the rollout and the chat completions/ZDR
    /// transcript, if enabled.
    pub(super) async fn record_conversation_items(&self, items: &[ResponseItem]) {
        debug!("Recording items for conversation: {items:?}");
        self.record_state_snapshot(items).await;

        self.state.lock().unwrap().history.record_items(items);

    }

    /// Clean up old screenshots and system status messages from conversation history
    /// This is called when a new user message arrives to keep history manageable
    pub(super) async fn cleanup_old_status_items(&self) {
        let mut state = self.state.lock().unwrap();
        let current_items = state.history.take_contents();

        let (items_to_keep, stats) = if self.env_ctx_v2 {
            let policy = crate::retention::RetentionPolicy {
                max_env_deltas: self.retention_config.max_env_deltas,
                max_browser_snapshots: self.retention_config.max_browser_snapshots,
                max_total_bytes: self.retention_config.max_total_bytes,
                keep_latest_baseline: self.retention_config.keep_latest_baseline,
            };

            let (kept, retention_stats) =
                crate::retention::apply_retention_policy_owned(current_items, &policy);

            crate::telemetry::global_telemetry().record_retention(&retention_stats);

            let legacy_stats = CleanupStats {
                removed_screenshots: retention_stats.removed_screenshots,
                removed_status: retention_stats.removed_status,
                removed_env_baselines: retention_stats.removed_env_baselines,
                removed_env_deltas: retention_stats.removed_env_deltas,
                removed_browser_snapshots: retention_stats.removed_browser_snapshots,
                kept_recent_screenshots: retention_stats.kept_recent_screenshots,
                kept_env_deltas: retention_stats.kept_env_deltas,
                kept_browser_snapshots: retention_stats.kept_browser_snapshots,
            };

            (kept, legacy_stats)
        } else {
            prune_history_items_owned(current_items)
        };

        state.history.replace_filtered(items_to_keep);
        drop(state);

        if stats.any_removed() {
            info!(
                "Cleaned up history: removed {} old screenshots, {} status messages, {} env baselines, {} env deltas, {} browser snapshots; kept {} recent screenshots, {} env deltas, {} browser snapshots",
                stats.removed_screenshots,
                stats.removed_status,
                stats.removed_env_baselines,
                stats.removed_env_deltas,
                stats.removed_browser_snapshots,
                stats.kept_recent_screenshots,
                stats.kept_env_deltas,
                stats.kept_browser_snapshots
            );
        }
    }

    async fn record_state_snapshot(&self, items: &[ResponseItem]) {
        let snapshot = { SessionStateSnapshot {} };

        let recorder = self.clone_rollout_recorder();

        if let Some(rec) = recorder {
            if let Err(e) = rec.record_state(snapshot).await {
                error!("failed to record rollout state: {e:#}");
            }
            if let Err(e) = rec.record_response_items(items).await {
                error!("failed to record rollout items: {e:#}");
            }
        }
    }

    pub(super) fn clone_rollout_recorder(&self) -> Option<RolloutRecorder> {
        let guard = self.rollout.lock().unwrap();
        guard.as_ref().cloned()
    }

    pub(crate) async fn persist_rollout_items(&self, items: &[RolloutItem]) {
        let recorder = {
            let guard = self.rollout.lock().unwrap();
            guard.as_ref().cloned()
        };
        if let Some(rec) = recorder {
            if let Err(e) = rec.record_items(items).await {
                error!("failed to record rollout items: {e:#}");
            }
        }
    }

    /// Build the full turn input by concatenating the current conversation
    /// history with additional items for this turn.
    /// Browser screenshots are filtered out from history to keep them ephemeral.
    pub fn turn_input_with_history(&self, extra: Vec<ResponseItem>) -> Vec<ResponseItem> {
        let history = self.state.lock().unwrap().history.contents();

        // Debug: Count function call outputs in history
        let fc_output_count = history
            .iter()
            .filter(|item| matches!(item, ResponseItem::FunctionCallOutput { .. }))
            .count();
        if fc_output_count > 0 {
            debug!(
                "History contains {} FunctionCallOutput items",
                fc_output_count
            );
        }

        // Count images in extra for debugging (we can't distinguish ephemeral at this level anymore)
        let images_in_extra = extra
            .iter()
            .filter(|item| {
                if let ResponseItem::Message { content, .. } = item {
                    content
                        .iter()
                        .any(|c| matches!(c, ContentItem::InputImage { .. }))
                } else {
                    false
                }
            })
            .count();

        if images_in_extra > 0 {
            tracing::info!(
                "Found {} images in current turn's extra items",
                images_in_extra
            );
        }

        // Helper closure to detect legacy XML environment context items
        let is_legacy_env_context = |item: &ResponseItem| -> bool {
            if let ResponseItem::Message { role, content, .. } = item {
                if role == "user" {
                    return content.iter().any(|c| {
                        if let ContentItem::InputText { text } = c {
                            text.contains("<environment_context>")
                        } else {
                            false
                        }
                    });
                }
            }
            false
        };

        // Filter out browser screenshots from historical messages
        // We identify them by the [EPHEMERAL:...] marker that precedes them
        // When env_ctx_v2 is enabled, also suppress legacy XML environment context messages
        let filtered_history: Vec<ResponseItem> = history
            .into_iter()
            .filter(|item| {
                if self.env_ctx_v2 && *crate::flags::CTX_UI && is_legacy_env_context(item) {
                    tracing::debug!("Suppressing legacy XML environment context item from history");
                    return false;
                }
                true
            })
            .map(|item| {
                if let ResponseItem::Message { id, role, content } = item {
                    if role == "user" {
                        // Filter out ephemeral content from user messages
                        let mut filtered_content: Vec<ContentItem> = Vec::new();
                        let mut skip_next_image = false;

                        for content_item in content {
                            match &content_item {
                                ContentItem::InputText { text }
                                    if text.starts_with("[EPHEMERAL:") =>
                                {
                                    // This is an ephemeral marker, skip it and the next image
                                    skip_next_image = true;
                                    tracing::info!("Filtering out ephemeral marker: {}", text);
                                }
                                ContentItem::InputImage { .. }
                                    if skip_next_image =>
                                {
                                    // Skip this image as it follows an ephemeral marker
                                    skip_next_image = false;
                                    tracing::info!("Filtering out ephemeral image from history");
                                }
                                _ => {
                                    // Keep everything else
                                    filtered_content.push(content_item);
                                }
                            }
                        }

                        ResponseItem::Message {
                            id,
                            role,
                            content: filtered_content,
                        }
                    } else {
                        // Keep assistant messages unchanged
                        ResponseItem::Message { id, role, content }
                    }
                } else {
                    item
                }
            })
            .collect();

        let filtered_extra = if self.env_ctx_v2 && *crate::flags::CTX_UI {
            extra
                .into_iter()
                .filter(|item| {
                    parse_env_snapshot_from_response(item).is_none()
                        && parse_env_delta_from_response(item).is_none()
                })
                .collect::<Vec<_>>()
        } else {
            extra
        };

        // Concatenate timeline items (baseline + limited deltas) ahead of history
        let mut result = Vec::new();
        if let Some(mut timeline_items) = self.assemble_from_timeline() {
            result.append(&mut timeline_items);
        }
        result.extend(filtered_history);
        result.extend(filtered_extra);

        let current_auth_mode = self
            .client
            .get_auth_manager()
            .and_then(|manager| manager.auth())
            .map(|auth| auth.mode);
        let sanitize_encrypted_reasoning = !matches!(current_auth_mode, Some(AuthMode::ChatGPT));

        if sanitize_encrypted_reasoning {
            let mut stripped = 0usize;
            result = result
                .into_iter()
                .map(|item| match item {
                    ResponseItem::Reasoning {
                        id,
                        summary,
                        content,
                        encrypted_content,
                    } => {
                        if encrypted_content.is_some() {
                            stripped += 1;
                        }
                        ResponseItem::Reasoning {
                            id,
                            summary,
                            content,
                            encrypted_content: None,
                        }
                    }
                    other => other,
                })
                .collect();
            if stripped > 0 {
                debug!(
                    "Stripped encrypted reasoning from {} history items before sending request",
                    stripped
                );
            }
        }

        debug_history("turn_input_with_history", &result);

        // Count total images in result for debugging
        let total_images = result
            .iter()
            .filter(|item| {
                if let ResponseItem::Message { content, .. } = item {
                    content
                        .iter()
                        .any(|c| matches!(c, ContentItem::InputImage { .. }))
                } else {
                    false
                }
            })
            .count();

        if total_images > 0 {
            tracing::info!("Total images being sent to model: {}", total_images);
        }

        result
    }

    pub(crate) fn build_initial_context(&self, turn_context: &TurnContext) -> Vec<ResponseItem> {
        let mut items = Vec::new();
        if let Some(user_instructions) = turn_context.user_instructions.as_deref() {
            items.push(UserInstructions::new(user_instructions.to_string()).into());
        }

        let env_context = EnvironmentContext::new(
            Some(turn_context.cwd.clone()),
            Some(turn_context.approval_policy),
            Some(turn_context.sandbox_policy.clone()),
            Some(self.user_shell.clone()),
        );

        if let Some(mut env_ctx_items) = self.maybe_emit_env_ctx_messages(
            &env_context,
            get_git_branch(&turn_context.cwd),
            Some(format!("{:?}", self.client.get_reasoning_effort())),
        ) {
            items.append(&mut env_ctx_items);
        }

        if !self.env_ctx_v2 {
            // Legacy XML payload remains so behaviour is unchanged when the feature flag is off.
            items.push(ResponseItem::from(env_context));
        }
        items
    }

    pub(super) fn maybe_emit_env_ctx_messages(
        &self,
        env_context: &EnvironmentContext,
        git_branch: Option<String>,
        reasoning_effort: Option<String>,
    ) -> Option<Vec<ResponseItem>> {
        if !self.env_ctx_v2 {
            return None;
        }

        let (stream_id, result) = {
            let mut state = self.state.lock().unwrap();
            let stream = state.context_stream_ids.env_stream_id(self.id);
            let result = match state.environment_context_tracker.emit_response_items(
                env_context,
                git_branch.clone(),
                reasoning_effort.clone(),
                Some(stream.as_str()),
            ) {
                Ok(Some((emission, items))) => {
                    state.environment_context_seq = emission.sequence();
                    state.last_environment_snapshot = Some(emission.snapshot().clone());

                    match &emission {
                        EnvironmentContextEmission::Full { snapshot, .. } => {
                            if let Err(err) = state.context_timeline.add_baseline_once(snapshot.clone()) {
                                tracing::trace!("env_ctx_v2: baseline already set in context timeline: {err}");
                            }
                            match state.context_timeline.record_snapshot(snapshot.clone()) {
                                Ok(true) => {
                                    crate::telemetry::global_telemetry().record_snapshot_commit();
                                }
                                Ok(false) => {
                                    crate::telemetry::global_telemetry().record_dedup_drop();
                                }
                                Err(err) => {
                                    tracing::trace!("env_ctx_v2: failed to record baseline snapshot: {err}");
                                }
                            }
                        }
                        EnvironmentContextEmission::Delta { sequence, delta, snapshot } => {
                            if state.context_timeline.baseline().is_none() {
                                if let Err(err) = state.context_timeline.add_baseline_once(snapshot.clone()) {
                                    tracing::warn!("env_ctx_v2: failed to seed baseline before delta: {err}");
                                }
                            }
                            if let Err(err) = state
                                .context_timeline
                                .apply_delta(*sequence, delta.clone())
                            {
                                tracing::warn!("env_ctx_v2: failed to apply delta to timeline: {err}");
                                if matches!(err, crate::context_timeline::TimelineError::DeltaSequenceOutOfOrder { .. }) {
                                    crate::telemetry::global_telemetry().record_delta_gap();
                                }
                            }
                            match state.context_timeline.record_snapshot(snapshot.clone()) {
                                Ok(true) => {
                                    crate::telemetry::global_telemetry().record_snapshot_commit();
                                }
                                Ok(false) => {
                                    crate::telemetry::global_telemetry().record_dedup_drop();
                                }
                                Err(err) => {
                                    tracing::warn!("env_ctx_v2: failed to record snapshot: {err}");
                                }
                            }
                        }
                    }

                    Ok(Some((emission, items)))
                }
                other => other,
            };
            (stream, result)
        };

        let (emission, mut items) = match result {
            Ok(Some(pair)) => pair,
            Ok(None) => return None,
            Err(err) => {
                warn!("env_ctx_v2: failed to serialize environment_context JSON: {err}");
                if *crate::flags::CTX_UI {
                    return Some(vec![ResponseItem::from(env_context.clone())]);
                }
                return None;
            }
        };

        let suppress_legacy_status = self.env_ctx_v2 && *crate::flags::CTX_UI;
        if suppress_legacy_status {
            items.clear();
        }

        let sequence = emission.sequence();

        let bytes_sent: usize = items
            .iter()
            .flat_map(|item| match item {
                ResponseItem::Message { content, .. } => content.iter(),
                _ => [].iter(),
            })
            .map(|content| match content {
                ContentItem::InputText { text } | ContentItem::OutputText { text } => text.len(),
                _ => 0,
            })
            .sum();

        trace!(
            "env_ctx_v2: emitted environment_context message (seq={}, bytes={})",
            sequence,
            bytes_sent
        );

        if *crate::flags::CTX_UI {
            self.emit_env_context_event(stream_id.as_str(), &emission);
        }

        Some(items)
    }

    /// Assemble environment context items from the timeline for prompt input.
    fn assemble_from_timeline(&self) -> Option<Vec<ResponseItem>> {
        if !self.env_ctx_v2 {
            return None;
        }

        let (timeline, stream_id, max_deltas) = {
            let mut state = self.state.lock().unwrap();
            if state.context_timeline.is_empty() {
                return None;
            }
            let stream_id = state.context_stream_ids.env_stream_id(self.id);
            (
                state.context_timeline.clone(),
                stream_id,
                self.retention_config.max_env_deltas,
            )
        };

        match timeline.assemble_prompt_items(max_deltas, Some(&stream_id)) {
            Ok(items) if !items.is_empty() => Some(items),
            Ok(_) => None,
            Err(err) => {
                warn!("env_ctx_v2: failed to assemble timeline prompt items: {err}");
                None
            }
        }
    }

    fn emit_env_context_event(
        &self,
        stream_id: &str,
        emission: &EnvironmentContextEmission,
    ) {
        use crate::protocol::OrderMeta;

        let sequence = emission.sequence();
        let order = OrderMeta {
            request_ordinal: self.current_request_ordinal(),
            output_index: None,
            sequence_number: Some(sequence),
        };

        let msg = match emission {
            EnvironmentContextEmission::Full { snapshot, .. } => {
                let Ok(snapshot_json) = serde_json::to_value(snapshot) else {
                    warn!("env_ctx_v2: failed to serialize environment context snapshot for event");
                    return;
                };
                EventMsg::EnvironmentContextFull(EnvironmentContextFullEvent {
                    snapshot: snapshot_json,
                    sequence: Some(sequence),
                })
            }
            EnvironmentContextEmission::Delta { delta, .. } => {
                let Ok(delta_json) = serde_json::to_value(delta) else {
                    warn!("env_ctx_v2: failed to serialize environment context delta for event");
                    return;
                };
                EventMsg::EnvironmentContextDelta(EnvironmentContextDeltaEvent {
                    delta: delta_json,
                    sequence: Some(sequence),
                    base_fingerprint: Some(delta.base_fingerprint.clone()),
                })
            }
        };

        let event = self.make_event_with_order(stream_id, msg, order, Some(sequence));
        if let Err(err) = self.tx_event.try_send(event) {
            warn!("env_ctx_v2: failed to send environment context event: {err}");
        }
    }

    pub(super) fn emit_browser_snapshot_event(&self, stream_id: &str, snapshot: &BrowserSnapshot) {
        use crate::protocol::OrderMeta;

        let Ok(snapshot_json) = serde_json::to_value(snapshot) else {
            warn!("env_ctx_v2: failed to serialize browser snapshot for event");
            return;
        };

        let order = OrderMeta {
            request_ordinal: self.current_request_ordinal(),
            output_index: None,
            sequence_number: None,
        };

        let msg = EventMsg::BrowserSnapshot(BrowserSnapshotEvent {
            snapshot: snapshot_json,
            url: Some(snapshot.url.clone()),
            captured_at: Some(snapshot.captured_at.clone()),
        });

        let event = self.make_event_with_order(stream_id, msg, order, None);
        if let Err(err) = self.tx_event.try_send(event) {
            warn!("env_ctx_v2: failed to send browser snapshot event: {err}");
        }
    }

    pub(crate) fn reconstruct_history_from_rollout(
        &self,
        turn_context: &TurnContext,
        rollout_items: &[RolloutItem],
    ) -> Vec<ResponseItem> {
        let mut history = self.build_initial_context(turn_context);
        let mut replay_ctx = TimelineReplayContext::default();

        for item in rollout_items {
            match item {
                RolloutItem::ResponseItem(response_item) => {
                    history.push(response_item.clone());
                    process_rollout_env_item(&mut replay_ctx, response_item);
                }
                RolloutItem::Compacted(compacted) => {
                    let snippets = collect_compaction_snippets(&history);
                    history = build_compacted_history(
                        self.build_initial_context(turn_context),
                        &snippets,
                        &compacted.message,
                    );
                }
                RolloutItem::Event(recorded_event) => {
                    if let code_protocol::protocol::EventMsg::UserMessage(user_msg_event) = &recorded_event.msg {
                        let response_item = ResponseItem::Message {
                            id: Some(recorded_event.id.clone()),
                            role: "user".to_string(),
                            content: vec![ContentItem::InputText {
                                text: user_msg_event.message.clone(),
                            }],
                        };
                        process_rollout_env_item(&mut replay_ctx, &response_item);
                        history.push(response_item);
                    }
                }
                _ => {}
            }
        }

        if replay_ctx.timeline.baseline().is_none() {
            if let Some(snapshot) = replay_ctx.legacy_baseline.clone() {
                if let Err(err) = replay_ctx.timeline.add_baseline_once(snapshot.clone()) {
                    tracing::warn!("env_ctx_v2: failed to map legacy status to baseline: {err}");
                }
                match replay_ctx.timeline.record_snapshot(snapshot.clone()) {
                    Ok(true) => crate::telemetry::global_telemetry().record_snapshot_commit(),
                    Ok(false) => crate::telemetry::global_telemetry().record_dedup_drop(),
                    Err(err) => tracing::warn!("env_ctx_v2: failed to record legacy baseline snapshot: {err}"),
                }
                replay_ctx.last_snapshot = Some(snapshot);
            }
        }

        let restored_snapshot = replay_ctx.last_snapshot.clone();
        let next_seq_value = replay_ctx.next_sequence;
        {
            let mut state = self.state.lock().unwrap();
            state.context_timeline = replay_ctx.timeline.clone();
            state.environment_context_seq = next_seq_value.saturating_sub(1);
            state.context_stream_ids = EnvironmentContextStreamRegistry::default();

            if let Some(snapshot) = restored_snapshot {
                state.last_environment_snapshot = Some(snapshot.clone());
                state
                    .environment_context_tracker
                    .restore(snapshot, next_seq_value);
            } else {
                state.last_environment_snapshot = None;
                state.environment_context_tracker = EnvironmentContextTracker::new();
            }
        }

        history
    }

    /// Returns the input if there was no agent running to inject into
    pub fn inject_input(&self, input: Vec<InputItem>) -> Result<(), Vec<InputItem>> {
        let mut state = self.state.lock().unwrap();
        if let Some(task) = state.current_task.as_ref() {
            let mut response = response_input_from_core_items(input);
            self.enforce_user_message_limits(&task.sub_id, &mut response);
            state.pending_input.push(response);
            Ok(())
        } else {
            Err(input)
        }
    }

    pub fn enqueue_manual_compact(&self, sub_id: String) -> bool {
        let mut state = self.state.lock().unwrap();
        let was_empty = state.pending_manual_compacts.is_empty();
        state.pending_manual_compacts.push_back(sub_id);
        was_empty
    }

    pub fn dequeue_manual_compact(&self) -> Option<String> {
        let mut state = self.state.lock().unwrap();
        state.pending_manual_compacts.pop_front()
    }


    pub fn get_pending_input(&self) -> Vec<ResponseInputItem> {
        self.get_pending_input_filtered(true)
    }

    /// Returns pending input for the current turn. Callers can decide whether
    /// queued user inputs should be drained immediately (`drain_user_inputs = true`)
    /// or preserved for a later turn—for example, review mode keeps them queued
    /// so the primary agent can resume once the review finishes.
    pub fn get_pending_input_filtered(&self, drain_user_inputs: bool) -> Vec<ResponseInputItem> {
        let mut state = self.state.lock().unwrap();
        if state.pending_input.is_empty()
            && (drain_user_inputs || state.pending_user_input.is_empty())
        {
            Vec::with_capacity(0)
        } else {
            let mut ret = Vec::new();
            if !state.pending_input.is_empty() {
                let mut model_inputs = Vec::new();
                std::mem::swap(&mut model_inputs, &mut state.pending_input);
                ret.extend(model_inputs);
            }

            if !state.pending_user_input.is_empty() {
                if drain_user_inputs {
                    let mut queued_user_inputs = Vec::new();
                    std::mem::swap(&mut queued_user_inputs, &mut state.pending_user_input);
                    ret.extend(
                        queued_user_inputs
                            .into_iter()
                            .map(|queued| queued.response_item),
                    );
                } else {
                    ret.extend(
                        state
                            .pending_user_input
                            .iter()
                            .map(|queued| queued.response_item.clone()),
                    );
                }
            }
            ret
        }
    }

    pub fn add_pending_input(&self, mut input: ResponseInputItem) {
        let mut state = self.state.lock().unwrap();
        if let Some(task) = state.current_task.as_ref() {
            self.enforce_user_message_limits(&task.sub_id, &mut input);
        }
        state.pending_input.push(input);
    }

    pub async fn call_tool(
        &self,
        server: &str,
        tool: &str,
        arguments: Option<serde_json::Value>,
        timeout: Option<Duration>,
    ) -> anyhow::Result<CallToolResult> {
        self.mcp_connection_manager
            .call_tool(server, tool, arguments, timeout)
            .await
    }

    pub(super) fn abort(&self) {
        info!("Aborting existing session");

        self.mark_all_running_execs_as_cancelled();

        let mut state = self.state.lock().unwrap();
        state.pending_approvals.clear();
        state.pending_tool_user_input.clear();
        // Do not clear `pending_input` here. When a user submits a new message
        // immediately after an interrupt, it may have been routed to
        // `pending_input` by an earlier code path. Clearing it would drop the
        // user's message and prevent the next turn from ever starting.
        state.turn_scratchpad = None;
        // Take current task while holding the lock, then drop the lock BEFORE calling abort
        let current = state.current_task.take();
        drop(state);
        if let Some(agent) = current {
            agent.abort(TurnAbortReason::Interrupted);
        }
        // Also terminate any running exec sessions (PTY-based) so child processes do not linger.
        // Best-effort cleanup for PTY-based exec sessions would go here. The
        // PTY implementation already kills processes on session drop; in the
        // common LocalShellCall path we also kill processes immediately via
        // KillOnDrop in exec.rs.
    }

    /// Spawn the configured notifier (if any) with the given JSON payload as
    /// the last argument. Failures are logged but otherwise ignored so that
    /// notification issues do not interfere with the main workflow.
    pub(super) fn maybe_notify(&self, notification: UserNotification) {
        let Some(notify_command) = &self.notify else {
            return;
        };

        if notify_command.is_empty() {
            return;
        }

        let Ok(json) = serde_json::to_string(&notification) else {
            error!("failed to serialise notification payload");
            return;
        };

        let mut command = std::process::Command::new(&notify_command[0]);
        if notify_command.len() > 1 {
            command.args(&notify_command[1..]);
        }
        command.arg(json);

        // Fire-and-forget – we do not wait for completion.
        if let Err(e) = crate::spawn::spawn_std_command_with_retry(&mut command) {
            warn!("failed to spawn notifier '{}': {e}", notify_command[0]);
        }
    }
}
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct CleanupStats {
    pub(super) removed_screenshots: usize,
    pub(super) removed_status: usize,
    pub(super) removed_env_baselines: usize,
    pub(super) removed_env_deltas: usize,
    pub(super) removed_browser_snapshots: usize,
    pub(super) kept_recent_screenshots: usize,
    pub(super) kept_env_deltas: usize,
    pub(super) kept_browser_snapshots: usize,
}

impl CleanupStats {
    pub(super) fn any_removed(&self) -> bool {
        self.removed_screenshots > 0
            || self.removed_status > 0
            || self.removed_env_baselines > 0
            || self.removed_env_deltas > 0
            || self.removed_browser_snapshots > 0
    }
}

#[cfg(test)]
pub(crate) fn prune_history_items(
    current_items: &[ResponseItem],
) -> (Vec<ResponseItem>, CleanupStats) {
    let mut real_user_messages = Vec::new();
    let mut status_messages = Vec::new();
    let mut env_baselines = Vec::new();
    let mut env_deltas = Vec::new();
    let mut browser_snapshot_messages = Vec::new();

    const MAX_ENV_DELTAS: usize = 3;
    const MAX_BROWSER_SNAPSHOTS: usize = 2;

    for (idx, item) in current_items.iter().enumerate() {
        if let ResponseItem::Message { role, content, .. } = item {
            if role != "user" {
                continue;
            }

            let has_status = content.iter().any(|c| {
                if let ContentItem::InputText { text } = c {
                    text.contains("== System Status ==")
                        || text.contains("Current working directory:")
                        || text.contains("Git branch:")
                        || text.contains(ENVIRONMENT_CONTEXT_OPEN_TAG)
                        || text.contains(ENVIRONMENT_CONTEXT_DELTA_OPEN_TAG)
                        || text.contains(BROWSER_SNAPSHOT_OPEN_TAG)
                } else {
                    false
                }
            });

            let has_screenshot = content
                .iter()
                .any(|c| matches!(c, ContentItem::InputImage { .. }));

            let has_real_text = content.iter().any(|c| {
                if let ContentItem::InputText { text } = c {
                    !text.contains("== System Status ==")
                        && !text.contains("Current working directory:")
                        && !text.contains("Git branch:")
                        && !text.trim().is_empty()
                        && !text.contains(ENVIRONMENT_CONTEXT_OPEN_TAG)
                        && !text.contains(ENVIRONMENT_CONTEXT_DELTA_OPEN_TAG)
                        && !text.contains(BROWSER_SNAPSHOT_OPEN_TAG)
                } else {
                    false
                }
            });

            let has_env_baseline = content.iter().any(|c| {
                if let ContentItem::InputText { text } = c {
                    text.contains(ENVIRONMENT_CONTEXT_OPEN_TAG)
                        && !text.contains(ENVIRONMENT_CONTEXT_DELTA_OPEN_TAG)
                } else {
                    false
                }
            });

            let has_env_delta = content.iter().any(|c| {
                if let ContentItem::InputText { text } = c {
                    text.contains(ENVIRONMENT_CONTEXT_DELTA_OPEN_TAG)
                } else {
                    false
                }
            });

            let has_browser_snapshot = content.iter().any(|c| {
                if let ContentItem::InputText { text } = c {
                    text.contains(BROWSER_SNAPSHOT_OPEN_TAG)
                } else {
                    false
                }
            });

            if has_real_text && !has_status && !has_screenshot {
                real_user_messages.push(idx);
            } else if has_status || has_screenshot {
                status_messages.push(idx);
            }

            if has_env_baseline {
                env_baselines.push(idx);
            }
            if has_env_delta {
                env_deltas.push(idx);
            }
            if has_browser_snapshot {
                browser_snapshot_messages.push(idx);
            }
        }
    }

    let mut screenshots_to_keep = std::collections::HashSet::new();
    for &user_idx in real_user_messages.iter().rev().take(2) {
        for &status_idx in status_messages.iter() {
            if status_idx > user_idx {
                if let Some(ResponseItem::Message { content, .. }) = current_items.get(status_idx)
                {
                    if content.iter().any(|c| matches!(c, ContentItem::InputImage { .. })) {
                        screenshots_to_keep.insert(status_idx);
                        break;
                    }
                }
            }
        }
    }

    let baseline_to_keep = env_baselines.last().copied();
    let env_deltas_to_keep: std::collections::HashSet<usize> = env_deltas
        .iter()
        .rev()
        .take(MAX_ENV_DELTAS)
        .copied()
        .collect();
    let browser_snapshots_to_keep: std::collections::HashSet<usize> = browser_snapshot_messages
        .iter()
        .rev()
        .take(MAX_BROWSER_SNAPSHOTS)
        .copied()
        .collect();

    let mut items_to_keep = Vec::new();
    let mut removed_screenshots = 0usize;
    let mut removed_status = 0usize;

    for (idx, item) in current_items.iter().enumerate() {
        let keep = if status_messages.contains(&idx) {
            screenshots_to_keep.contains(&idx)
                || browser_snapshots_to_keep.contains(&idx)
                || baseline_to_keep == Some(idx)
                || env_deltas_to_keep.contains(&idx)
        } else {
            true
        };

        if keep {
            items_to_keep.push(item.clone());
        } else if let ResponseItem::Message { content, .. } = item {
            if content
                .iter()
                .any(|c| matches!(c, ContentItem::InputImage { .. }))
            {
                removed_screenshots += 1;
            } else {
                removed_status += 1;
            }
        }
    }

    let stats = CleanupStats {
        removed_screenshots,
        removed_status,
        removed_env_baselines: env_baselines
            .len()
            .saturating_sub(if baseline_to_keep.is_some() { 1 } else { 0 }),
        removed_env_deltas: env_deltas.len().saturating_sub(env_deltas_to_keep.len()),
        removed_browser_snapshots: browser_snapshot_messages
            .len()
            .saturating_sub(browser_snapshots_to_keep.len()),
        kept_recent_screenshots: screenshots_to_keep.len(),
        kept_env_deltas: env_deltas_to_keep.len(),
        kept_browser_snapshots: browser_snapshots_to_keep.len(),
    };

    (items_to_keep, stats)
}

fn prune_history_items_owned(current_items: Vec<ResponseItem>) -> (Vec<ResponseItem>, CleanupStats) {
    let mut real_user_messages = Vec::new();
    let mut status_messages = Vec::new();
    let mut env_baselines = Vec::new();
    let mut env_deltas = Vec::new();
    let mut browser_snapshot_messages = Vec::new();

    const MAX_ENV_DELTAS: usize = 3;
    const MAX_BROWSER_SNAPSHOTS: usize = 2;

    for (idx, item) in current_items.iter().enumerate() {
        if let ResponseItem::Message { role, content, .. } = item {
            if role != "user" {
                continue;
            }

            let has_status = content.iter().any(|c| {
                if let ContentItem::InputText { text } = c {
                    text.contains("== System Status ==")
                        || text.contains("Current working directory:")
                        || text.contains("Git branch:")
                        || text.contains(ENVIRONMENT_CONTEXT_OPEN_TAG)
                        || text.contains(ENVIRONMENT_CONTEXT_DELTA_OPEN_TAG)
                        || text.contains(BROWSER_SNAPSHOT_OPEN_TAG)
                } else {
                    false
                }
            });

            let has_screenshot = content
                .iter()
                .any(|c| matches!(c, ContentItem::InputImage { .. }));

            let has_real_text = content.iter().any(|c| {
                if let ContentItem::InputText { text } = c {
                    !text.contains("== System Status ==")
                        && !text.contains("Current working directory:")
                        && !text.contains("Git branch:")
                        && !text.trim().is_empty()
                        && !text.contains(ENVIRONMENT_CONTEXT_OPEN_TAG)
                        && !text.contains(ENVIRONMENT_CONTEXT_DELTA_OPEN_TAG)
                        && !text.contains(BROWSER_SNAPSHOT_OPEN_TAG)
                } else {
                    false
                }
            });

            let has_env_baseline = content.iter().any(|c| {
                if let ContentItem::InputText { text } = c {
                    text.contains(ENVIRONMENT_CONTEXT_OPEN_TAG)
                        && !text.contains(ENVIRONMENT_CONTEXT_DELTA_OPEN_TAG)
                } else {
                    false
                }
            });

            let has_env_delta = content.iter().any(|c| {
                if let ContentItem::InputText { text } = c {
                    text.contains(ENVIRONMENT_CONTEXT_DELTA_OPEN_TAG)
                } else {
                    false
                }
            });

            let has_browser_snapshot = content.iter().any(|c| {
                if let ContentItem::InputText { text } = c {
                    text.contains(BROWSER_SNAPSHOT_OPEN_TAG)
                } else {
                    false
                }
            });

            if has_real_text && !has_status && !has_screenshot {
                real_user_messages.push(idx);
            } else if has_status || has_screenshot {
                status_messages.push(idx);
            }

            if has_env_baseline {
                env_baselines.push(idx);
            }
            if has_env_delta {
                env_deltas.push(idx);
            }
            if has_browser_snapshot {
                browser_snapshot_messages.push(idx);
            }
        }
    }

    let mut screenshots_to_keep = std::collections::HashSet::new();
    for &user_idx in real_user_messages.iter().rev().take(2) {
        for &status_idx in status_messages.iter() {
            if status_idx > user_idx {
                if let Some(ResponseItem::Message { content, .. }) = current_items.get(status_idx)
                {
                    if content.iter().any(|c| matches!(c, ContentItem::InputImage { .. })) {
                        screenshots_to_keep.insert(status_idx);
                        break;
                    }
                }
            }
        }
    }

    let baseline_to_keep = env_baselines.last().copied();
    let env_deltas_to_keep: std::collections::HashSet<usize> = env_deltas
        .iter()
        .rev()
        .take(MAX_ENV_DELTAS)
        .copied()
        .collect();
    let browser_snapshots_to_keep: std::collections::HashSet<usize> = browser_snapshot_messages
        .iter()
        .rev()
        .take(MAX_BROWSER_SNAPSHOTS)
        .copied()
        .collect();

    let mut items_to_keep = Vec::new();
    let mut removed_screenshots = 0usize;
    let mut removed_status = 0usize;

    for (idx, item) in current_items.into_iter().enumerate() {
        let keep = if status_messages.contains(&idx) {
            screenshots_to_keep.contains(&idx)
                || browser_snapshots_to_keep.contains(&idx)
                || baseline_to_keep == Some(idx)
                || env_deltas_to_keep.contains(&idx)
        } else {
            true
        };

        if keep {
            items_to_keep.push(item);
        } else if let ResponseItem::Message { content, .. } = &item {
            if content
                .iter()
                .any(|c| matches!(c, ContentItem::InputImage { .. }))
            {
                removed_screenshots += 1;
            } else {
                removed_status += 1;
            }
        }
    }

    let stats = CleanupStats {
        removed_screenshots,
        removed_status,
        removed_env_baselines: env_baselines
            .len()
            .saturating_sub(if baseline_to_keep.is_some() { 1 } else { 0 }),
        removed_env_deltas: env_deltas.len().saturating_sub(env_deltas_to_keep.len()),
        removed_browser_snapshots: browser_snapshot_messages
            .len()
            .saturating_sub(browser_snapshots_to_keep.len()),
        kept_recent_screenshots: screenshots_to_keep.len(),
        kept_env_deltas: env_deltas_to_keep.len(),
        kept_browser_snapshots: browser_snapshots_to_keep.len(),
    };

    (items_to_keep, stats)
}

impl Drop for Session {
    fn drop(&mut self) {
        // Interrupt any running turn when the session is dropped.
        self.abort();
    }
}

impl State {
    pub fn partial_clone(&self) -> Self {
        Self {
            approved_commands: self.approved_commands.clone(),
            history: self.history.clone(),
            // Preserve request_ordinal so reconfigurations (e.g., /reasoning)
            // do not reset provider ordering mid-session.
            request_ordinal: self.request_ordinal,
            background_seq_by_sub_id: self.background_seq_by_sub_id.clone(),
            dry_run_guard: self.dry_run_guard.clone(),
            next_internal_sub_id: self.next_internal_sub_id,
            context_timeline: self.context_timeline.clone(),
            environment_context_tracker: self.environment_context_tracker.clone(),
            environment_context_seq: self.environment_context_seq,
            last_environment_snapshot: self.last_environment_snapshot.clone(),
            context_stream_ids: self.context_stream_ids.clone(),
            ..Default::default()
        }
    }
}
