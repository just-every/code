use code_core::config_types::ReasoningEffort;
use code_core::config_types::TextVerbosity;
use code_core::config_types::ThemeName;
use code_core::protocol::Event;
use code_core::protocol::OrderMeta;
use code_core::protocol::ValidationGroup;
use code_core::protocol::ApprovedCommandMatchKind;
use code_core::protocol::TokenUsage;
use code_core::git_info::CommitLogEntry;
use code_core::protocol::ReviewContextMetadata;
use code_file_search::FileMatch;
use code_common::model_presets::ModelPreset;
use crossterm::event::KeyEvent;
use crossterm::event::MouseEvent;
use ratatui::text::Line;
use crate::streaming::StreamKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModelSelectionKind {
    Session,
    Review,
    Planning,
    AutoDrive,
    ReviewResolve,
    AutoReview,
    AutoReviewResolve,
}
use crate::history::state::HistorySnapshot;
use std::time::Duration;
use uuid::Uuid;

use code_git_tooling::{GhostCommit, GitToolingError};
use code_cloud_tasks_client::{ApplyOutcome, CloudTaskError, CreatedTask, TaskSummary};

use crate::app::ChatWidgetArgs;
use crate::chrome_launch::ChromeLaunchOption;
use crate::slash_command::SlashCommand;
use code_protocol::models::ResponseItem;
use code_protocol::request_user_input::RequestUserInputResponse;
use std::fmt;
use std::path::PathBuf;
use std::sync::mpsc::Sender as StdSender;
use crate::cloud_tasks_service::CloudEnvironment;
use crate::resume::discovery::ResumeCandidate;

/// Wrapper to allow including non-Debug types in Debug enums without leaking internals.
pub(crate) struct Redacted<T>(pub T);

impl<T> fmt::Debug for Redacted<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TerminalRunController {
    pub tx: StdSender<TerminalRunEvent>,
}

#[derive(Debug, Clone)]
pub(crate) struct TerminalLaunch {
    pub id: u64,
    pub title: String,
    pub command: Vec<String>,
    pub command_display: String,
    pub controller: Option<TerminalRunController>,
    pub auto_close_on_success: bool,
    pub start_running: bool,
}

#[derive(Debug, Clone)]
pub(crate) enum TerminalRunEvent {
    Chunk { data: Vec<u8>, _is_stderr: bool },
    Exit { exit_code: Option<i32>, _duration: Duration },
}

#[derive(Debug, Clone)]
pub(crate) enum TerminalCommandGate {
    Run(String),
    Cancel,
}

#[derive(Debug, Clone)]
pub(crate) enum TerminalAfter {
    RefreshAgentsAndClose { selected_index: usize },
}

#[derive(Debug, Clone)]
pub(crate) enum GitInitResume {
    SubmitText { text: String },
    DispatchCommand { command: SlashCommand, command_text: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackgroundPlacement {
    /// Default: append to the end of the current request/history window.
    Tail,
    /// Display immediately before the next provider/tool output for the active request.
    BeforeNextOutput,
}

pub(crate) use code_auto_drive_core::{
    AutoContinueMode,
    AutoCoordinatorStatus,
    AutoTurnAgentsAction,
    AutoTurnAgentsTiming,
    AutoTurnCliAction,
};

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub(crate) enum AppEvent {
    CodexEvent(Event),

    /// Request a redraw which will be debounced by the [`App`].
    RequestRedraw,

    /// Update the model preset list used by the TUI model picker.
    ///
    /// The picker boots with built-in presets; when a remote-merged list arrives
    /// asynchronously, the in-memory list is swapped and any open model
    /// selection view is updated in-place.
    #[allow(dead_code)]
    ModelPresetsUpdated {
        presets: Vec<ModelPreset>,
        default_model: Option<String>,
    },

    /// Actually draw the next frame.
    Redraw,

    /// Update the terminal title override. `None` restores the default title.
    SetTerminalTitle { title: Option<String> },

    /// Emit a best-effort OSC 9 notification from the terminal.
    EmitTuiNotification { title: String, body: Option<String> },

    /// Schedule a one-shot animation frame roughly after the given duration.
    /// Multiple requests are coalesced by the central frame scheduler.
    ScheduleFrameIn(Duration),

    /// Background ghost snapshot job finished (success or failure).
    GhostSnapshotFinished {
        job_id: u64,
        result: Result<GhostCommit, GitToolingError>,
        elapsed: Duration,
    },

    /// Background auto-review baseline capture finished (non-blocking).
    AutoReviewBaselineCaptured {
        turn_sequence: u64,
        result: Result<GhostCommit, GitToolingError>,
    },

    /// Internal: flush any pending out-of-order ExecEnd events that did not
    /// receive a matching ExecBegin within a short pairing window. This lets
    /// the TUI render a fallback "Ran call_<id>" cell so output is not lost.
    FlushPendingExecEnds,
    /// Internal: refresh frozen history cell heights after resize.
    SyncHistoryVirtualization,
    /// Internal: when interrupts queue up behind a stalled/idle stream,
    /// finalize the stream and flush the queue so Exec/Tool cells render.
    FlushInterruptsIfIdle,

    KeyEvent(KeyEvent),

    MouseEvent(MouseEvent),

    /// Text pasted from the terminal clipboard.
    Paste(String),

    /// Request to exit the application gracefully.
    ExitRequest,

    /// Forward an `Op` to the Agent. Using an `AppEvent` for this avoids
    /// bubbling channels through layers of widgets.
    CodexOp(code_core::protocol::Op),

    /// Submit a response for a pending `request_user_input` tool call.
    RequestUserInputAnswer {
        turn_id: String,
        response: RequestUserInputResponse,
    },

    AutoCoordinatorDecision {
        seq: u64,
        status: AutoCoordinatorStatus,
        status_title: Option<String>,
        status_sent_to_user: Option<String>,
        goal: Option<String>,
        cli: Option<AutoTurnCliAction>,
        agents_timing: Option<AutoTurnAgentsTiming>,
        agents: Vec<AutoTurnAgentsAction>,
        transcript: Vec<ResponseItem>,
    },
    AutoCoordinatorUserReply {
        user_response: Option<String>,
        cli_command: Option<String>,
    },
    AutoCoordinatorThinking {
        delta: String,
        summary_index: Option<u32>,
    },
    AutoCoordinatorAction {
        message: String,
    },
    AutoCoordinatorTokenMetrics {
        total_usage: TokenUsage,
        last_turn_usage: TokenUsage,
        turn_count: u32,
        duplicate_items: u32,
        replay_updates: u32,
    },
    AutoCoordinatorCompactedHistory {
        conversation: std::sync::Arc<[ResponseItem]>,
        show_notice: bool,
    },
    AutoCoordinatorStopAck,
    AutoCoordinatorCountdown {
        countdown_id: u64,
        seconds_left: u8,
    },
    /// Trigger an automatic Auto Drive restart after a transient failure.
    AutoCoordinatorRestart {
        token: u64,
        attempt: u32,
    },
    ShowAutoDriveSettings,
    CloseAutoDriveSettings,
    AutoDriveSettingsChanged {
        review_enabled: bool,
        agents_enabled: bool,
        cross_check_enabled: bool,
        qa_automation_enabled: bool,
        continue_mode: AutoContinueMode,
    },

    /// Dispatch a recognized slash command from the UI (composer) to the app
    /// layer so it can be handled centrally. Includes the full command text.
    DispatchCommand(SlashCommand, String),

    /// Restore workspace state according to the chosen undo scope.
    PerformUndoRestore {
        commit: Option<String>,
        restore_files: bool,
        restore_conversation: bool,
    },

    /// Switch to a new working directory by rebuilding the chat widget with
    /// the same configuration but a different `cwd`. Optionally submits an
    /// initial prompt once the new session is ready.
    SwitchCwd(std::path::PathBuf, Option<String>),

    /// Resume picker data finished loading
    ResumePickerLoaded {
        cwd: std::path::PathBuf,
        candidates: Vec<ResumeCandidate>,
    },

    /// Resume picker failed to load
    ResumePickerLoadFailed { message: String },

    /// Signal that agents are about to start (triggered when /plan, /solve, /code commands are entered)
    PrepareAgents,

    /// Update the model and optional reasoning effort preset
    UpdateModelSelection {
        model: String,
        effort: Option<ReasoningEffort>,
    },

    /// Update the dedicated review model + reasoning effort
    UpdateReviewModelSelection {
        model: String,
        effort: ReasoningEffort,
    },
    /// Update the resolve model + reasoning effort for /review auto-resolve
    UpdateReviewResolveModelSelection {
        model: String,
        effort: ReasoningEffort,
    },
    /// Toggle review model inheritance from chat model
    UpdateReviewUseChatModel(bool),
    /// Toggle resolve model inheritance from chat model
    UpdateReviewResolveUseChatModel(bool),
    /// Update the planning (read-only) model + reasoning effort
    UpdatePlanningModelSelection {
        model: String,
        effort: ReasoningEffort,
    },
    /// Toggle planning model inheritance from chat model
    UpdatePlanningUseChatModel(bool),

    /// Update the Auto Drive model + reasoning effort
    UpdateAutoDriveModelSelection {
        model: String,
        effort: ReasoningEffort,
    },
    /// Toggle Auto Drive model inheritance from chat model
    UpdateAutoDriveUseChatModel(bool),

    /// Update the Auto Review model + reasoning effort
    UpdateAutoReviewModelSelection {
        model: String,
        effort: ReasoningEffort,
    },
    /// Toggle Auto Review model inheritance from chat model
    UpdateAutoReviewUseChatModel(bool),

    /// Update the Auto Review resolve model + reasoning effort
    UpdateAutoReviewResolveModelSelection {
        model: String,
        effort: ReasoningEffort,
    },
    /// Toggle Auto Review resolve model inheritance from chat model
    UpdateAutoReviewResolveUseChatModel(bool),

    /// Model selection UI closed (accepted or cancelled)
    ModelSelectionClosed {
        target: ModelSelectionKind,
        accepted: bool,
    },

    /// Update the text verbosity level
    UpdateTextVerbosity(TextVerbosity),

    /// Update the TUI notifications toggle
    UpdateTuiNotifications(bool),
    /// Enable or disable Auto Resolve for review flows
    UpdateReviewAutoResolveEnabled(bool),
    /// Enable or disable background Auto Review
    UpdateAutoReviewEnabled(bool),
    /// Set the maximum number of Auto Resolve re-review attempts
    UpdateReviewAutoResolveAttempts(u32),
    /// Set the maximum number of Auto Review follow-up reviews
    UpdateAutoReviewFollowupAttempts(u32),
    /// Open the review model selector overlay
    ShowReviewModelSelector,
    /// Open the resolve model selector overlay for /review auto-resolve
    ShowReviewResolveModelSelector,
    /// Open the planning model selector overlay
    ShowPlanningModelSelector,
    /// Open the Auto Drive model selector overlay
    ShowAutoDriveModelSelector,
    /// Open the Auto Review review model selector overlay
    ShowAutoReviewModelSelector,
    /// Open the Auto Review resolve model selector overlay
    ShowAutoReviewResolveModelSelector,
    /// Enable/disable a specific validation tool
    UpdateValidationTool { name: String, enable: bool },
    /// Enable/disable an entire validation group
    UpdateValidationGroup { group: ValidationGroup, enable: bool },
    /// Start installing a validation tool through the terminal overlay
    RequestValidationToolInstall { name: String, command: String },

    /// Enable/disable a specific MCP server
    #[allow(dead_code)]
    UpdateMcpServer { name: String, enable: bool },

    /// Prefill the composer input with the given text
    #[allow(dead_code)]
    PrefillComposer(String),

    /// Confirm and run git init, then resume a pending action.
    ConfirmGitInit { resume: GitInitResume },
    /// Continue without git; disables git-dependent actions for this session.
    DeclineGitInit,
    /// Git init completed (success or failure).
    GitInitFinished { ok: bool, message: String },

    /// Submit a message with hidden preface instructions
    SubmitTextWithPreface { visible: String, preface: String },

    /// Submit a hidden message that is not rendered in history but still sent to the LLM.
    /// When `surface_notice` is true, the TUI shows a developer-style notice with the
    /// injected text; when false, the injection is silent.
    SubmitHiddenTextWithPreface {
        agent_text: String,
        preface: String,
        surface_notice: bool,
    },

    /// Run a review with an explicit prompt/hint pair (used by TUI selections)
    RunReviewWithScope {
        prompt: String,
        hint: String,
        preparation_label: Option<String>,
        metadata: Option<ReviewContextMetadata>,
        auto_resolve: bool,
    },

    /// Background Auto Review lifecycle notifications
    BackgroundReviewStarted {
        worktree_path: PathBuf,
        branch: String,
        agent_id: Option<String>,
        snapshot: Option<String>,
    },
    BackgroundReviewFinished {
        worktree_path: PathBuf,
        branch: String,
        has_findings: bool,
        findings: usize,
        summary: Option<String>,
        error: Option<String>,
        agent_id: Option<String>,
        snapshot: Option<String>,
    },

    /// Run the review command with the given argument string (mirrors `/review <args>`)
    RunReviewCommand(String),

    /// Open a bottom-pane form that lets the user select a commit to review.
    StartReviewCommitPicker,
    /// Populate the commit picker with retrieved commit entries.
    PresentReviewCommitPicker { commits: Vec<CommitLogEntry> },
    /// Open a bottom-pane form that lets the user select a base branch to diff against.
    StartReviewBranchPicker,
    /// Populate the branch picker with branch metadata once loaded asynchronously.
    PresentReviewBranchPicker {
        current_branch: Option<String>,
        branches: Vec<String>,
    },

    /// Show the multi-line prompt input to collect custom review instructions.
    OpenReviewCustomPrompt,

    /// Cloud tasks: fetch the latest list based on the active environment filter.
    FetchCloudTasks { environment: Option<String> },
    /// Cloud tasks: response containing the refreshed task list.
    PresentCloudTasks { environment: Option<String>, tasks: Vec<TaskSummary> },
    /// Cloud tasks: generic error surfaced to the UI.
    CloudTasksError { message: String },
    /// Cloud tasks: fetch available environments to filter against.
    FetchCloudEnvironments,
    /// Cloud tasks: populated environment list ready for selection.
    PresentCloudEnvironments { environments: Vec<CloudEnvironment> },
    /// Cloud tasks: update the active environment filter (None = all environments).
    SetCloudEnvironment { environment: Option<CloudEnvironment> },
    /// Cloud tasks: show actions for a specific task.
    ShowCloudTaskActions { task_id: String },
    /// Cloud tasks: load diff for a task (current attempt).
    FetchCloudTaskDiff { task_id: String },
    /// Cloud tasks: load assistant messages for a task (current attempt).
    FetchCloudTaskMessages { task_id: String },
    /// Cloud tasks: run apply or preflight on a task.
    ApplyCloudTask { task_id: String, preflight: bool },
    /// Cloud tasks: apply/preflight finished.
    CloudTaskApplyFinished {
        task_id: String,
        outcome: Result<ApplyOutcome, CloudTaskError>,
        preflight: bool,
    },
    /// Cloud tasks: open the create-task prompt.
    OpenCloudTaskCreate,
    /// Cloud tasks: submit a new task creation request.
    SubmitCloudTaskCreate { env_id: String, prompt: String, best_of_n: usize },
    /// Cloud tasks: new task creation result.
    CloudTaskCreated {
        env_id: String,
        result: Result<CreatedTask, CloudTaskError>,
    },

    /// Update the theme (with history event)
    #[allow(dead_code)]
    UpdateTheme(ThemeName),
    /// Add or update a subagent command in memory (UI already persisted to config.toml)
    UpdateSubagentCommand(code_core::config_types::SubagentCommandConfig),
    /// Remove a subagent command from memory (UI already deleted from config.toml)
    DeleteSubagentCommand(String),
    /// Return to the Agents settings list view
    // ShowAgentsSettings removed; overview replaces it
    /// Return to the Agents overview (Agents + Commands)
    ShowAgentsOverview,
    /// Open the agent editor form for a specific agent name
    ShowAgentEditor { name: String },
    /// Open a blank agent editor form for adding a new agent
    ShowAgentEditorNew,
    // ShowSubagentEditor removed; use ShowSubagentEditorForName or ShowSubagentEditorNew
    /// Open the subagent editor for a specific command name; ChatWidget supplies data
    ShowSubagentEditorForName { name: String },
    /// Open a blank subagent editor to create a new command
    ShowSubagentEditorNew,

    /// Preview theme (no history event)
    #[allow(dead_code)]
    PreviewTheme(ThemeName),
    /// Update the loading spinner style (with history event)
    #[allow(dead_code)]
    UpdateSpinner(String),
    /// Preview loading spinner (no history event)
    #[allow(dead_code)]
    PreviewSpinner(String),
    /// Rotate access/safety preset (Read Only → Write with Approval → Full Access)
    CycleAccessMode,
    /// Cycle Auto Drive composer styling variants (Sentinel → Whisper → …)
    CycleAutoDriveVariant,
    /// Bottom composer expanded (e.g., slash command popup opened)
    ComposerExpanded,

    /// Show the main account picker view for /login
    ShowLoginAccounts,
    /// Show the add-account flow for /login
    ShowLoginAddAccount,

    /// Kick off an asynchronous file search for the given query (text after
    /// the `@`). Previous searches may be cancelled by the app layer so there
    /// is at most one in-flight search.
    StartFileSearch(String),

    /// Result of a completed asynchronous file search. The `query` echoes the
    /// original search term so the UI can decide whether the results are
    /// still relevant.
    FileSearchResult {
        query: String,
        matches: Vec<FileMatch>,
    },

    /// Result of computing a `/diff` command.
    #[allow(dead_code)]
    DiffResult(String),

    InsertHistory(Vec<Line<'static>>),
    InsertHistoryWithKind { id: Option<String>, kind: StreamKind, lines: Vec<Line<'static>> },
    /// Finalized assistant answer with raw markdown for re-rendering under theme changes.
    InsertFinalAnswer { id: Option<String>, lines: Vec<Line<'static>>, source: String },
    /// Insert a background event with explicit placement semantics.
    InsertBackgroundEvent {
        message: String,
        placement: BackgroundPlacement,
        order: Option<OrderMeta>,
    },

    AutoUpgradeCompleted { version: String },

    /// Background rate limit refresh failed (threaded request).
    RateLimitFetchFailed { message: String },

    /// Background rate limit refresh persisted an account snapshot.
    RateLimitSnapshotStored { account_id: String },

    #[allow(dead_code)]
    StartCommitAnimation,
    #[allow(dead_code)]
    StopCommitAnimation,
    CommitTick,

    /// Onboarding: result of login_with_chatgpt.
    OnboardingAuthComplete(Result<(), String>),
    OnboardingComplete(ChatWidgetArgs),

    /// Begin ChatGPT login flow from the in-app login manager.
    LoginStartChatGpt,
    /// Begin device code login flow from the in-app login manager.
    LoginStartDeviceCode,
    /// Cancel an in-progress ChatGPT login flow triggered via `/login`.
    LoginCancelChatGpt,
    /// ChatGPT login flow has completed (success or failure).
    LoginChatGptComplete { result: Result<(), String> },
    /// Device code login flow produced a user code/link.
    LoginDeviceCodeReady { authorize_url: String, user_code: String },
    /// Device code login flow failed before completion.
    LoginDeviceCodeFailed { message: String },
    /// Device code login flow completed (success or failure).
    LoginDeviceCodeComplete { result: Result<(), String> },
    /// The active authentication mode changed (e.g., switched accounts).
    LoginUsingChatGptChanged { using_chatgpt_auth: bool },

    /// Show Chrome launch options dialog
    #[allow(dead_code)]
    ShowChromeOptions(Option<u16>),

    /// Chrome launch option selected by user
    ChromeLaunchOptionSelected(ChromeLaunchOption, Option<u16>),

    /// Start a new chat session by resuming from the given rollout file
    ResumeFrom(std::path::PathBuf),

    /// Begin jump-back to the Nth last user message (1 = latest).
    /// Trims visible history up to that point and pre-fills the composer.
    JumpBack { nth: usize, prefill: String, history_snapshot: Option<HistorySnapshot> },
    /// Result of an async jump-back fork operation performed off the UI thread.
    /// Carries the forked conversation, trimmed prefix to replay, and composer prefill.
    JumpBackForked {
        cfg: code_core::config::Config,
        new_conv: Redacted<code_core::NewConversation>,
        prefix_items: Vec<ResponseItem>,
        prefill: String,
    },

    /// Register an image placeholder inserted by the composer with its backing path
    /// so ChatWidget can resolve it to a LocalImage on submit.
    RegisterPastedImage { placeholder: String, path: PathBuf },

    /// Immediately cancel any running task in the ChatWidget. This is used by
    /// the approval modal to reflect a user's Abort decision instantly in the UI
    /// (clear spinner/status, finalize running exec/tool cells) while the core
    /// continues its own abort/cleanup in parallel.
    CancelRunningTask,
    /// Register a command pattern as approved, optionally persisting to config.
    RegisterApprovedCommand {
        command: Vec<String>,
        match_kind: ApprovedCommandMatchKind,
        persist: bool,
        semantic_prefix: Option<Vec<String>>,
    },
    /// Indicate that an approval was denied so the UI can clear transient
    /// spinner/status state without interrupting the core task.
    MarkTaskIdle,
    OpenTerminal(TerminalLaunch),
    TerminalChunk {
        id: u64,
        chunk: Vec<u8>,
        _is_stderr: bool,
    },
    TerminalExit {
        id: u64,
        exit_code: Option<i32>,
        _duration: Duration,
    },
    TerminalCancel { id: u64 },
    TerminalRunCommand {
        id: u64,
        command: Vec<String>,
        command_display: String,
        controller: Option<TerminalRunController>,
    },
    TerminalSendInput {
        id: u64,
        data: Vec<u8>,
    },
    TerminalResize {
        id: u64,
        rows: u16,
        cols: u16,
    },
    TerminalRerun { id: u64 },
    TerminalUpdateMessage { id: u64, message: String },
    TerminalForceClose { id: u64 },
    TerminalAfter(TerminalAfter),
    TerminalSetAssistantMessage { id: u64, message: String },
    TerminalAwaitCommand {
        id: u64,
        suggestion: String,
        ack: Redacted<StdSender<TerminalCommandGate>>,
    },
    TerminalApprovalDecision { id: u64, approved: bool },
    StartAutoDriveCelebration {
        message: Option<String>,
    },
    StopAutoDriveCelebration,
    RunUpdateCommand {
        command: Vec<String>,
        display: String,
        latest_version: Option<String>,
    },
    SetAutoUpgradeEnabled(bool),
    SetAutoSwitchAccountsOnRateLimit(bool),
    SetApiKeyFallbackOnAllAccountsLimited(bool),
    RequestAgentInstall { name: String, selected_index: usize },
    AgentsOverviewSelectionChanged { index: usize },
    /// Add or update an agent's settings (enabled, params, instructions)
    UpdateAgentConfig {
        name: String,
        enabled: bool,
        args_read_only: Option<Vec<String>>,
        args_write: Option<Vec<String>>,
        instructions: Option<String>,
        description: Option<String>,
        command: String,
    },
    AgentValidationFinished {
        name: String,
        result: Result<(), String>,
        attempt_id: Uuid,
    },
    
}

// No helper constructor; use `AppEvent::CodexEvent(ev)` directly to avoid shadowing.
