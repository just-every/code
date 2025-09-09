// Poisoned mutex should fail the program
#![allow(clippy::unwrap_used)]

use std::borrow::Cow;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicU64;
use std::time::Duration;

use async_channel::Receiver;
use async_channel::Sender;
use base64::Engine;
use codex_apply_patch::ApplyPatchAction;
use codex_apply_patch::MaybeApplyPatchVerified;
use codex_apply_patch::maybe_parse_apply_patch_verified;
// unused: AuthManager
// unused: ConversationHistoryResponseEvent
use codex_protocol::protocol::TurnAbortReason;
// unused: TurnAbortedEvent
use futures::prelude::*;
use mcp_types::CallToolResult;
use serde::Serialize;
use serde_json;
use serde_json::json;
use tokio::sync::oneshot;
use tokio::task::AbortHandle;
use tracing::debug;
use tracing::error;
use tracing::info;
use tracing::trace;
use tracing::warn;
use uuid::Uuid;
use crate::CodexAuth;
use crate::protocol::WebSearchBeginEvent;
use crate::protocol::WebSearchCompleteEvent;
use codex_protocol::models::WebSearchAction;

/// Initial submission ID for session configuration
pub(crate) const INITIAL_SUBMIT_ID: &str = "";

/// Gather ephemeral, per-turn context that should not be persisted to history.
/// Combines environment info and (when enabled) a live browser snapshot and status.
struct EphemeralJar {
    items: Vec<ResponseItem>,
}

impl EphemeralJar {
    fn new() -> Self {
        Self { items: Vec::new() }
    }

    fn into_items(self) -> Vec<ResponseItem> {
        self.items
    }
}

/// Convert a vector of core `InputItem`s into a single `ResponseInputItem`
/// suitable for sending to the model. Handles images (local and pre‑encoded)
/// and our fork's ephemeral image variant by inlining a brief metadata marker
/// followed by the image as a data URL.
fn response_input_from_core_items(items: Vec<InputItem>) -> ResponseInputItem {
    let mut content_items = Vec::new();

    for item in items {
        match item {
            InputItem::Text { text } => {
                content_items.push(ContentItem::InputText { text });
            }
            InputItem::Image { image_url } => {
                content_items.push(ContentItem::InputImage {
                    image_url,
                    detail: None,
                });
            }
            InputItem::LocalImage { path } => match std::fs::read(&path) {
                Ok(bytes) => {
                    let mime = mime_guess::from_path(&path)
                        .first()
                        .map(|m| m.essence_str().to_owned())
                        .unwrap_or_else(|| "application/octet-stream".to_string());
                    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
                    content_items.push(ContentItem::InputImage {
                        image_url: format!("data:{mime};base64,{encoded}"),
                        detail: None,
                    });
                }
                Err(err) => {
                    tracing::warn!(
                        "Skipping image {} – could not read file: {}",
                        path.display(),
                        err
                    );
                }
            },
            InputItem::EphemeralImage { path, metadata } => {
                tracing::info!(
                    "Processing ephemeral image: {} with metadata: {:?}",
                    path.display(),
                    metadata
                );

                if let Some(meta) = metadata {
                    content_items.push(ContentItem::InputText {
                        text: format!("[EPHEMERAL:{}]", meta),
                    });
                }

                match std::fs::read(&path) {
                    Ok(bytes) => {
                        let mime = mime_guess::from_path(&path)
                            .first()
                            .map(|m| m.essence_str().to_owned())
                            .unwrap_or_else(|| "application/octet-stream".to_string());
                        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
                        tracing::info!("Created ephemeral image data URL with mime: {}", mime);
                        content_items.push(ContentItem::InputImage {
                            image_url: format!("data:{mime};base64,{encoded}"),
                            detail: Some("high".to_string()),
                        });
                    }
                    Err(err) => {
                        tracing::error!(
                            "Failed to read ephemeral image {} – {}",
                            path.display(),
                            err
                        );
                    }
                }
            }
        }
    }

    ResponseInputItem::Message {
        role: "user".to_string(),
        content: content_items,
    }
}

fn convert_call_tool_result_to_function_call_output_payload(
    result: &Result<CallToolResult, String>,
) -> FunctionCallOutputPayload {
    match result {
        Ok(ok) => FunctionCallOutputPayload {
            content: serde_json::to_string(ok)
                .unwrap_or_else(|e| format!("JSON serialization error: {e}")),
            success: Some(true),
        },
        Err(e) => FunctionCallOutputPayload {
            content: format!("err: {e:?}"),
            success: Some(false),
        },
    }
}

fn get_git_branch(cwd: &std::path::Path) -> Option<String> {
    let head_path = cwd.join(".git/HEAD");
    if let Ok(contents) = std::fs::read_to_string(&head_path) {
        if let Some(rest) = contents.trim().strip_prefix("ref: ") {
            if let Some(branch) = rest.trim().rsplit('/').next() {
                return Some(branch.to_string());
            }
        }
    }
    None
}

async fn build_turn_status_items(sess: &Session) -> Vec<ResponseItem> {
    let mut jar = EphemeralJar::new();

    // Collect environment context
    let cwd = sess.cwd.to_string_lossy().to_string();
    let branch = get_git_branch(&sess.cwd).unwrap_or_else(|| "unknown".to_string());
    let reasoning_effort = sess.client.get_reasoning_effort();

    // Build current system status
    let mut current_status = format!(
        r#"== System Status ==
[automatic message added by system]

cwd: {}
branch: {}
reasoning: {:?}"#,
        cwd, branch, reasoning_effort
    );

    // Prepare browser context + optional screenshot
    let mut screenshot_content: Option<ContentItem> = None;
    let mut include_screenshot = false;

    if let Some(browser_manager) = codex_browser::global::get_browser_manager().await {
        if browser_manager.is_enabled().await {
            // Get current URL and browser info
            let url = browser_manager
                .get_current_url()
                .await
                .unwrap_or_else(|| "unknown".to_string());

            // Try to get a tab title if available
            let title = match browser_manager.get_or_create_page().await {
                Ok(page) => page.get_title().await,
                Err(_) => None,
            };

            // Get browser type description
            let browser_type = browser_manager.get_browser_type().await;

            // Get viewport dimensions
            let (viewport_width, viewport_height) = browser_manager.get_viewport_size().await;
            let viewport_info = format!(" | Viewport: {}x{}", viewport_width, viewport_height);

            // Get cursor position
            let cursor_info = match browser_manager.get_cursor_position().await {
                Ok((x, y)) => format!(
                    " | Mouse position: ({:.0}, {:.0}) [shown as a blue cursor in the screenshot]",
                    x, y
                ),
                Err(_) => String::new(),
            };

            // Try to capture screenshot and compare with last one
            let screenshot_status = match capture_browser_screenshot(sess).await {
                Ok((screenshot_path, _url)) => {
                    // Always update the UI with the latest screenshot, even if unchanged for LLM payload
                    // This ensures the user sees that a fresh capture occurred each turn.
                    add_pending_screenshot(sess, screenshot_path.clone(), url.clone());
                    // Check if screenshot has changed using image hashing
                    let mut last_screenshot_info = sess.last_screenshot_info.lock().unwrap();

                    // Compute hash for current screenshot
                    let current_hash =
                        crate::image_comparison::compute_image_hash(&screenshot_path).ok();

                    let should_include_screenshot = if let (
                        Some((_last_path, last_phash, last_dhash)),
                        Some((cur_phash, cur_dhash)),
                    ) =
                        (last_screenshot_info.as_ref(), current_hash.as_ref())
                    {
                        // Compare hashes to see if screenshots are similar
                        let similar = crate::image_comparison::are_hashes_similar(
                            last_phash, last_dhash, cur_phash, cur_dhash,
                        );

                        if !similar {
                            // Screenshot has changed, include it
                            *last_screenshot_info = Some((
                                screenshot_path.clone(),
                                cur_phash.clone(),
                                cur_dhash.clone(),
                            ));
                            true
                        } else {
                            // Screenshot unchanged
                            false
                        }
                    } else {
                        // No previous screenshot or hash computation failed, include it
                        if let Some((phash, dhash)) = current_hash {
                            *last_screenshot_info = Some((screenshot_path.clone(), phash, dhash));
                        }
                        true
                    };

                    if should_include_screenshot {
                        if let Ok(bytes) = std::fs::read(&screenshot_path) {
                            let mime = mime_guess::from_path(&screenshot_path)
                                .first()
                                .map(|m| m.to_string())
                                .unwrap_or_else(|| "image/png".to_string());
                            let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
                            screenshot_content = Some(ContentItem::InputImage {
                                image_url: format!("data:{mime};base64,{encoded}"),
                                detail: Some("high".to_string()),
                            });
                            include_screenshot = true;
                            ""
                        } else {
                            " [Screenshot file read failed]"
                        }
                    } else {
                        " [Screenshot unchanged]"
                    }
                }
                Err(err_msg) => {
                    // Include error message so LLM knows screenshot failed
                    format!(" [Screenshot unavailable: {}]", err_msg).leak()
                }
            };

            let status_line = if let Some(t) = title {
                format!(
                    "Browser url: {} — {} ({}){}{}{}. You can interact with it using browser_* tools.",
                    url, t, browser_type, viewport_info, cursor_info, screenshot_status
                )
            } else {
                format!(
                    "Browser url: {} ({}){}{}{}. You can interact with it using browser_* tools.",
                    url, browser_type, viewport_info, cursor_info, screenshot_status
                )
            };
            current_status.push_str("\n");
            current_status.push_str(&status_line);
        }
    }

    // Check if system status has changed
    let mut last_status = sess.last_system_status.lock().unwrap();
    let status_changed = last_status.as_ref() != Some(&current_status);

    if status_changed {
        // Update last status
        *last_status = Some(current_status.clone());
    }

    // Only include items if something has changed or is new
    let mut content: Vec<ContentItem> = Vec::new();

    if status_changed {
        content.push(ContentItem::InputText {
            text: current_status,
        });
    }

    if include_screenshot {
        if let Some(image) = screenshot_content {
            content.push(image);
        }
    }

    if !content.is_empty() {
        jar.items.push(ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content,
        });
    }

    jar.into_items()
}
use crate::agent_tool::AGENT_MANAGER;
use crate::agent_tool::AgentStatus;
use crate::agent_tool::CancelAgentParams;
use crate::agent_tool::CheckAgentStatusParams;
use crate::agent_tool::GetAgentResultParams;
use crate::agent_tool::ListAgentsParams;
use crate::agent_tool::RunAgentParams;
use crate::agent_tool::WaitForAgentParams;
use crate::apply_patch::ApplyPatchExec;
use crate::apply_patch::CODEX_APPLY_PATCH_ARG1;
use crate::apply_patch::InternalApplyPatchInvocation;
use crate::apply_patch::convert_apply_patch_to_protocol;
use crate::apply_patch::get_writable_roots;
use crate::apply_patch::{self};
use crate::client::ModelClient;
use crate::client_common::Prompt;
use crate::client_common::ResponseEvent;
use crate::environment_context::EnvironmentContext;
use crate::config::Config;
use crate::config_types::ShellEnvironmentPolicy;
use crate::conversation_history::ConversationHistory;
use crate::error::CodexErr;
use crate::error::Result as CodexResult;
use crate::error::SandboxErr;
use crate::error::get_error_message_ui;
use crate::exec::ExecParams;
use crate::exec::ExecToolCallOutput;
use crate::exec::SandboxType;
use crate::exec::StdoutStream;
use crate::exec::StreamOutput;
use crate::exec::process_exec_tool_call;
use crate::exec_env::create_env;
use crate::mcp_connection_manager::McpConnectionManager;
use crate::mcp_tool_call::handle_mcp_tool_call;
use codex_protocol::models::ContentItem;
use codex_protocol::models::FunctionCallOutputPayload;
use codex_protocol::models::LocalShellAction;
use codex_protocol::models::ReasoningItemContent;
use codex_protocol::models::ReasoningItemReasoningSummary;
use codex_protocol::models::ResponseInputItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::models::ShellToolCallParams;
use crate::openai_tools::ToolsConfig;
use crate::openai_tools::get_openai_tools;
use crate::parse_command::parse_command;
use crate::plan_tool::handle_update_plan;
use crate::project_doc::get_user_instructions;
use crate::protocol::AgentMessageDeltaEvent;
use crate::protocol::AgentMessageEvent;
use crate::protocol::AgentReasoningDeltaEvent;
use crate::protocol::AgentReasoningEvent;
use crate::protocol::AgentReasoningRawContentDeltaEvent;
use crate::protocol::AgentReasoningRawContentEvent;
use crate::protocol::AgentReasoningSectionBreakEvent;
use crate::protocol::AgentStatusUpdateEvent;
use crate::protocol::ApplyPatchApprovalRequestEvent;
use crate::protocol::AskForApproval;
use crate::protocol::BackgroundEventEvent;
use crate::protocol::BrowserScreenshotUpdateEvent;
use crate::protocol::ErrorEvent;
use crate::protocol::Event;
use crate::protocol::EventMsg;
use crate::protocol::ExecApprovalRequestEvent;
use crate::protocol::ExecCommandBeginEvent;
use crate::protocol::ExecCommandEndEvent;
use crate::protocol::FileChange;
use crate::protocol::InputItem;
use crate::protocol::Op;
use crate::protocol::PatchApplyBeginEvent;
use crate::protocol::PatchApplyEndEvent;
use crate::protocol::ReviewDecision;
use crate::protocol::SandboxPolicy;
use crate::protocol::SessionConfiguredEvent;
use crate::protocol::Submission;
use crate::protocol::TaskCompleteEvent;
use crate::protocol::TurnDiffEvent;
use crate::rollout::RolloutRecorder;
use crate::safety::SafetyCheck;
use crate::safety::assess_command_safety;
use crate::safety::assess_safety_for_untrusted_command;
use crate::shell;
use crate::turn_diff_tracker::TurnDiffTracker;
use crate::user_notification::UserNotification;
use crate::util::backoff;
use serde_json::Value;
use crate::exec_command::ExecSessionManager;

/// The high-level interface to the Codex system.
/// It operates as a queue pair where you send submissions and receive events.
pub struct Codex {
    next_id: AtomicU64,
    tx_sub: Sender<Submission>,
    rx_event: Receiver<Event>,
}

/// Wrapper returned by [`Codex::spawn`] containing the spawned [`Codex`],
/// the submission id for the initial `ConfigureSession` request and the
/// unique session id.
pub struct CodexSpawnOk {
    pub codex: Codex,
    pub init_id: String,
    pub session_id: Uuid,
}

impl Codex {
    /// Spawn a new [`Codex`] and initialize the session.
    pub async fn spawn(config: Config, auth: Option<CodexAuth>) -> CodexResult<CodexSpawnOk> {
        // experimental resume path (undocumented)
        let resume_path = config.experimental_resume.clone();
        info!("resume_path: {resume_path:?}");
        // Use an unbounded submission queue to avoid any possibility of back‑pressure
        // between the TUI submit worker and the core loop during interrupts/cancels.
        let (tx_sub, rx_sub) = async_channel::unbounded();
        let (tx_event, rx_event) = async_channel::unbounded();

        let user_instructions = get_user_instructions(&config).await;

        let configure_session = Op::ConfigureSession {
            provider: config.model_provider.clone(),
            model: config.model.clone(),
            model_reasoning_effort: config.model_reasoning_effort,
            model_reasoning_summary: config.model_reasoning_summary,
            model_text_verbosity: config.model_text_verbosity,
            user_instructions,
            base_instructions: config.base_instructions.clone(),
            approval_policy: config.approval_policy,
            sandbox_policy: config.sandbox_policy.clone(),
            disable_response_storage: config.disable_response_storage,
            notify: config.notify.clone(),
            cwd: config.cwd.clone(),
            resume_path: resume_path.clone(),
        };

        let config = Arc::new(config);

        // Generate a unique ID for the lifetime of this Codex session.
        let session_id = Uuid::new_v4();

        // This task will run until Op::Shutdown is received.
        tokio::spawn(submission_loop(session_id, config, auth, rx_sub, tx_event));
        let codex = Codex {
            next_id: AtomicU64::new(0),
            tx_sub,
            rx_event,
        };
        let init_id = codex.submit(configure_session).await?;

        Ok(CodexSpawnOk {
            codex,
            init_id,
            session_id,
        })
    }

    /// Submit the `op` wrapped in a `Submission` with a unique ID.
    pub async fn submit(&self, op: Op) -> CodexResult<String> {
        let id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            .to_string();
        let sub = Submission { id: id.clone(), op };
        self.submit_with_id(sub).await?;
        Ok(id)
    }

    /// Use sparingly: prefer `submit()` so Codex is responsible for generating
    /// unique IDs for each submission.
    pub async fn submit_with_id(&self, sub: Submission) -> CodexResult<()> {
        self.tx_sub
            .send(sub)
            .await
            .map_err(|_| CodexErr::InternalAgentDied)?;
        Ok(())
    }

    pub async fn next_event(&self) -> CodexResult<Event> {
        let event = self
            .rx_event
            .recv()
            .await
            .map_err(|_| CodexErr::InternalAgentDied)?;
        Ok(event)
    }
}

/// Mutable state of the agent
#[derive(Default)]
struct State {
    approved_commands: HashSet<Vec<String>>,
    current_agent: Option<AgentAgent>,
    pending_approvals: HashMap<String, oneshot::Sender<ReviewDecision>>,
    pending_input: Vec<ResponseInputItem>,
    history: ConversationHistory,
    /// Tracks which completed agents (by id) have already been returned to the
    /// model for a given batch when using `agent_wait` without `return_all`.
    /// This enables sequential waiting behavior across multiple calls.
    seen_completed_agents_by_batch: HashMap<String, HashSet<String>>,
    /// Scratchpad that buffers streamed items/deltas for the current HTTP attempt
    /// so we can seed retries without losing progress.
    turn_scratchpad: Option<TurnScratchpad>,
    /// Per-submission monotonic event sequence (resets at TaskStarted)
    event_seq_by_sub_id: HashMap<String, u64>,
    /// 1-based ordinal of the current HTTP request attempt in this session.
    request_ordinal: u64,
}

/// Buffers partial turn progress produced during a single HTTP streaming attempt.
/// This is not recorded to persistent history. It is only used to seed retries
/// when the SSE stream disconnects mid‑turn.
#[derive(Default, Clone, Debug)]
struct TurnScratchpad {
    /// Output items that reached `response.output_item.done` during this attempt
    items: Vec<ResponseItem>,
    /// Tool outputs we produced locally in reaction to output items
    responses: Vec<ResponseInputItem>,
    /// Last assistant text fragment received via deltas (not yet finalized)
    partial_assistant_text: String,
    /// Last reasoning summary fragment received via deltas (not yet finalized)
    partial_reasoning_summary: String,
}

/// Context for an initialized model agent
///
/// A session has at most 1 running agent at a time, and can be interrupted by user input.
pub(crate) struct Session {
    client: ModelClient,
    tx_event: Sender<Event>,

    /// The session's current working directory. All relative paths provided by
    /// the model as well as sandbox policies are resolved against this path
    /// instead of `std::env::current_dir()`.
    cwd: PathBuf,
    base_instructions: Option<String>,
    user_instructions: Option<String>,
    approval_policy: AskForApproval,
    sandbox_policy: SandboxPolicy,
    shell_environment_policy: ShellEnvironmentPolicy,
    _writable_roots: Vec<PathBuf>,
    disable_response_storage: bool,
    tools_config: ToolsConfig,

    /// Manager for external MCP servers/tools.
    mcp_connection_manager: McpConnectionManager,
    #[allow(dead_code)]
    session_manager: ExecSessionManager,

    /// Configuration for available agent models
    agents: Vec<crate::config_types::AgentConfig>,

    /// External notifier command (will be passed as args to exec()). When
    /// `None` this feature is disabled.
    notify: Option<Vec<String>>,

    /// Optional rollout recorder for persisting the conversation transcript so
    /// sessions can be replayed or inspected later.
    rollout: Mutex<Option<RolloutRecorder>>,
    state: Mutex<State>,
    codex_linux_sandbox_exe: Option<PathBuf>,
    user_shell: shell::Shell,
    show_raw_agent_reasoning: bool,
    /// Pending browser screenshots to include in the next model request
    #[allow(dead_code)]
    pending_browser_screenshots: Mutex<Vec<PathBuf>>,
    /// Track the last system status to detect changes
    last_system_status: Mutex<Option<String>>,
    /// Track the last screenshot path and hash to detect changes
    last_screenshot_info: Mutex<Option<(PathBuf, Vec<u8>, Vec<u8>)>>, // (path, phash, dhash)
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

    pub(crate) fn get_cwd(&self) -> &Path {
        &self.cwd
    }

    pub(crate) fn get_sandbox_policy(&self) -> &SandboxPolicy {
        &self.sandbox_policy
    }

    fn resolve_path(&self, path: Option<String>) -> PathBuf {
        path.as_ref()
            .map(PathBuf::from)
            .map_or_else(|| self.cwd.clone(), |p| self.cwd.join(p))
    }

    // ────────────────────────────
    // Scratchpad helpers
    // ────────────────────────────
    fn begin_attempt_scratchpad(&self) {
        let mut state = self.state.lock().unwrap();
        state.turn_scratchpad = Some(TurnScratchpad::default());
    }

    /// Bump the per-session HTTP request attempt ordinal so `OrderMeta`
    /// reflects the correct provider request index for this attempt.
    fn begin_http_attempt(&self) {
        let mut state = self.state.lock().unwrap();
        state.request_ordinal = state.request_ordinal.saturating_add(1);
    }

    fn scratchpad_push(&self, item: &ResponseItem, response: &Option<ResponseInputItem>) {
        let mut state = self.state.lock().unwrap();
        if let Some(sp) = &mut state.turn_scratchpad {
            sp.items.push(item.clone());
            if let Some(r) = response {
                sp.responses.push(r.clone());
            }
        }
    }

    fn scratchpad_add_text_delta(&self, delta: &str) {
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

    fn scratchpad_add_reasoning_delta(&self, delta: &str) {
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

    fn scratchpad_clear_partial_message(&self) {
        let mut state = self.state.lock().unwrap();
        if let Some(sp) = &mut state.turn_scratchpad {
            sp.partial_assistant_text.clear();
        }
    }

    fn take_scratchpad(&self) -> Option<TurnScratchpad> {
        let mut state = self.state.lock().unwrap();
        state.turn_scratchpad.take()
    }

    fn clear_scratchpad(&self) {
        let mut state = self.state.lock().unwrap();
        state.turn_scratchpad = None;
    }
}

impl Session {
    pub fn set_agent(&self, agent: AgentAgent) {
        let mut state = self.state.lock().unwrap();
        if let Some(current_agent) = state.current_agent.take() {
            current_agent.abort(TurnAbortReason::Replaced);
        }
        state.current_agent = Some(agent);
    }

    pub fn remove_agent(&self, sub_id: &str) {
        let mut state = self.state.lock().unwrap();
        if let Some(agent) = &state.current_agent {
            if agent.sub_id == sub_id {
                state.current_agent.take();
            }
        }
    }

    /// Sends the given event to the client and swallows the send error, if
    /// any, logging it as an error.
    pub(crate) async fn send_event(&self, event: Event) {
        if let Err(e) = self.tx_event.send(event).await {
            error!("failed to send tool call event: {e}");
        }
    }

    /// Create a stamped Event with a per-turn sequence number.
    fn make_event(&self, sub_id: &str, msg: EventMsg) -> Event {
        let mut state = self.state.lock().unwrap();
        let seq = match msg {
            EventMsg::TaskStarted => {
                // Reset per-sub_id sequence at the start of a turn.
                // We increment request_ordinal per HTTP attempt instead
                // (see `begin_http_attempt`).
                let e = state
                    .event_seq_by_sub_id
                    .entry(sub_id.to_string())
                    .or_insert(0);
                *e = 0;
                0
            }
            _ => {
                let e = state
                    .event_seq_by_sub_id
                    .entry(sub_id.to_string())
                    .or_insert(0);
                *e = e.saturating_add(1);
                *e
            }
        };
        Event { id: sub_id.to_string(), event_seq: seq, msg, order: None }
    }

    /// Same as make_event but allows supplying a provider sequence_number
    /// (e.g., Responses API SSE event). We DO NOT overwrite `event_seq`
    /// with this hint because `event_seq` must remain monotonic per turn
    /// and local to our runtime. Provider ordering is carried via
    /// `OrderMeta` when applicable.
    fn make_event_with_hint(&self, sub_id: &str, msg: EventMsg, _seq_hint: Option<u64>) -> Event {
        // Preserve the monotonic invariant of event_seq by delegating to make_event.
        // Any ordering hints from the provider should be conveyed through
        // OrderMeta (see make_event_with_order) rather than event_seq.
        self.make_event(sub_id, msg)
    }

    fn make_event_with_order(
        &self,
        sub_id: &str,
        msg: EventMsg,
        order: crate::protocol::OrderMeta,
        seq_hint: Option<u64>,
    ) -> Event {
        let mut ev = self.make_event_with_hint(sub_id, msg, seq_hint);
        ev.order = Some(order);
        ev
    }

    // Kept private helpers focused on ctx-based flow to avoid misuse.

    pub(crate) async fn send_ordered_from_ctx(&self, ctx: &ToolCallCtx, msg: EventMsg) {
        let order = ctx.order_meta(self.current_request_ordinal());
        let ev = self.make_event_with_order(&ctx.sub_id, msg, order, ctx.seq_hint);
        let _ = self.tx_event.send(ev).await;
    }

    fn current_request_ordinal(&self) -> u64 {
        let state = self.state.lock().unwrap();
        state.request_ordinal
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

    pub fn add_approved_command(&self, cmd: Vec<String>) {
        let mut state = self.state.lock().unwrap();
        state.approved_commands.insert(cmd);
    }

    /// Records items to both the rollout and the chat completions/ZDR
    /// transcript, if enabled.
    async fn record_conversation_items(&self, items: &[ResponseItem]) {
        debug!("Recording items for conversation: {items:?}");
        self.record_state_snapshot(items).await;

        self.state.lock().unwrap().history.record_items(items);
    }

    /// Clean up old screenshots and system status messages from conversation history
    /// This is called when a new user message arrives to keep history manageable
    async fn cleanup_old_status_items(&self) {
        let mut state = self.state.lock().unwrap();

        // Get current history items
        let current_items = state.history.contents();

        // Track various message types and their positions
        let mut real_user_messages = Vec::new(); // Non-status user messages
        let mut status_messages = Vec::new(); // Messages with screenshots or status

        for (idx, item) in current_items.iter().enumerate() {
            match item {
                ResponseItem::Message { role, content, .. } if role == "user" => {
                    // Check message content
                    let has_status = content.iter().any(|c| {
                        if let ContentItem::InputText { text } = c {
                            text.contains("== System Status ==")
                                || text.contains("Current working directory:")
                                || text.contains("Git branch:")
                        } else {
                            false
                        }
                    });

                    let has_screenshot = content
                        .iter()
                        .any(|c| matches!(c, ContentItem::InputImage { .. }));

                    let has_real_text = content.iter().any(|c| {
                        if let ContentItem::InputText { text } = c {
                            // Real user text doesn't contain system status markers
                            !text.contains("== System Status ==")
                                && !text.contains("Current working directory:")
                                && !text.contains("Git branch:")
                                && !text.trim().is_empty()
                        } else {
                            false
                        }
                    });

                    if has_real_text && !has_status && !has_screenshot {
                        // This is a real user message
                        real_user_messages.push(idx);
                    } else if has_status || has_screenshot {
                        // This is a status/screenshot message
                        status_messages.push(idx);
                    }
                }
                _ => {}
            }
        }

        // Find screenshots to keep: last 2 that directly follow real user commands
        let mut screenshots_to_keep = std::collections::HashSet::new();

        // Work backwards through real user messages
        for &user_idx in real_user_messages.iter().rev().take(2) {
            // Find the first status message after this user message
            for &status_idx in status_messages.iter() {
                if status_idx > user_idx {
                    // Check if this status message contains a screenshot
                    if let Some(ResponseItem::Message { content, .. }) =
                        current_items.get(status_idx)
                    {
                        let has_screenshot = content
                            .iter()
                            .any(|c| matches!(c, ContentItem::InputImage { .. }));
                        if has_screenshot {
                            screenshots_to_keep.insert(status_idx);
                            break; // Only keep one screenshot per user message
                        }
                    }
                }
            }
        }

        // Build the filtered history
        let mut items_to_keep = Vec::new();
        let mut removed_screenshots = 0;
        let mut removed_status = 0;

        for (idx, item) in current_items.iter().enumerate() {
            let should_keep = if status_messages.contains(&idx) {
                // This is a status/screenshot message
                if screenshots_to_keep.contains(&idx) {
                    true // Keep this screenshot
                } else {
                    // Count what we're removing
                    if let ResponseItem::Message { content, .. } = item {
                        let has_screenshot = content
                            .iter()
                            .any(|c| matches!(c, ContentItem::InputImage { .. }));
                        if has_screenshot {
                            removed_screenshots += 1;
                        } else {
                            removed_status += 1;
                        }
                    }
                    false // Remove this status/screenshot
                }
            } else {
                true // Keep all non-status messages (real user messages, assistant messages, etc.)
            };

            if should_keep {
                items_to_keep.push(item.clone());
            }
        }

        // Replace the history with cleaned items
        state.history = ConversationHistory::new();
        state.history.record_items(&items_to_keep);

        if removed_screenshots > 0 || removed_status > 0 {
            info!(
                "Cleaned up history: removed {} old screenshots and {} status messages, kept {} recent screenshots",
                removed_screenshots,
                removed_status,
                screenshots_to_keep.len()
            );
        }
    }

    async fn record_state_snapshot(&self, items: &[ResponseItem]) {
        let snapshot = { crate::rollout::SessionStateSnapshot {} };

        let recorder = {
            let guard = self.rollout.lock().unwrap();
            guard.as_ref().cloned()
        };

        if let Some(rec) = recorder {
            if let Err(e) = rec.record_state(snapshot).await {
                error!("failed to record rollout state: {e:#}");
            }
            if let Err(e) = rec.record_items(items).await {
                error!("failed to record rollout items: {e:#}");
            }
        }
    }

    async fn on_exec_command_begin(
        &self,
        turn_diff_tracker: &mut TurnDiffTracker,
        exec_command_context: ExecCommandContext,
        seq_hint: Option<u64>,
        output_index: Option<u32>,
        attempt_req: u64,
    ) {
        let ExecCommandContext {
            sub_id,
            call_id,
            command_for_display,
            cwd,
            apply_patch,
        } = exec_command_context;
        let msg = match apply_patch {
            Some(ApplyPatchCommandContext {
                user_explicitly_approved_this_action,
                changes,
            }) => {
                turn_diff_tracker.on_patch_begin(&changes);

                EventMsg::PatchApplyBegin(PatchApplyBeginEvent {
                    call_id,
                    auto_approved: !user_explicitly_approved_this_action,
                    changes,
                })
            }
            None => EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
                call_id,
                command: command_for_display.clone(),
                cwd,
                parsed_cmd: parse_command(&command_for_display),
            }),
        };
        let order = crate::protocol::OrderMeta { request_ordinal: attempt_req, output_index, sequence_number: seq_hint };
        let event = self.make_event_with_order(&sub_id, msg, order, seq_hint);
        let _ = self.tx_event.send(event).await;
    }

    async fn on_exec_command_end(
        &self,
        turn_diff_tracker: &mut TurnDiffTracker,
        sub_id: &str,
        call_id: &str,
        output: &ExecToolCallOutput,
        is_apply_patch: bool,
        seq_hint: Option<u64>,
        output_index: Option<u32>,
        attempt_req: u64,
    ) {
        let ExecToolCallOutput {
            stdout,
            stderr,
            aggregated_output: _,
            duration,
            exit_code,
        } = output;
        // Because stdout and stderr could each be up to 100 KiB, we send
        // truncated versions.
        const MAX_STREAM_OUTPUT: usize = 5 * 1024; // 5KiB
        let stdout = stdout.text.chars().take(MAX_STREAM_OUTPUT).collect();
        let stderr = stderr.text.chars().take(MAX_STREAM_OUTPUT).collect();
        // Precompute formatted output if needed in future for logging/pretty UI.

        let msg = if is_apply_patch {
            EventMsg::PatchApplyEnd(PatchApplyEndEvent {
                call_id: call_id.to_string(),
                stdout,
                stderr,
                success: *exit_code == 0,
            })
        } else {
            EventMsg::ExecCommandEnd(ExecCommandEndEvent {
                call_id: call_id.to_string(),
                stdout,
                stderr,
                exit_code: *exit_code,
                duration: *duration,
            })
        };
        let order = crate::protocol::OrderMeta { request_ordinal: attempt_req, output_index, sequence_number: seq_hint };
        let event = self.make_event_with_order(sub_id, msg, order, seq_hint);
        let _ = self.tx_event.send(event).await;

        // If this is an apply_patch, after we emit the end patch, emit a second event
        // with the full turn diff if there is one.
        if is_apply_patch {
            let unified_diff = turn_diff_tracker.get_unified_diff();
            if let Ok(Some(unified_diff)) = unified_diff {
                let msg = EventMsg::TurnDiff(TurnDiffEvent { unified_diff });
                let event = self.make_event(sub_id, msg);
                let _ = self.tx_event.send(event).await;
            }
        }
    }
    /// Runs the exec tool call and emits events for the begin and end of the
    /// command even on error.
    ///
    /// Returns the output of the exec tool call.
    async fn run_exec_with_events<'a>(
        &self,
        turn_diff_tracker: &mut TurnDiffTracker,
        begin_ctx: ExecCommandContext,
        exec_args: ExecInvokeArgs<'a>,
        seq_hint: Option<u64>,
        output_index: Option<u32>,
        attempt_req: u64,
    ) -> crate::error::Result<ExecToolCallOutput> {
        let is_apply_patch = begin_ctx.apply_patch.is_some();
        let sub_id = begin_ctx.sub_id.clone();
        let call_id = begin_ctx.call_id.clone();

        self.on_exec_command_begin(turn_diff_tracker, begin_ctx.clone(), seq_hint, output_index, attempt_req)
            .await;

            let result = process_exec_tool_call(
                exec_args.params,
                exec_args.sandbox_type,
                exec_args.sandbox_policy,
                exec_args.codex_linux_sandbox_exe,
                exec_args.stdout_stream,
            )
            .await;

        let output_stderr;
        let borrowed: &ExecToolCallOutput = match &result {
            Ok(output) => output,
            Err(e) => {
                output_stderr = ExecToolCallOutput {
                    exit_code: -1,
                    stdout: StreamOutput::new(String::new()),
                    stderr: StreamOutput::new(get_error_message_ui(e)),
                    aggregated_output: StreamOutput::new(get_error_message_ui(e)),
                    duration: Duration::default(),
                };
                &output_stderr
            }
        };
        self.on_exec_command_end(
            turn_diff_tracker,
            &sub_id,
            &call_id,
            borrowed,
            is_apply_patch,
            seq_hint.map(|h| h.saturating_add(1)),
            output_index,
            attempt_req,
        )
        .await;

        result
    }

    /// Helper that emits a BackgroundEvent with the given message. This keeps
    /// the call‑sites terse so adding more diagnostics does not clutter the
    /// core agent logic.
    async fn notify_background_event(&self, sub_id: &str, message: impl Into<String>) {
        let event = self.make_event(
            sub_id,
            EventMsg::BackgroundEvent(BackgroundEventEvent { message: message.into() }),
        );
        let _ = self.tx_event.send(event).await;
    }

    async fn notify_stream_error(&self, sub_id: &str, message: impl Into<String>) {
        let event = self.make_event(
            sub_id,
            EventMsg::Error(ErrorEvent { message: message.into() }),
        );
        let _ = self.tx_event.send(event).await;
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

        // Filter out browser screenshots from historical messages
        // We identify them by the [EPHEMERAL:...] marker that precedes them
        let filtered_history: Vec<ResponseItem> = history
            .into_iter()
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

        // Concatenate filtered history with current turn's extras (which includes current ephemeral images)
        let result = [filtered_history, extra].concat();

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

    /// Returns the input if there was no agent running to inject into
    pub fn inject_input(&self, input: Vec<InputItem>) -> Result<(), Vec<InputItem>> {
        let mut state = self.state.lock().unwrap();
        if state.current_agent.is_some() {
            state
                .pending_input
                .push(response_input_from_core_items(input));
            Ok(())
        } else {
            Err(input)
        }
    }

    pub fn get_pending_input(&self) -> Vec<ResponseInputItem> {
        let mut state = self.state.lock().unwrap();
        if state.pending_input.is_empty() {
            Vec::with_capacity(0)
        } else {
            let mut ret = Vec::new();
            std::mem::swap(&mut ret, &mut state.pending_input);
            ret
        }
    }

    pub fn add_pending_input(&self, input: ResponseInputItem) {
        let mut state = self.state.lock().unwrap();
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

    fn abort(&self) {
        info!("Aborting existing session");
        // (debug removed)

        let mut state = self.state.lock().unwrap();
        // (debug removed)
        state.pending_approvals.clear();
        // Do not clear `pending_input` here. When a user submits a new message
        // immediately after an interrupt, it may have been routed to
        // `pending_input` by an earlier code path. Clearing it would drop the
        // user's message and prevent the next turn from ever starting.
        state.turn_scratchpad = None;
        // Take current agent while holding the lock, then drop the lock BEFORE calling abort
        let current = state.current_agent.take();
        drop(state);
        if let Some(agent) = current {
            agent.abort(TurnAbortReason::Interrupted);
            // (debug removed)
        } else {
            // (debug removed)
        }
        // Also terminate any running exec sessions (PTY-based) so child processes do not linger.
        // Best-effort cleanup for PTY-based exec sessions would go here. The
        // PTY implementation already kills processes on session drop; in the
        // common LocalShellCall path we also kill processes immediately via
        // KillOnDrop in exec.rs.

        // (debug removed)
    }

    /// Spawn the configured notifier (if any) with the given JSON payload as
    /// the last argument. Failures are logged but otherwise ignored so that
    /// notification issues do not interfere with the main workflow.
    fn maybe_notify(&self, notification: UserNotification) {
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
        if let Err(e) = command.spawn() {
            warn!("failed to spawn notifier '{}': {e}", notify_command[0]);
        }
    }
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
            ..Default::default()
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ExecCommandContext {
    pub(crate) sub_id: String,
    pub(crate) call_id: String,
    pub(crate) command_for_display: Vec<String>,
    pub(crate) cwd: PathBuf,
    pub(crate) apply_patch: Option<ApplyPatchCommandContext>,
}

#[derive(Clone, Debug)]
pub(crate) struct ApplyPatchCommandContext {
    pub(crate) user_explicitly_approved_this_action: bool,
    pub(crate) changes: HashMap<PathBuf, FileChange>,
}

/// A series of Turns in response to user input.
pub(crate) struct AgentAgent {
    sess: Arc<Session>,
    sub_id: String,
    handle: AbortHandle,
}

impl AgentAgent {
    fn spawn(sess: Arc<Session>, sub_id: String, input: Vec<InputItem>) -> Self {
        let handle = tokio::spawn(run_agent(Arc::clone(&sess), sub_id.clone(), input)).abort_handle();
        Self {
            sess,
            sub_id,
            handle,
        }
    }

    fn compact(
        sess: Arc<Session>,
        sub_id: String,
        input: Vec<InputItem>,
        compact_instructions: String,
    ) -> Self {
        let handle = tokio::spawn(run_compact_agent(
            Arc::clone(&sess),
            sub_id.clone(),
            input,
            compact_instructions,
        ))
        .abort_handle();
        Self {
            sess,
            sub_id,
            handle,
        }
    }

    fn abort(self, _reason: TurnAbortReason) {
        // TOCTOU?
        if !self.handle.is_finished() {
            self.handle.abort();
            let stamped = self
                .sess
                .make_event(&self.sub_id, EventMsg::Error(ErrorEvent { message: "Turn interrupted".to_string() }));
            let tx_event = self.sess.tx_event.clone();
            tokio::spawn(async move {
                tx_event.send(stamped).await.ok();
            });
        }
    }
}

async fn submission_loop(
    mut session_id: Uuid,
    config: Arc<Config>,
    auth: Option<CodexAuth>,
    rx_sub: Receiver<Submission>,
    tx_event: Sender<Event>,
) {
    let mut sess: Option<Arc<Session>> = None;
    let mut agent_manager_initialized = false;
    // shorthand - send an event when there is no active session
    let send_no_session_event = |sub_id: String| async {
        let event = Event {
            id: sub_id,
            event_seq: 0,
            msg: EventMsg::Error(ErrorEvent { message: "No session initialized, expected 'ConfigureSession' as first Op".to_string() }),
            order: None,
        };
        tx_event.send(event).await.ok();
    };

    // To break out of this loop, send Op::Shutdown.
    while let Ok(sub) = rx_sub.recv().await {
        debug!(?sub, "Submission");
        // (submission diagnostics removed)
        match sub.op {
            Op::Interrupt => {
                let sess = match sess.as_ref() {
                    Some(sess) => sess.clone(),
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };
                tokio::spawn(async move { sess.abort() });
            }
            Op::ConfigureSession {
                provider,
                model,
                model_reasoning_effort,
                model_reasoning_summary,
                model_text_verbosity,
                user_instructions,
                base_instructions,
                approval_policy,
                sandbox_policy,
                disable_response_storage,
                notify,
                cwd,
                resume_path,
            } => {
                debug!(
                    "Configuring session: model={model}; provider={provider:?}; resume={resume_path:?}"
                );
                if !cwd.is_absolute() {
                    let message = format!("cwd is not absolute: {cwd:?}");
                    error!(message);
                    let event = Event { id: sub.id, event_seq: 0, msg: EventMsg::Error(ErrorEvent { message }), order: None };
                    if let Err(e) = tx_event.send(event).await {
                        error!("failed to send error message: {e:?}");
                    }
                    return;
                }
                // Optionally resume an existing rollout.
                let mut restored_items: Option<Vec<ResponseItem>> = None;
                let rollout_recorder: Option<RolloutRecorder> =
                    if let Some(path) = resume_path.as_ref() {
                        match RolloutRecorder::resume(&config, path).await {
                            Ok((rec, saved)) => {
                                session_id = saved.session_id;
                                if !saved.items.is_empty() {
                                    restored_items = Some(saved.items);
                                }
                                Some(rec)
                            }
                            Err(e) => {
                                warn!("failed to resume rollout from {path:?}: {e}");
                                None
                            }
                        }
                    } else {
                        None
                    };

                let rollout_recorder = match rollout_recorder {
                    Some(rec) => Some(rec),
                    None => {
                        match RolloutRecorder::new(&config, session_id, user_instructions.clone())
                            .await
                        {
                            Ok(r) => Some(r),
                            Err(e) => {
                                warn!("failed to initialise rollout recorder: {e}");
                                None
                            }
                        }
                    }
                };

                // Create debug logger based on config
                let debug_logger = match crate::debug_logger::DebugLogger::new(config.debug) {
                    Ok(logger) => std::sync::Arc::new(std::sync::Mutex::new(logger)),
                    Err(e) => {
                        warn!("Failed to create debug logger: {}", e);
                        // Create a disabled logger as fallback
                        std::sync::Arc::new(std::sync::Mutex::new(
                            crate::debug_logger::DebugLogger::new(false).unwrap(),
                        ))
                    }
                };

                // Wrap provided auth (if any) in a minimal AuthManager for client usage.
                let auth_manager = auth
                    .as_ref()
                    .map(|a| crate::AuthManager::from_auth_for_testing(a.clone()));
                let client = ModelClient::new(
                    config.clone(),
                    auth_manager,
                    provider.clone(),
                    model_reasoning_effort,
                    model_reasoning_summary,
                    model_text_verbosity,
                    session_id,
                    debug_logger,
                );

                // abort any current running session and clone its state
                let state = match sess.take() {
                    Some(sess) => {
                        sess.abort();
                        sess.state.lock().unwrap().partial_clone()
                    }
                    None => State {
                        history: ConversationHistory::new(),
                        ..Default::default()
                    },
                };

                let writable_roots = get_writable_roots(&cwd);

                // Error messages to dispatch after SessionConfigured is sent.
                let mut mcp_connection_errors = Vec::<Event>::new();
                let (mcp_connection_manager, failed_clients) =
                    match McpConnectionManager::new(config.mcp_servers.clone()).await {
                        Ok((mgr, failures)) => (mgr, failures),
                        Err(e) => {
                            let message = format!("Failed to create MCP connection manager: {e:#}");
                            error!("{message}");
                            mcp_connection_errors.push(Event { id: sub.id.clone(), event_seq: 0, msg: EventMsg::Error(ErrorEvent { message }), order: None });
                            (McpConnectionManager::default(), Default::default())
                        }
                    };

                // Surface individual client start-up failures to the user.
                if !failed_clients.is_empty() {
                    for (server_name, err) in failed_clients {
                        let message =
                            format!("MCP client for `{server_name}` failed to start: {err:#}");
                        error!("{message}");
                        mcp_connection_errors.push(Event { id: sub.id.clone(), event_seq: 0, msg: EventMsg::Error(ErrorEvent { message }), order: None });
                    }
                }
                let default_shell = shell::default_user_shell().await;
                let mut tools_config = ToolsConfig::new(
                    &config.model_family,
                    approval_policy,
                    sandbox_policy.clone(),
                    config.include_plan_tool,
                    config.include_apply_patch_tool,
                    config.tools_web_search_request,
                    config.use_experimental_streamable_shell_tool,
                    config.include_view_image_tool,
                );
                tools_config.web_search_allowed_domains =
                    config.tools_web_search_allowed_domains.clone();

                sess = Some(Arc::new(Session {
                    client,
                    tools_config,
                    tx_event: tx_event.clone(),
                    user_instructions,
                    base_instructions,
                    approval_policy,
                    sandbox_policy,
                    shell_environment_policy: config.shell_environment_policy.clone(),
                    cwd,
                    _writable_roots: writable_roots,
                    mcp_connection_manager,
                    session_manager: crate::exec_command::ExecSessionManager::default(),
                    agents: config.agents.clone(),
                    notify,
                    state: Mutex::new(state),
                    rollout: Mutex::new(rollout_recorder),
                    codex_linux_sandbox_exe: config.codex_linux_sandbox_exe.clone(),
                    disable_response_storage,
                    user_shell: default_shell,
                    show_raw_agent_reasoning: config.show_raw_agent_reasoning,
                    pending_browser_screenshots: Mutex::new(Vec::new()),
                    last_system_status: Mutex::new(None),
                    last_screenshot_info: Mutex::new(None),
                }));

                // Patch restored state into the newly created session.
                if let Some(sess_arc) = &sess {
                    if let Some(items) = &restored_items {
                        let mut st = sess_arc.state.lock().unwrap();
                        st.history.record_items(items.iter());
                    }
                }

                // Gather history metadata for SessionConfiguredEvent.
                let (history_log_id, history_entry_count) =
                    crate::message_history::history_metadata(&config).await;

                // ack
                let events = std::iter::once(Event {
                    id: INITIAL_SUBMIT_ID.to_string(),
                    event_seq: 0,
                    msg: EventMsg::SessionConfigured(SessionConfiguredEvent {
                        session_id,
                        model,
                        history_log_id,
                        history_entry_count,
                    }),
                    order: None,
                })
                .chain(mcp_connection_errors.into_iter());
                for event in events {
                    if let Err(e) = tx_event.send(event).await {
                        error!("failed to send event: {e:?}");
                    }
                }
                // If we resumed from a rollout, replay the prior transcript into the UI.
                if let Some(items) = restored_items {
                    let event = Event { id: sub.id.clone(), event_seq: 0, msg: EventMsg::ReplayHistory(crate::protocol::ReplayHistoryEvent { items }), order: None };
                    if let Err(e) = tx_event.send(event).await {
                        warn!("failed to send ReplayHistory event: {e}");
                    }
                }
                
                // Initialize agent manager after SessionConfigured is sent
                if !agent_manager_initialized {
                    let mut manager = AGENT_MANAGER.write().await;
                    let (agent_tx, mut agent_rx) = tokio::sync::mpsc::unbounded_channel();
                    manager.set_event_sender(agent_tx);
                    drop(manager);

                    // Forward agent events to the main event channel
                    let tx_event_clone = tx_event.clone();
                    tokio::spawn(async move {
                        while let Some(event) = agent_rx.recv().await {
                            let _ = tx_event_clone.send(event).await;
                        }
                    });
                    agent_manager_initialized = true;
                }
            }
            Op::UserInput { items } => {
                let sess = match sess.as_ref() {
                    Some(sess) => sess,
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };

                // Clean up old status items when new user input arrives
                // This prevents token buildup from old screenshots/status messages
                sess.cleanup_old_status_items().await;

                // Abort synchronously here to avoid a race that can kill the
                // newly spawned agent if the async abort runs after set_agent.
                sess.abort();

                // Spawn a new agent for this user input.
                let agent = AgentAgent::spawn(Arc::clone(sess), sub.id, items);
                sess.set_agent(agent);
            }
            Op::ExecApproval { id, decision } => {
                let sess = match sess.as_ref() {
                    Some(sess) => sess,
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };
                match decision {
                    ReviewDecision::Abort => {
                        sess.abort();
                    }
                    other => sess.notify_approval(&id, other),
                }
            }
            Op::PatchApproval { id, decision } => {
                let sess = match sess.as_ref() {
                    Some(sess) => sess,
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };
                match decision {
                    ReviewDecision::Abort => {
                        sess.abort();
                    }
                    other => sess.notify_approval(&id, other),
                }
            }
            Op::AddToHistory { text } => {
                // TODO: What should we do if we got AddToHistory before ConfigureSession?
                // currently, if ConfigureSession has resume path, this history will be ignored
                let id = session_id;
                let config = config.clone();
                tokio::spawn(async move {
                    if let Err(e) = crate::message_history::append_entry(&text, &id, &config).await
                    {
                        warn!("failed to append to message history: {e}");
                    }
                });
            }

            Op::GetHistoryEntryRequest { offset, log_id } => {
                let config = config.clone();
                let tx_event = tx_event.clone();
                let sub_id = sub.id.clone();

                tokio::spawn(async move {
                    // Run lookup in blocking thread because it does file IO + locking.
                    let entry_opt = tokio::task::spawn_blocking(move || {
                        crate::message_history::lookup(log_id, offset, &config)
                    })
                    .await
                    .unwrap_or(None);

                    let event = Event {
                        id: sub_id,
                        event_seq: 0,
                        msg: EventMsg::GetHistoryEntryResponse(
                            crate::protocol::GetHistoryEntryResponseEvent {
                                offset,
                                log_id,
                                entry: entry_opt,
                            },
                        ),
                        order: None,
                    };

                    if let Err(e) = tx_event.send(event).await {
                        warn!("failed to send GetHistoryEntryResponse event: {e}");
                    }
                });
            }
            // Upstream protocol no longer includes ListMcpTools; skip handling here.
            Op::Compact => {
                let sess = match sess.as_ref() {
                    Some(sess) => sess,
                    None => {
                        send_no_session_event(sub.id).await;
                        continue;
                    }
                };

                // Create a summarization request as user input
                const SUMMARIZATION_PROMPT: &str = include_str!("prompt_for_compact_command.md");

                // Attempt to inject input into current agent
                if let Err(items) = sess.inject_input(vec![InputItem::Text {
                    text: "Start Summarization".to_string(),
                }]) {
                    let agent = AgentAgent::compact(
                        sess.clone(),
                        sub.id,
                        items,
                        SUMMARIZATION_PROMPT.to_string(),
                    );
                    sess.set_agent(agent);
                }
            }
            Op::Shutdown => {
                info!("Shutting down Codex instance");

                // Ensure any running agent is aborted so streaming stops promptly.
                if let Some(sess_arc) = sess.as_ref() {
                    let s2 = sess_arc.clone();
                    tokio::spawn(async move { s2.abort(); });
                }

                // Gracefully flush and shutdown rollout recorder on session end so tests
                // that inspect the rollout file do not race with the background writer.
                if let Some(sess_arc) = sess {
                    let recorder_opt = sess_arc.rollout.lock().unwrap().take();
                    if let Some(rec) = recorder_opt {
                        if let Err(e) = rec.shutdown().await {
                            warn!("failed to shutdown rollout recorder: {e}");
                            let event = Event { id: sub.id.clone(), event_seq: 0, msg: EventMsg::Error(ErrorEvent { message: "Failed to shutdown rollout recorder".to_string() }), order: None };
                            if let Err(e) = tx_event.send(event).await {
                                warn!("failed to send error message: {e:?}");
                            }
                        }
                    }
                }
                let event = Event { id: sub.id.clone(), event_seq: 0, msg: EventMsg::ShutdownComplete, order: None };
                if let Err(e) = tx_event.send(event).await {
                    warn!("failed to send Shutdown event: {e}");
                }
                break;
            }
        }
    }
    debug!("Agent loop exited");
}

/// Takes a user message as input and runs a loop where, at each turn, the model
/// replies with either:
///
/// - requested function calls
/// - an assistant message
///
/// While it is possible for the model to return multiple of these items in a
/// single turn, in practice, we generally one item per turn:
///
/// - If the model requests a function call, we execute it and send the output
///   back to the model in the next turn.
/// - If the model sends only an assistant message, we record it in the
///   conversation history and consider the agent complete.
async fn run_agent(sess: Arc<Session>, sub_id: String, input: Vec<InputItem>) {
    if input.is_empty() {
        return;
    }
    let event = sess.make_event(&sub_id, EventMsg::TaskStarted);
    if sess.tx_event.send(event).await.is_err() {
        return;
    }

    // Debug logging for ephemeral images
    let ephemeral_count = input
        .iter()
        .filter(|item| matches!(item, InputItem::EphemeralImage { .. }))
        .count();

    if ephemeral_count > 0 {
        tracing::info!(
            "Processing {} ephemeral images in user input",
            ephemeral_count
        );
    }

    // Convert input to ResponseInputItem
    let initial_input_for_turn: ResponseInputItem = response_input_from_core_items(input);
    let initial_response_item: ResponseItem = initial_input_for_turn.clone().into();

    // Record to history but we'll handle ephemeral images separately
    sess.record_conversation_items(&[initial_response_item.clone()])
        .await;

    let mut last_task_message: Option<String> = None;
    // Although from the perspective of codex.rs, TurnDiffTracker has the lifecycle of a Agent which contains
    // many turns, from the perspective of the user, it is a single turn.
    let mut turn_diff_tracker = TurnDiffTracker::new();

    // Track if this is the first iteration - if so, include the initial input
    let mut first_iteration = true;

    loop {
        // Note that pending_input would be something like a message the user
        // submitted through the UI while the model was running. Though the UI
        // may support this, the model might not.
        let pending_input = sess
            .get_pending_input()
            .into_iter()
            .map(ResponseItem::from)
            .collect::<Vec<ResponseItem>>();

        // Do not duplicate the initial input in `pending_input`.
        // It is already recorded to history above; ephemeral items are appended separately.
        if first_iteration {
            first_iteration = false;
        } else {
            // Only record pending input to history on subsequent iterations
            sess.record_conversation_items(&pending_input).await;
        }

        // Construct the input that we will send to the model. When using the
        // Chat completions API (or ZDR clients), the model needs the full
        // conversation history on each turn. The rollout file, however, should
        // only record the new items that originated in this turn so that it
        // represents an append-only log without duplicates.
        let turn_input: Vec<ResponseItem> = sess.turn_input_with_history(pending_input);

        let turn_input_messages: Vec<String> = turn_input
            .iter()
            .filter_map(|item| match item {
                ResponseItem::Message { content, .. } => Some(content),
                _ => None,
            })
            .flat_map(|content| {
                content.iter().filter_map(|item| match item {
                    ContentItem::OutputText { text } => Some(text.clone()),
                    _ => None,
                })
            })
            .collect();
        match run_turn(&sess, &mut turn_diff_tracker, sub_id.clone(), turn_input).await {
            Ok(turn_output) => {
                let mut items_to_record_in_conversation_history = Vec::<ResponseItem>::new();
                let mut responses = Vec::<ResponseInputItem>::new();
                for processed_response_item in turn_output {
                    let ProcessedResponseItem { item, response } = processed_response_item;
                    match (&item, &response) {
                        (ResponseItem::Message { role, .. }, None) if role == "assistant" => {
                            // If the model returned a message, we need to record it.
                            items_to_record_in_conversation_history.push(item);
                        }
                        (
                            ResponseItem::LocalShellCall { .. },
                            Some(ResponseInputItem::FunctionCallOutput { call_id, output }),
                        ) => {
                            items_to_record_in_conversation_history.push(item);
                            items_to_record_in_conversation_history.push(
                                ResponseItem::FunctionCallOutput {
                                    call_id: call_id.clone(),
                                    output: output.clone(),
                                },
                            );
                        }
                        (
                            ResponseItem::FunctionCall { .. },
                            Some(ResponseInputItem::FunctionCallOutput { call_id, output }),
                        ) => {
                            debug!(
                                "Recording function call and output for call_id: {}",
                                call_id
                            );
                            items_to_record_in_conversation_history.push(item);
                            items_to_record_in_conversation_history.push(
                                ResponseItem::FunctionCallOutput {
                                    call_id: call_id.clone(),
                                    output: output.clone(),
                                },
                            );
                        }
                        (
                            ResponseItem::CustomToolCall { .. },
                            Some(ResponseInputItem::CustomToolCallOutput { call_id, output }),
                        ) => {
                            items_to_record_in_conversation_history.push(item);
                            items_to_record_in_conversation_history.push(
                                ResponseItem::CustomToolCallOutput {
                                    call_id: call_id.clone(),
                                    output: output.clone(),
                                },
                            );
                        }
                        (
                            ResponseItem::FunctionCall { .. },
                            Some(ResponseInputItem::McpToolCallOutput { call_id, result }),
                        ) => {
                            items_to_record_in_conversation_history.push(item);
                            let output =
                                convert_call_tool_result_to_function_call_output_payload(&result);
                            items_to_record_in_conversation_history.push(
                                ResponseItem::FunctionCallOutput {
                                    call_id: call_id.clone(),
                                    output,
                                },
                            );
                        }
                        (
                            ResponseItem::Reasoning {
                                id,
                                summary,
                                content,
                                encrypted_content,
                            },
                            None,
                        ) => {
                            items_to_record_in_conversation_history.push(ResponseItem::Reasoning {
                                id: id.clone(),
                                summary: summary.clone(),
                                content: content.clone(),
                                encrypted_content: encrypted_content.clone(),
                            });
                        }
                        _ => {
                            warn!("Unexpected response item: {item:?} with response: {response:?}");
                        }
                    };
                    if let Some(response) = response {
                        responses.push(response);
                    }
                }

                // Only attempt to take the lock if there is something to record.
                if !items_to_record_in_conversation_history.is_empty() {
                    // Record items in their original chronological order to maintain
                    // proper sequence of events. This ensures function calls and their
                    // outputs appear in the correct order in conversation history.
                    sess.record_conversation_items(&items_to_record_in_conversation_history)
                        .await;
                }

                // If there are responses, add them to pending input for the next iteration
                if !responses.is_empty() {
                    for response in &responses {
                        sess.add_pending_input(response.clone());
                    }
                }

                if responses.is_empty() {
                    debug!("Turn completed");
                    last_task_message = get_last_assistant_message_from_turn(
                        &items_to_record_in_conversation_history,
                    );
                    if let Some(m) = last_task_message.as_ref() {
                        tracing::info!("core.turn completed: last_assistant_message.len={}", m.len());
                    }
                    sess.maybe_notify(UserNotification::AgentTurnComplete {
                        turn_id: sub_id.clone(),
                        input_messages: turn_input_messages,
                        last_assistant_message: last_task_message.clone(),
                    });
                    break;
                }
            }
            Err(e) => {
                info!("Turn error: {e:#}");
                let event = Event { id: sub_id.clone(), event_seq: 0, msg: EventMsg::Error(ErrorEvent { message: e.to_string() }), order: None };
                sess.tx_event.send(event).await.ok();
                // let the user continue the conversation
                break;
            }
        }
    }
    sess.remove_agent(&sub_id);
    let event = Event { id: sub_id, event_seq: 0, msg: EventMsg::TaskComplete(TaskCompleteEvent { last_agent_message: last_task_message }), order: None };
    match &event.msg {
        EventMsg::TaskComplete(TaskCompleteEvent { last_agent_message: Some(m) }) => {
            tracing::info!("core.emit TaskComplete last_agent_message.len={}", m.len());
        }
        _ => {}
    }
    sess.tx_event.send(event).await.ok();
}

async fn run_turn(
    sess: &Session,
    turn_diff_tracker: &mut TurnDiffTracker,
    sub_id: String,
    input: Vec<ResponseItem>,
) -> CodexResult<Vec<ProcessedResponseItem>> {
    // Check if browser is enabled
    let browser_enabled = codex_browser::global::get_browser_manager().await.is_some();
    
    let tools = get_openai_tools(
        &sess.tools_config,
        Some(sess.mcp_connection_manager.list_all_tools()),
        browser_enabled,
    );

    let mut retries = 0;
    // Ensure we only auto-compact once per turn to avoid loops
    let mut did_auto_compact = false;
    // Attempt input starts as the provided input, and may be augmented with
    // items from a previous dropped stream attempt so we don't lose progress.
    let mut attempt_input: Vec<ResponseItem> = input.clone();
    loop {
        // Each loop iteration corresponds to a single provider HTTP request.
        // Increment the attempt ordinal first and capture its value so all
        // OrderMeta emitted during this attempt share the same `req`, even if
        // later attempts start before all events have been delivered.
        sess.begin_http_attempt();
        let attempt_req = sess.current_request_ordinal();
        // Build status items (screenshots, system status) fresh for each attempt
        let status_items = build_turn_status_items(sess).await;

        let prompt = Prompt {
            input: attempt_input.clone(),
            user_instructions: sess.user_instructions.clone(),
            store: !sess.disable_response_storage,
            tools: tools.clone(),
            base_instructions_override: sess.base_instructions.clone(),
            environment_context: Some(EnvironmentContext::new(
                Some(sess.cwd.clone()),
                Some(sess.approval_policy),
                Some(sess.sandbox_policy.clone()),
                Some(sess.user_shell.clone()),
            )),
            status_items, // Include status items with this request
        };

        // Start a new scratchpad for this HTTP attempt
        sess.begin_attempt_scratchpad();

        match try_run_turn(sess, turn_diff_tracker, &sub_id, &prompt, attempt_req).await {
            Ok(output) => {
                // Record status items to conversation history after successful turn
                // This ensures they persist for future requests in the right chronological order
                if !prompt.status_items.is_empty() {
                    sess.record_conversation_items(&prompt.status_items).await;
                }
                // Commit successful attempt – scratchpad is no longer needed.
                sess.clear_scratchpad();
                return Ok(output);
            }
            Err(CodexErr::Interrupted) => return Err(CodexErr::Interrupted),
            Err(CodexErr::EnvVar(var)) => return Err(CodexErr::EnvVar(var)),
            Err(e @ (CodexErr::UsageLimitReached(_) | CodexErr::UsageNotIncluded)) => {
                return Err(e);
            }
            Err(e) => {
                // Detect context-window overflow and auto-run a compact summarization once
                if !did_auto_compact {
                    if let CodexErr::Stream(msg, _maybe_delay) = &e {
                        let lower = msg.to_ascii_lowercase();
                        let looks_like_context_overflow =
                            lower.contains("exceeds the context window")
                                || lower.contains("exceed the context window")
                                || lower.contains("context length exceeded")
                                || lower.contains("maximum context length")
                                || (lower.contains("context window")
                                    && (lower.contains("exceed")
                                        || lower.contains("exceeded")
                                        || lower.contains("full")
                                        || lower.contains("too long")));

                        if looks_like_context_overflow {
                            did_auto_compact = true;

                            // Inform UI and run a one-off compact turn inline, then retry
                            sess
                                .notify_stream_error(
                                    &sub_id,
                                    "Model hit context-window limit; running /compact and retrying…"
                                        .to_string(),
                                )
                                .await;

                            const SUMMARIZATION_PROMPT: &str =
                                include_str!("prompt_for_compact_command.md");

                            let compact_input = response_input_from_core_items(vec![InputItem::Text {
                                text: "Start Summarization".to_string(),
                            }]);
                            let compact_turn_input: Vec<ResponseItem> =
                                sess.turn_input_with_history(vec![compact_input.clone().into()]);

                            let compact_prompt = Prompt {
                                input: compact_turn_input,
                                user_instructions: None,
                                store: !sess.disable_response_storage,
                                tools: Vec::new(),
                                base_instructions_override: Some(SUMMARIZATION_PROMPT.to_string()),
                                environment_context: None,
                                status_items: Vec::new(),
                            };

                            match drain_to_completed(sess, &sub_id, &compact_prompt).await {
                                Ok(()) => {
                                    // Keep only the summary to shrink history
                                    {
                                        let mut state = sess.state.lock().unwrap();
                                        state.history.keep_last_messages(1);
                                    }

                                    // Reset any partial attempt state and retry immediately
                                    sess.clear_scratchpad();
                                    sess
                                        .notify_stream_error(
                                            &sub_id,
                                            "/compact completed; retrying with condensed history…"
                                                .to_string(),
                                        )
                                        .await;
                                    attempt_input = input.clone();
                                    continue;
                                }
                                Err(err) => {
                                    sess
                                        .notify_stream_error(
                                            &sub_id,
                                            format!(
                                                "/compact failed: {err}; falling back to normal retries…"
                                            ),
                                        )
                                        .await;
                                }
                            }
                        }
                    }
                }
                // Use the configured provider-specific stream retry budget.
                let max_retries = sess.client.get_provider().stream_max_retries();
                if retries < max_retries {
                    retries += 1;
                    let delay = match e {
                        CodexErr::Stream(_, Some(delay)) => delay,
                        _ => backoff(retries),
                    };
                    warn!(
                        "stream disconnected - retrying turn ({retries}/{max_retries} in {delay:?})...",
                    );

                    // Surface retry information to any UI/front‑end so the
                    // user understands what is happening instead of staring
                    // at a seemingly frozen screen.
                    sess.notify_stream_error(
                        &sub_id,
                        format!(
                            "stream error: {e}; retrying {retries}/{max_retries} in {delay:?}…"
                        ),
                    )
                    .await;
                    // Pull any partial progress from this attempt and append to
                    // the next request's input so we do not lose tool progress
                    // or already-finalized items.
                    if let Some(sp) = sess.take_scratchpad() {
                        // Build a set of call_ids we have already included to avoid duplicate calls
                        let mut seen_calls: std::collections::HashSet<String> = attempt_input
                            .iter()
                            .filter_map(|ri| match ri {
                                ResponseItem::FunctionCall { call_id, .. } => Some(call_id.clone()),
                                ResponseItem::LocalShellCall { call_id: Some(c), .. } => Some(c.clone()),
                                _ => None,
                            })
                            .collect();

                        // Append finalized function/local shell calls from the dropped attempt
                        for item in sp.items {
                            match &item {
                                ResponseItem::FunctionCall { call_id, .. } => {
                                    if seen_calls.insert(call_id.clone()) {
                                        attempt_input.push(item.clone());
                                    }
                                }
                                ResponseItem::LocalShellCall { call_id: Some(c), .. } => {
                                    if seen_calls.insert(c.clone()) {
                                        attempt_input.push(item.clone());
                                    }
                                }
                                _ => {
                                    // Avoid injecting assistant/Reasoning messages on retry to reduce duplication.
                                }
                            }
                        }

                        // Append tool outputs produced during the dropped attempt
                        for resp in sp.responses {
                            attempt_input.push(ResponseItem::from(resp));
                        }

                        // If we have partial deltas, include a short ephemeral hint so the model can resume.
                        if !sp.partial_assistant_text.is_empty() || !sp.partial_reasoning_summary.is_empty() {
                            use codex_protocol::models::ContentItem;
                            let mut hint = String::from(
                                "[EPHEMERAL:RETRY_HINT]\nPrevious attempt aborted mid-stream. Continue without repeating.\n",
                            );
                            if !sp.partial_reasoning_summary.is_empty() {
                                let s = &sp.partial_reasoning_summary;
                                // Take the last 800 characters, respecting UTF-8 boundaries
                                let start_idx = if s.chars().count() > 800 {
                                    s.char_indices()
                                        .rev()
                                        .nth(800 - 1)
                                        .map(|(i, _)| i)
                                        .unwrap_or(0)
                                } else {
                                    0
                                };
                                let tail = &s[start_idx..];
                                hint.push_str(&format!("Last reasoning summary fragment:\n{}\n\n", tail));
                            }
                            if !sp.partial_assistant_text.is_empty() {
                                let s = &sp.partial_assistant_text;
                                // Take the last 800 characters, respecting UTF-8 boundaries
                                let start_idx = if s.chars().count() > 800 {
                                    s.char_indices()
                                        .rev()
                                        .nth(800 - 1)
                                        .map(|(i, _)| i)
                                        .unwrap_or(0)
                                } else {
                                    0
                                };
                                let tail = &s[start_idx..];
                                hint.push_str(&format!("Last assistant text fragment:\n{}\n", tail));
                            }
                            attempt_input.push(ResponseItem::Message {
                                id: None,
                                role: "user".to_string(),
                                content: vec![ContentItem::InputText { text: hint }],
                            });
                        }
                    }

                    tokio::time::sleep(delay).await;
                } else {
                    return Err(e);
                }
            }
        }
    }
}

/// When the model is prompted, it returns a stream of events. Some of these
/// events map to a `ResponseItem`. A `ResponseItem` may need to be
/// "handled" such that it produces a `ResponseInputItem` that needs to be
/// sent back to the model on the next turn.
#[derive(Debug)]
struct ProcessedResponseItem {
    item: ResponseItem,
    response: Option<ResponseInputItem>,
}

async fn try_run_turn(
    sess: &Session,
    turn_diff_tracker: &mut TurnDiffTracker,
    sub_id: &str,
    prompt: &Prompt,
    attempt_req: u64,
) -> CodexResult<Vec<ProcessedResponseItem>> {
    // call_ids that are part of this response.
    let completed_call_ids = prompt
        .input
        .iter()
        .filter_map(|ri| match ri {
            ResponseItem::FunctionCallOutput { call_id, .. } => Some(call_id),
            ResponseItem::LocalShellCall {
                call_id: Some(call_id),
                ..
            } => Some(call_id),
            ResponseItem::CustomToolCallOutput { call_id, .. } => Some(call_id),
            _ => None,
        })
        .collect::<Vec<_>>();

    // call_ids that were pending but are not part of this response.
    // This usually happens because the user interrupted the model before we responded to one of its tool calls
    // and then the user sent a follow-up message.
    let missing_calls = {
        prompt
            .input
            .iter()
            .filter_map(|ri| match ri {
                ResponseItem::FunctionCall { call_id, .. } => Some(call_id),
                ResponseItem::LocalShellCall {
                    call_id: Some(call_id),
                    ..
                } => Some(call_id),
                ResponseItem::CustomToolCall { call_id, .. } => Some(call_id),
                _ => None,
            })
            .filter_map(|call_id| {
                if completed_call_ids.contains(&call_id) {
                    None
                } else {
                    Some(call_id.clone())
                }
            })
            .map(|call_id| ResponseItem::CustomToolCallOutput {
                call_id: call_id.clone(),
                output: "aborted".to_string(),
            })
            .collect::<Vec<_>>()
    };
    let prompt: Cow<Prompt> = if missing_calls.is_empty() {
        Cow::Borrowed(prompt)
    } else {
        // Add the synthetic aborted missing calls to the beginning of the input to ensure all call ids have responses.
        let input = [missing_calls, prompt.input.clone()].concat();
        Cow::Owned(Prompt {
            input,
            ..prompt.clone()
        })
    };

    let mut stream = sess.client.clone().stream(&prompt).await?;

    let mut output = Vec::new();
    loop {
        // Poll the next item from the model stream. We must inspect *both* Ok and Err
        // cases so that transient stream failures (e.g., dropped SSE connection before
        // `response.completed`) bubble up and trigger the caller's retry logic.
        let event = stream.next().await;
        let Some(event) = event else {
            // Channel closed without yielding a final Completed event or explicit error.
            // Treat as a disconnected stream so the caller can retry.
            return Err(CodexErr::Stream(
                "stream closed before response.completed".into(),
                None,
            ));
        };

        let event = match event {
            Ok(ev) => ev,
            Err(e) => {
                // Propagate the underlying stream error to the caller (run_turn), which
                // will apply the configured `stream_max_retries` policy.
                return Err(e);
            }
        };

        match event {
            ResponseEvent::Created => {}
            ResponseEvent::OutputItemDone { item, sequence_number, output_index } => {
                let response =
                    handle_response_item(sess, turn_diff_tracker, sub_id, item.clone(), sequence_number, output_index, attempt_req).await?;

                // Save into scratchpad so we can seed a retry if the stream drops later.
                sess.scratchpad_push(&item, &response);

                // If this was a finalized assistant message, clear partial text buffer
                if let ResponseItem::Message { .. } = &item {
                    sess.scratchpad_clear_partial_message();
                }

                output.push(ProcessedResponseItem { item, response });
            }
            ResponseEvent::WebSearchCallBegin { call_id } => {
                // Stamp OrderMeta so the TUI can place the search block within
                // the correct request window instead of using an internal epilogue.
                let ctx = ToolCallCtx::new(sub_id.to_string(), call_id.clone(), None, None);
                let order = ctx.order_meta(attempt_req);
                let ev = sess.make_event_with_order(
                    &sub_id,
                    EventMsg::WebSearchBegin(WebSearchBeginEvent { call_id, query: None }),
                    order,
                    None,
                );
                sess.send_event(ev).await;
            }
            ResponseEvent::WebSearchCallCompleted { call_id, query } => {
                let ctx = ToolCallCtx::new(sub_id.to_string(), call_id.clone(), None, None);
                let order = ctx.order_meta(attempt_req);
                let ev = sess.make_event_with_order(
                    &sub_id,
                    EventMsg::WebSearchComplete(WebSearchCompleteEvent { call_id, query }),
                    order,
                    None,
                );
                sess.send_event(ev).await;
            }
            ResponseEvent::Completed {
                response_id: _,
                token_usage,
            } => {
                if let Some(token_usage) = token_usage {
                    sess.tx_event
                        .send(sess.make_event(&sub_id, EventMsg::TokenCount(token_usage)))
                        .await
                        .ok();
                }

                let unified_diff = turn_diff_tracker.get_unified_diff();
                if let Ok(Some(unified_diff)) = unified_diff {
                    let msg = EventMsg::TurnDiff(TurnDiffEvent { unified_diff });
                    let _ = sess.tx_event.send(sess.make_event(&sub_id, msg)).await;
                }

                return Ok(output);
            }
            ResponseEvent::OutputTextDelta { delta, item_id, sequence_number, output_index } => {
                // Don't append to history during streaming - only send UI events.
                // The complete message will be added to history when OutputItemDone arrives.
                // This ensures items are recorded in the correct chronological order.

                // Use the item_id if present, otherwise fall back to sub_id
                let event_id = item_id.unwrap_or_else(|| sub_id.to_string());
                let order = crate::protocol::OrderMeta {
                    request_ordinal: attempt_req,
                    output_index,
                    sequence_number,
                };
                let stamped = sess.make_event_with_order(&event_id, EventMsg::AgentMessageDelta(AgentMessageDeltaEvent { delta: delta.clone() }), order, sequence_number);
                sess.tx_event.send(stamped).await.ok();

                // Track partial assistant text in the scratchpad to help resume on retry.
                // Only accumulate when we have an item context or a single active stream.
                // We deliberately do not scope by item_id to keep implementation simple.
                sess.scratchpad_add_text_delta(&delta);
            }
            ResponseEvent::ReasoningSummaryDelta { delta, item_id, sequence_number, output_index, summary_index } => {
                // Use the item_id if present, otherwise fall back to sub_id
                let mut event_id = item_id.unwrap_or_else(|| sub_id.to_string());
                if let Some(si) = summary_index { event_id = format!("{}#s{}", event_id, si); }
                let order = crate::protocol::OrderMeta { request_ordinal: attempt_req, output_index, sequence_number };
                let stamped = sess.make_event_with_order(&event_id, EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent { delta: delta.clone() }), order, sequence_number);
                sess.tx_event.send(stamped).await.ok();

                // Buffer reasoning summary so we can include a hint on retry.
                sess.scratchpad_add_reasoning_delta(&delta);
            }
            ResponseEvent::ReasoningSummaryPartAdded => {
                let stamped = sess.make_event(&sub_id, EventMsg::AgentReasoningSectionBreak(AgentReasoningSectionBreakEvent {}));
                sess.tx_event.send(stamped).await.ok();
            }
            ResponseEvent::ReasoningContentDelta { delta, item_id, sequence_number, output_index, content_index } => {
                if sess.show_raw_agent_reasoning {
                    // Use the item_id if present, otherwise fall back to sub_id
                    let mut event_id = item_id.unwrap_or_else(|| sub_id.to_string());
                    if let Some(ci) = content_index { event_id = format!("{}#c{}", event_id, ci); }
                    let order = crate::protocol::OrderMeta { request_ordinal: attempt_req, output_index, sequence_number };
                    let stamped = sess.make_event_with_order(&event_id, EventMsg::AgentReasoningRawContentDelta(AgentReasoningRawContentDeltaEvent { delta }), order, sequence_number);
                    sess.tx_event.send(stamped).await.ok();
                }
            }
            // Note: ReasoningSummaryPartAdded handled above without scratchpad mutation.
        }
    }
}

async fn run_compact_agent(
    sess: Arc<Session>,
    sub_id: String,
    input: Vec<InputItem>,
    compact_instructions: String,
) {
    let start_event = sess.make_event(&sub_id, EventMsg::TaskStarted);
    if sess.tx_event.send(start_event).await.is_err() {
        return;
    }

    let initial_input_for_turn: ResponseInputItem = response_input_from_core_items(input);
    let turn_input: Vec<ResponseItem> =
        sess.turn_input_with_history(vec![initial_input_for_turn.clone().into()]);

    let max_retries = sess.client.get_provider().stream_max_retries();
    let mut retries = 0;

    loop {
        // Bump request_ordinal for this provider request attempt so
        // downstream OrderMeta carries the correct `req` index.
        sess.begin_http_attempt();
        // Build status items (screenshots, system status) fresh for each attempt
        let status_items = build_turn_status_items(&sess).await;

        let prompt = Prompt {
            input: turn_input.clone(),
            user_instructions: None,
            store: !sess.disable_response_storage,
            environment_context: None,
            tools: Vec::new(),
            base_instructions_override: Some(compact_instructions.clone()),
            status_items, // Include status items with this request
        };

        let attempt_result = drain_to_completed(&sess, &sub_id, &prompt).await;

        match attempt_result {
            Ok(()) => {
                // Record status items to conversation history after successful turn
                if !prompt.status_items.is_empty() {
                    sess.record_conversation_items(&prompt.status_items).await;
                }
                break;
            }
            Err(CodexErr::Interrupted) => return,
            Err(e) => {
                if retries < max_retries {
                    retries += 1;
                    let delay = backoff(retries);
                    sess.notify_stream_error(
                        &sub_id,
                        format!(
                            "stream error: {e}; retrying {retries}/{max_retries} in {delay:?}…"
                        ),
                    )
                    .await;
                    tokio::time::sleep(delay).await;
                    continue;
                } else {
                    let event = Event { id: sub_id.clone(), event_seq: 0, msg: EventMsg::Error(ErrorEvent { message: e.to_string() }), order: None };
                    sess.send_event(event).await;
                    // Ensure the UI is released from running state even on errors.
                    let done = Event { id: sub_id.clone(), event_seq: 0, msg: EventMsg::TaskComplete(TaskCompleteEvent { last_agent_message: None }), order: None };
                    sess.send_event(done).await;
                    return;
                }
            }
        }
    }

    sess.remove_agent(&sub_id);
    let event = Event { id: sub_id.clone(), event_seq: 0, msg: EventMsg::AgentMessage(AgentMessageEvent { message: "Compact agent completed".to_string() }), order: None };
    sess.send_event(event).await;
    let event = sess.make_event(
        &sub_id,
        EventMsg::TaskComplete(TaskCompleteEvent { last_agent_message: None }),
    );
    sess.send_event(event).await;

    let mut state = sess.state.lock().unwrap();
    state.history.keep_last_messages(1);
}

async fn handle_response_item(
    sess: &Session,
    turn_diff_tracker: &mut TurnDiffTracker,
    sub_id: &str,
    item: ResponseItem,
    seq_hint: Option<u64>,
    output_index: Option<u32>,
    attempt_req: u64,
) -> CodexResult<Option<ResponseInputItem>> {
    debug!(?item, "Output item");
    let output = match item {
        ResponseItem::Message { content, id, .. } => {
            // Use the item_id if present, otherwise fall back to sub_id
            let event_id = id.unwrap_or_else(|| sub_id.to_string());
            for item in content {
                if let ContentItem::OutputText { text } = item {
                    let order = crate::protocol::OrderMeta { request_ordinal: attempt_req, output_index, sequence_number: seq_hint };
                    let stamped = sess.make_event_with_order(&event_id, EventMsg::AgentMessage(AgentMessageEvent { message: text }), order, seq_hint);
                    sess.tx_event.send(stamped).await.ok();
                }
            }
            None
        }
        ResponseItem::Reasoning {
            id,
            summary,
            content,
            encrypted_content: _,
        } => {
            // Use the item_id if present and not empty, otherwise fall back to sub_id
            let event_id = if !id.is_empty() {
                id.clone()
            } else {
                sub_id.to_string()
            };
            for (i, item) in summary.into_iter().enumerate() {
                let text = match item {
                    ReasoningItemReasoningSummary::SummaryText { text } => text,
                };
                let eid = format!("{}#s{}", event_id, i);
                let order = crate::protocol::OrderMeta { request_ordinal: attempt_req, output_index, sequence_number: seq_hint };
                let stamped = sess.make_event_with_order(&eid, EventMsg::AgentReasoning(AgentReasoningEvent { text }), order, seq_hint);
                sess.tx_event.send(stamped).await.ok();
            }
            if sess.show_raw_agent_reasoning && content.is_some() {
                let content = content.unwrap();
                for item in content.into_iter() {
                    let text = match item {
                        ReasoningItemContent::ReasoningText { text } => text,
                        ReasoningItemContent::Text { text } => text,
                    };
                    let order = crate::protocol::OrderMeta { request_ordinal: attempt_req, output_index, sequence_number: seq_hint };
                    let stamped = sess.make_event_with_order(&event_id, EventMsg::AgentReasoningRawContent(AgentReasoningRawContentEvent { text }), order, seq_hint);
                    sess.tx_event.send(stamped).await.ok();
                }
            }
            None
        }
        ResponseItem::FunctionCall {
            name,
            arguments,
            call_id,
            ..
        } => {
            info!("FunctionCall: {name}({arguments})");
            Some(
                handle_function_call(
                    sess,
                    turn_diff_tracker,
                    sub_id.to_string(),
                    name,
                    arguments,
                    call_id,
                    seq_hint,
                    output_index,
                    attempt_req,
                )
                .await,
            )
        }
        ResponseItem::LocalShellCall {
            id,
            call_id,
            status: _,
            action,
        } => {
            let LocalShellAction::Exec(action) = action;
            tracing::info!("LocalShellCall: {action:?}");
            let params = ShellToolCallParams {
                command: action.command,
                workdir: action.working_directory,
                timeout_ms: action.timeout_ms,
                with_escalated_permissions: None,
                justification: None,
            };
            let effective_call_id = match (call_id, id) {
                (Some(call_id), _) => call_id,
                (None, Some(id)) => id,
                (None, None) => {
                    error!("LocalShellCall without call_id or id");
                    return Ok(Some(ResponseInputItem::FunctionCallOutput {
                        call_id: "".to_string(),
                        output: FunctionCallOutputPayload {
                            content: "LocalShellCall without call_id or id".to_string(),
                            success: None,
                        },
                    }));
                }
            };

            let exec_params = to_exec_params(params, sess);
            Some(
            handle_container_exec_with_params(
                exec_params,
                sess,
                turn_diff_tracker,
                sub_id.to_string(),
                effective_call_id,
                seq_hint,
                output_index,
                attempt_req,
            )
            .await,
            )
        }
        ResponseItem::CustomToolCall { call_id, name, .. } => {
            // Minimal placeholder: custom tools are not handled here.
            Some(ResponseInputItem::FunctionCallOutput {
                call_id,
                output: FunctionCallOutputPayload {
                    content: format!("Custom tool '{name}' is not supported in this build"),
                    success: Some(false),
                },
            })
        }
        ResponseItem::FunctionCallOutput { .. } => {
            debug!("unexpected FunctionCallOutput from stream");
            None
        }
        ResponseItem::CustomToolCallOutput { .. } => {
            debug!("unexpected CustomToolCallOutput from stream");
            None
        }
        ResponseItem::WebSearchCall { id, action, .. } => {
            if let WebSearchAction::Search { query } = action {
                let call_id = id.unwrap_or_else(|| "".to_string());
                let event = sess.make_event_with_hint(&sub_id, EventMsg::WebSearchComplete(WebSearchCompleteEvent { call_id, query: Some(query) }), seq_hint);
                sess.tx_event.send(event).await.ok();
            }
            None
        }
        ResponseItem::Other => None,
    };
    Ok(output)
}

// Helper utilities for agent output/progress management
fn ensure_agent_dir(cwd: &Path, agent_id: &str) -> Result<PathBuf, String> {
    let dir = cwd.join(".code").join("agents").join(agent_id);
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create agent dir {}: {}", dir.display(), e))?;
    Ok(dir)
}

fn write_agent_file(dir: &Path, filename: &str, content: &str) -> Result<PathBuf, String> {
    let path = dir.join(filename);
    std::fs::write(&path, content)
        .map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;
    Ok(path)
}

fn preview_first_n_lines(s: &str, n: usize) -> (String, usize) {
    let mut lines = s.lines();
    let mut collected: Vec<&str> = Vec::new();
    for _ in 0..n {
        if let Some(l) = lines.next() {
            collected.push(l);
        } else {
            break;
        }
    }
    (collected.join("\n"), s.lines().count())
}

async fn handle_function_call(
    sess: &Session,
    turn_diff_tracker: &mut TurnDiffTracker,
    sub_id: String,
    name: String,
    arguments: String,
    call_id: String,
    seq_hint: Option<u64>,
    output_index: Option<u32>,
    attempt_req: u64,
) -> ResponseInputItem {
    let ctx = ToolCallCtx::new(sub_id.clone(), call_id.clone(), seq_hint, output_index);
    match name.as_str() {
        "container.exec" | "shell" => {
            let params = match parse_container_exec_arguments(arguments, sess, &call_id) {
                Ok(params) => params,
                Err(output) => {
                    return *output;
                }
            };
            handle_container_exec_with_params(params, sess, turn_diff_tracker, sub_id, call_id, seq_hint, output_index, attempt_req)
                .await
        }
        "update_plan" => handle_update_plan(sess, &ctx, arguments).await,
        // agent_* tools
        "agent_run" => handle_run_agent(sess, &ctx, arguments).await,
        "agent_check" => handle_check_agent_status(sess, &ctx, arguments).await,
        "agent_result" => handle_get_agent_result(sess, &ctx, arguments).await,
        "agent_cancel" => handle_cancel_agent(sess, &ctx, arguments).await,
        "agent_wait" => handle_wait_for_agent(sess, &ctx, arguments).await,
        "agent_list" => handle_list_agents(sess, &ctx, arguments).await,
        // browser_* tools
        "browser_open" => handle_browser_open(sess, &ctx, arguments).await,
        "browser_close" => handle_browser_close(sess, &ctx).await,
        "browser_status" => handle_browser_status(sess, &ctx).await,
        "browser_click" => handle_browser_click(sess, &ctx, arguments).await,
        "browser_move" => handle_browser_move(sess, &ctx, arguments).await,
        "browser_type" => handle_browser_type(sess, &ctx, arguments).await,
        "browser_key" => handle_browser_key(sess, &ctx, arguments).await,
        "browser_javascript" => handle_browser_javascript(sess, &ctx, arguments).await,
        "browser_scroll" => handle_browser_scroll(sess, &ctx, arguments).await,
        "browser_history" => handle_browser_history(sess, &ctx, arguments).await,
        "browser_console" => handle_browser_console(sess, &ctx, arguments).await,
        "browser_inspect" => handle_browser_inspect(sess, &ctx, arguments).await,
        "browser_cdp" => handle_browser_cdp(sess, &ctx, arguments).await,
        "browser_cleanup" => handle_browser_cleanup(sess, &ctx).await,
        "web_fetch" => handle_web_fetch(sess, &ctx, arguments).await,
        _ => {
            match sess.mcp_connection_manager.parse_tool_name(&name) {
                Some((server, tool_name)) => {
                    // TODO(mbolin): Determine appropriate timeout for tool call.
                    let timeout = None;
                    handle_mcp_tool_call(sess, &ctx, server, tool_name, arguments, timeout)
                    .await
                }
                None => {
                    // Unknown function: reply with structured failure so the model can adapt.
                    ResponseInputItem::FunctionCallOutput {
                        call_id,
                        output: FunctionCallOutputPayload {
                            content: format!("unsupported call: {name}"),
                            success: None,
                        },
                    }
                }
            }
        }
    }
}

async fn handle_browser_cleanup(sess: &Session, ctx: &ToolCallCtx) -> ResponseInputItem {
    let call_id_clone = ctx.call_id.clone();
    let _sess_clone = sess;
    execute_custom_tool(
        sess,
        ctx,
        "browser_cleanup".to_string(),
        Some(serde_json::json!({})),
        || async move {
            if let Some(browser_manager) = get_browser_manager_for_session(_sess_clone).await {
                match browser_manager.cleanup().await {
                    Ok(_) => ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload { content: "Browser cleanup completed".to_string(), success: Some(true) },
                    },
                    Err(e) => ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload { content: format!("Cleanup failed: {}", e), success: Some(false) },
                    },
                }
            } else {
                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload { content: "Browser is not initialized. Use browser_open to start the browser.".to_string(), success: Some(false) },
                }
            }
        }
    ).await
}

async fn handle_web_fetch(sess: &Session, ctx: &ToolCallCtx, arguments: String) -> ResponseInputItem {
    // Include raw params in begin event for observability
    let params_for_event = serde_json::from_str(&arguments).ok();
    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();

    execute_custom_tool(
        sess,
        ctx,
        "web_fetch".to_string(),
        params_for_event,
        || async move {
            #[derive(serde::Deserialize)]
            struct WebFetchParams {
                url: String,
                #[serde(default)]
                timeout_ms: Option<u64>,
                #[serde(default)]
                mode: Option<String>, // "auto" (default), "browser", or "http"
            }

            let parsed: Result<WebFetchParams, _> = serde_json::from_str(&arguments_clone);
            let params = match parsed {
                Ok(p) => p,
                Err(e) => {
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload {
                            content: format!("Invalid web_fetch arguments: {e}"),
                            success: None,
                        },
                    };
                }
            };

            // Helper: build a client with a specific UA and common headers.
            async fn do_request(
                url: &str,
                ua: &str,
                timeout: Duration,
                extra_headers: Option<&[(reqwest::header::HeaderName, &'static str)]>,
            ) -> Result<reqwest::Response, reqwest::Error> {
                let client = reqwest::Client::builder()
                    .timeout(timeout)
                    .user_agent(ua)
                    .build()?;
                let mut req = client.get(url)
                    // Add a few browser-like headers to reduce blocks
                    .header(reqwest::header::ACCEPT, "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
                    .header(reqwest::header::ACCEPT_LANGUAGE, "en-US,en;q=0.9");
                if let Some(pairs) = extra_headers {
                    for (k, v) in pairs.iter() {
                        req = req.header(k, *v);
                    }
                }
                req.send().await
            }

            // Helper: remove obvious noisy blocks before markdown conversion.
            // This uses a lightweight ASCII-insensitive scan to drop whole
            // elements whose contents should never be surfaced to the model
            // (scripts, styles, templates, headers/footers/navigation, etc.).
            fn strip_noisy_tags(mut html: String) -> String {
                // Remove <script>, <style>, and <noscript> blocks with a simple
                // ASCII case-insensitive scan that preserves UTF-8 boundaries.
                // This avoids allocating lowercase copies and accidentally using
                // indices from a different string representation.
                fn eq_ascii_ci(a: u8, b: u8) -> bool {
                    a.to_ascii_lowercase() == b.to_ascii_lowercase()
                }
                fn starts_with_tag_ci(bytes: &[u8], tag: &[u8]) -> bool {
                    if bytes.len() < tag.len() { return false; }
                    for i in 0..tag.len() {
                        if !eq_ascii_ci(bytes[i], tag[i]) { return false; }
                    }
                    true
                }
                // Find the next opening tag like "<script" (allowing whitespace after '<').
                fn find_open_tag_ci(s: &str, tag: &str, from: usize) -> Option<usize> {
                    let bytes = s.as_bytes();
                    let tag_bytes = tag.as_bytes();
                    let mut i = from;
                    while i + 1 < bytes.len() {
                        if bytes[i] == b'<' {
                            let mut j = i + 1;
                            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t' || bytes[j] == b'\n' || bytes[j] == b'\r') {
                                j += 1;
                            }
                            if j < bytes.len() && starts_with_tag_ci(&bytes[j..], tag_bytes) {
                                return Some(i);
                            }
                        }
                        i += 1;
                    }
                    None
                }
                // Find the corresponding closing tag like "</script>" starting at or after `from`.
                // Returns the byte index just after the closing '>' if found.
                fn find_close_after_ci(s: &str, tag: &str, from: usize) -> Option<usize> {
                    let bytes = s.as_bytes();
                    let tag_bytes = tag.as_bytes();
                    let mut i = from;
                    while i + 2 < bytes.len() { // need at least '<' '/' and one tag byte
                        if bytes[i] == b'<' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                            let mut j = i + 2;
                            // Optional whitespace before tag name
                            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t' || bytes[j] == b'\n' || bytes[j] == b'\r') {
                                j += 1;
                            }
                            if starts_with_tag_ci(&bytes[j..], tag_bytes) {
                                // Advance past tag name
                                j += tag_bytes.len();
                                // Skip optional whitespace until '>'
                                while j < bytes.len() && bytes[j] != b'>' {
                                    j += 1;
                                }
                                if j < bytes.len() && bytes[j] == b'>' {
                                    return Some(j + 1);
                                }
                                return None; // No closing '>'
                            }
                        }
                        i += 1;
                    }
                    None
                }

                // Keep this conservative to avoid dropping content.
                let tags = ["script", "style", "noscript"];
                for tag in tags.iter() {
                    let mut guard = 0;
                    loop {
                        if guard > 64 { break; }
                        let Some(start) = find_open_tag_ci(&html, tag, 0) else { break; };
                        let search_from = start + 1; // after '<'
                        if let Some(end) = find_close_after_ci(&html, tag, search_from) {
                            // Safe because both start and end are on ASCII boundaries ('<' and '>')
                            html.replace_range(start..end, "");
                        } else {
                            // No close tag found; drop from the opening tag to end
                            html.truncate(start);
                            break;
                        }
                        guard += 1;
                    }
                }
                html
            }

            // Try to keep only <main> content if present; drastically reduces
            // boilerplate from navigation and login banners on many sites.
            fn extract_main(html: &str) -> Option<String> {
                // Find opening <main ...>
                let bytes = html.as_bytes();
                let open = {
                    let mut i = 0usize;
                    let tag = b"main";
                    while i + 5 < bytes.len() { // < m a i n > (min)
                        if bytes[i] == b'<' {
                            // skip '<' and whitespace
                            let mut j = i + 1;
                            while j < bytes.len() && bytes[j].is_ascii_whitespace() { j += 1; }
                            if j + tag.len() <= bytes.len() && bytes[j..j+tag.len()].eq_ignore_ascii_case(tag) {
                                // Found '<main'; now find '>'
                                while j < bytes.len() && bytes[j] != b'>' { j += 1; }
                                if j < bytes.len() { Some((i, j + 1)) } else { None }
                            } else { None }
                        } else { None }
                            .map(|pair| return pair);
                        i += 1;
                    }
                    None
                };
                let (start, after_open) = open?;
                // Find closing </main>
                let mut i = after_open;
                let tag_close = b"</main";
                while i + tag_close.len() + 1 < bytes.len() {
                    if bytes[i] == b'<' && bytes[i+1] == b'/' {
                        if bytes[i..].len() >= tag_close.len() && bytes[i..i+tag_close.len()].eq_ignore_ascii_case(tag_close) {
                            // Find closing '>'
                            let mut j = i + tag_close.len();
                            while j < bytes.len() && bytes[j] != b'>' { j += 1; }
                            if j < bytes.len() {
                                return Some(html[start..j+1].to_string());
                            } else {
                                return Some(html[start..].to_string());
                            }
                        }
                    }
                    i += 1;
                }
                Some(html[start..].to_string())
            }

            // Inside fenced code blocks, collapse massively-escaped Windows paths like
            // `C:\\Users\\...` to `C:\Users\...`. Only applies to drive-rooted paths.
            fn unescape_windows_paths(line: &str) -> String {
                let bytes = line.as_bytes();
                let mut out = String::with_capacity(line.len());
                let mut i = 0usize;
                while i < bytes.len() {
                    // Pattern: [A-Za-z] : \\+
                    if i + 3 < bytes.len()
                        && bytes[i].is_ascii_alphabetic()
                        && bytes[i+1] == b':'
                        && bytes[i+2] == b'\\'
                        && bytes[i+3] == b'\\'
                    {
                        // Emit drive and a single backslash
                        out.push(bytes[i] as char);
                        out.push(':');
                        out.push('\\');
                        // Skip all following backslashes in this run
                        i += 4;
                        while i < bytes.len() && bytes[i] == b'\\' { i += 1; }
                        continue;
                    }
                    out.push(bytes[i] as char);
                    i += 1;
                }
                out
            }

            // Lightweight cleanup on the resulting markdown to remove leaked
            // JSON blobs and obvious client boot payloads that sometimes escape
            // the <script> filter on complex sites. Avoids touching fenced code.
            fn postprocess_markdown(md: &str) -> String {
                let mut out: Vec<String> = Vec::with_capacity(md.len() / 64 + 1);
                let mut in_fence = false;
                let mut empty_run = 0usize;
                for line in md.lines() {
                    // Track fenced code blocks
                    if let Some(rest) = line.trim_start().strip_prefix("```") {
                        in_fence = !in_fence;
                        let _lang = if in_fence { Some(rest.trim()) } else { None };
                        out.push(line.to_string());
                        empty_run = 0;
                        continue;
                    }
                    if in_fence {
                        // Only normalize Windows path over-escaping; do not alter other content.
                        let normalized = unescape_windows_paths(line);
                        out.push(normalized);
                        continue;
                    }

                    let trimmed = line.trim();
                    // Drop extremely long single lines only if they're likely SPA boot payloads
                    if trimmed.len() > 8000 { continue; }
                    // Common SPA boot keys that shouldn't appear in human output.
                    // Keep this list tight to avoid dropping legitimate examples.
                    if trimmed.contains("\"payload\"") || trimmed.contains("\"props\"") || trimmed.contains("\"preloaded_records\"") || trimmed.contains("\"appPayload\"") || trimmed.contains("\"preloadedQueries\"") {
                        continue;
                    }

                    if trimmed.is_empty() {
                        // Collapse multiple empty lines to max 1
                        if empty_run == 0 {
                            out.push(String::new());
                        }
                        empty_run += 1;
                    } else {
                        out.push(line.to_string());
                        empty_run = 0;
                    }
                }
                // Trim leading/trailing blank lines
                let mut s = out.join("\n");
                while s.starts_with('\n') { s.remove(0); }
                while s.ends_with('\n') { s.pop(); }
                s
            }

            // Domain-specific: extract rich content from GitHub issue/PR pages
            // without requiring a JS-capable browser. We parse JSON-LD and the
            // inlined GraphQL payload (preloadedQueries) to reconstruct the
            // issue body and comments into readable markdown.
            fn try_extract_github_issue_markdown(html: &str) -> Option<String> {
                // Helper: extract the first <script type="application/ld+json"> block
                fn extract_ld_json(html: &str) -> Option<serde_json::Value> {
                    let mut s = html;
                    loop {
                        let start = s.find("<script").map(|i| i)?;
                        let rest = &s[start + 7..];
                        if rest.to_lowercase().contains("type=\"application/ld+json\"") {
                            // Find end of script open tag
                            let open_end_rel = rest.find('>')?;
                            let open_end = start + 7 + open_end_rel + 1;
                            let after_open = &s[open_end..];
                            // Find closing </script>
                            if let Some(close_rel) = after_open.to_lowercase().find("</script>") {
                                let json_str = &after_open[..close_rel];
                                if let Ok(v) = serde_json::from_str::<serde_json::Value>(json_str) {
                                    return Some(v);
                                }
                                // Some pages JSON-encode the JSON-LD; try to unescape once
                                if let Ok(un) = serde_json::from_str::<String>(json_str) {
                                    if let Ok(v2) = serde_json::from_str::<serde_json::Value>(&un) {
                                        return Some(v2);
                                    }
                                }
                                // Advance after this script to search for next
                                s = &after_open[close_rel + 9..];
                                continue;
                            }
                        }
                        // Advance and continue search
                        s = &rest[1..];
                    }
                }

                // Helper: extract substring for the JSON array that follows key
                fn extract_json_array_after(html: &str, key: &str) -> Option<String> {
                    let idx = html.find(key)?;
                    let bytes = html.as_bytes();
                    // Find the first '[' after key
                    let mut i = idx + key.len();
                    while i < bytes.len() && bytes[i] != b'[' { i += 1; }
                    if i >= bytes.len() { return None; }
                    let start = i;
                    // Scan to matching ']' accounting for strings and escapes
                    let mut depth: i32 = 0;
                    let mut in_str = false;
                    let mut escape = false;
                    while i < bytes.len() {
                        let c = bytes[i] as char;
                        if in_str {
                            if escape { escape = false; }
                            else if c == '\\' { escape = true; }
                            else if c == '"' { in_str = false; }
                            i += 1; continue;
                        }
                        match c {
                            '"' => { in_str = true; },
                            '[' => { depth += 1; },
                            ']' => { depth -= 1; if depth == 0 { let end = i + 1; return Some(html[start..end].to_string()); } },
                            _ => {}
                        }
                        i += 1;
                    }
                    None
                }

                // Parse JSON-LD for headline, articleBody, author, date
                let mut title: Option<String> = None;
                let mut issue_body_md: Option<String> = None;
                let mut opened_by: Option<String> = None;
                let mut opened_at: Option<String> = None;
                if let Some(ld) = extract_ld_json(html) {
                    if ld.get("@type").and_then(|v| v.as_str()) == Some("DiscussionForumPosting") {
                        title = ld.get("headline").and_then(|v| v.as_str()).map(|s| s.to_string());
                        issue_body_md = ld.get("articleBody").and_then(|v| v.as_str()).map(|s| s.to_string());
                        opened_by = ld.get("author").and_then(|a| a.get("name")).and_then(|v| v.as_str()).map(|s| s.to_string());
                        opened_at = ld.get("datePublished").and_then(|v| v.as_str()).map(|s| s.to_string());
                    }
                }

                // Parse GraphQL payload for comments and state
                let arr_str = extract_json_array_after(html, "\"preloadedQueries\"")?;
                let arr: serde_json::Value = serde_json::from_str(&arr_str).ok()?;
                let mut comments: Vec<(String, String, String)> = Vec::new();
                let mut state: Option<String> = None;
                let mut state_reason: Option<String> = None;
                if let Some(items) = arr.as_array() {
                    for item in items {
                        let repo = item.get("result").and_then(|v| v.get("data")).and_then(|v| v.get("repository"));
                        let issue = repo.and_then(|r| r.get("issue"));
                        if let Some(issue) = issue {
                            if state.is_none() {
                                state = issue.get("state").and_then(|v| v.as_str()).map(|s| s.to_string());
                                state_reason = issue.get("stateReason").and_then(|v| v.as_str()).map(|s| s.to_string());
                            }
                            if let Some(edges) = issue.get("frontTimelineItems").and_then(|v| v.get("edges")).and_then(|v| v.as_array()) {
                                for e in edges {
                                    let node = e.get("node");
                                    let typename = node.and_then(|n| n.get("__typename")).and_then(|v| v.as_str()).unwrap_or("");
                                    if typename == "IssueComment" {
                                        let author = node.and_then(|n| n.get("author")).and_then(|a| a.get("login")).and_then(|v| v.as_str()).unwrap_or("");
                                        let created = node.and_then(|n| n.get("createdAt")).and_then(|v| v.as_str()).unwrap_or("");
                                        let body = node.and_then(|n| n.get("body")).and_then(|v| v.as_str()).unwrap_or("");
                                        if !body.is_empty() {
                                            comments.push((author.to_string(), created.to_string(), body.to_string()));
                                        } else {
                                            let body_html = node.and_then(|n| n.get("bodyHTML")).and_then(|v| v.as_str()).unwrap_or("");
                                            if !body_html.is_empty() {
                                                // Minimal HTML→MD for comments if body missing
                                                let options = htmd::options::Options { heading_style: htmd::options::HeadingStyle::Atx, code_block_style: htmd::options::CodeBlockStyle::Fenced, link_style: htmd::options::LinkStyle::Inlined, ..Default::default() };
                                                let conv = htmd::HtmlToMarkdown::builder().options(options).build();
                                                if let Ok(md) = conv.convert(body_html) {
                                                    comments.push((author.to_string(), created.to_string(), md));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // If nothing meaningful extracted, bail out.
                if title.is_none() && comments.is_empty() && issue_body_md.is_none() {
                    return None;
                }

                // Compose readable markdown
                let mut out = String::new();
                if let Some(t) = title { out.push_str(&format!("# {}\n\n", t)); }
                if let (Some(by), Some(at)) = (opened_by, opened_at) { out.push_str(&format!("Opened by {} on {}\n\n", by, at)); }
                if let (Some(s), _) = (state.clone(), state_reason.clone()) { out.push_str(&format!("State: {}\n\n", s)); }
                if let Some(body) = issue_body_md { out.push_str(&format!("{}\n\n", body)); }
                if !comments.is_empty() {
                    out.push_str("## Comments\n\n");
                    for (author, created, body) in comments {
                        out.push_str(&format!("- {} — {}\n\n{}\n\n", author, created, body));
                    }
                }
                Some(out)
            }

            // Helper: convert HTML to markdown and truncate if too large.
            fn convert_html_to_markdown_trimmed(html: String, max_chars: usize) -> crate::error::Result<(String, bool)> {
                let options = htmd::options::Options {
                    heading_style: htmd::options::HeadingStyle::Atx,
                    code_block_style: htmd::options::CodeBlockStyle::Fenced,
                    link_style: htmd::options::LinkStyle::Inlined,
                    ..Default::default()
                };
                let converter = htmd::HtmlToMarkdown::builder().options(options).build();
                let reduced = extract_main(&html).unwrap_or(html);
                let sanitized = strip_noisy_tags(reduced);
                let markdown = converter.convert(&sanitized)?;
                let markdown = postprocess_markdown(&markdown);
                let mut truncated = false;
                let rendered = {
                    let char_count = markdown.chars().count();
                    if char_count > max_chars {
                        truncated = true;
                        let mut s: String = markdown.chars().take(max_chars).collect();
                        s.push_str("\n\n… (truncated)\n");
                        s
                    } else {
                        markdown
                    }
                };
                Ok((rendered, truncated))
            }

            // Helper: detect WAF/challenge pages to avoid dumping challenge content.
            fn detect_block_vendor(_status: reqwest::StatusCode, body: &str) -> Option<&'static str> {
                // Identify common bot-challenge pages regardless of HTTP status.
                // Cloudflare often returns 200 with a challenge that requires JS/cookies.
                let lower = body.to_lowercase();
                if lower.contains("cloudflare")
                    || lower.contains("cf-ray")
                    || lower.contains("_cf_chl_opt")
                    || lower.contains("challenge-platform")
                    || lower.contains("checking if the site connection is secure")
                    || lower.contains("waiting for")
                    || lower.contains("just a moment")
                {
                    return Some("cloudflare");
                }
                None
            }

            fn headers_indicate_block(headers: &reqwest::header::HeaderMap) -> bool {
                let h = headers;
                let has_cf_ray = h.get("cf-ray").is_some();
                let has_cf_mitigated = h.get("cf-mitigated").is_some();
                let has_cf_bm = h.get("set-cookie").and_then(|v| v.to_str().ok()).map(|s| s.contains("__cf_bm=")).unwrap_or(false);
                let has_chlray = h.get("server-timing").and_then(|v| v.to_str().ok()).map(|s| s.to_lowercase().contains("chlray")).unwrap_or(false);
                has_cf_ray || has_cf_mitigated || has_cf_bm || has_chlray
            }

            fn looks_like_challenge_markdown(md: &str) -> bool {
                let l = md.to_lowercase();
                l.contains("just a moment") || l.contains("enable javascript and cookies") || l.contains("waiting for ")
            }

            let timeout = Duration::from_millis(params.timeout_ms.unwrap_or(15000));
            let codex_ua = crate::default_client::get_codex_user_agent(Some("web_fetch"));

            // Heuristic: some domains render key content client-side. Prefer a
            // quick browser render first so we capture comments/timelines.
            let is_dynamic_domain = {
                let u = params.url.to_lowercase();
                u.contains("github.com/")
            };

            if !matches!(params.mode.as_deref(), Some("http")) && is_dynamic_domain {
                let browser_manager = codex_browser::global::get_or_create_browser_manager().await;
                if let Ok(res) = browser_manager.goto(&params.url).await {
                    // Poll briefly for discussion/timeline elements to appear
                    for _ in 0..6 {
                        let js = r#"(function(){ const sel1 = document.querySelectorAll('[data-test-selector=\"issue-comment-body\"]'); const sel2 = document.querySelectorAll('.js-timeline-item'); return (sel1.length + sel2.length); })()"#;
                        if let Ok(val) = browser_manager.execute_javascript(js).await {
                            let n = val.get("value").and_then(|v| v.as_i64()).unwrap_or(0);
                            if n > 0 { break; }
                        }
                        tokio::time::sleep(Duration::from_millis(800)).await;
                    }
                    if let Ok(val) = browser_manager.execute_javascript(r#"(function(){ return { html: document.documentElement.outerHTML, title: document.title||'' }; })()"#).await {
                        if let Some(html) = val.get("value").and_then(|v| v.get("html")).and_then(|v| v.as_str()) {
                            let (markdown, truncated) = match convert_html_to_markdown_trimmed(html.to_string(), 120_000) {
                                Ok(t) => t,
                                Err(e) => {
                                    return ResponseInputItem::FunctionCallOutput { call_id: call_id_clone, output: FunctionCallOutputPayload { content: format!("Markdown conversion failed: {e}"), success: Some(false) } };
                                }
                            };
                            let body = serde_json::json!({
                                "url": params.url,
                                "status": 200,
                                "final_url": res.url,
                                "content_type": "text/html",
                                "used_browser_ua": true,
                                "via_browser": true,
                                "truncated": truncated,
                                "markdown": markdown,
                            });
                            return ResponseInputItem::FunctionCallOutput { call_id: call_id_clone, output: FunctionCallOutputPayload { content: body.to_string(), success: Some(true) } };
                        }
                    }
                }
            }

            // If explicit browser mode requested, try the internal browser first.
            if matches!(params.mode.as_deref(), Some("browser")) {
                {
                    let browser_manager = codex_browser::global::get_or_create_browser_manager().await;
                    if let Ok(res) = browser_manager.goto(&params.url).await {
                        // Allow a few short settles for JS/cookie challenges to auto-resolve
                        let mut html: Option<String> = None;
                        for _ in 0..4 {
                            if let Ok(val) = browser_manager.execute_javascript("(function(){ return { html: document.documentElement.outerHTML, title: document.title||'' }; })()").await {
                                html = val.get("value").and_then(|v| v.get("html")).and_then(|v| v.as_str()).map(|s| s.to_string());
                                let t_low = val.get("value").and_then(|v| v.get("title")).and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                                if !(t_low.contains("just a moment") || t_low.contains("checking if") || t_low.contains("waiting for")) {
                                    break;
                                }
                            }
                            tokio::time::sleep(Duration::from_millis(1200)).await;
                        }
                        if let Some(html) = html {
                            let (markdown, truncated) = match convert_html_to_markdown_trimmed(html, 120_000) {
                                Ok(t) => t,
                                Err(e) => {
                                    return ResponseInputItem::FunctionCallOutput {
                                        call_id: call_id_clone,
                                        output: FunctionCallOutputPayload { content: format!("Markdown conversion failed: {e}"), success: Some(false) },
                                    };
                                }
                            };
                            let body = serde_json::json!({
                                "url": params.url,
                                "status": 200,
                                "final_url": res.url,
                                "content_type": "text/html",
                                "used_browser_ua": true,
                                "via_browser": true,
                                "truncated": truncated,
                                "markdown": markdown,
                            });
                            return ResponseInputItem::FunctionCallOutput { call_id: call_id_clone, output: FunctionCallOutputPayload { content: body.to_string(), success: Some(true) } };
                        }
                    }
                }
            }
            // Attempt 1: Codex UA + polite headers
            let resp = match do_request(&params.url, &codex_ua, timeout, None).await {
                Ok(r) => r,
                Err(e) => {
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload { content: format!("Request failed: {e}"), success: Some(false) },
                    };
                }
            };

            // Capture metadata before consuming the response body.
            let mut status = resp.status();
            let mut final_url = resp.url().to_string();
            let mut headers = resp.headers().clone();
            // Read body
            let mut body_text = match resp.text().await {
                Ok(t) => t,
                Err(e) => {
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload { content: format!("Failed to read response body: {e}"), success: Some(false) },
                    };
                }
            };
            let mut used_browser_ua = false;
            let browser_ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36";
            if !matches!(params.mode.as_deref(), Some("http")) && (detect_block_vendor(status, &body_text).is_some() || headers_indicate_block(&headers)) {
                // Simple retry with a browser UA and extra headers
                let extra = [
                    (reqwest::header::HeaderName::from_static("upgrade-insecure-requests"), "1"),
                ];
                if let Ok(r2) = do_request(&params.url, browser_ua, timeout, Some(&extra)).await {
                    let status2 = r2.status();
                    let final_url2 = r2.url().to_string();
                    let headers2 = r2.headers().clone();
                    if let Ok(t2) = r2.text().await {
                        used_browser_ua = true;
                        status = status2;
                        final_url = final_url2;
                        headers = headers2;
                        body_text = t2;
                    }
                }
            }

            // Response metadata
            let content_type = headers
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();

            // Provide structured diagnostics if blocked by WAF (even if HTTP 200)
            if !matches!(params.mode.as_deref(), Some("http")) && (detect_block_vendor(status, &body_text).is_some() || headers_indicate_block(&headers)) {
                let vendor = "cloudflare";
                let retry_after = headers
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string());
                let cf_ray = headers
                    .get("cf-ray")
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string());

                let mut diag = serde_json::json!({
                    "final_url": final_url,
                    "content_type": content_type,
                    "used_browser_ua": used_browser_ua,
                    "blocked_by_waf": true,
                    "vendor": vendor,
                });
                if let Some(ra) = retry_after { diag["retry_after"] = serde_json::json!(ra); }
                if let Some(ray) = cf_ray { diag["cf_ray"] = serde_json::json!(ray); }

                // Attempt a last-resort browser-based fetch if the live browser is available.
                {
                    let browser_manager = codex_browser::global::get_or_create_browser_manager().await;
                    browser_manager.set_enabled_sync(true);
                    // Try navigate and extract outerHTML via JS (after SPA settles per manager config)
                    if browser_manager.goto(&params.url).await.is_ok() {
                        let js = "(function(){ return { html: document.documentElement.outerHTML, title: document.title||'' }; })()";
                        if let Ok(val) = browser_manager.execute_javascript(js).await {
                            let html = val.get("value").and_then(|v| v.get("html")).and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let title = val.get("value").and_then(|v| v.get("title")).and_then(|v| v.as_str()).unwrap_or("");
                            if !html.is_empty() && title.to_lowercase() != "just a moment..." {
                                let (markdown, truncated) = match convert_html_to_markdown_trimmed(html, 120_000) {
                                    Ok(t) => t,
                                    Err(e) => {
                                        return ResponseInputItem::FunctionCallOutput {
                                            call_id: call_id_clone,
                                            output: FunctionCallOutputPayload { content: format!("Markdown conversion failed: {e}"), success: Some(false) },
                                        };
                                    }
                                };
                                diag["via_browser"] = serde_json::json!(true);
                                let body = serde_json::json!({
                                    "url": params.url,
                                    "status": 200,
                                    "final_url": final_url,
                                    "content_type": content_type,
                                    "used_browser_ua": used_browser_ua,
                                    "truncated": truncated,
                                    "markdown": markdown,
                                });
                                return ResponseInputItem::FunctionCallOutput { call_id: call_id_clone, output: FunctionCallOutputPayload { content: body.to_string(), success: Some(true) } };
                            }
                        }
                
                        // If JS extraction failed, try a CDP outerHTML of root
                        let root = browser_manager.execute_cdp("DOM.getDocument", json!({"depth": 1})).await.ok();
                        if let Some(root) = root {
                            if let Some(node_id) = root.get("root").and_then(|r| r.get("nodeId")).and_then(|n| n.as_u64()) {
                                if let Ok(outer) = browser_manager.execute_cdp("DOM.getOuterHTML", json!({"nodeId": node_id})).await {
                                    if let Some(html) = outer.get("outerHTML").and_then(|v| v.as_str()) {
                                        let (markdown, truncated) = match convert_html_to_markdown_trimmed(html.to_string(), 120_000) {
                                            Ok(t) => t,
                                            Err(e) => {
                                                return ResponseInputItem::FunctionCallOutput {
                                                    call_id: call_id_clone,
                                                    output: FunctionCallOutputPayload { content: format!("Markdown conversion failed: {e}"), success: Some(false) },
                                                };
                                            }
                                        };
                                        diag["via_browser"] = serde_json::json!(true);
                                        let body = serde_json::json!({
                                            "url": params.url,
                                            "status": 200,
                                            "final_url": final_url,
                                            "content_type": content_type,
                                            "used_browser_ua": used_browser_ua,
                                            "truncated": truncated,
                                            "markdown": markdown,
                                        });
                                        return ResponseInputItem::FunctionCallOutput { call_id: call_id_clone, output: FunctionCallOutputPayload { content: body.to_string(), success: Some(true) } };
                                    }
                                }
                            }
                        }
                    }
                }

                let (md_preview, _trunc) = match convert_html_to_markdown_trimmed(body_text, 2000) {
                    Ok(t) => t,
                    Err(_) => ("".to_string(), false),
                };

                let body = serde_json::json!({
                    "url": params.url,
                    "status": status.as_u16(),
                    "error": "Blocked by site challenge",
                    "diagnostics": diag,
                    "markdown": md_preview,
                });

                return ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload { content: body.to_string(), success: Some(false) },
                };
            }

            // If not success, provide structured, minimal diagnostics without dumping content.
            if !status.is_success() {
                let waf_vendor = detect_block_vendor(status, &body_text);
                let retry_after = headers
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string());
                let cf_ray = headers
                    .get("cf-ray")
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string());

                let mut diag = serde_json::json!({
                    "final_url": final_url,
                    "content_type": content_type,
                    "used_browser_ua": used_browser_ua,
                });
                if let Some(vendor) = waf_vendor { diag["blocked_by_waf"] = serde_json::json!(true); diag["vendor"] = serde_json::json!(vendor); }
                if let Some(ra) = retry_after { diag["retry_after"] = serde_json::json!(ra); }
                if let Some(ray) = cf_ray { diag["cf_ray"] = serde_json::json!(ray); }

                // Provide a tiny, safe preview of visible text only (converted and truncated).
                let (md_preview, _trunc) = match convert_html_to_markdown_trimmed(body_text, 2000) {
                    Ok(t) => t,
                    Err(_) => ("".to_string(), false),
                };

                let body = serde_json::json!({
                    "url": params.url,
                    "status": status.as_u16(),
                    "error": format!("HTTP {} {}", status.as_u16(), status.canonical_reason().unwrap_or("")),
                    "diagnostics": diag,
                    // Keep a short, human-friendly preview; avoid dumping raw HTML or long JS.
                    "markdown": md_preview,
                });

                return ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload { content: body.to_string(), success: Some(false) },
                };
            }

            // Domain-specific extraction first (e.g., GitHub issues)
            if params.url.contains("github.com/") && params.url.contains("/issues/") {
                if let Some(md) = try_extract_github_issue_markdown(&body_text) {
                    let body = serde_json::json!({
                        "url": params.url,
                        "status": status.as_u16(),
                        "final_url": final_url,
                        "content_type": content_type,
                        "used_browser_ua": used_browser_ua,
                        "truncated": false,
                        "markdown": md,
                    });
                    return ResponseInputItem::FunctionCallOutput { call_id: call_id_clone, output: FunctionCallOutputPayload { content: body.to_string(), success: Some(true) } };
                }
            }

            // Success: convert to markdown (sanitized and size-limited)
            let (markdown, truncated) = match convert_html_to_markdown_trimmed(body_text, 120_000) {
                Ok(t) => t,
                Err(e) => {
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload { content: format!("Markdown conversion failed: {e}"), success: Some(false) },
                    };
                }
            };

            // If the rendered markdown still looks like a challenge page, attempt browser fallback (unless http-only).
            if !matches!(params.mode.as_deref(), Some("http")) && looks_like_challenge_markdown(&markdown) {
                {
                    let browser_manager = codex_browser::global::get_or_create_browser_manager().await;
                    browser_manager.set_enabled_sync(true);
                    if browser_manager.goto(&params.url).await.is_ok() {
                        let js = "(function(){ return { html: document.documentElement.outerHTML, title: document.title||'' }; })()";
                        let mut html: Option<String> = None;
                        for _ in 0..3 {
                            if let Ok(val) = browser_manager.execute_javascript(js).await {
                                html = val.get("value").and_then(|v| v.get("html")).and_then(|v| v.as_str()).map(|s| s.to_string());
                                let t_low = val.get("value").and_then(|v| v.get("title")).and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                                if !(t_low.contains("just a moment") || t_low.contains("checking if") || t_low.contains("waiting for")) {
                                    break;
                                }
                            }
                            tokio::time::sleep(Duration::from_millis(1200)).await;
                        }
                        if let Some(html) = html {
                            let (md2, truncated2) = match convert_html_to_markdown_trimmed(html, 120_000) {
                                Ok(t) => t,
                                Err(e) => {
                                    return ResponseInputItem::FunctionCallOutput { call_id: call_id_clone, output: FunctionCallOutputPayload { content: format!("Markdown conversion failed: {e}"), success: Some(false) } };
                                }
                            };
                            let body = serde_json::json!({
                                "url": params.url,
                                "status": 200,
                                "final_url": final_url,
                                "content_type": content_type,
                                "used_browser_ua": true,
                                "via_browser": true,
                                "truncated": truncated2,
                                "markdown": md2,
                            });
                            return ResponseInputItem::FunctionCallOutput { call_id: call_id_clone, output: FunctionCallOutputPayload { content: body.to_string(), success: Some(true) } };
                        }
                    }
                }

                // If fallback not possible, return structured error rather than a useless challenge page
                let body = serde_json::json!({
                    "url": params.url,
                    "status": 200,
                    "error": "Blocked by site challenge",
                    "diagnostics": { "final_url": final_url, "content_type": content_type, "used_browser_ua": used_browser_ua, "blocked_by_waf": true, "vendor": "cloudflare", "detected_via": "markdown" },
                    "markdown": markdown.chars().take(2000).collect::<String>(),
                });
                return ResponseInputItem::FunctionCallOutput { call_id: call_id_clone, output: FunctionCallOutputPayload { content: body.to_string(), success: Some(false) } };
            }

            let body = serde_json::json!({
                "url": params.url,
                "status": status.as_u16(),
                "final_url": final_url,
                "content_type": content_type,
                "used_browser_ua": used_browser_ua,
                "truncated": truncated,
                "markdown": markdown,
            });

            ResponseInputItem::FunctionCallOutput { call_id: call_id_clone, output: FunctionCallOutputPayload { content: body.to_string(), success: Some(true) } }
        },
    ).await
}

fn to_exec_params(params: ShellToolCallParams, sess: &Session) -> ExecParams {
    ExecParams {
        command: params.command,
        cwd: sess.resolve_path(params.workdir.clone()),
        timeout_ms: params.timeout_ms,
        env: create_env(&sess.shell_environment_policy),
        with_escalated_permissions: params.with_escalated_permissions,
        justification: params.justification,
    }
}

fn parse_container_exec_arguments(
    arguments: String,
    sess: &Session,
    call_id: &str,
) -> Result<ExecParams, Box<ResponseInputItem>> {
    // parse command
    match serde_json::from_str::<ShellToolCallParams>(&arguments) {
        Ok(shell_tool_call_params) => Ok(to_exec_params(shell_tool_call_params, sess)),
        Err(e) => {
            // allow model to re-sample
            let output = ResponseInputItem::FunctionCallOutput {
                call_id: call_id.to_string(),
                output: FunctionCallOutputPayload {
                    content: format!("failed to parse function arguments: {e}"),
                    success: None,
                },
            };
            Err(Box::new(output))
        }
    }
}

pub struct ExecInvokeArgs<'a> {
    pub params: ExecParams,
    pub sandbox_type: SandboxType,
    pub sandbox_policy: &'a SandboxPolicy,
    pub codex_linux_sandbox_exe: &'a Option<PathBuf>,
    pub stdout_stream: Option<StdoutStream>,
}

fn maybe_run_with_user_profile(params: ExecParams, sess: &Session) -> ExecParams {
    if sess.shell_environment_policy.use_profile {
        let maybe_command = sess
            .user_shell
            .format_default_shell_invocation(params.command.clone());
        if let Some(command) = maybe_command {
            return ExecParams { command, ..params };
        }
    }
    params
}

async fn handle_run_agent(sess: &Session, ctx: &ToolCallCtx, arguments: String) -> ResponseInputItem {
    let params_for_event = serde_json::from_str(&arguments).ok();
    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();
    execute_custom_tool(
        sess,
        ctx,
        "agent_run".to_string(),
        params_for_event,
        || async move {
    match serde_json::from_str::<RunAgentParams>(&arguments_clone) {
        Ok(params) => {
            let mut manager = AGENT_MANAGER.write().await;

            // Handle model parameter (can be string or array)
            let models = match params.model {
                Some(serde_json::Value::String(model)) => vec![model],
                Some(serde_json::Value::Array(models)) => models
                    .into_iter()
                    .filter_map(|m| m.as_str().map(String::from))
                    .collect(),
                _ => vec!["code".to_string()], // Default model
            };

            // Helper: derive the command to check for a given model/config pair.
            fn resolve_command_for_check(model: &str, cfg: Option<&crate::config_types::AgentConfig>) -> (String, bool) {
                if let Some(c) = cfg { return (c.command.clone(), false); }
                let m = model.to_lowercase();
                match m.as_str() {
                    // Built-in: always available via current_exe fallback.
                    "code" | "codex" => (m, true),
                    // External CLIs expected to be in PATH
                    "claude" => ("claude".to_string(), false),
                    "gemini" => ("gemini".to_string(), false),
                    _ => (m, false),
                }
            }

            // Helper: PATH lookup to determine if a command exists.
            fn command_exists(cmd: &str) -> bool {
                // Absolute/relative path with separators: verify it points to a file.
                if cmd.contains(std::path::MAIN_SEPARATOR) || cmd.contains('/') || cmd.contains('\\') {
                    return std::fs::metadata(cmd).map(|m| m.is_file()).unwrap_or(false);
                }

                #[cfg(target_os = "windows")]
                {
                    return which::which(cmd).map(|p| p.is_file()).unwrap_or(false);
                }

                #[cfg(not(target_os = "windows"))]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let Some(path_os) = std::env::var_os("PATH") else { return false; };
                    for dir in std::env::split_paths(&path_os) {
                        if dir.as_os_str().is_empty() { continue; }
                        let candidate = dir.join(cmd);
                        if let Ok(meta) = std::fs::metadata(&candidate) {
                            if meta.is_file() {
                                let mode = meta.permissions().mode();
                                if mode & 0o111 != 0 { return true; }
                            }
                        }
                    }
                    false
                }
            }

            let batch_id = if models.len() > 1 {
                Some(Uuid::new_v4().to_string())
            } else {
                None
            };

            let mut agent_ids = Vec::new();
            let mut skipped: Vec<String> = Vec::new();
            for model in models {
                // Check if this model is configured and enabled
                let agent_config = sess.agents.iter().find(|a| {
                    a.name.to_lowercase() == model.to_lowercase()
                        || a.command.to_lowercase() == model.to_lowercase()
                });

                if let Some(config) = agent_config {
                    if !config.enabled {
                        continue; // Skip disabled agents
                    }

                    let (cmd_to_check, is_builtin) = resolve_command_for_check(&model, Some(config));
                    if !is_builtin && !command_exists(&cmd_to_check) {
                        skipped.push(format!("{} (missing: {})", model, cmd_to_check));
                        continue;
                    }

                    // Override read_only if agent is configured as read-only
                    let read_only = config.read_only || params.read_only.unwrap_or(false);

                    let agent_id = manager
                        .create_agent_with_config(
                            model,
                            params.task.clone(),
                            params.context.clone(),
                            params.output.clone(),
                            params.files.clone().unwrap_or_default(),
                            read_only,
                            batch_id.clone(),
                            config.clone(),
                        )
                        .await;
                    agent_ids.push(agent_id);
                } else {
                    // Use default configuration for unknown agents
                    let (cmd_to_check, is_builtin) = resolve_command_for_check(&model, None);
                    if !is_builtin && !command_exists(&cmd_to_check) {
                        skipped.push(format!("{} (missing: {})", model, cmd_to_check));
                        continue;
                    }
                    let agent_id = manager
                        .create_agent(
                            model,
                            params.task.clone(),
                            params.context.clone(),
                            params.output.clone(),
                            params.files.clone().unwrap_or_default(),
                            params.read_only.unwrap_or(false),
                            batch_id.clone(),
                        )
                        .await;
                    agent_ids.push(agent_id);
                }
            }

            // If nothing runnable remains, fall back to a single built‑in Codex agent.
            if agent_ids.is_empty() {
                let agent_id = manager
                    .create_agent(
                        "code".to_string(),
                        params.task.clone(),
                        params.context.clone(),
                        params.output.clone(),
                        params.files.clone().unwrap_or_default(),
                        params.read_only.unwrap_or(false),
                        None,
                    )
                    .await;
                agent_ids.push(agent_id);
            }

            // Send agent status update event
            drop(manager); // Release the write lock first
            if agent_ids.len() > 0 {
                send_agent_status_update(sess).await;
            }

            let response = if let Some(batch_id) = batch_id {
                serde_json::json!({
                    "batch_id": batch_id,
                    "agent_ids": agent_ids,
                    "status": "started",
                    "message": format!("Started {} agents", agent_ids.len()),
                    "skipped": if skipped.is_empty() { None } else { Some(skipped) }
                })
            } else {
                serde_json::json!({
                    "agent_id": agent_ids[0],
                    "status": "started",
                    "message": "Agent started successfully",
                    "skipped": if skipped.is_empty() { None } else { Some(skipped) }
                })
            };

            ResponseInputItem::FunctionCallOutput {
                call_id: call_id_clone,
                output: FunctionCallOutputPayload {
                    content: response.to_string(),
                    success: Some(true),
                },
            }
        }
        Err(e) => ResponseInputItem::FunctionCallOutput {
            call_id: call_id_clone,
            output: FunctionCallOutputPayload {
                content: format!("Invalid agent_run arguments: {}", e),
                success: None,
            },
        },
    }
        },
    ).await
}

async fn handle_check_agent_status(sess: &Session, ctx: &ToolCallCtx, arguments: String) -> ResponseInputItem {
    let params_for_event = serde_json::from_str(&arguments).ok();
    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();
    execute_custom_tool(
        sess,
        ctx,
        "agent_check".to_string(),
        params_for_event,
        || async move {
    match serde_json::from_str::<CheckAgentStatusParams>(&arguments_clone) {
        Ok(params) => {
            let manager = AGENT_MANAGER.read().await;

            if let Some(agent) = manager.get_agent(&params.agent_id) {
                // Limit progress in the response; write full progress to file if large
                let max_progress_lines = 50usize;
                let total_progress = agent.progress.len();
                let progress_preview: Vec<String> = if total_progress > max_progress_lines {
                    agent
                        .progress
                        .iter()
                        .skip(total_progress - max_progress_lines)
                        .cloned()
                        .collect()
                } else {
                    agent.progress.clone()
                };

                let mut progress_file: Option<String> = None;
                if total_progress > max_progress_lines {
                    let cwd = sess.get_cwd().to_path_buf();
                    drop(manager);
                    let dir = match ensure_agent_dir(&cwd, &agent.id) {
                        Ok(d) => d,
                        Err(e) => {
                            return ResponseInputItem::FunctionCallOutput {
                                call_id: call_id_clone,
                                output: FunctionCallOutputPayload {
                                    content: format!("Failed to prepare agent progress file: {}", e),
                                    success: Some(false),
                                },
                            };
                        }
                    };
                    // Re-acquire manager to get fresh progress after potential delay
                    let manager = AGENT_MANAGER.read().await;
                    if let Some(agent) = manager.get_agent(&params.agent_id) {
                        let joined = agent.progress.join("\n");
                        match write_agent_file(&dir, "progress.log", &joined) {
                            Ok(p) => progress_file = Some(p.display().to_string()),
                            Err(e) => {
                                return ResponseInputItem::FunctionCallOutput {
                                    call_id: call_id_clone,
                                    output: FunctionCallOutputPayload {
                                        content: format!("Failed to write progress file: {}", e),
                                        success: Some(false),
                                    },
                                };
                            }
                        }
                    }
                } else {
                    drop(manager);
                }

                let response = serde_json::json!({
                    "agent_id": params.agent_id,
                    "status": agent.status,
                    "model": agent.model,
                    "created_at": agent.created_at,
                    "started_at": agent.started_at,
                    "completed_at": agent.completed_at,
                    "progress_preview": progress_preview,
                    "progress_total": total_progress,
                    "progress_file": progress_file,
                    "error": agent.error,
                    "worktree_path": agent.worktree_path,
                    "branch_name": agent.branch_name,
                });

                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        content: response.to_string(),
                        success: Some(true),
                    },
                }
            } else {
                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        content: format!("Agent not found: {}", params.agent_id),
                        success: Some(false),
                    },
                }
            }
        }
        Err(e) => ResponseInputItem::FunctionCallOutput {
            call_id: call_id_clone,
            output: FunctionCallOutputPayload {
                content: format!("Invalid agent_check arguments: {}", e),
                success: None,
            },
        },
    }
        },
    ).await
}

async fn handle_get_agent_result(sess: &Session, ctx: &ToolCallCtx, arguments: String) -> ResponseInputItem {
    let params_for_event = serde_json::from_str(&arguments).ok();
    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();
    execute_custom_tool(
        sess,
        ctx,
        "agent_result".to_string(),
        params_for_event,
        || async move {
    match serde_json::from_str::<GetAgentResultParams>(&arguments_clone) {
        Ok(params) => {
            let manager = AGENT_MANAGER.read().await;

            if let Some(agent) = manager.get_agent(&params.agent_id) {
                let cwd = sess.get_cwd().to_path_buf();
                let dir = match ensure_agent_dir(&cwd, &params.agent_id) {
                    Ok(d) => d,
                    Err(e) => {
                        return ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone,
                            output: FunctionCallOutputPayload {
                                content: format!("Failed to prepare agent output dir: {}", e),
                                success: Some(false),
                            },
                        };
                    }
                };

                match agent.status {
                    AgentStatus::Completed => {
                        let output_text = agent.result.unwrap_or_default();
                        let (preview, total_lines) = preview_first_n_lines(&output_text, 500);
                        let file_path = match write_agent_file(&dir, "result.txt", &output_text) {
                            Ok(p) => p.display().to_string(),
                            Err(e) => format!("Failed to write result file: {}", e),
                        };
                        let response = serde_json::json!({
                            "agent_id": params.agent_id,
                            "status": agent.status,
                            "output_preview": preview,
                            "output_total_lines": total_lines,
                            "output_file": file_path,
                        });
                        ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone,
                            output: FunctionCallOutputPayload {
                                content: response.to_string(),
                                success: Some(true),
                            },
                        }
                    }
                    AgentStatus::Failed => {
                        let error_text = agent.error.unwrap_or_else(|| "Unknown error".to_string());
                        let (preview, total_lines) = preview_first_n_lines(&error_text, 500);
                        let file_path = match write_agent_file(&dir, "error.txt", &error_text) {
                            Ok(p) => p.display().to_string(),
                            Err(e) => format!("Failed to write error file: {}", e),
                        };
                        let response = serde_json::json!({
                            "agent_id": params.agent_id,
                            "status": agent.status,
                            "error_preview": preview,
                            "error_total_lines": total_lines,
                            "error_file": file_path,
                        });
                        ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone,
                            output: FunctionCallOutputPayload {
                                content: response.to_string(),
                                success: Some(false),
                            },
                        }
                    }
                    _ => ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload {
                            content: format!(
                                "Agent is still {}: cannot get result yet",
                                serde_json::to_string(&agent.status)
                                    .unwrap_or_else(|_| "running".to_string())
                            ),
                            success: Some(false),
                        },
                    },
                }
            } else {
                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        content: format!("Agent not found: {}", params.agent_id),
                        success: Some(false),
                    },
                }
            }
        }
        Err(e) => ResponseInputItem::FunctionCallOutput {
            call_id: call_id_clone,
            output: FunctionCallOutputPayload {
                content: format!("Invalid agent_result arguments: {}", e),
                success: None,
            },
        },
    }
        },
    ).await
}

async fn handle_cancel_agent(sess: &Session, ctx: &ToolCallCtx, arguments: String) -> ResponseInputItem {
    let params_for_event = serde_json::from_str(&arguments).ok();
    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();
    execute_custom_tool(
        sess,
        ctx,
        "agent_cancel".to_string(),
        params_for_event,
        || async move {
    match serde_json::from_str::<CancelAgentParams>(&arguments_clone) {
        Ok(params) => {
            let mut manager = AGENT_MANAGER.write().await;

            if let Some(agent_id) = params.agent_id {
                if manager.cancel_agent(&agent_id).await {
                    ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload {
                            content: format!("Agent {} cancelled", agent_id),
                            success: Some(true),
                        },
                    }
                } else {
                    ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload {
                            content: format!("Failed to cancel agent {}", agent_id),
                            success: Some(false),
                        },
                    }
                }
            } else if let Some(batch_id) = params.batch_id {
                let count = manager.cancel_batch(&batch_id).await;
                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        content: format!("Cancelled {} agents in batch {}", count, batch_id),
                        success: Some(true),
                    },
                }
            } else {
                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        content: "Either agent_id or batch_id must be provided".to_string(),
                        success: Some(false),
                    },
                }
            }
        }
        Err(e) => ResponseInputItem::FunctionCallOutput {
            call_id: call_id_clone,
            output: FunctionCallOutputPayload {
                content: format!("Invalid agent_cancel arguments: {}", e),
                success: None,
            },
        },
    }
        },
    ).await
}

async fn handle_wait_for_agent(sess: &Session, ctx: &ToolCallCtx, arguments: String) -> ResponseInputItem {
    let params_for_event = serde_json::from_str(&arguments).ok();
    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();
    execute_custom_tool(
        sess,
        ctx,
        "agent_wait".to_string(),
        params_for_event,
        || async move {
    match serde_json::from_str::<WaitForAgentParams>(&arguments_clone) {
        Ok(params) => {
            let timeout =
                std::time::Duration::from_secs(params.timeout_seconds.unwrap_or(300).min(600));
            let start = std::time::Instant::now();

            loop {
                if start.elapsed() > timeout {
                    return ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload {
                            content: "Timeout waiting for agent completion".to_string(),
                            success: Some(false),
                        },
                    };
                }

                let manager = AGENT_MANAGER.read().await;

                if let Some(agent_id) = &params.agent_id {
                    if let Some(agent) = manager.get_agent(agent_id) {
                        if matches!(
                            agent.status,
                            AgentStatus::Completed | AgentStatus::Failed | AgentStatus::Cancelled
                        ) {
                            // Include output/error preview and file path
                            let cwd = sess.get_cwd().to_path_buf();
                            let dir = ensure_agent_dir(&cwd, &agent.id).unwrap_or_else(|_| cwd.clone());
                            let (preview_key, file_key, preview, file_path, total_lines) = match agent.status {
                                AgentStatus::Completed => {
                                    let text = agent.result.clone().unwrap_or_default();
                                    let (p, total) = preview_first_n_lines(&text, 500);
                                    let fp = write_agent_file(&dir, "result.txt", &text)
                                        .map(|p| p.display().to_string())
                                        .unwrap_or_else(|e| format!("Failed to write result file: {}", e));
                                    ("output_preview", "output_file", p, fp, total)
                                }
                                AgentStatus::Failed => {
                                    let text = agent.error.clone().unwrap_or_else(|| "Unknown error".to_string());
                                    let (p, total) = preview_first_n_lines(&text, 500);
                                    let fp = write_agent_file(&dir, "error.txt", &text)
                                        .map(|p| p.display().to_string())
                                        .unwrap_or_else(|e| format!("Failed to write error file: {}", e));
                                    ("error_preview", "error_file", p, fp, total)
                                }
                                AgentStatus::Cancelled => {
                                    let text = "Agent cancelled".to_string();
                                    let (p, total) = preview_first_n_lines(&text, 500);
                                    let fp = write_agent_file(&dir, "status.txt", &text)
                                        .map(|p| p.display().to_string())
                                        .unwrap_or_else(|e| format!("Failed to write status file: {}", e));
                                    ("status_preview", "status_file", p, fp, total)
                                }
                                _ => unreachable!(),
                            };

                            let mut response = serde_json::json!({
                                "agent_id": agent.id,
                                "status": agent.status,
                                "wait_time_seconds": start.elapsed().as_secs(),
                                "total_lines": total_lines,
                            });
                            if let Some(obj) = response.as_object_mut() {
                                obj.insert(preview_key.to_string(), serde_json::Value::String(preview));
                                obj.insert(file_key.to_string(), serde_json::Value::String(file_path));
                            }
                            return ResponseInputItem::FunctionCallOutput {
                                call_id: call_id_clone,
                                output: FunctionCallOutputPayload {
                                    content: response.to_string(),
                                    success: Some(true),
                                },
                            };
                        }
                    }
                } else if let Some(batch_id) = &params.batch_id {
                    let agents = manager.list_agents(None, Some(batch_id.clone()), false);

                    // Separate terminal vs non-terminal agents
                    let completed_agents: Vec<_> = agents
                        .iter()
                        .filter(|t| {
                            matches!(
                                t.status,
                                AgentStatus::Completed
                                    | AgentStatus::Failed
                                    | AgentStatus::Cancelled
                            )
                        })
                        .cloned()
                        .collect();
                    let any_in_progress = agents.iter().any(|a| {
                        matches!(a.status, AgentStatus::Pending | AgentStatus::Running)
                    });

                    if params.return_all.unwrap_or(false) {
                        // Wait for ALL agents in the batch to reach a terminal state
                        if !any_in_progress {
                            let response = serde_json::json!({
                                "batch_id": batch_id,
                                "completed_agents": completed_agents.iter().map(|t| t.id.clone()).collect::<Vec<_>>(),
                                "wait_time_seconds": start.elapsed().as_secs(),
                            });
                            return ResponseInputItem::FunctionCallOutput {
                                call_id: call_id_clone,
                                output: FunctionCallOutputPayload {
                                    content: response.to_string(),
                                    success: Some(true),
                                },
                            };
                        }
                    } else {
                        // Sequential behavior: return the next unseen completed agent if available
                        let mut state = sess.state.lock().unwrap();
                        let seen = state
                            .seen_completed_agents_by_batch
                            .entry(batch_id.clone())
                            .or_default();

                        // Find the first completed agent that we haven't returned yet
                        if let Some(unseen) = completed_agents
                            .iter()
                            .find(|a| !seen.contains(&a.id))
                            .cloned()
                        {
                            // Record as seen and return immediately
                            seen.insert(unseen.id.clone());
                            drop(state);

                            // Include output/error preview for the unseen completed agent
                            let cwd = sess.get_cwd().to_path_buf();
                            let dir = ensure_agent_dir(&cwd, &unseen.id).unwrap_or_else(|_| cwd.clone());
                            let (preview_key, file_key, preview, file_path, total_lines) = match unseen.status {
                                AgentStatus::Completed => {
                                    let text = unseen.result.clone().unwrap_or_default();
                                    let (p, total) = preview_first_n_lines(&text, 500);
                                    let fp = write_agent_file(&dir, "result.txt", &text)
                                        .map(|p| p.display().to_string())
                                        .unwrap_or_else(|e| format!("Failed to write result file: {}", e));
                                    ("output_preview", "output_file", p, fp, total)
                                }
                                AgentStatus::Failed => {
                                    let text = unseen.error.clone().unwrap_or_else(|| "Unknown error".to_string());
                                    let (p, total) = preview_first_n_lines(&text, 500);
                                    let fp = write_agent_file(&dir, "error.txt", &text)
                                        .map(|p| p.display().to_string())
                                        .unwrap_or_else(|e| format!("Failed to write error file: {}", e));
                                    ("error_preview", "error_file", p, fp, total)
                                }
                                AgentStatus::Cancelled => {
                                    let text = "Agent cancelled".to_string();
                                    let (p, total) = preview_first_n_lines(&text, 500);
                                    let fp = write_agent_file(&dir, "status.txt", &text)
                                        .map(|p| p.display().to_string())
                                        .unwrap_or_else(|e| format!("Failed to write status file: {}", e));
                                    ("status_preview", "status_file", p, fp, total)
                                }
                                _ => unreachable!(),
                            };

                            let mut response = serde_json::json!({
                                "agent_id": unseen.id,
                                "status": unseen.status,
                                "wait_time_seconds": start.elapsed().as_secs(),
                                "total_lines": total_lines,
                            });
                            if let Some(obj) = response.as_object_mut() {
                                obj.insert(preview_key.to_string(), serde_json::Value::String(preview));
                                obj.insert(file_key.to_string(), serde_json::Value::String(file_path));
                            }
                            return ResponseInputItem::FunctionCallOutput {
                                call_id: call_id_clone,
                                output: FunctionCallOutputPayload {
                                    content: response.to_string(),
                                    success: Some(true),
                                },
                            };
                        }

                        // If all agents in the batch are terminal and all have been seen, return immediately
                        if !any_in_progress && !completed_agents.is_empty() {
                            // Mark all as seen to keep state consistent
                            for a in &completed_agents {
                                seen.insert(a.id.clone());
                            }
                            drop(state);

                            let response = serde_json::json!({
                                "batch_id": batch_id,
                                "status": "no_agents_remaining",
                                "wait_time_seconds": start.elapsed().as_secs(),
                            });
                            return ResponseInputItem::FunctionCallOutput {
                                call_id: call_id_clone,
                                output: FunctionCallOutputPayload {
                                    content: response.to_string(),
                                    success: Some(true),
                                },
                            };
                        }
                    }
                }

                drop(manager);
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
        Err(e) => ResponseInputItem::FunctionCallOutput {
            call_id: call_id_clone,
            output: FunctionCallOutputPayload {
                content: format!("Invalid wait_for_agent arguments: {}", e),
                success: None,
            },
        },
    }
        },
    ).await
}

async fn handle_list_agents(sess: &Session, ctx: &ToolCallCtx, arguments: String) -> ResponseInputItem {
    let params_for_event = serde_json::from_str(&arguments).ok();
    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();
    execute_custom_tool(
        sess,
        ctx,
        "agent_list".to_string(),
        params_for_event,
        || async move {
    match serde_json::from_str::<ListAgentsParams>(&arguments_clone) {
        Ok(params) => {
            let manager = AGENT_MANAGER.read().await;

            let status_filter =
                params
                    .status_filter
                    .and_then(|s| match s.to_lowercase().as_str() {
                        "pending" => Some(AgentStatus::Pending),
                        "running" => Some(AgentStatus::Running),
                        "completed" => Some(AgentStatus::Completed),
                        "failed" => Some(AgentStatus::Failed),
                        "cancelled" => Some(AgentStatus::Cancelled),
                        _ => None,
                    });

            let agents = manager.list_agents(
                status_filter,
                params.batch_id,
                params.recent_only.unwrap_or(false),
            );

            // Count running agents for status update
            let running_count = agents
                .iter()
                .filter(|a| a.status == AgentStatus::Running)
                .count();
            if running_count > 0 {
                let status_msg = format!(
                    "🤖 {} agent{} currently running",
                    running_count,
                    if running_count != 1 { "s" } else { "" }
                );
    let event = Event { id: "agent-status".to_string(), event_seq: 0, msg: EventMsg::BackgroundEvent(BackgroundEventEvent { message: status_msg }), order: None };
                let _ = sess.tx_event.send(event).await;
            }

            // Add status counts to summary
            let pending_count = agents
                .iter()
                .filter(|a| a.status == AgentStatus::Pending)
                .count();
            let running_count = agents
                .iter()
                .filter(|a| a.status == AgentStatus::Running)
                .count();
            let completed_count = agents
                .iter()
                .filter(|a| a.status == AgentStatus::Completed)
                .count();
            let failed_count = agents
                .iter()
                .filter(|a| a.status == AgentStatus::Failed)
                .count();
            let cancelled_count = agents
                .iter()
                .filter(|a| a.status == AgentStatus::Cancelled)
                .count();

            let summary = serde_json::json!({
                "total_agents": agents.len(),
                "status_counts": {
                    "pending": pending_count,
                    "running": running_count,
                    "completed": completed_count,
                    "failed": failed_count,
                    "cancelled": cancelled_count,
                },
                "agents": agents.iter().map(|t| {
                    serde_json::json!({
                        "id": t.id,
                        "model": t.model,
                        "status": t.status,
                        "created_at": t.created_at,
                        "batch_id": t.batch_id,
                        "worktree_path": t.worktree_path,
                        "branch_name": t.branch_name,
                    })
                }).collect::<Vec<_>>(),
            });

            ResponseInputItem::FunctionCallOutput {
                call_id: call_id_clone,
                output: FunctionCallOutputPayload {
                    content: summary.to_string(),
                    success: Some(true),
                },
            }
        }
        Err(e) => ResponseInputItem::FunctionCallOutput {
            call_id: call_id_clone,
            output: FunctionCallOutputPayload {
                content: format!("Invalid list_agents arguments: {}", e),
                success: None,
            },
        },
    }
        },
    ).await
}

async fn handle_container_exec_with_params(
    params: ExecParams,
    sess: &Session,
    turn_diff_tracker: &mut TurnDiffTracker,
    sub_id: String,
    call_id: String,
    seq_hint: Option<u64>,
    output_index: Option<u32>,
    attempt_req: u64,
) -> ResponseInputItem {
    // Intercept risky git branch-changing commands and require an explicit confirm prefix.
    // We support a simple convention: prefix the script with `confirm:` to proceed.
    // The prefix is stripped before execution.
    fn extract_shell_script_from_wrapper(argv: &[String]) -> Option<(usize, String)> {
        // Return (index_of_script, script) if argv matches: <shell> (-lc|-c) <script>
        if argv.len() == 3 {
            let shell = std::path::Path::new(&argv[0])
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            let is_shell = matches!(shell, "bash" | "sh" | "zsh");
            let is_flag = matches!(argv[1].as_str(), "-lc" | "-c");
            if is_shell && is_flag {
                return Some((2, argv[2].clone()));
            }
        }
        None
    }

    fn looks_like_branch_change(script: &str) -> bool {
        // Goal: detect branch-changing git invocations while avoiding false
        // positives from commit messages or other quoted strings. We do a
        // lightweight scan that strips quoted regions before token analysis.

        // 1) Strip single- and double-quoted segments (keep length with spaces).
        let mut cleaned = String::with_capacity(script.len());
        let mut in_squote = false;
        let mut in_dquote = false;
        let mut prev_was_backslash = false;
        for ch in script.chars() {
            let mut emit_space = false;
            match ch {
                '\\' => {
                    // Track escapes inside double quotes; in single quotes, backslash has no special meaning in POSIX sh.
                    prev_was_backslash = !prev_was_backslash;
                }
                '\'' if !in_dquote => {
                    in_squote = !in_squote;
                    emit_space = true;
                    prev_was_backslash = false;
                }
                '"' if !in_squote && !prev_was_backslash => {
                    in_dquote = !in_dquote;
                    emit_space = true;
                    prev_was_backslash = false;
                }
                _ => {
                    prev_was_backslash = false;
                }
            }
            if in_squote || in_dquote || emit_space {
                cleaned.push(' ');
            } else {
                cleaned.push(ch);
            }
        }

        // 2) Split into simple commands at common separators.
        for chunk in cleaned.split(|c| matches!(c, ';' | '\n' | '\r')) {
            // Further split on conditional operators while keeping order.
            for part in chunk.split(|c| matches!(c, '|' | '&')) {
                let s = part.trim();
                if s.is_empty() { continue; }
                // Tokenize on whitespace.
                let mut it = s.split_whitespace();
                // Skip leading env assignments (FOO=bar) and `env`.
                let mut first = None;
                while let Some(tok) = it.next() {
                    if tok.contains('=') && !tok.starts_with('=') && !tok.starts_with('-') {
                        continue;
                    }
                    if tok == "env" { continue; }
                    first = Some(tok);
                    break;
                }
                let Some(cmd) = first else { continue };
                // Identify `git` executable (allow path prefixes).
                let is_git = cmd.ends_with("/git") || cmd == "git";
                if !is_git { continue; }
                // Next token is the subcommand.
                let Some(sub) = it.next() else { continue; };
                match sub {
                    "checkout" => {
                        // If any of the strong branch-changing flags are present, flag it.
                        let mut saw_branch_change_flag = false;
                        let mut args: Vec<&str> = Vec::new();
                        for a in it.clone() { args.push(a); }
                        for a in &args {
                            if matches!(*a, "-b" | "-B" | "--orphan" | "--detach") {
                                saw_branch_change_flag = true;
                                break;
                            }
                        }
                        if saw_branch_change_flag { return true; }
                        // If `--` is present, this is a path checkout, not branch.
                        if args.iter().any(|a| *a == "--") { continue; }
                        // Heuristic: a single non-flag argument likely denotes a branch.
                        // To reduce false positives (e.g. `git checkout .`), only flag
                        // when the first arg does not start with '-' and is not a solitary '.' or '..'.
                        if let Some(first_arg) = args.first() {
                            let a = *first_arg;
                            if !a.starts_with('-') && a != "." && a != ".." {
                                return true;
                            }
                        }
                    }
                    "switch" => {
                        // `git switch -c <name>` creates; `git switch <name>` changes.
                        let mut args = it;
                        let mut saw_c = false;
                        let mut first_non_flag: Option<&str> = None;
                        while let Some(a) = args.next() {
                            if a == "-c" { saw_c = true; break; }
                            if a.starts_with('-') { continue; }
                            first_non_flag = Some(a);
                            break;
                        }
                        if saw_c || first_non_flag.is_some() { return true; }
                    }
                    // Future: consider `git branch -D/-m` as branch‑modifying, but keep
                    // this minimal to avoid over‑blocking normal workflows.
                    _ => {}
                }
            }
        }
        false
    }

    // If the argv is a shell wrapper, analyze and optionally strip `confirm:`.
    let mut params = params;
    let mut seq_hint_for_exec = seq_hint;
    if let Some((script_index, script)) = extract_shell_script_from_wrapper(&params.command) {
        let trimmed = script.trim_start();
        let confirm_prefixes = ["confirm:", "CONFIRM:"];
        let has_confirm_prefix = confirm_prefixes
            .iter()
            .any(|p| trimmed.starts_with(p));

        // If no confirm prefix and it looks like a branch change, reject with guidance.
        if !has_confirm_prefix && looks_like_branch_change(trimmed) {
            // Provide the exact argv the model should resend with the confirm prefix.
            let mut argv_confirm = params.command.clone();
            argv_confirm[script_index] = format!("confirm: {}", script.trim_start());
            let suggested = serde_json::to_string(&argv_confirm)
                .unwrap_or_else(|_| "<failed to serialize suggested argv>".to_string());
            let guidance = format!(
                "blocked potentially destructive git branch change. Git branching should only be performed when explicitly requested by the user. To proceed, resend the shell call with a confirmation prefix to indicate it was explicitly requested. Please use 'confirm:' to confirm it was requested.\n\noriginal_script: {}\nresend_exact_argv: {}",
                script,
                suggested
            );
            return ResponseInputItem::FunctionCallOutput {
                call_id,
                output: FunctionCallOutputPayload { content: guidance, success: None },
            };
        }

        // If confirm prefix present, strip it before execution.
        if has_confirm_prefix {
            let without_prefix = confirm_prefixes
                .iter()
                .find_map(|p| {
                    let t = trimmed.strip_prefix(p)?;
                    Some(t.trim_start().to_string())
                })
                .unwrap_or_else(|| trimmed.to_string());
            params.command[script_index] = without_prefix;
        }

        // Detect an embedded `apply_patch <<EOF ... EOF` in a larger script and split it out
        // so the UI can render a distinct "Updated" block before the "Run" block.
        //
        // If present, we will:
        //  1) Execute the patch as a standalone apply_patch exec (with proper events)
        //  2) Remove the statement from the script and continue with the remainder
        //     (only if the patch succeeded)
        if let Ok(Some(found)) = codex_apply_patch::find_embedded_apply_patch(&params.command[script_index]) {
            // Build a synthetic minimal script for verified parsing of the patch action,
            // preserving an optional cd path for correct path resolution.
            let synthetic_script = if let Some(cd) = &found.cd_path {
                format!("cd {} && apply_patch <<'EOF'\n{}\nEOF\n", cd, found.patch_body)
            } else {
                format!("apply_patch <<'EOF'\n{}\nEOF\n", found.patch_body)
            };

            // Resolve into an ApplyPatchAction using the verified path
            let cwd_path = std::path::Path::new(&params.cwd);
            let verified = codex_apply_patch::maybe_parse_apply_patch_verified(
                &vec!["bash".to_string(), "-lc".to_string(), synthetic_script.clone()],
                cwd_path,
            );
            if let codex_apply_patch::MaybeApplyPatchVerified::Body(action) = verified {
                // First, run the patch apply as its own Exec with proper events
                let path_to_codex = std::env::current_exe()
                    .ok()
                    .map(|p| p.to_string_lossy().to_string());
                if let Some(path_to_codex) = path_to_codex {
                    let patch_params = ExecParams {
                        command: vec![
                            path_to_codex,
                            CODEX_APPLY_PATCH_ARG1.to_string(),
                            action.patch.clone(),
                        ],
                        cwd: action.cwd.clone(),
                        timeout_ms: params.timeout_ms,
                        env: HashMap::new(),
                        with_escalated_permissions: params.with_escalated_permissions,
                        justification: params.justification.clone(),
                    };

                    // Safety for patch step mirrors normal patch handling
                    let safety = assess_safety_for_untrusted_command(
                        sess.approval_policy,
                        &sess.sandbox_policy,
                        patch_params.with_escalated_permissions.unwrap_or(false),
                    );

                    let exec_command_context = ExecCommandContext {
                        sub_id: sub_id.clone(),
                        call_id: format!("{call_id}.apply_patch"),
                        command_for_display: vec!["apply_patch".to_string(), action.patch.clone()],
                        cwd: patch_params.cwd.clone(),
                        apply_patch: Some(ApplyPatchCommandContext {
                            user_explicitly_approved_this_action: matches!(safety, SafetyCheck::AutoApprove { .. }),
                            changes: convert_apply_patch_to_protocol(&action),
                        }),
                    };

                    let patch_result = sess
                        .run_exec_with_events(
                            turn_diff_tracker,
                            exec_command_context,
                            ExecInvokeArgs {
                                params: patch_params.clone(),
                                sandbox_type: match safety { SafetyCheck::AutoApprove { sandbox_type } => sandbox_type, SafetyCheck::AskUser => SandboxType::None, SafetyCheck::Reject { .. } => SandboxType::None },
                                sandbox_policy: &sess.sandbox_policy,
                                codex_linux_sandbox_exe: &sess.codex_linux_sandbox_exe,
                                stdout_stream: None,
                            },
                            seq_hint, // occupy the provided sequence range first
                            output_index,
                            attempt_req,
                        )
                        .await;

                    let mut should_continue = false;
                    if let Ok(ref out) = patch_result { should_continue = out.exit_code == 0; }

                    // If patch step succeeded, strip it from the original script and proceed.
                    if should_continue {
                        let (start, end) = found.stmt_byte_range;
                        let mut residual = String::new();
                        residual.push_str(&params.command[script_index][..start]);
                        residual.push_str(&params.command[script_index][end..]);
                        // Clean leading/trailing separators to avoid syntax errors
                        let mut residual = residual.trim().to_string();
                        // Remove leading connectors like &&, ||, ; and trailing ones
                        let trim_connectors = |s: &str| -> String {
                            let mut s = s.trim().to_string();
                            // Leading
                            for _ in 0..2 {
                                let st = s.trim_start();
                                let new = if st.starts_with("&&") { &st[2..] } else if st.starts_with("||") { &st[2..] } else if st.starts_with(';') { &st[1..] } else { st };
                                s = new.trim_start().to_string();
                            }
                            // Trailing
                            for _ in 0..2 {
                                let st = s.trim_end();
                                let new = if st.ends_with("&&") { &st[..st.len()-2] } else if st.ends_with("||") { &st[..st.len()-2] } else if st.ends_with(';') { &st[..st.len()-1] } else { st };
                                s = new.trim_end().to_string();
                            }
                            s
                        };
                        residual = trim_connectors(&residual);

                        if !residual.is_empty() {
                            params.command[script_index] = residual;
                            // Bump the seq hint for the following run cell
                            seq_hint_for_exec = seq_hint.map(|h| h.saturating_add(2));

                            // Continue with normal flow using updated params (fallthrough below)
                            // We overwrite `script` in local scope to avoid borrow issues
                        } else {
                            // No more commands to run; return the patch result as the tool output
                            let ok = patch_result
                                .as_ref()
                                .map(|o| o.exit_code == 0)
                                .unwrap_or(false);
                            let content = match patch_result {
                                Ok(out) => format_exec_output_with_limit(sess, &sub_id, &call_id, &out),
                                Err(e) => get_error_message_ui(&e),
                            };
                            return ResponseInputItem::FunctionCallOutput { call_id, output: FunctionCallOutputPayload { content, success: Some(ok) } };
                        }
                    } else {
                        // Patch failed; return immediately with patch output (do not run remainder)
                        let ok = patch_result
                            .as_ref()
                            .map(|o| o.exit_code == 0)
                            .unwrap_or(false);
                        let content = match patch_result {
                            Ok(out) => format_exec_output_with_limit(sess, &sub_id, &call_id, &out),
                            Err(e) => get_error_message_ui(&e),
                        };
                        return ResponseInputItem::FunctionCallOutput { call_id, output: FunctionCallOutputPayload { content, success: Some(ok) } };
                    }
                }
            }
        }
    }
    // check if this was a patch, and apply it if so
    let apply_patch_exec = match maybe_parse_apply_patch_verified(&params.command, &params.cwd) {
        MaybeApplyPatchVerified::Body(changes) => {
            match apply_patch::apply_patch(sess, &sub_id, &call_id, changes).await {
                InternalApplyPatchInvocation::Output(item) => return item,
                InternalApplyPatchInvocation::DelegateToExec(apply_patch_exec) => {
                    Some(apply_patch_exec)
                }
            }
        }
        MaybeApplyPatchVerified::CorrectnessError(parse_error) => {
            // It looks like an invocation of `apply_patch`, but we
            // could not resolve it into a patch that would apply
            // cleanly. Return to model for resample.
            return ResponseInputItem::FunctionCallOutput {
                call_id,
                output: FunctionCallOutputPayload {
                    content: format!("error: {parse_error:#}"),
                    success: None,
                },
            };
        }
        MaybeApplyPatchVerified::ShellParseError(error) => {
            trace!("Failed to parse shell command, {error:?}");
            None
        }
        MaybeApplyPatchVerified::NotApplyPatch => None,
    };

    let (params, safety, command_for_display) = match &apply_patch_exec {
        Some(ApplyPatchExec {
            action: ApplyPatchAction { patch, cwd, .. },
            user_explicitly_approved_this_action,
        }) => {
            let path_to_codex = std::env::current_exe()
                .ok()
                .map(|p| p.to_string_lossy().to_string());
            let Some(path_to_codex) = path_to_codex else {
                return ResponseInputItem::FunctionCallOutput {
                    call_id,
                    output: FunctionCallOutputPayload {
                        content: "failed to determine path to codex executable".to_string(),
                        success: None,
                    },
                };
            };

            let params = ExecParams {
                command: vec![
                    path_to_codex,
                    CODEX_APPLY_PATCH_ARG1.to_string(),
                    patch.clone(),
                ],
                cwd: cwd.clone(),
                timeout_ms: params.timeout_ms,
                env: HashMap::new(),
                with_escalated_permissions: params.with_escalated_permissions,
                justification: params.justification.clone(),
            };
            let safety = if *user_explicitly_approved_this_action {
                SafetyCheck::AutoApprove {
                    sandbox_type: SandboxType::None,
                }
            } else {
                assess_safety_for_untrusted_command(
                    sess.approval_policy,
                    &sess.sandbox_policy,
                    params.with_escalated_permissions.unwrap_or(false),
                )
            };
            (
                params,
                safety,
                vec!["apply_patch".to_string(), patch.clone()],
            )
        }
        None => {
            let safety = {
                let state = sess.state.lock().unwrap();
                assess_command_safety(
                    &params.command,
                    sess.approval_policy,
                    &sess.sandbox_policy,
                    &state.approved_commands,
                    params.with_escalated_permissions.unwrap_or(false),
                )
            };
            let command_for_display = params.command.clone();
            (params, safety, command_for_display)
        }
    };

    let sandbox_type = match safety {
        SafetyCheck::AutoApprove { sandbox_type } => sandbox_type,
        SafetyCheck::AskUser => {
            let rx_approve = sess
                .request_command_approval(
                    sub_id.clone(),
                    call_id.clone(),
                    params.command.clone(),
                    params.cwd.clone(),
                    params.justification.clone(),
                )
                .await;
            match rx_approve.await.unwrap_or_default() {
                ReviewDecision::Approved => (),
                ReviewDecision::ApprovedForSession => {
                    sess.add_approved_command(params.command.clone());
                }
                ReviewDecision::Denied | ReviewDecision::Abort => {
                    return ResponseInputItem::FunctionCallOutput {
                        call_id,
                        output: FunctionCallOutputPayload {
                            content: "exec command rejected by user".to_string(),
                            success: None,
                        },
                    };
                }
            }
            // No sandboxing is applied because the user has given
            // explicit approval. Often, we end up in this case because
            // the command cannot be run in a sandbox, such as
            // installing a new dependency that requires network access.
            SandboxType::None
        }
        SafetyCheck::Reject { reason } => {
            return ResponseInputItem::FunctionCallOutput {
                call_id,
                output: FunctionCallOutputPayload {
                    content: format!("exec command rejected: {reason}"),
                    success: None,
                },
            };
        }
    };

    let exec_command_context = ExecCommandContext {
        sub_id: sub_id.clone(),
        call_id: call_id.clone(),
        command_for_display: command_for_display.clone(),
        cwd: params.cwd.clone(),
        apply_patch: apply_patch_exec.map(
            |ApplyPatchExec {
                 action,
                 user_explicitly_approved_this_action,
             }| ApplyPatchCommandContext {
                user_explicitly_approved_this_action,
                changes: convert_apply_patch_to_protocol(&action),
            },
        ),
    };

    let params = maybe_run_with_user_profile(params, sess);
    let output_result = sess
        .run_exec_with_events(
            turn_diff_tracker,
            exec_command_context.clone(),
            ExecInvokeArgs {
                params: params.clone(),
                sandbox_type,
                sandbox_policy: &sess.sandbox_policy,
                codex_linux_sandbox_exe: &sess.codex_linux_sandbox_exe,
                stdout_stream: if exec_command_context.apply_patch.is_some() {
                    None
                } else {
                    Some(StdoutStream {
                        sub_id: sub_id.clone(),
                        call_id: call_id.clone(),
                        tx_event: sess.tx_event.clone(),
                    })
                },
            },
            seq_hint_for_exec,
            output_index,
            attempt_req,
        )
        .await;

    match output_result {
        Ok(output) => {
            let ExecToolCallOutput { exit_code, .. } = &output;

            let is_success = *exit_code == 0;
            let content = format_exec_output_with_limit(sess, &sub_id, &call_id, &output);
            ResponseInputItem::FunctionCallOutput {
                call_id: call_id.clone(),
                output: FunctionCallOutputPayload {
                    content,
                    success: Some(is_success),
                },
            }
        }
        Err(CodexErr::Sandbox(error)) => {
            handle_sandbox_error(
                turn_diff_tracker,
                params,
                exec_command_context,
                error,
                sandbox_type,
                sess,
                attempt_req,
            )
            .await
        }
        Err(e) => ResponseInputItem::FunctionCallOutput {
            call_id: call_id.clone(),
            output: FunctionCallOutputPayload {
                content: format!("execution error: {e}"),
                success: None,
            },
        },
    }
}

async fn handle_sandbox_error(
    turn_diff_tracker: &mut TurnDiffTracker,
    params: ExecParams,
    exec_command_context: ExecCommandContext,
    error: SandboxErr,
    sandbox_type: SandboxType,
    sess: &Session,
    attempt_req: u64,
) -> ResponseInputItem {
    let call_id = exec_command_context.call_id.clone();
    let sub_id = exec_command_context.sub_id.clone();
    let cwd = exec_command_context.cwd.clone();

    // Early out if either the user never wants to be asked for approval, or
    // we're letting the model manage escalation requests. Otherwise, continue
    match sess.approval_policy {
        AskForApproval::Never | AskForApproval::OnRequest => {
            // Clarify when Read Only mode is the reason a command cannot proceed.
            let content = if matches!(sess.sandbox_policy, SandboxPolicy::ReadOnly) {
                format!(
                    "command blocked by Read Only mode: {}",
                    error
                )
            } else {
                format!(
                    "failed in sandbox {sandbox_type:?} with execution error: {error}"
                )
            };
            return ResponseInputItem::FunctionCallOutput {
                call_id,
                output: FunctionCallOutputPayload {
                    content,
                    success: Some(false),
                },
            };
        }
        AskForApproval::UnlessTrusted | AskForApproval::OnFailure => (),
    }

    // similarly, if the command timed out, we can simply return this failure to the model
    if matches!(error, SandboxErr::Timeout) {
        return ResponseInputItem::FunctionCallOutput {
            call_id,
            output: FunctionCallOutputPayload {
                content: format!(
                    "command timed out after {} milliseconds",
                    params.timeout_duration().as_millis()
                ),
                success: Some(false),
            },
        };
    }

    // Note that when `error` is `SandboxErr::Denied`, it could be a false
    // positive. That is, it may have exited with a non-zero exit code, not
    // because the sandbox denied it, but because that is its expected behavior,
    // i.e., a grep command that did not match anything. Ideally we would
    // include additional metadata on the command to indicate whether non-zero
    // exit codes merit a retry.

    // For now, we categorically ask the user to retry without sandbox and
    // emit the raw error as a background event.
    sess.notify_background_event(&sub_id, format!("Execution failed: {error}"))
        .await;

    let rx_approve = sess
        .request_command_approval(
            sub_id.clone(),
            call_id.clone(),
            params.command.clone(),
            cwd.clone(),
            Some("command failed; retry without sandbox?".to_string()),
        )
        .await;

    match rx_approve.await.unwrap_or_default() {
        ReviewDecision::Approved | ReviewDecision::ApprovedForSession => {
            // Persist this command as pre‑approved for the
            // remainder of the session so future
            // executions skip the sandbox directly.
            // TODO(ragona): Isn't this a bug? It always saves the command in an | fork?
            sess.add_approved_command(params.command.clone());
            // Inform UI we are retrying without sandbox.
            sess.notify_background_event(&sub_id, "retrying command without sandbox")
                .await;

            // This is an escalated retry; the policy will not be
            // examined and the sandbox has been set to `None`.
            // Use the same attempt_req as the tool call that failed; this retry
            // is still part of the current provider attempt.
            let retry_output_result = sess
                .run_exec_with_events(
                    turn_diff_tracker,
                    exec_command_context.clone(),
                    ExecInvokeArgs {
                        params,
                        sandbox_type: SandboxType::None,
                        sandbox_policy: &sess.sandbox_policy,
                        codex_linux_sandbox_exe: &sess.codex_linux_sandbox_exe,
                        stdout_stream: if exec_command_context.apply_patch.is_some() {
                            None
                        } else {
                            Some(StdoutStream {
                                sub_id: sub_id.clone(),
                                call_id: call_id.clone(),
                                tx_event: sess.tx_event.clone(),
                            })
                        },
                    },
                    None,
                    None,
                    attempt_req,
                )
                .await;

            match retry_output_result {
                Ok(retry_output) => {
                    let ExecToolCallOutput { exit_code, .. } = &retry_output;

                    let is_success = *exit_code == 0;
                    let content = format_exec_output_with_limit(sess, &sub_id, &call_id, &retry_output);

                    ResponseInputItem::FunctionCallOutput {
                        call_id: call_id.clone(),
                        output: FunctionCallOutputPayload {
                            content,
                            success: Some(is_success),
                        },
                    }
                }
                Err(e) => ResponseInputItem::FunctionCallOutput {
                    call_id: call_id.clone(),
                    output: FunctionCallOutputPayload {
                        content: format!("retry failed: {e}"),
                        success: None,
                    },
                },
            }
        }
        ReviewDecision::Denied | ReviewDecision::Abort => {
            // Fall through to original failure handling.
            ResponseInputItem::FunctionCallOutput {
                call_id,
                output: FunctionCallOutputPayload {
                    content: "exec command rejected by user".to_string(),
                    success: None,
                },
            }
        }
    }
}

// Limit extremely large tool outputs before sending to the model to avoid
// context overflows. Keep this conservative because multiple tool outputs
// can appear in a single turn. The limit is in bytes (on the UTF‑8 string).
const MAX_TOOL_OUTPUT_BYTES_FOR_MODEL: usize = 32 * 1024; // 32 KiB

fn truncate_middle_bytes(s: &str, max_bytes: usize) -> (String, bool) {
    if s.len() <= max_bytes {
        return (s.to_string(), false);
    }
    if max_bytes == 0 {
        return ("…truncated…".to_string(), true);
    }

    // Try to keep some head/tail, favoring newline boundaries when possible.
    let keep = max_bytes.saturating_sub("…truncated…\n".len());
    let left_budget = keep / 2;
    let right_budget = keep - left_budget;

    // Safe prefix end on a char boundary, prefer last newline within budget.
    let prefix_end = {
        let mut end = left_budget.min(s.len());
        if let Some(head) = s.get(..end) {
            if let Some(i) = head.rfind('\n') { end = i + 1; }
        }
        while end > 0 && !s.is_char_boundary(end) { end -= 1; }
        end
    };

    // Safe suffix start on a char boundary, prefer first newline within budget.
    let suffix_start = {
        let mut start = s.len().saturating_sub(right_budget);
        if let Some(tail) = s.get(start..) {
            if let Some(i) = tail.find('\n') { start += i + 1; }
        }
        while start < s.len() && !s.is_char_boundary(start) { start += 1; }
        start
    };

    let mut out = String::with_capacity(max_bytes);
    out.push_str(&s[..prefix_end]);
    out.push_str("…truncated…\n");
    out.push_str(&s[suffix_start..]);
    (out, true)
}

fn format_exec_output_str(exec_output: &ExecToolCallOutput) -> String {
    let ExecToolCallOutput {
        aggregated_output,
        ..
    } = exec_output;

    // Always use the aggregated (stdout + stderr interleaved) stream so the
    // model sees the full build log regardless of which stream a tool used.
    let mut formatted_output = aggregated_output.text.clone();
    if let Some(truncated_after_lines) = aggregated_output.truncated_after_lines {
        formatted_output.push_str(&format!(
            "\n\n[Output truncated after {truncated_after_lines} lines: too many lines or bytes.]",
        ));
    }

    formatted_output
}

/// Exec output serialized for the model. If the payload is too large,
/// write the full output to a file and include a truncated preview here.
fn format_exec_output_with_limit(
    sess: &Session,
    sub_id: &str,
    call_id: &str,
    exec_output: &ExecToolCallOutput,
) -> String {
    let ExecToolCallOutput {
        exit_code,
        duration,
        ..
    } = exec_output;

    #[derive(Serialize)]
    struct ExecMetadata {
        exit_code: i32,
        duration_seconds: f32,
    }

    #[derive(Serialize)]
    struct ExecOutput<'a> { output: &'a str, metadata: ExecMetadata }

    // round to 1 decimal place
    let duration_seconds = ((duration.as_secs_f32()) * 10.0).round() / 10.0;

    let full = format_exec_output_str(exec_output);
    let (maybe_truncated, was_truncated) =
        truncate_middle_bytes(&full, MAX_TOOL_OUTPUT_BYTES_FOR_MODEL);

    // If truncated, persist the full output under .code/agents/<agent>/exec-<call_id>.txt
    // so users can inspect it and the model can refer to a short, stable path.
    let final_output = if was_truncated {
        let cwd = sess.get_cwd().to_path_buf();
        let file_note = match ensure_agent_dir(&cwd, sub_id)
            .and_then(|dir| write_agent_file(&dir, &format!("exec-{call_id}.txt"), &full))
        {
            Ok(path) => format!("\n\n[Full output saved to: {}]", path.display()),
            Err(e) => format!("\n\n[Full output was too large and truncation applied; failed to save file: {e}]")
        };
        let mut s = maybe_truncated;
        s.push_str(&file_note);
        s
    } else {
        maybe_truncated
    };

    let payload = ExecOutput {
        output: &final_output,
        metadata: ExecMetadata {
            exit_code: *exit_code,
            duration_seconds,
        },
    };

    #[expect(clippy::expect_used)]
    serde_json::to_string(&payload).expect("serialize ExecOutput")
}

fn get_last_assistant_message_from_turn(responses: &[ResponseItem]) -> Option<String> {
    responses.iter().rev().find_map(|item| {
        if let ResponseItem::Message { role, content, .. } = item {
            if role == "assistant" {
                content.iter().rev().find_map(|ci| {
                    if let ContentItem::OutputText { text } = ci {
                        Some(text.clone())
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        } else {
            None
        }
    })
}

async fn drain_to_completed(sess: &Session, sub_id: &str, prompt: &Prompt) -> CodexResult<()> {
    let mut stream = sess.client.clone().stream(prompt).await?;
    loop {
        let maybe_event = stream.next().await;
        let Some(event) = maybe_event else {
            return Err(CodexErr::Stream(
                "stream closed before response.completed".into(),
                None,
            ));
        };
        match event {
            Ok(ResponseEvent::OutputItemDone { item, sequence_number: _, output_index: _ }) => {
                // Record only to in-memory conversation history; avoid state snapshot.
                let mut state = sess.state.lock().unwrap();
                state.history.record_items(std::slice::from_ref(&item));
            }
            Ok(ResponseEvent::Completed {
                response_id: _,
                token_usage,
            }) => {
                // some providers don't return token usage, so we default
                // TODO: consider approximate token usage
                let token_usage = token_usage.unwrap_or_default();
    sess.tx_event
        .send(sess.make_event(&sub_id, EventMsg::TokenCount(token_usage)))
        .await
        .ok();

                return Ok(());
            }
            Ok(_) => continue,
            Err(e) => return Err(e),
        }
    }
}

/// Capture a screenshot from the browser and store it for the next model request
async fn capture_browser_screenshot(_sess: &Session) -> Result<(PathBuf, String), String> {
    let browser_manager = codex_browser::global::get_browser_manager()
        .await
        .ok_or_else(|| "No browser manager available".to_string())?;

    if !browser_manager.is_enabled().await {
        return Err("Browser manager is not enabled".to_string());
    }

    // Get current URL first
    let url = browser_manager
        .get_current_url()
        .await
        .unwrap_or_else(|| "Browser".to_string());
    tracing::debug!("Attempting to capture screenshot at URL: {}", url);

    match browser_manager.capture_screenshot().await {
        Ok(screenshots) => {
            if let Some(first_screenshot) = screenshots.first() {
                tracing::info!(
                    "Captured browser screenshot: {} at URL: {}",
                    first_screenshot.display(),
                    url
                );
                Ok((first_screenshot.clone(), url))
            } else {
                let msg = format!("Screenshot capture returned empty results at URL: {}", url);
                tracing::warn!("{}", msg);
                Err(msg)
            }
        }
        Err(e) => {
            let msg = format!("Failed to capture screenshot at {}: {}", url, e);
            tracing::warn!("{}", msg);
            Err(msg)
        }
    }
}

/// Send agent status update event to the TUI
async fn send_agent_status_update(sess: &Session) {
    let manager = AGENT_MANAGER.read().await;

    // Collect all agents; include completed/failed so HUD can show final messages
    let agents: Vec<crate::protocol::AgentInfo> = manager
        .get_all_agents()
        .map(|agent| crate::protocol::AgentInfo {
            id: agent.id.clone(),
            name: agent.model.clone(), // Use model name as the display name
            status: match agent.status {
                AgentStatus::Pending => "pending".to_string(),
                AgentStatus::Running => "running".to_string(),
                AgentStatus::Completed => "completed".to_string(),
                AgentStatus::Failed => "failed".to_string(),
                AgentStatus::Cancelled => "cancelled".to_string(),
            },
            model: Some(agent.model.clone()),
            last_progress: agent.progress.last().cloned(),
            result: agent.result.clone(),
            error: agent.error.clone(),
        })
        .collect();

    let event = Event {
        id: "agent_status".to_string(),
        event_seq: 0,
        msg: EventMsg::AgentStatusUpdate(AgentStatusUpdateEvent {
            agents,
            context: None,
            task: None,
        }),
        order: None,
    };

    // Send event asynchronously
    let tx_event = sess.tx_event.clone();
    tokio::spawn(async move {
        if let Err(e) = tx_event.send(event).await {
            tracing::error!("Failed to send agent status update event: {}", e);
        }
    });
}

/// Add a screenshot to pending screenshots for the next model request
fn add_pending_screenshot(sess: &Session, screenshot_path: PathBuf, url: String) {
    // Do not queue screenshots for next turn anymore; we inject fresh per-turn.
    tracing::info!("Captured screenshot; updating UI and using per-turn injection");

    // Also send an immediate event to update the TUI display
    let event = Event {
        id: "browser_screenshot".to_string(),
        event_seq: 0,
        msg: EventMsg::BrowserScreenshotUpdate(BrowserScreenshotUpdateEvent {
            screenshot_path,
            url,
        }),
        order: None,
    };

    // Send event asynchronously to avoid blocking
    let tx_event = sess.tx_event.clone();
    tokio::spawn(async move {
        if let Err(e) = tx_event.send(event).await {
            tracing::error!("Failed to send browser screenshot update event: {}", e);
        }
    });
}

/// Consume pending screenshots and return them as ResponseInputItems
#[allow(dead_code)]
fn consume_pending_screenshots(sess: &Session) -> Vec<ResponseInputItem> {
    let mut pending = sess.pending_browser_screenshots.lock().unwrap();
    let screenshots = pending.drain(..).collect::<Vec<_>>();

    screenshots
        .into_iter()
        .map(|path| {
            let metadata = format!(
                "[EPHEMERAL:browser_screenshot] Browser screenshot at {}",
                chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
            );

            // Read the screenshot file and create an ephemeral image
            match std::fs::read(&path) {
                Ok(bytes) => {
                    let mime = mime_guess::from_path(&path)
                        .first()
                        .map(|m| m.to_string())
                        .unwrap_or_else(|| "image/png".to_string());
                    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);

                    ResponseInputItem::Message {
                        role: "user".to_string(),
                        content: vec![
                            ContentItem::InputText { text: metadata },
                            ContentItem::InputImage {
                                image_url: format!("data:{mime};base64,{encoded}"),
                                detail: Some("high".to_string()),
                            },
                        ],
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to read screenshot {}: {}", path.display(), e);
                    ResponseInputItem::Message {
                        role: "user".to_string(),
                        content: vec![ContentItem::InputText {
                            text: format!("Failed to load browser screenshot: {}", e),
                        }],
                    }
                }
            }
        })
        .collect()
}

/// Helper function to wrap custom tool calls with events
async fn execute_custom_tool<F, Fut>(
    sess: &Session,
    ctx: &ToolCallCtx,
    tool_name: String,
    parameters: Option<serde_json::Value>,
    tool_fn: F,
) -> ResponseInputItem
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = ResponseInputItem>,
{
    use crate::protocol::{CustomToolCallBeginEvent, CustomToolCallEndEvent};
    use std::time::Instant;

    // Send begin event with ordering
    let begin_msg = EventMsg::CustomToolCallBegin(CustomToolCallBeginEvent {
        call_id: ctx.call_id.clone(),
        tool_name: tool_name.clone(),
        parameters: parameters.clone(),
    });
    let begin_order = ctx.order_meta(sess.current_request_ordinal());
    let begin_event = sess.make_event_with_order(&ctx.sub_id, begin_msg, begin_order, ctx.seq_hint);
    sess.send_event(begin_event).await;

    // Execute the tool
    let start = Instant::now();
    let result = tool_fn().await;
    let duration = start.elapsed();

    // Extract success/failure from result. Prefer explicit success flag when available.
    let (success, message) = match &result {
        ResponseInputItem::FunctionCallOutput { output, .. } => {
            let content = &output.content;
            let success_flag = output.success;
            (success_flag.unwrap_or(true), content.clone())
        }
        _ => (true, String::from("Tool completed")),
    };

    // Send end event with ordering
    let end_msg = EventMsg::CustomToolCallEnd(CustomToolCallEndEvent {
        call_id: ctx.call_id.clone(),
        tool_name,
        parameters,
        duration,
        result: if success { Ok(message) } else { Err(message) },
    });
    let end_order = ctx.order_meta(sess.current_request_ordinal());
    let end_event = sess.make_event_with_order(&ctx.sub_id, end_msg, end_order, ctx.seq_hint);
    sess.send_event(end_event).await;

    result
}

async fn handle_browser_open(sess: &Session, ctx: &ToolCallCtx, arguments: String) -> ResponseInputItem {
    // Parse arguments as JSON for the event
    let params = serde_json::from_str(&arguments).ok();

    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();

    execute_custom_tool(
        sess,
        ctx,
        "browser_open".to_string(),
        params,
        || async move {
            // Parse the URL from arguments
            let args: Result<Value, _> = serde_json::from_str(&arguments_clone);

            match args {
                Ok(json) => {
                    let url = json
                        .get("url")
                        .and_then(|v| v.as_str())
                        .unwrap_or("about:blank");

                    // Use the global browser manager (create if needed)
                    let browser_manager = {
                        let existing_global = codex_browser::global::get_browser_manager().await;
                        if let Some(existing) = existing_global {
                            tracing::info!("Using existing global browser manager");
                            Some(existing)
                        } else {
                            tracing::info!("Creating new browser manager");
                            let new_manager =
                                codex_browser::global::get_or_create_browser_manager().await;
                            Some(new_manager)
                        }
                    };

                    if let Some(browser_manager) = browser_manager {
                        // Ensure the browser manager is marked enabled so status reflects reality
                        browser_manager.set_enabled_sync(true);
                        // Clear any lingering node highlight from previous commands
                        let _ = browser_manager
                            .execute_cdp("Overlay.hideHighlight", serde_json::json!({}))
                            .await;
                        // Navigate to the URL with detailed timing logs
                        let step_start = std::time::Instant::now();
                        tracing::info!("[browser_open] begin goto: {}", url);
                        match browser_manager.goto(url).await {
                            Ok(_) => {
                                tracing::info!(
                                    "[browser_open] goto success: {} in {:?}",
                                    url,
                                    step_start.elapsed()
                                );
                                ResponseInputItem::FunctionCallOutput {
                                    call_id: call_id_clone.clone(),
                                    output: FunctionCallOutputPayload {
                                        content: format!("Browser opened to: {}", url),
                                        success: Some(true),
                                    },
                                }
                            }
                            Err(e) => ResponseInputItem::FunctionCallOutput {
                                call_id: call_id_clone.clone(),
                                output: FunctionCallOutputPayload {
                                    content: format!(
                                        "Failed to navigate browser to {}: {}",
                                        url, e
                                    ),
                                    success: Some(false),
                                },
                            },
                        }
                    } else {
                        ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone.clone(),
                            output: FunctionCallOutputPayload {
                                content: "Failed to initialize browser manager.".to_string(),
                                success: Some(false),
                            },
                        }
                    }
                }
                Err(e) => ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        content: format!("Failed to parse browser_open arguments: {}", e),
                        success: Some(false),
                    },
                },
            }
        },
    )
    .await
}

/// Get the browser manager for the session (always uses global)
async fn get_browser_manager_for_session(
    _sess: &Session,
) -> Option<Arc<codex_browser::BrowserManager>> {
    // Always use the global browser manager
    codex_browser::global::get_browser_manager().await
}

async fn handle_browser_close(sess: &Session, ctx: &ToolCallCtx) -> ResponseInputItem {
    let sess_clone = sess;
    let call_id_clone = ctx.call_id.clone();

    execute_custom_tool(
        sess,
        ctx,
        "browser_close".to_string(),
        None,
        || async move {
            let browser_manager = get_browser_manager_for_session(sess_clone).await;
            if let Some(browser_manager) = browser_manager {
                // Clear any lingering highlight before closing
                let _ = browser_manager
                    .execute_cdp("Overlay.hideHighlight", serde_json::json!({}))
                    .await;
                match browser_manager.stop().await {
                    Ok(_) => {
                        // Clear the browser manager from global
                        codex_browser::global::clear_browser_manager().await;
                        ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone.clone(),
                            output: FunctionCallOutputPayload {
                                content: "Browser closed. Screenshot capture disabled.".to_string(),
                                success: Some(true),
                            },
                        }
                    }
                    Err(e) => ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone.clone(),
                        output: FunctionCallOutputPayload {
                            content: format!("Failed to close browser: {}", e),
                            success: Some(false),
                        },
                    },
                }
            } else {
                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        content: "Browser is not currently open.".to_string(),
                        success: Some(false),
                    },
                }
            }
        },
    )
    .await
}

async fn handle_browser_status(sess: &Session, ctx: &ToolCallCtx) -> ResponseInputItem {
    let sess_clone = sess;
    let call_id_clone = ctx.call_id.clone();

    execute_custom_tool(
        sess,
        ctx,
        "browser_status".to_string(),
        None,
        || async move {
            let browser_manager = get_browser_manager_for_session(sess_clone).await;
            if let Some(browser_manager) = browser_manager {
                let _ = browser_manager
                    .execute_cdp("Overlay.hideHighlight", serde_json::json!({}))
                    .await;
                let status = browser_manager.get_status().await;
                let status_msg = if status.enabled {
                    if let Some(url) = status.current_url {
                        format!("Browser status: Enabled, currently at {}", url)
                    } else {
                        "Browser status: Enabled, no page loaded".to_string()
                    }
                } else {
                    "Browser status: Disabled".to_string()
                };

                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone.clone(),
                    output: FunctionCallOutputPayload {
                        content: status_msg,
                        success: Some(true),
                    },
                }
            } else {
                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        content:
                            "Browser is not initialized. Use browser_open to start the browser."
                                .to_string(),
                        success: Some(false),
                    },
                }
            }
        },
    )
    .await
}

async fn handle_browser_click(sess: &Session, ctx: &ToolCallCtx, arguments: String) -> ResponseInputItem {
    let params = serde_json::from_str::<serde_json::Value>(&arguments).ok();
    let sess_clone = sess;
    let call_id_clone = ctx.call_id.clone();

    execute_custom_tool(
        sess,
        ctx,
        "browser_click".to_string(),
        params.clone(),
        || async move {
            let browser_manager = get_browser_manager_for_session(sess_clone).await;

            if let Some(browser_manager) = browser_manager {
                let _ = browser_manager
                    .execute_cdp("Overlay.hideHighlight", serde_json::json!({}))
                    .await;
                // Determine click type: default 'click', or 'mousedown'/'mouseup'
                let click_type = params
                    .as_ref()
                    .and_then(|v| v.get("type"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("click")
                    .to_lowercase();

                // Optional absolute coordinates
                let (mut target_x, mut target_y) = (None, None);
                if let Some(p) = params.as_ref() {
                    if let Some(vx) = p.get("x").and_then(|v| v.as_f64()) {
                        target_x = Some(vx);
                    }
                    if let Some(vy) = p.get("y").and_then(|v| v.as_f64()) {
                        target_y = Some(vy);
                    }
                }

                // If x or y provided, resolve missing coord from current position, then move
                if target_x.is_some() || target_y.is_some() {
                    // get current cursor for missing values
                    match browser_manager.get_cursor_position().await {
                        Ok((cx, cy)) => {
                            let x = target_x.unwrap_or(cx);
                            let y = target_y.unwrap_or(cy);
                            if let Err(e) = browser_manager.move_mouse(x, y).await {
                                return ResponseInputItem::FunctionCallOutput {
                                    call_id: call_id_clone.clone(),
                                    output: FunctionCallOutputPayload {
                                        content: format!("Failed to move before click: {}", e),
                                        success: Some(false),
                                    },
                                };
                            }
                        }
                        Err(e) => {
                            return ResponseInputItem::FunctionCallOutput {
                                call_id: call_id_clone.clone(),
                                output: FunctionCallOutputPayload {
                                    content: format!("Failed to get current cursor position: {}", e),
                                    success: Some(false),
                                },
                            };
                        }
                    }
                }

                // Perform the action at current (possibly moved) position
                let action_result = match click_type.as_str() {
                    "mousedown" => match browser_manager.mouse_down_at_current().await {
                        Ok((x, y)) => Ok((x, y, "Mouse down".to_string())),
                        Err(e) => Err(e),
                    },
                    "mouseup" => match browser_manager.mouse_up_at_current().await {
                        Ok((x, y)) => Ok((x, y, "Mouse up".to_string())),
                        Err(e) => Err(e),
                    },
                    "click" | _ => match browser_manager.click_at_current().await {
                        Ok((x, y)) => Ok((x, y, "Clicked".to_string())),
                        Err(e) => Err(e),
                    },
                };

                match action_result {
                    Ok((x, y, label)) => {
                        ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone.clone(),
                            output: FunctionCallOutputPayload {
                                content: format!("{} at ({}, {})", label, x, y),
                                success: Some(true),
                            },
                        }
                    }
                    Err(e) => ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone.clone(),
                        output: FunctionCallOutputPayload {
                            content: format!("Failed to perform mouse action: {}", e),
                            success: Some(false),
                        },
                    },
                }
    } else {
        ResponseInputItem::FunctionCallOutput {
            call_id: call_id_clone,
            output: FunctionCallOutputPayload {
                content: "Browser is not initialized. Use browser_open to start the browser."
                    .to_string(),
                success: Some(false),
            },
        }
    }
        },
    )
    .await
}

async fn handle_browser_move(sess: &Session, ctx: &ToolCallCtx, arguments: String) -> ResponseInputItem {
    let params = serde_json::from_str(&arguments).ok();
    let sess_clone = sess;
    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();

    execute_custom_tool(
        sess,
        ctx,
        "browser_move".to_string(),
        params,
        || async move {
            let browser_manager = get_browser_manager_for_session(sess_clone).await;

            if let Some(browser_manager) = browser_manager {
                let _ = browser_manager
                    .execute_cdp("Overlay.hideHighlight", serde_json::json!({}))
                    .await;
                let args: Result<Value, _> = serde_json::from_str(&arguments_clone);
                match args {
                    Ok(json) => {
                        // Check if we have relative movement (dx, dy) or absolute (x, y)
                        let has_dx = json.get("dx").is_some();
                        let has_dy = json.get("dy").is_some();
                        let has_x = json.get("x").is_some();
                        let has_y = json.get("y").is_some();

                        let result = if has_dx || has_dy {
                            // Relative movement
                            let dx = json.get("dx").and_then(|v| v.as_f64()).unwrap_or(0.0);
                            let dy = json.get("dy").and_then(|v| v.as_f64()).unwrap_or(0.0);
                            browser_manager.move_mouse_relative(dx, dy).await
                        } else if has_x || has_y {
                            // Absolute movement
                            let x = json.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
                            let y = json.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
                            browser_manager.move_mouse(x, y).await.map(|_| (x, y))
                        } else {
                            // No parameters provided, just return current position
                            browser_manager.get_cursor_position().await
                        };

                        match result {
                            Ok((x, y)) => {
                                ResponseInputItem::FunctionCallOutput {
                                    call_id: call_id_clone.clone(),
                                    output: FunctionCallOutputPayload {
                                        content: format!("Moved mouse position to ({}, {})", x, y),
                                        success: Some(true),
                                    },
                                }
                            }
                            Err(e) => ResponseInputItem::FunctionCallOutput {
                                call_id: call_id_clone.clone(),
                                output: FunctionCallOutputPayload {
                                    content: format!("Failed to move mouse: {}", e),
                                    success: Some(false),
                                },
                            },
                        }
                    }
                    Err(e) => ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone.clone(),
                        output: FunctionCallOutputPayload {
                            content: format!("Failed to parse browser_move arguments: {}", e),
                            success: Some(false),
                        },
                    },
                }
            } else {
                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        content: "Browser is not initialized. Use browser_open to start the browser."
                            .to_string(),
                        success: Some(false),
                    },
                }
            }
        },
    )
    .await
}

async fn handle_browser_type(sess: &Session, ctx: &ToolCallCtx, arguments: String) -> ResponseInputItem {
    let params = serde_json::from_str(&arguments).ok();
    let sess_clone = sess;
    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();

    execute_custom_tool(
        sess,
        ctx,
        "browser_type".to_string(),
        params,
        || async move {
            let browser_manager = get_browser_manager_for_session(sess_clone).await;
            if let Some(browser_manager) = browser_manager {
                let _ = browser_manager
                    .execute_cdp("Overlay.hideHighlight", serde_json::json!({}))
                    .await;
                let args: Result<Value, _> = serde_json::from_str(&arguments_clone);
                match args {
                    Ok(json) => {
                        let text = json.get("text").and_then(|v| v.as_str()).unwrap_or("");

                        match browser_manager.type_text(text).await {
                            Ok(_) => {
                                ResponseInputItem::FunctionCallOutput {
                                    call_id: call_id_clone.clone(),
                                    output: FunctionCallOutputPayload {
                                        content: format!("Typed: {}", text),
                                        success: Some(true),
                                    },
                                }
                            }
                            Err(e) => ResponseInputItem::FunctionCallOutput {
                                call_id: call_id_clone.clone(),
                                output: FunctionCallOutputPayload {
                                    content: format!("Failed to type text: {}", e),
                                    success: Some(false),
                                },
                            },
                        }
                    }
                    Err(e) => ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone.clone(),
                        output: FunctionCallOutputPayload {
                            content: format!("Failed to parse browser_type arguments: {}", e),
                            success: Some(false),
                        },
                    },
                }
            } else {
                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        content:
                            "Browser is not initialized. Use browser_open to start the browser."
                                .to_string(),
                        success: Some(false),
                    },
                }
            }
        },
    )
    .await
}

async fn handle_browser_key(sess: &Session, ctx: &ToolCallCtx, arguments: String) -> ResponseInputItem {
    let params = serde_json::from_str(&arguments).ok();
    let sess_clone = sess;
    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();

    execute_custom_tool(
        sess,
        ctx,
        "browser_key".to_string(),
        params,
        || async move {
            let browser_manager = get_browser_manager_for_session(sess_clone).await;
            if let Some(browser_manager) = browser_manager {
                let _ = browser_manager
                    .execute_cdp("Overlay.hideHighlight", serde_json::json!({}))
                    .await;
                let args: Result<Value, _> = serde_json::from_str(&arguments_clone);
                match args {
                    Ok(json) => {
                        let key = json.get("key").and_then(|v| v.as_str()).unwrap_or("");

                        match browser_manager.press_key(key).await {
                            Ok(_) => {
                                ResponseInputItem::FunctionCallOutput {
                                    call_id: call_id_clone.clone(),
                                    output: FunctionCallOutputPayload {
                                        content: format!("Pressed key: {}", key),
                                        success: Some(true),
                                    },
                                }
                            }
                            Err(e) => ResponseInputItem::FunctionCallOutput {
                                call_id: call_id_clone.clone(),
                                output: FunctionCallOutputPayload {
                                    content: format!("Failed to press key: {}", e),
                                    success: Some(false),
                                },
                            },
                        }
                    }
                    Err(e) => ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload {
                            content: format!("Failed to parse browser_key arguments: {}", e),
                            success: Some(false),
                        },
                    },
                }
            } else {
                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        content:
                            "Browser is not initialized. Use browser_open to start the browser."
                                .to_string(),
                        success: Some(false),
                    },
                }
            }
        },
    )
    .await
}

async fn handle_browser_javascript(sess: &Session, ctx: &ToolCallCtx, arguments: String) -> ResponseInputItem {
    let params = serde_json::from_str(&arguments).ok();
    let sess_clone = sess;
    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();

    execute_custom_tool(
        sess,
        ctx,
        "browser_javascript".to_string(),
        params,
        || async move {
            let browser_manager = get_browser_manager_for_session(sess_clone).await;
            if let Some(browser_manager) = browser_manager {
                let _ = browser_manager
                    .execute_cdp("Overlay.hideHighlight", serde_json::json!({}))
                    .await;
                let args: Result<Value, _> = serde_json::from_str(&arguments_clone);
                match args {
                    Ok(json) => {
                        let code = json.get("code").and_then(|v| v.as_str()).unwrap_or("");

                        match browser_manager.execute_javascript(code).await {
                            Ok(result) => {
                                // Log the JavaScript execution result
                                tracing::info!("JavaScript execution returned: {:?}", result);

                                // Format the result for the LLM
                                let formatted_result = if let Some(obj) = result.as_object() {
                                    // Check if it's our wrapped result format
                                    if let (Some(success), Some(value)) =
                                        (obj.get("success"), obj.get("value"))
                                    {
                                        let logs = obj.get("logs").and_then(|v| v.as_array());
                                        let mut output = String::new();

                                        if let Some(logs) = logs {
                                            if !logs.is_empty() {
                                                output.push_str("Console logs:\n");
                                                for log in logs {
                                                    if let Some(log_str) = log.as_str() {
                                                        output
                                                            .push_str(&format!("  {}\n", log_str));
                                                    }
                                                }
                                                output.push_str("\n");
                                            }
                                        }

                                        if success.as_bool().unwrap_or(false) {
                                            output.push_str("Result: ");
                                            output.push_str(
                                                &serde_json::to_string_pretty(value)
                                                    .unwrap_or_else(|_| "null".to_string()),
                                            );
                                        } else if let Some(error) = obj.get("error") {
                                            output.push_str("Error: ");
                                            output.push_str(&error.to_string());
                                        }

                                        output
                                    } else {
                                        // Fallback to raw JSON if not in expected format
                                        serde_json::to_string_pretty(&result)
                                            .unwrap_or_else(|_| "null".to_string())
                                    }
                                } else {
                                    // Not an object, return as-is
                                    serde_json::to_string_pretty(&result)
                                        .unwrap_or_else(|_| "null".to_string())
                                };

                                tracing::info!("Returning to LLM: {}", formatted_result);

                                ResponseInputItem::FunctionCallOutput {
                                    call_id: call_id_clone.clone(),
                                    output: FunctionCallOutputPayload {
                                        content: formatted_result,
                                        success: Some(true),
                                    },
                                }
                            }
                            Err(e) => ResponseInputItem::FunctionCallOutput {
                                call_id: call_id_clone.clone(),
                                output: FunctionCallOutputPayload {
                                    content: format!("Failed to execute JavaScript: {}", e),
                                    success: Some(false),
                                },
                            },
                        }
                    }
                    Err(e) => ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload {
                            content: format!("Failed to parse browser_javascript arguments: {}", e),
                            success: Some(false),
                        },
                    },
                }
            } else {
                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        content:
                            "Browser is not initialized. Use browser_open to start the browser."
                                .to_string(),
                        success: Some(false),
                    },
                }
            }
        },
    )
    .await
}

async fn handle_browser_scroll(sess: &Session, ctx: &ToolCallCtx, arguments: String) -> ResponseInputItem {
    let params = serde_json::from_str(&arguments).ok();
    let sess_clone = sess;
    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();

    execute_custom_tool(
        sess,
        ctx,
        "browser_scroll".to_string(),
        params,
        || async move {
            let browser_manager = get_browser_manager_for_session(sess_clone).await;
            if let Some(browser_manager) = browser_manager {
                let _ = browser_manager
                    .execute_cdp("Overlay.hideHighlight", serde_json::json!({}))
                    .await;
                let args: Result<Value, _> = serde_json::from_str(&arguments_clone);
                match args {
                    Ok(json) => {
                        let dx = json.get("dx").and_then(|v| v.as_f64()).unwrap_or(0.0);
                        let dy = json.get("dy").and_then(|v| v.as_f64()).unwrap_or(0.0);

                        match browser_manager.scroll_by(dx, dy).await {
                    Ok(_) => {
                        ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone.clone(),
                            output: FunctionCallOutputPayload {
                                content: format!("Scrolled by ({}, {})", dx, dy),
                                success: Some(true),
                            },
                        }
                    }
                    Err(e) => ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone.clone(),
                        output: FunctionCallOutputPayload {
                            content: format!("Failed to scroll: {}", e),
                            success: Some(false),
                        },
                    },
                }
            }
            Err(e) => ResponseInputItem::FunctionCallOutput {
                call_id: call_id_clone,
                output: FunctionCallOutputPayload {
                    content: format!("Failed to parse browser_scroll arguments: {}", e),
                    success: Some(false),
                },
            },
        }
    } else {
        ResponseInputItem::FunctionCallOutput {
            call_id: call_id_clone,
            output: FunctionCallOutputPayload {
                content: "Browser is not initialized. Use browser_open to start the browser.".to_string(),
                success: Some(false),
            },
        }
    }
        },
    )
    .await
}

async fn handle_browser_console(sess: &Session, ctx: &ToolCallCtx, arguments: String) -> ResponseInputItem {
    let params = serde_json::from_str(&arguments).ok();
    let sess_clone = sess;
    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();

    execute_custom_tool(
        sess,
        ctx,
        "browser_console".to_string(),
        params,
        || async move {
            let browser_manager = get_browser_manager_for_session(sess_clone).await;
            if let Some(browser_manager) = browser_manager {
                let args: Result<Value, _> = serde_json::from_str(&arguments_clone);
                let lines = match args {
                    Ok(json) => json.get("lines").and_then(|v| v.as_u64()).map(|n| n as usize),
                    Err(_) => None,
                };

                match browser_manager.get_console_logs(lines).await {
                    Ok(logs) => {
                        // Format the logs for display
                        let formatted = if let Some(logs_array) = logs.as_array() {
                            if logs_array.is_empty() {
                                "No console logs captured.".to_string()
                            } else {
                                let mut output = String::new();
                                output.push_str("Console logs:\n");
                                for log in logs_array {
                                    if let Some(log_obj) = log.as_object() {
                                        let timestamp = log_obj.get("timestamp")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("");
                                        let level = log_obj.get("level")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("log");
                                        let message = log_obj.get("message")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("");
                                        
                                        output.push_str(&format!("[{}] [{}] {}\n", timestamp, level.to_uppercase(), message));
                                    }
                                }
                                output
                            }
                        } else {
                            "No console logs captured.".to_string()
                        };

                        ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone,
                            output: FunctionCallOutputPayload {
                                content: formatted,
                                success: Some(true),
                            },
                        }
                    }
                    Err(e) => ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload {
                            content: format!("Failed to get console logs: {}", e),
                            success: Some(false),
                        },
                    },
                }
            } else {
                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        content: "Browser is not enabled. Use browser_open to enable it first.".to_string(),
                        success: Some(false),
                    },
                }
            }
        },
    )
    .await
}

async fn handle_browser_cdp(sess: &Session, ctx: &ToolCallCtx, arguments: String) -> ResponseInputItem {
    let params = serde_json::from_str(&arguments).ok();
    let sess_clone = sess;
    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();

    execute_custom_tool(
        sess,
        ctx,
        "browser_cdp".to_string(),
        params,
        || async move {
            let browser_manager = get_browser_manager_for_session(sess_clone).await;
            if let Some(browser_manager) = browser_manager {
                let _ = browser_manager
                    .execute_cdp("Overlay.hideHighlight", serde_json::json!({}))
                    .await;
                let args: Result<Value, _> = serde_json::from_str(&arguments_clone);
                match args {
                    Ok(json) => {
                        let method = json
                            .get("method")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let params = json.get("params").cloned().unwrap_or_else(|| Value::Object(serde_json::Map::new()));
                        let target = json
                            .get("target")
                            .and_then(|v| v.as_str())
                            .unwrap_or("page");

                        if method.is_empty() {
                            return ResponseInputItem::FunctionCallOutput {
                                call_id: call_id_clone,
                                output: FunctionCallOutputPayload {
                                    content: "Missing required field: method".to_string(),
                                    success: Some(false),
                                },
                            };
                        }

                        let exec_res = if target == "browser" {
                            browser_manager.execute_cdp_browser(&method, params).await
                        } else {
                            browser_manager.execute_cdp(&method, params).await
                        };

                        match exec_res {
                            Ok(result) => {
                                let pretty = serde_json::to_string_pretty(&result)
                                    .unwrap_or_else(|_| "<non-serializable result>".to_string());
                                ResponseInputItem::FunctionCallOutput {
                                    call_id: call_id_clone,
                                    output: FunctionCallOutputPayload {
                                        content: pretty,
                                        success: Some(true),
                                    },
                                }
                            }
                            Err(e) => ResponseInputItem::FunctionCallOutput {
                                call_id: call_id_clone,
                                output: FunctionCallOutputPayload {
                                    content: format!("Failed to execute CDP command: {}", e),
                                    success: Some(false),
                                },
                            },
                        }
                    }
                    Err(e) => ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload {
                            content: format!("Failed to parse browser_cdp arguments: {}", e),
                            success: Some(false),
                        },
                    },
                }
            } else {
                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        content: "Browser is not initialized. Use browser_open to start the browser.".to_string(),
                        success: Some(false),
                    },
                }
            }
        },
    )
    .await
}

async fn handle_browser_inspect(sess: &Session, ctx: &ToolCallCtx, arguments: String) -> ResponseInputItem {
    use serde_json::json;
    let params = serde_json::from_str(&arguments).ok();
    let sess_clone = sess;
    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();

    execute_custom_tool(
        sess,
        ctx,
        "browser_inspect".to_string(),
        params,
        || async move {
            let browser_manager = get_browser_manager_for_session(sess_clone).await;
            if let Some(browser_manager) = browser_manager {
                let args: Result<Value, _> = serde_json::from_str(&arguments_clone);
                match args {
                    Ok(json) => {
                        // Determine target element: by id, by coords, or by cursor
                        let id_attr = json.get("id").and_then(|v| v.as_str()).map(|s| s.to_string());
                        let mut x = json.get("x").and_then(|v| v.as_f64());
                        let mut y = json.get("y").and_then(|v| v.as_f64());

                        if (x.is_none() || y.is_none()) && id_attr.is_none() {
                            // No coords provided; use current cursor
                            if let Ok((cx, cy)) = browser_manager.get_cursor_position().await {
                                x = Some(cx);
                                y = Some(cy);
                            }
                        }

                        // Resolve nodeId
                        let node_id_value = if let Some(id_attr) = id_attr.clone() {
                            // Use DOM.getDocument -> DOM.querySelector with selector `#id`
                            let doc = browser_manager
                                .execute_cdp("DOM.getDocument", json!({}))
                                .await
                                .map_err(|e| e);
                            let root_id = match doc {
                                Ok(v) => v.get("root").and_then(|r| r.get("nodeId")).and_then(|n| n.as_u64()),
                                Err(_) => None,
                            };
                            if let Some(root_node_id) = root_id {
                                let sel = format!("#{}", id_attr);
                                let q = browser_manager
                                    .execute_cdp(
                                        "DOM.querySelector",
                                        json!({"nodeId": root_node_id, "selector": sel}),
                                    )
                                    .await;
                                match q {
                                    Ok(v) => v.get("nodeId").cloned(),
                                    Err(_) => None,
                                }
                            } else {
                                None
                            }
                        } else if let (Some(x), Some(y)) = (x, y) {
                            // Use DOM.getNodeForLocation
                            let res = browser_manager
                                .execute_cdp(
                                    "DOM.getNodeForLocation",
                                    json!({
                                        "x": x,
                                        "y": y,
                                        "includeUserAgentShadowDOM": true
                                    }),
                                )
                                .await;
                            match res {
                                Ok(v) => {
                                    // Prefer nodeId; if absent, push backendNodeId
                                    if let Some(n) = v.get("nodeId").cloned() {
                                        Some(n)
                                    } else if let Some(backend) = v.get("backendNodeId").and_then(|b| b.as_u64()) {
                                        let pushed = browser_manager
                                            .execute_cdp(
                                                "DOM.pushNodesByBackendIdsToFrontend",
                                                json!({ "backendNodeIds": [backend] }),
                                            )
                                            .await
                                            .ok();
                                        pushed
                                            .and_then(|pv| pv.get("nodeIds").and_then(|arr| arr.as_array().cloned()))
                                            .and_then(|arr| arr.first().cloned())
                                    } else {
                                        None
                                    }
                                }
                                Err(_) => None,
                            }
                        } else {
                            None
                        };

                        let node_id = match node_id_value.and_then(|v| v.as_u64()) {
                            Some(id) => id,
                            None => {
                                return ResponseInputItem::FunctionCallOutput {
                                    call_id: call_id_clone,
                                    output: FunctionCallOutputPayload {
                                        content: "Failed to resolve target node for inspection".to_string(),
                                        success: Some(false),
                                    },
                                };
                            }
                        };

                        // Enable CSS domain to get matched rules
                        let _ = browser_manager.execute_cdp("CSS.enable", json!({})).await;

                        // Gather details
                        let attrs = browser_manager
                            .execute_cdp("DOM.getAttributes", json!({"nodeId": node_id}))
                            .await
                            .unwrap_or_else(|_| json!({}));
                        let outer = browser_manager
                            .execute_cdp("DOM.getOuterHTML", json!({"nodeId": node_id}))
                            .await
                            .unwrap_or_else(|_| json!({}));
                        let box_model = browser_manager
                            .execute_cdp("DOM.getBoxModel", json!({"nodeId": node_id}))
                            .await
                            .unwrap_or_else(|_| json!({}));
                        let styles = browser_manager
                            .execute_cdp("CSS.getMatchedStylesForNode", json!({"nodeId": node_id}))
                            .await
                            .unwrap_or_else(|_| json!({}));

                        // Highlight the inspected node using Overlay domain (no screenshot capture here)
                        let _ = browser_manager.execute_cdp("Overlay.enable", json!({})).await;
                        let highlight_config = json!({
                            "showInfo": true,
                            "showStyles": false,
                            "showRulers": false,
                            "contentColor": {"r": 111, "g": 168, "b": 220, "a": 0.20},
                            "paddingColor": {"r": 147, "g": 196, "b": 125, "a": 0.55},
                            "borderColor": {"r": 255, "g": 229, "b": 153, "a": 0.60},
                            "marginColor": {"r": 246, "g": 178, "b": 107, "a": 0.60}
                        });
                        let _ = browser_manager.execute_cdp(
                            "Overlay.highlightNode",
                            json!({ "nodeId": node_id, "highlightConfig": highlight_config })
                        ).await;
                        // Do not hide here; keep highlight until the next browser command.

                        // Format output
                        let mut out = String::new();
                        if let (Some(ix), Some(iy)) = (x, y) {
                            out.push_str(&format!("Target: coordinates ({}, {})\n", ix, iy));
                        }
                        if let Some(id_attr) = id_attr {
                            out.push_str(&format!("Target: id '#{}'\n", id_attr));
                        }
                        out.push_str(&format!("NodeId: {}\n", node_id));

                        // Attributes
                        if let Some(arr) = attrs.get("attributes").and_then(|v| v.as_array()) {
                            out.push_str("Attributes:\n");
                            let mut it = arr.iter();
                            while let (Some(k), Some(v)) = (it.next(), it.next()) {
                                out.push_str(&format!("  {}=\"{}\"\n", k.as_str().unwrap_or(""), v.as_str().unwrap_or("")));
                            }
                        }

                        // Outer HTML
                        if let Some(html) = outer.get("outerHTML").and_then(|v| v.as_str()) {
                            let one = html.replace('\n', " ");
                            let snippet: String = one.chars().take(800).collect();
                            out.push_str("\nOuterHTML (truncated):\n");
                            out.push_str(&snippet);
                            if one.len() > snippet.len() { out.push_str("…"); }
                            out.push('\n');
                        }

                        // Box Model summary
                        if box_model.get("model").is_some() {
                            out.push_str("\nBoxModel: available (content/padding/border/margin)\n");
                        }

                        // Matched styles summary
                        if let Some(rules) = styles.get("matchedCSSRules").and_then(|v| v.as_array()) {
                            out.push_str(&format!("Matched CSS rules: {}\n", rules.len()));
                        }

                        // No inline screenshot capture; result reflects DOM details only.

                        ResponseInputItem::FunctionCallOutput {
                            call_id: call_id_clone,
                            output: FunctionCallOutputPayload { content: out, success: Some(true) },
                        }
                    }
                    Err(e) => ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload {
                            content: format!("Failed to parse browser_inspect arguments: {}", e),
                            success: Some(false),
                        },
                    },
                }
            } else {
                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        content: "Browser is not initialized. Use browser_open to start the browser.".to_string(),
                        success: Some(false),
                    },
                }
            }
        },
    )
    .await
}

async fn handle_browser_history(sess: &Session, ctx: &ToolCallCtx, arguments: String) -> ResponseInputItem {
    let params = serde_json::from_str(&arguments).ok();
    let sess_clone = sess;
    let arguments_clone = arguments.clone();
    let call_id_clone = ctx.call_id.clone();

    execute_custom_tool(
        sess,
        ctx,
        "browser_history".to_string(),
        params,
        || async move {
            let browser_manager = get_browser_manager_for_session(sess_clone).await;
            if let Some(browser_manager) = browser_manager {
                let args: Result<Value, _> = serde_json::from_str(&arguments_clone);
                match args {
                    Ok(json) => {
                        let direction =
                            json.get("direction").and_then(|v| v.as_str()).unwrap_or("");

                        if direction != "back" && direction != "forward" {
                            return ResponseInputItem::FunctionCallOutput {
                                call_id: call_id_clone,
                                output: FunctionCallOutputPayload {
                                    content: format!(
                                        "Unsupported direction: {} (expected 'back' or 'forward')",
                                        direction
                                    ),
                                    success: Some(false),
                                },
                            };
                        }

                        let action_res = if direction == "back" {
                            browser_manager.history_back().await
                        } else {
                            browser_manager.history_forward().await
                        };

                        match action_res {
                            Ok(_) => {
                                ResponseInputItem::FunctionCallOutput {
                                    call_id: call_id_clone.clone(),
                                    output: FunctionCallOutputPayload {
                                        content: format!("History {} triggered", direction),
                                        success: Some(true),
                                    },
                                }
                            }
                            Err(e) => ResponseInputItem::FunctionCallOutput {
                                call_id: call_id_clone.clone(),
                                output: FunctionCallOutputPayload {
                                    content: format!("Failed to navigate history: {}", e),
                                    success: Some(false),
                                },
                            },
                        }
                    }
                    Err(e) => ResponseInputItem::FunctionCallOutput {
                        call_id: call_id_clone,
                        output: FunctionCallOutputPayload {
                            content: format!("Failed to parse browser_history arguments: {}", e),
                            success: Some(false),
                        },
                    },
                }
            } else {
                ResponseInputItem::FunctionCallOutput {
                    call_id: call_id_clone,
                    output: FunctionCallOutputPayload {
                        content:
                            "Browser is not initialized. Use browser_open to start the browser."
                                .to_string(),
                        success: Some(false),
                    },
                }
            }
        },
    )
    .await
}
