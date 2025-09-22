use codex_core::config_types::ReasoningEffort;
use codex_core::config_types::TextVerbosity;
use codex_core::config_types::ThemeName;
use codex_core::protocol::Event;
use codex_core::protocol::ValidationGroup;
use codex_core::protocol::ApprovedCommandMatchKind;
use codex_core::git_info::CommitLogEntry;
use codex_core::protocol::ReviewContextMetadata;
use codex_file_search::FileMatch;
use crossterm::event::KeyEvent;
use crossterm::event::MouseEvent;
use ratatui::text::Line;
use crate::streaming::StreamKind;
use std::time::Duration;

use crate::app::ChatWidgetArgs;
use crate::bottom_pane::chrome_selection_view::ChromeLaunchOption;
use crate::slash_command::SlashCommand;
use codex_protocol::models::ResponseItem;
use std::fmt;
use std::path::PathBuf;
use std::sync::mpsc::Sender as StdSender;

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

    /// Notification that a pending approval has been resolved by the user.
    ApprovalRequestResolved,

    /// Schedule a one-shot animation frame roughly after the given duration.
    /// Multiple requests are coalesced by the central frame scheduler.
    ScheduleFrameIn(Duration),

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
    CodexOp(codex_core::protocol::Op),

    /// Dispatch a recognized slash command from the UI (composer) to the app
    /// layer so it can be handled centrally. Includes the full command text.
    DispatchCommand(SlashCommand, String),

    /// Open undo options for a previously captured snapshot.
    ShowUndoOptions { index: usize },
    /// Restore workspace state according to the chosen undo scope.
    PerformUndoRestore {
        index: usize,
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
    /// Enable/disable a specific validation tool
    UpdateValidationTool { name: String, enable: bool },
    /// Enable/disable an entire validation group
    UpdateValidationGroup { group: ValidationGroup, enable: bool },
    /// Start installing a validation tool through the terminal overlay
    RequestValidationToolInstall { name: String, command: String },

    /// Enable/disable a specific MCP server
    UpdateMcpServer { name: String, enable: bool },

    /// Prefill the composer input with the given text
    PrefillComposer(String),

    /// Submit a message with hidden preface instructions
    SubmitTextWithPreface { visible: String, preface: String },

    /// Run a review with an explicit prompt/hint pair (used by TUI selections)
    RunReviewWithScope {
        prompt: String,
        hint: String,
        preparation_label: Option<String>,
        metadata: Option<ReviewContextMetadata>,
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

    /// Update the theme (with history event)
    UpdateTheme(ThemeName),
    /// Add or update a subagent command in memory (UI already persisted to config.toml)
    UpdateSubagentCommand(codex_core::config_types::SubagentCommandConfig),
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
    PreviewTheme(ThemeName),
    /// Update the loading spinner style (with history event)
    UpdateSpinner(String),
    /// Preview loading spinner (no history event)
    PreviewSpinner(String),
    /// Rotate access/safety preset (Read Only → Write with Approval → Full Access)
    CycleAccessMode,
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
    JumpBack { nth: usize, prefill: String },
    /// Result of an async jump-back fork operation performed off the UI thread.
    /// Carries the forked conversation, trimmed prefix to replay, and composer prefill.
    JumpBackForked {
        cfg: codex_core::config::Config,
        new_conv: Redacted<codex_core::NewConversation>,
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
    },
    
}

// No helper constructor; use `AppEvent::CodexEvent(ev)` directly to avoid shadowing.
