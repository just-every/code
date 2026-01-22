//! Defines the protocol for a Codex session between a client and an agent.
//!
//! Uses a SQ (Submission Queue) / EQ (Event Queue) pattern to asynchronously communicate
//! between user and agent.

use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use mcp_types::CallToolResult;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde::Serialize;
use serde_bytes::ByteBuf;
use serde_json::Value;
use strum_macros::Display;
use uuid::Uuid;

use crate::config_types::ReasoningEffort as ReasoningEffortConfig;
use crate::config_types::ReasoningSummary as ReasoningSummaryConfig;
use crate::config_types::TextVerbosity as TextVerbosityConfig;
use crate::message_history::HistoryEntry;
use crate::model_provider_info::ModelProviderInfo;
use crate::client_common::TextFormat;
use crate::parse_command::ParsedCommand;
use crate::plan_tool::UpdatePlanArgs;

// Re-export review types from the shared protocol crate so callers can use
// `code_core::protocol::ReviewFinding` and friends.
pub use code_protocol::protocol::ReviewCodeLocation;
pub use code_protocol::protocol::ReviewFinding;
pub use code_protocol::protocol::ReviewLineRange;
pub use code_protocol::protocol::ReviewOutputEvent;
pub use code_protocol::protocol::{ReviewContextMetadata, ReviewRequest};
pub use code_protocol::protocol::GitInfo;
pub use code_protocol::protocol::RolloutItem;
pub use code_protocol::protocol::RolloutLine;
pub use code_protocol::protocol::ConversationPathResponseEvent;
pub use code_protocol::protocol::ListCustomPromptsResponseEvent;
pub use code_protocol::protocol::ListSkillsResponseEvent;
pub use code_protocol::protocol::ViewImageToolCallEvent;
pub use code_protocol::skills::Skill;
pub use code_protocol::protocol::ENVIRONMENT_CONTEXT_OPEN_TAG;
pub use code_protocol::protocol::ExitedReviewModeEvent;
pub use code_protocol::protocol::ReviewSnapshotInfo;
pub use code_protocol::request_user_input::RequestUserInputEvent;
pub use code_protocol::request_user_input::RequestUserInputResponse;

/// Submission Queue Entry - requests from user
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Submission {
    /// Unique id for this Submission to correlate with Events
    pub id: String,
    /// Payload
    pub op: Op,
}

/// High-level toggles for validation checks.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ValidationGroup {
    Functional,
    Stylistic,
}

/// Submission operation
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
#[non_exhaustive]
pub enum Op {
    /// Configure the model session.
    ConfigureSession {
        /// Provider identifier ("openai", "openrouter", ...).
        provider: ModelProviderInfo,

        /// If not specified, server will use its default model.
        model: String,

        /// True when the model choice is explicitly set by the user.
        ///
        /// When false, the core may adopt a server-provided default model
        /// (e.g. "codex-auto-balanced") when available.
        #[serde(default)]
        model_explicit: bool,

        model_reasoning_effort: ReasoningEffortConfig,
        /// Optional user-preferred reasoning effort for the chat model.
        /// When present, the core will persist it separately from the effective clamped effort.
        #[serde(skip_serializing_if = "Option::is_none")]
        #[serde(default)]
        preferred_model_reasoning_effort: Option<ReasoningEffortConfig>,
        model_reasoning_summary: ReasoningSummaryConfig,
        model_text_verbosity: TextVerbosityConfig,

        /// Model instructions that are appended to the base instructions.
        user_instructions: Option<String>,

        /// Base instructions override.
        base_instructions: Option<String>,

        /// When to escalate for approval for execution
        approval_policy: AskForApproval,
        /// How to sandbox commands executed in the system
        sandbox_policy: SandboxPolicy,
        /// Disable server-side response storage (send full context each request)
        #[serde(default)]
        disable_response_storage: bool,

        /// Optional external notifier command tokens. Present only when the
        /// client wants the agent to spawn a program after each completed
        /// turn.
        #[serde(skip_serializing_if = "Option::is_none")]
        #[serde(default)]
        notify: Option<Vec<String>>,

        /// Working directory that should be treated as the *root* of the
        /// session. All relative paths supplied by the model as well as the
        /// execution sandbox are resolved against this directory **instead**
        /// of the process-wide current working directory. CLI front-ends are
        /// expected to expand this to an absolute path before sending the
        /// `ConfigureSession` operation so that the business-logic layer can
        /// operate deterministically.
        cwd: std::path::PathBuf,

        /// Path to a rollout file to resume from.
        #[serde(skip_serializing_if = "Option::is_none")]
        resume_path: Option<std::path::PathBuf>,

        /// Optional developer-role message to prepend to every turn for demos.
        /// This is a CLI-only knob; it is not persisted in config files.
        #[serde(skip_serializing_if = "Option::is_none")]
        #[serde(default)]
        demo_developer_message: Option<String>,
    },

    /// Abort current task.
    /// This server sends no corresponding Event
    Interrupt,

    /// Cancel running agents immediately without waiting for the model to issue a tool call.
    CancelAgents {
        /// Agent batch identifiers to cancel.
        #[serde(default)]
        batch_ids: Vec<String>,
        /// Specific agent identifiers to cancel when no batch is available.
        #[serde(default)]
        agent_ids: Vec<String>,
    },

    /// Input from the user
    UserInput {
        /// User input items, see `InputItem`
        items: Vec<InputItem>,
        /// Optional JSON Schema used to constrain the final assistant message for this turn.
        #[serde(skip_serializing_if = "Option::is_none")]
        final_output_json_schema: Option<Value>,
    },

    /// Queue user input to be appended to the next model request without
    /// interrupting the current turn.
    QueueUserInput {
        /// User input items, see `InputItem`
        items: Vec<InputItem>,
    },

    /// Set a one-off text format to apply on the next turn.
    SetNextTextFormat {
        format: TextFormat,
    },

    /// Approve a command execution
    ExecApproval {
        /// The id of the submission we are approving
        id: String,
        /// The user's decision in response to the request.
        decision: ReviewDecision,
    },

    /// Register a command pattern as approved for the remainder of the session.
    RegisterApprovedCommand {
        command: Vec<String>,
        match_kind: ApprovedCommandMatchKind,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[serde(default)]
        semantic_prefix: Option<Vec<String>>,
    },

    /// Approve a code patch
    PatchApproval {
        /// The id of the submission we are approving
        id: String,
        /// The user's decision in response to the request.
        decision: ReviewDecision,
    },

    /// Resolve a request_user_input tool call.
    #[serde(rename = "user_input_answer", alias = "request_user_input_response")]
    UserInputAnswer {
        /// Turn id for the in-flight request.
        id: String,
        /// User-provided answers.
        response: RequestUserInputResponse,
    },

    /// Update a specific validation tool toggle for the session.
    UpdateValidationTool {
        name: String,
        enable: bool,
    },

    /// Update a validation group toggle for the session.
    UpdateValidationGroup {
        group: ValidationGroup,
        enable: bool,
    },

    /// Append an entry to the persistent cross-session message history.
    ///
    /// Note the entry is not guaranteed to be logged if the user has
    /// history disabled, it matches the list of "sensitive" patterns, etc.
    AddToHistory {
        /// The message text to be stored.
        text: String,
    },

    /// Persist the full chat history snapshot for the current session.
    PersistHistorySnapshot {
        snapshot: serde_json::Value,
    },

    /// Execute a project-scoped custom command defined in configuration.
    RunProjectCommand {
        name: String,
    },

    /// Internally queue a developer-role message to be included in the next turn.
    AddPendingInputDeveloper {
        /// The developer message text to add to pending input.
        text: String,
    },

    /// Request a single history entry identified by `log_id` + `offset`.
    GetHistoryEntryRequest { offset: usize, log_id: u64 },

    /// Request the list of available custom prompts.
    /// Reply is delivered via `EventMsg::ListCustomPromptsResponse`.
    ListCustomPrompts,

    /// Request the list of available skills.
    /// Reply is delivered via `EventMsg::ListSkillsResponse`.
    ListSkills,

    /// Request the agent to summarize the current conversation context.
    /// The agent will use its existing context (either conversation history or previous response id)
    /// to generate a summary which will be returned as an AgentMessage event.
    Compact,
    /// Request the agent to perform a dedicated code review.
    Review { review_request: ReviewRequest },
    /// Request to shut down codex instance.
    Shutdown,
}

/// Determines the conditions under which the user is consulted to approve
/// running the command proposed by Codex.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum AskForApproval {
    /// Under this policy, only "known safe" commands—as determined by
    /// `is_safe_command()`—that **only read files** are auto‑approved.
    /// Everything else will ask the user to approve.
    #[serde(rename = "untrusted")]
    #[strum(serialize = "untrusted")]
    UnlessTrusted,

    /// *All* commands are auto‑approved, but they are expected to run inside a
    /// sandbox where network access is disabled and writes are confined to a
    /// specific set of paths. If the command fails, it will be escalated to
    /// the user to approve execution without a sandbox.
    OnFailure,

    /// The model decides when to ask the user for approval.
    #[default]
    OnRequest,

    /// Never ask the user to approve commands. Failures are immediately returned
    /// to the model, and never escalated to the user for approval.
    Never,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovedCommandMatchKind {
    Exact,
    Prefix,
}

/// Determines execution restrictions for model shell commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Display)]
#[strum(serialize_all = "kebab-case")]
#[serde(tag = "mode", rename_all = "kebab-case")]
pub enum SandboxPolicy {
    /// No restrictions whatsoever. Use with caution.
    #[serde(rename = "danger-full-access")]
    DangerFullAccess,

    /// Read-only access to the entire file-system.
    #[serde(rename = "read-only")]
    ReadOnly,

    /// Same as `ReadOnly` but additionally grants write access to the current
    /// working directory ("workspace").
    #[serde(rename = "workspace-write")]
    WorkspaceWrite {
        /// Additional folders (beyond cwd and possibly TMPDIR) that should be
        /// writable from within the sandbox.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        writable_roots: Vec<PathBuf>,

        /// When set to `true`, outbound network access is allowed. `false` by
        /// default.
        #[serde(default)]
        network_access: bool,

        /// When set to `true`, will NOT include the per-user `TMPDIR`
        /// environment variable among the default writable roots. Defaults to
        /// `false`.
        #[serde(default)]
        exclude_tmpdir_env_var: bool,

        /// When set to `true`, will NOT include the `/tmp` among the default
        /// writable roots on UNIX. Defaults to `false`.
        #[serde(default)]
        exclude_slash_tmp: bool,

        /// When true, do not protect the top-level `.git` folder under a
        /// writable root. Defaults to true (historical behavior allows Git writes).
        #[serde(default = "crate::protocol::default_true_bool")]
        allow_git_writes: bool,
    },
}

// Serde helper: default to true for flags where we want historical permissive behavior.
pub(crate) const fn default_true_bool() -> bool { true }

/// A writable root path accompanied by a list of subpaths that should remain
/// read‑only even when the root is writable. This is primarily used to ensure
/// top‑level VCS metadata directories (e.g. `.git`) under a writable root are
/// not modified by the agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WritableRoot {
    pub root: PathBuf,
    pub read_only_subpaths: Vec<PathBuf>,
}

impl WritableRoot {
    pub fn is_path_writable(&self, path: &Path) -> bool {
        if !path.starts_with(&self.root) {
            return false;
        }
        for sub in &self.read_only_subpaths {
            if path.starts_with(sub) {
                return false;
            }
        }
        true
    }
}

impl FromStr for SandboxPolicy {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(s)
    }
}

impl SandboxPolicy {
    /// Returns a policy with read-only disk access and no network.
    pub fn new_read_only_policy() -> Self {
        SandboxPolicy::ReadOnly
    }

    /// Returns a policy that can read the entire disk, but can only write to
    /// the current working directory and the per-user tmp dir on macOS. It does
    /// not allow network access.
    pub fn new_workspace_write_policy() -> Self {
        SandboxPolicy::WorkspaceWrite {
            writable_roots: vec![],
            network_access: false,
            exclude_tmpdir_env_var: false,
            exclude_slash_tmp: false,
            allow_git_writes: true,
        }
    }

    /// Always returns `true`; restricting read access is not supported.
    pub fn has_full_disk_read_access(&self) -> bool {
        true
    }

    pub fn has_full_disk_write_access(&self) -> bool {
        match self {
            SandboxPolicy::DangerFullAccess => true,
            SandboxPolicy::ReadOnly => false,
            SandboxPolicy::WorkspaceWrite { .. } => false,
        }
    }

    pub fn has_full_network_access(&self) -> bool {
        match self {
            SandboxPolicy::DangerFullAccess => true,
            SandboxPolicy::ReadOnly => false,
            SandboxPolicy::WorkspaceWrite { network_access, .. } => *network_access,
        }
    }

    /// Returns the list of writable roots (tailored to the current working
    /// directory) together with subpaths that should remain read‑only under
    /// each writable root.
    pub fn get_writable_roots_with_cwd(&self, cwd: &Path) -> Vec<WritableRoot> {
        match self {
            SandboxPolicy::DangerFullAccess => Vec::new(),
            SandboxPolicy::ReadOnly => Vec::new(),
            SandboxPolicy::WorkspaceWrite {
                writable_roots,
                exclude_tmpdir_env_var,
                exclude_slash_tmp,
                allow_git_writes,
                network_access: _,
            } => {
                // Start from explicitly configured writable roots.
                let mut roots: Vec<PathBuf> = writable_roots.clone();

                // Always include defaults: cwd, /tmp (if present on Unix), and
                // on macOS, the per-user TMPDIR unless explicitly excluded.
                roots.push(cwd.to_path_buf());

                // Include /tmp on Unix unless explicitly excluded.
                if cfg!(unix) && !exclude_slash_tmp {
                    let slash_tmp = PathBuf::from("/tmp");
                    if slash_tmp.is_dir() {
                        roots.push(slash_tmp);
                    }
                }

                // Include $TMPDIR unless explicitly excluded. On macOS, TMPDIR
                // is per-user, so writes to TMPDIR should not be readable by
                // other users on the system.
                //
                // By comparison, TMPDIR is not guaranteed to be defined on
                // Linux or Windows, but supporting it here gives users a way to
                // provide the model with their own temporary directory without
                // having to hardcode it in the config.
                if !exclude_tmpdir_env_var {
                    if let Some(tmpdir) = std::env::var_os("TMPDIR") {
                        if !tmpdir.is_empty() {
                            roots.push(PathBuf::from(tmpdir));
                        }
                    }
                }

                // For each root, compute subpaths that should remain read-only.
                roots
                    .into_iter()
                    .map(|writable_root| {
                        let mut subpaths = Vec::new();
                        if !allow_git_writes {
                            let top_level_git = writable_root.join(".git");
                            if top_level_git.is_dir() {
                                subpaths.push(top_level_git);
                            }
                        }
                        WritableRoot {
                            root: writable_root,
                            read_only_subpaths: subpaths,
                        }
                    })
                    .collect()
            }
        }
    }
}

/// User input
#[non_exhaustive]
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputItem {
    Text {
        text: String,
    },
    /// Pre‑encoded data: URI image.
    Image {
        image_url: String,
    },

    /// Local image path provided by the user.  This will be converted to an
    /// `Image` variant (base64 data URL) during request serialization.
    LocalImage {
        path: std::path::PathBuf,
    },

    /// Ephemeral image (like browser screenshots) that should not be persisted in history.
    /// This will be converted to an `Image` variant but marked as ephemeral.
    EphemeralImage {
        path: std::path::PathBuf,
        /// Optional metadata to help identify the image (e.g., "screenshot:1234567890:https://example.com")
        metadata: Option<String>,
    },
}

/// Event Queue Entry - events from agent
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Event {
    /// Submission `id` that this event is correlated with.
    pub id: String,
    /// Monotonic, per‑turn sequence for ordering within a submission id.
    /// Resets to 0 at TaskStarted and increments for each subsequent event.
    pub event_seq: u64,
    /// Payload
    pub msg: EventMsg,
    /// Optional model-provided ordering metadata (when applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<OrderMeta>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RecordedEvent {
    pub id: String,
    pub event_seq: u64,
    pub order: Option<OrderMeta>,
    pub msg: EventMsg,
}

pub fn event_msg_to_protocol(msg: &EventMsg) -> Option<code_protocol::protocol::EventMsg> {
    match msg {
        EventMsg::ReplayHistory(_) => None,
        EventMsg::TokenCount(payload) => {
            let info = convert_value(&payload.info)?;
            let rate_limits = payload
                .rate_limits
                .as_ref()
                .map(rate_limit_snapshot_to_protocol);
            Some(code_protocol::protocol::EventMsg::TokenCount(
                code_protocol::protocol::TokenCountEvent { info, rate_limits },
            ))
        }
        _ => convert_value(msg),
    }
}


pub fn event_msg_from_protocol(msg: &code_protocol::protocol::EventMsg) -> Option<EventMsg> {
    match msg {
        code_protocol::protocol::EventMsg::TokenCount(payload) => {
            let info = convert_value(&payload.info).unwrap_or(None);
            let rate_limits = payload
                .rate_limits
                .as_ref()
                .map(rate_limit_snapshot_from_protocol);
            Some(EventMsg::TokenCount(TokenCountEvent { info, rate_limits }))
        }
        _ => {
            let converted = convert_value(msg)?;
            if matches!(converted, EventMsg::ReplayHistory(_)) {
                return None;
            }
            Some(converted)
        }
    }
}


pub fn order_meta_to_protocol(
    order: &OrderMeta,
) -> code_protocol::protocol::OrderMeta {
    code_protocol::protocol::OrderMeta {
        request_ordinal: order.request_ordinal,
        output_index: order.output_index,
        sequence_number: order.sequence_number,
    }
}

pub fn order_meta_from_protocol(
    order: &code_protocol::protocol::OrderMeta,
) -> OrderMeta {
    OrderMeta {
        request_ordinal: order.request_ordinal,
        output_index: order.output_index,
        sequence_number: order.sequence_number,
    }
}


pub fn recorded_event_to_protocol(
    event: &RecordedEvent,
) -> Option<code_protocol::protocol::RecordedEvent> {
    let msg = event_msg_to_protocol(&event.msg)?;
    let order = event
        .order
        .as_ref()
        .map(order_meta_to_protocol);
    Some(code_protocol::protocol::RecordedEvent {
        id: event.id.clone(),
        event_seq: event.event_seq,
        order,
        msg,
    })
}


pub fn recorded_event_from_protocol(
    src: code_protocol::protocol::RecordedEvent,
) -> Option<RecordedEvent> {
    let msg = event_msg_from_protocol(&src.msg)?;
    let order = src.order.as_ref().map(order_meta_from_protocol);
    Some(RecordedEvent {
        id: src.id,
        event_seq: src.event_seq,
        order,
        msg,
    })
}

fn convert_value<T, U>(value: &T) -> Option<U>
where
    T: Serialize,
    U: DeserializeOwned,
{
    let Ok(json) = serde_json::to_value(value) else {
        return None;
    };
    serde_json::from_value(json).ok()
}

fn rate_limit_snapshot_to_protocol(
    snapshot: &RateLimitSnapshotEvent,
) -> code_protocol::protocol::RateLimitSnapshot {
    let primary = code_protocol::protocol::RateLimitWindow {
        used_percent: snapshot.primary_used_percent,
        window_minutes: Some(snapshot.primary_window_minutes),
        resets_in_seconds: snapshot.primary_reset_after_seconds,
    };
    let secondary = code_protocol::protocol::RateLimitWindow {
        used_percent: snapshot.secondary_used_percent,
        window_minutes: Some(snapshot.secondary_window_minutes),
        resets_in_seconds: snapshot.secondary_reset_after_seconds,
    };
    code_protocol::protocol::RateLimitSnapshot {
        primary: Some(primary),
        secondary: Some(secondary),
    }
}

fn rate_limit_snapshot_from_protocol(
    snapshot: &code_protocol::protocol::RateLimitSnapshot,
) -> RateLimitSnapshotEvent {
    let primary_used = snapshot
        .primary
        .as_ref()
        .map(|window| window.used_percent)
        .unwrap_or(0.0)
        .clamp(0.0, 100.0);
    let secondary_used = snapshot
        .secondary
        .as_ref()
        .map(|window| window.used_percent)
        .unwrap_or(0.0)
        .clamp(0.0, 100.0);
    let primary_window_minutes = snapshot
        .primary
        .as_ref()
        .and_then(|window| window.window_minutes)
        .unwrap_or(0);
    let primary_reset_after_seconds = snapshot
        .primary
        .as_ref()
        .and_then(|window| window.resets_in_seconds);
    let secondary_window_minutes = snapshot
        .secondary
        .as_ref()
        .and_then(|window| window.window_minutes)
        .unwrap_or(0);
    let secondary_reset_after_seconds = snapshot
        .secondary
        .as_ref()
        .and_then(|window| window.resets_in_seconds);

    let ratio_percent = match (primary_window_minutes, secondary_window_minutes) {
        (0, _) | (_, 0) => f64::NAN,
        (primary, secondary) => {
            let ratio = (primary as f64) / (secondary as f64);
            (ratio * 100.0).clamp(0.0, 100.0)
        }
    };

    RateLimitSnapshotEvent {
        primary_used_percent: primary_used,
        secondary_used_percent: secondary_used,
        primary_to_secondary_ratio_percent: ratio_percent,
        primary_window_minutes,
        secondary_window_minutes,
        primary_reset_after_seconds,
        secondary_reset_after_seconds,
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OrderMeta {
    /// 1-based ordinal of this request/turn in the session
    pub request_ordinal: u64,
    /// Model-provided output_index for the top-level item
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_index: Option<u32>,
    /// Model-provided sequence_number within the output_index stream
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence_number: Option<u64>,
}

/// Response event from the agent
#[derive(Debug, Clone, Deserialize, Serialize, Display)]
#[serde(tag = "type", rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum EventMsg {
    /// Error while executing a submission
    Error(ErrorEvent),

    /// Agent has started a task
    TaskStarted,

    /// Agent has completed all actions
    TaskComplete(TaskCompleteEvent),

    /// Token count event, sent periodically to report the number of tokens
    /// used in the current session and the latest rate limit snapshot.
    TokenCount(TokenCountEvent),

    /// Agent text output message
    AgentMessage(AgentMessageEvent),

    /// Agent text output delta message
    AgentMessageDelta(AgentMessageDeltaEvent),

    /// Reasoning event from agent.
    AgentReasoning(AgentReasoningEvent),

    /// Agent reasoning delta event from agent.
    AgentReasoningDelta(AgentReasoningDeltaEvent),

    /// Raw chain-of-thought from agent.
    AgentReasoningRawContent(AgentReasoningRawContentEvent),

    /// Agent reasoning content delta event from agent.
    AgentReasoningRawContentDelta(AgentReasoningRawContentDeltaEvent),
    /// Signaled when the model begins a new reasoning summary section (e.g., a new titled block).
    AgentReasoningSectionBreak(AgentReasoningSectionBreakEvent),

    /// Full environment context snapshot emitted to the model.
    EnvironmentContextFull(EnvironmentContextFullEvent),

    /// Environment context delta emitted to the model.
    EnvironmentContextDelta(EnvironmentContextDeltaEvent),

    /// Browser snapshot metadata emitted alongside environment context.
    BrowserSnapshot(BrowserSnapshotEvent),

    /// Warning that the platform compacted conversation history to stay within limits.
    CompactionCheckpointWarning(code_protocol::protocol::CompactionCheckpointWarningEvent),

    /// Ack the client's configure message.
    SessionConfigured(SessionConfiguredEvent),

    McpToolCallBegin(McpToolCallBeginEvent),

    McpToolCallEnd(McpToolCallEndEvent),

    /// Model requested a native web search
    WebSearchBegin(WebSearchBeginEvent),
    /// Native web search call completed
    WebSearchComplete(WebSearchCompleteEvent),

    /// Custom tool call events for non-MCP tools (browser, agent, etc)
    CustomToolCallBegin(CustomToolCallBeginEvent),
    CustomToolCallEnd(CustomToolCallEndEvent),

    /// Notification that the server is about to execute a command.
    ExecCommandBegin(ExecCommandBeginEvent),

    /// Incremental chunk of output from a running command.
    ExecCommandOutputDelta(ExecCommandOutputDeltaEvent),

    ExecCommandEnd(ExecCommandEndEvent),

    ExecApprovalRequest(ExecApprovalRequestEvent),

    RequestUserInput(RequestUserInputEvent),

    ApplyPatchApprovalRequest(ApplyPatchApprovalRequestEvent),

    BackgroundEvent(BackgroundEventEvent),

    /// Notification that the agent is about to apply a code patch. Mirrors
    /// `ExecCommandBegin` so front‑ends can show progress indicators.
    PatchApplyBegin(PatchApplyBeginEvent),

    /// Notification that a patch application has finished.
    PatchApplyEnd(PatchApplyEndEvent),

    TurnDiff(TurnDiffEvent),

    /// Response to GetHistoryEntryRequest.
    GetHistoryEntryResponse(GetHistoryEntryResponseEvent),

    /// List of custom prompts available to the agent.
    ListCustomPromptsResponse(ListCustomPromptsResponseEvent),

    /// List of skills available to the agent.
    ListSkillsResponse(ListSkillsResponseEvent),

    PlanUpdate(UpdatePlanArgs),

    /// Browser screenshot has been captured and is ready for display
    BrowserScreenshotUpdate(BrowserScreenshotUpdateEvent),

    /// Notification that the agent attached a local image via the image_view tool.
    ViewImageToolCall(ViewImageToolCallEvent),

    /// Agent status has been updated
    AgentStatusUpdate(AgentStatusUpdateEvent),

    /// User/system input message (what was sent to the model)
    UserMessage(code_protocol::protocol::UserMessageEvent),

    /// Notification that the agent is shutting down.
    ShutdownComplete,

    /// The system aborted the current turn (e.g., due to interruption).
    TurnAborted(code_protocol::protocol::TurnAbortedEvent),

    /// Response to a conversation path request.
    ConversationPath(code_protocol::protocol::ConversationPathResponseEvent),

    /// Entered review mode with the provided request.
    EnteredReviewMode(code_protocol::protocol::ReviewRequest),

    /// Exited review mode with an optional final result to apply.
    ExitedReviewMode(code_protocol::protocol::ExitedReviewModeEvent),

    /// Replay a previously recorded transcript into the UI.
    /// Used after resuming from a rollout file so the user sees the full
    /// history for that session without re-executing any actions.
    ReplayHistory(ReplayHistoryEvent),
}

// Individual event payload types matching each `EventMsg` variant.

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ErrorEvent {
    pub message: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskCompleteEvent {
    pub last_agent_message: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_output_tokens: u64,
    pub total_tokens: u64,
}

impl TokenUsage {
    pub fn is_zero(&self) -> bool {
        self.total_tokens == 0
    }

    pub fn cached_input(&self) -> u64 {
        self.cached_input_tokens
    }

    pub fn non_cached_input(&self) -> u64 {
        self.input_tokens.saturating_sub(self.cached_input())
    }

    /// Primary count for display as a single absolute value: non-cached input + output.
    pub fn blended_total(&self) -> u64 {
        self.non_cached_input() + self.output_tokens
    }

    /// For estimating what % of the model's context window is used, we need to account
    /// for reasoning output tokens from prior turns being dropped from the context window.
    /// We approximate this here by subtracting reasoning output tokens from the total.
    /// This will be off for the current turn and pending function calls.
    pub fn tokens_in_context_window(&self) -> u64 {
        self.total_tokens
            .saturating_sub(self.reasoning_output_tokens)
    }

    /// Estimate the remaining user-controllable percentage of the model's context window.
    pub fn percent_of_context_window_remaining(&self, context_window: u64) -> u8 {
        if context_window <= BASELINE_TOKENS {
            return 0;
        }

        let effective_window = context_window - BASELINE_TOKENS;
        let used = self
            .tokens_in_context_window()
            .saturating_sub(BASELINE_TOKENS);
        let remaining = effective_window.saturating_sub(used);
        ((remaining as f32 / effective_window as f32) * 100.0).clamp(0.0, 100.0) as u8
    }

    /// In-place element-wise sum of token counts.
    pub fn add_assign(&mut self, other: &TokenUsage) {
        self.input_tokens += other.input_tokens;
        self.cached_input_tokens += other.cached_input_tokens;
        self.output_tokens += other.output_tokens;
        self.reasoning_output_tokens += other.reasoning_output_tokens;
        self.total_tokens += other.total_tokens;
    }
}

/// Includes prompts, tools and space to call compact.
const BASELINE_TOKENS: u64 = 12_000;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Default)]
pub struct TokenUsageInfo {
    pub total_token_usage: TokenUsage,
    pub last_token_usage: TokenUsage,
    pub model_context_window: Option<u64>,
}

impl TokenUsageInfo {
    pub fn new_or_append(
        info: &Option<TokenUsageInfo>,
        last: &Option<TokenUsage>,
        model_context_window: Option<u64>,
    ) -> Option<Self> {
        if info.is_none() && last.is_none() {
            return None;
        }

        let mut info = match info {
            Some(info) => info.clone(),
            None => Self {
                total_token_usage: TokenUsage::default(),
                last_token_usage: TokenUsage::default(),
                model_context_window,
            },
        };

        if let Some(last) = last {
            info.append_last_usage(last);
        }

        if info.model_context_window.is_none() {
            info.model_context_window = model_context_window;
        }

        Some(info)
    }

    pub fn append_last_usage(&mut self, last: &TokenUsage) {
        self.total_token_usage.add_assign(last);
        self.last_token_usage = last.clone();
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct RateLimitSnapshotEvent {
    /// Percentage (0-100) of the primary window that has been consumed.
    pub primary_used_percent: f64,
    /// Percentage (0-100) of the secondary window that has been consumed.
    pub secondary_used_percent: f64,
    /// Size of the primary window relative to secondary (0-100).
    pub primary_to_secondary_ratio_percent: f64,
    /// Rolling window duration for the primary limit, in minutes.
    pub primary_window_minutes: u64,
    /// Rolling window duration for the secondary limit, in minutes.
    pub secondary_window_minutes: u64,
    /// Seconds until the primary window resets, if reported by the API.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_reset_after_seconds: Option<u64>,
    /// Seconds until the secondary window resets, if reported by the API.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary_reset_after_seconds: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct TokenCountEvent {
    pub info: Option<TokenUsageInfo>,
    pub rate_limits: Option<RateLimitSnapshotEvent>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FinalOutput {
    pub token_usage: TokenUsage,
}

/// Payload for `ReplayHistory` containing prior `ResponseItem`s.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReplayHistoryEvent {
    /// Items to render in order. Front-ends should render these as static
    /// history without triggering any tool execution.
    pub items: Vec<code_protocol::models::ResponseItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history_snapshot: Option<serde_json::Value>,
}

impl From<TokenUsage> for FinalOutput {
    fn from(token_usage: TokenUsage) -> Self {
        Self { token_usage }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WebSearchBeginEvent {
    pub call_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WebSearchCompleteEvent {
    pub call_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
}

impl fmt::Display for FinalOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let token_usage = &self.token_usage;
        write!(
            f,
            "Token usage: total={} input={}{} output={}{}",
            token_usage.blended_total(),
            token_usage.non_cached_input(),
            if token_usage.cached_input() > 0 {
                format!(" (+ {} cached)", token_usage.cached_input())
            } else {
                String::new()
            },
            token_usage.output_tokens,
            if token_usage.reasoning_output_tokens > 0 {
                format!(" (reasoning {})", token_usage.reasoning_output_tokens)
            } else {
                String::new()
            }
        )
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentMessageEvent {
    pub message: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentMessageDeltaEvent {
    pub delta: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentReasoningEvent {
    pub text: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentReasoningRawContentEvent {
    pub text: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentReasoningRawContentDeltaEvent {
    pub delta: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentReasoningSectionBreakEvent {}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentReasoningDeltaEvent {
    pub delta: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpInvocation {
    /// Name of the MCP server as defined in the config.
    pub server: String,
    /// Name of the tool as given by the MCP server.
    pub tool: String,
    /// Arguments to the tool call.
    pub arguments: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpToolCallBeginEvent {
    /// Identifier so this can be paired with the McpToolCallEnd event.
    pub call_id: String,
    pub invocation: McpInvocation,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpToolCallEndEvent {
    /// Identifier for the corresponding McpToolCallBegin that finished.
    pub call_id: String,
    pub invocation: McpInvocation,
    pub duration: Duration,
    /// Result of the tool call. Note this could be an error.
    pub result: Result<CallToolResult, String>,
}

impl McpToolCallEndEvent {
    pub fn is_success(&self) -> bool {
        match &self.result {
            Ok(result) => !result.is_error.unwrap_or(false),
            Err(_) => false,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CustomToolCallBeginEvent {
    /// Identifier so this can be paired with the CustomToolCallEnd event.
    pub call_id: String,
    /// Name of the tool (e.g., "browser_navigate", "agent")
    pub tool_name: String,
    /// Parameters passed to the tool as JSON
    pub parameters: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CustomToolCallEndEvent {
    /// Identifier for the corresponding CustomToolCallBegin that finished.
    pub call_id: String,
    /// Name of the tool
    pub tool_name: String,
    /// Parameters passed to the tool as JSON
    pub parameters: Option<serde_json::Value>,
    /// Duration of the tool call
    pub duration: Duration,
    /// Result of the tool call (success message or error)
    pub result: Result<String, String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExecCommandBeginEvent {
    /// Identifier so this can be paired with the ExecCommandEnd event.
    pub call_id: String,
    /// The command to be executed.
    pub command: Vec<String>,
    /// The command's working directory if not the default cwd for the agent.
    pub cwd: PathBuf,
    pub parsed_cmd: Vec<ParsedCommand>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExecCommandEndEvent {
    /// Identifier for the ExecCommandBegin that finished.
    pub call_id: String,
    /// Captured stdout
    pub stdout: String,
    /// Captured stderr
    pub stderr: String,
    /// The command's exit code.
    pub exit_code: i32,
    /// The duration of the command execution.
    pub duration: Duration,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecOutputStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExecCommandOutputDeltaEvent {
    /// Identifier for the ExecCommandBegin that produced this chunk.
    pub call_id: String,
    /// Which stream produced this chunk.
    pub stream: ExecOutputStream,
    /// Raw bytes from the stream (may not be valid UTF-8).
    #[serde(with = "serde_bytes")]
    pub chunk: ByteBuf,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExecApprovalRequestEvent {
    /// Identifier for the associated exec call, if available.
    pub call_id: String,
    /// The command to be executed.
    pub command: Vec<String>,
    /// The command's working directory.
    pub cwd: PathBuf,
    /// Optional human-readable reason for the approval (e.g. retry without sandbox).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApplyPatchApprovalRequestEvent {
    /// Responses API call id for the associated patch apply call, if available.
    pub call_id: String,
    pub changes: HashMap<PathBuf, FileChange>,
    /// Optional explanatory reason (e.g. request for extra write access).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// When set, the agent is asking the user to allow writes under this root for the remainder of the session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grant_root: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BackgroundEventEvent {
    pub message: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PatchApplyBeginEvent {
    /// Identifier so this can be paired with the PatchApplyEnd event.
    pub call_id: String,
    /// If true, there was no ApplyPatchApprovalRequest for this patch.
    pub auto_approved: bool,
    /// The changes to be applied.
    pub changes: HashMap<PathBuf, FileChange>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PatchApplyEndEvent {
    /// Identifier for the PatchApplyBegin that finished.
    pub call_id: String,
    /// Captured stdout (summary printed by apply_patch).
    pub stdout: String,
    /// Captured stderr (parser errors, IO failures, etc.).
    pub stderr: String,
    /// Whether the patch was applied successfully.
    pub success: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TurnDiffEvent {
    pub unified_diff: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetHistoryEntryResponseEvent {
    pub offset: usize,
    pub log_id: u64,
    /// The entry at the requested offset, if available and parseable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry: Option<HistoryEntry>,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct SessionConfiguredEvent {
    /// Unique id for this session.
    pub session_id: Uuid,

    /// Tell the client what model is being queried.
    pub model: String,

    /// Identifier of the history log file (inode on Unix, 0 otherwise).
    pub history_log_id: u64,

    /// Current number of entries in the history log.
    pub history_entry_count: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BrowserScreenshotUpdateEvent {
    /// Path to the screenshot file
    pub screenshot_path: PathBuf,
    /// Current URL of the browser
    pub url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentStatusUpdateEvent {
    /// List of currently active agents
    pub agents: Vec<AgentInfo>,
    /// Shared context for all agents (if available)
    pub context: Option<String>,
    /// Shared task/output goal for all agents (if available)  
    pub task: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentSourceKind {
    Default,
    AutoReview,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentInfo {
    /// Unique identifier for the agent
    pub id: String,
    /// Display name for the agent
    pub name: String,
    /// Current status of the agent
    pub status: String,
    /// Optional batch identifier when the agent was launched via `agent` action "create"
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub batch_id: Option<String>,
    /// Optional model being used
    pub model: Option<String>,
    /// Latest progress line (if any) for UI previews
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub last_progress: Option<String>,
    /// Final success message, when `status == "completed"`
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub result: Option<String>,
    /// Final error message, when `status == "failed"`
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub error: Option<String>,
    /// Total elapsed time in milliseconds (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub elapsed_ms: Option<u64>,
    /// Approximate token count consumed by the agent (if reported)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub token_count: Option<u64>,

    /// Last time the agent reported activity (RFC3339). Only populated for active agents.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub last_activity_at: Option<String>,

    /// Seconds since last activity, computed at emission time. Only for active agents.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub seconds_since_last_activity: Option<u64>,

    /// Source category for this agent (e.g., auto_review) to support UI filtering.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub source_kind: Option<AgentSourceKind>,
}

/// User's decision in response to an ExecApprovalRequest.
#[derive(Debug, Default, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDecision {
    /// User has approved this command and the agent should execute it.
    Approved,

    /// User has approved this command and wants to automatically approve any
    /// future identical instances (`command` and `cwd` match exactly) for the
    /// remainder of the session.
    ApprovedForSession,

    /// User has denied this command and the agent should not execute it, but
    /// it should continue the session and try something else.
    #[default]
    Denied,

    /// User has denied this command and the agent should not do anything until
    /// the user's next command.
    Abort,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileChange {
    Add {
        content: String,
    },
    Delete,
    Update {
        unified_diff: String,
        move_path: Option<PathBuf>,
        original_content: String,
        new_content: String,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Chunk {
    /// 1-based line index of the first line in the original file
    pub orig_index: u32,
    pub deleted_lines: Vec<String>,
    pub inserted_lines: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EnvironmentContextFullEvent {
    /// JSON serialization of the environment context snapshot.
    pub snapshot: serde_json::Value,
    /// Sequence number associated with the snapshot emission.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EnvironmentContextDeltaEvent {
    /// JSON serialization of the environment context delta.
    pub delta: serde_json::Value,
    /// Sequence number associated with the delta emission.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence: Option<u64>,
    /// Fingerprint of the baseline snapshot for this delta.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BrowserSnapshotEvent {
    /// JSON serialization of the browser snapshot metadata.
    pub snapshot: serde_json::Value,
    /// URL associated with the snapshot, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Timestamp when the snapshot was captured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub captured_at: Option<String>,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    /// Serialize Event to verify that its JSON representation has the expected
    /// amount of nesting.
    #[test]
    fn serialize_event() {
        let session_id: Uuid = uuid::uuid!("67e55044-10b1-426f-9247-bb680e5fe0c8");
        let event = Event {
            id: "1234".to_string(),
            event_seq: 0,
            msg: EventMsg::SessionConfigured(SessionConfiguredEvent {
                session_id,
                model: "codex-mini-latest".to_string(),
                history_log_id: 0,
                history_entry_count: 0,
            }),
            order: None,
        };
        let serialized = serde_json::to_string(&event).unwrap();
        assert_eq!(
            serialized,
            r#"{"id":"1234","event_seq":0,"msg":{"type":"session_configured","session_id":"67e55044-10b1-426f-9247-bb680e5fe0c8","model":"codex-mini-latest","history_log_id":0,"history_entry_count":0}}"#
        );
    }
}
