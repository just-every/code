use code_core::config_types::ReasoningEffort;
use code_core::config_types::TextVerbosity;
use code_core::config_types::ThemeName;
use code_core::protocol::Event;
use code_core::protocol::OrderMeta;
use code_core::protocol::ValidationGroup;
use code_core::protocol::ApprovedCommandMatchKind;
use code_core::git_info::CommitLogEntry;
use code_core::protocol::ReviewContextMetadata;
use code_file_search::FileMatch;
use crossterm::event::KeyEvent;
use crossterm::event::MouseEvent;
use ratatui::text::Line;
use crate::streaming::StreamKind;
use crate::history::state::HistorySnapshot;
use std::time::Duration;

use code_git_tooling::{GhostCommit, GitToolingError};
use code_cloud_tasks_client::{ApplyOutcome, CloudTaskError, CreatedTask, TaskSummary};

use crate::app::ChatWidgetArgs;
use crate::auto_review::AutoReviewOutcome;
use crate::chrome_launch::ChromeLaunchOption;
use crate::slash_command::SlashCommand;
use code_protocol::models::ResponseItem;
use std::fmt;
use std::path::PathBuf;
use std::sync::mpsc::Sender as StdSender;
use crate::cloud_tasks_service::CloudEnvironment;

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
    ReviewCommitDescriptor,
    TurnDescriptor,
};

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub(crate) enum AppEvent {
    CodexEvent(Event),

    /// Request a redraw which will be debounced by the [`App`].
    RequestRedraw,

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

    /// Internal: flush any pending out-of-order ExecEnd events that did not
    /// receive a matching ExecBegin within a short pairing window. This lets
    /// the TUI render a fallback "Ran call_<id>" cell so output is not lost.
    FlushPendingExecEnds,

    KeyEvent(KeyEvent),

    MouseEvent(MouseEvent),

    /// Text pasted from the terminal clipboard.
    Paste(String),

    /// Request to exit the application gracefully.
    ExitRequest,

    /// Forward an `Op` to the Agent. Using an `AppEvent` for this avoids
    /// bubbling channels through layers of widgets.
    CodexOp(code_core::protocol::Op),

    AutoCoordinatorDecision {
        status: AutoCoordinatorStatus,
        progress_past: Option<String>,
        progress_current: Option<String>,
        goal: Option<String>,
        cli: Option<AutoTurnCliAction>,
        agents_timing: Option<AutoTurnAgentsTiming>,
        agents: Vec<AutoTurnAgentsAction>,
        transcript: Vec<ResponseItem>,
        turn_descriptor: Option<TurnDescriptor>,
        review_commit: Option<ReviewCommitDescriptor>,
    },
    AutoCoordinatorUserReply {
        user_response: Option<String>,
        cli_command: Option<String>,
    },
    AutoCoordinatorThinking {
        delta: String,
        summary_index: Option<u32>,
    },
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
        review_auto_resolve: bool,
    },

    /// Commit review preparation finished; launch or skip accordingly.
    AutoReviewCommitReady { outcome: AutoReviewOutcome },

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

    /// Signal that agents are about to start (triggered when /plan, /solve, /code commands are entered)
    PrepareAgents,

    /// Update the model and optional reasoning effort preset
    UpdateModelSelection {
        model: String,
        effort: Option<ReasoningEffort>,
    },

    /// Update the text verbosity level
    UpdateTextVerbosity(TextVerbosity),

    /// Update GitHub workflow monitoring toggle
    UpdateGithubWatcher(bool),
    /// Update the TUI notifications toggle
    UpdateTuiNotifications(bool),
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

    /// Submit a message with hidden preface instructions
    SubmitTextWithPreface { visible: String, preface: String },

    /// Run a review with an explicit prompt/hint pair (used by TUI selections)
    RunReviewWithScope {
        prompt: String,
        hint: String,
        preparation_label: Option<String>,
        metadata: Option<ReviewContextMetadata>,
        auto_resolve: bool,
    },

    /// Run the review command with the given argument string (mirrors `/review <args>`)
    RunReviewCommand(String),

    /// Toggle the persisted auto-resolve setting for reviews.
    ToggleReviewAutoResolve,

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
    /// Cancel an in-progress ChatGPT login flow triggered via `/login`.
    LoginCancelChatGpt,
    /// ChatGPT login flow has completed (success or failure).
    LoginChatGptComplete { result: Result<(), String> },
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
    RunUpdateCommand {
        command: Vec<String>,
        display: String,
        latest_version: Option<String>,
    },
    SetAutoUpgradeEnabled(bool),
    RequestAgentInstall { name: String, selected_index: usize },
    AgentsOverviewSelectionChanged { index: usize },
    /// Add or update an agent's settings (enabled, params, instructions)
    UpdateAgentConfig {
        name: String,
        enabled: bool,
        args_read_only: Option<Vec<String>>,
        args_write: Option<Vec<String>>,
        instructions: Option<String>,
        command: String,
    },
    
}

// No helper constructor; use `AppEvent::CodexEvent(ev)` directly to avoid shadowing.
