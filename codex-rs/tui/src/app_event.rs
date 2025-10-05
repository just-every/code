use std::path::PathBuf;

use codex_common::model_presets::ModelPreset;
use codex_core::protocol::ConversationPathResponseEvent;
use codex_core::protocol::Event;
use codex_file_search::FileMatch;

use crate::bottom_pane::ApprovalRequest;
use crate::history_cell::HistoryCell;

use codex_core::protocol::AskForApproval;
use codex_core::protocol::SandboxPolicy;
use codex_core::protocol_config_types::ReasoningEffort;

#[allow(clippy::large_enum_variant)]
pub(crate) enum AppEvent {
    CodexEvent(Event),

    /// Start a new session.
    NewSession,

    /// Request to exit the application gracefully.
    ExitRequest,

    /// Forward an `Op` to the Agent. Using an `AppEvent` for this avoids
    /// bubbling channels through layers of widgets.
    CodexOp(codex_core::protocol::Op),

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
    DiffResult(String),

    InsertHistoryCell(Box<dyn HistoryCell>),

    StartCommitAnimation,
    StopCommitAnimation,
    CommitTick,

    /// Update the current reasoning effort in the running app and widget.
    UpdateReasoningEffort(Option<ReasoningEffort>),

    /// Update the current model slug in the running app and widget.
    UpdateModel(String),

    /// Persist the selected model and reasoning effort to the appropriate config.
    PersistModelSelection {
        model: String,
        effort: Option<ReasoningEffort>,
    },

    /// Open the reasoning selection popup after picking a model.
    OpenReasoningPopup {
        model: String,
        presets: Vec<ModelPreset>,
    },

    /// Update the current approval policy in the running app and widget.
    UpdateAskForApprovalPolicy(AskForApproval),

    /// Update the current sandbox policy in the running app and widget.
    UpdateSandboxPolicy(SandboxPolicy),

    /// Forwarded conversation history snapshot from the current conversation.
    ConversationHistory(ConversationPathResponseEvent),

    /// Open the branch picker option from the review popup.
    OpenReviewBranchPicker(PathBuf),

    /// Open the commit picker option from the review popup.
    OpenReviewCommitPicker(PathBuf),

    /// Open the custom prompt option from the review popup.
    OpenReviewCustomPrompt,

    /// Open the approval popup.
    FullScreenApprovalRequest(ApprovalRequest),
}

impl std::fmt::Debug for AppEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CodexEvent(arg0) => f.debug_tuple("CodexEvent").field(arg0).finish(),
            Self::NewSession => write!(f, "NewSession"),
            Self::ExitRequest => write!(f, "ExitRequest"),
            Self::CodexOp(arg0) => f.debug_tuple("CodexOp").field(arg0).finish(),
            Self::StartFileSearch(arg0) => f.debug_tuple("StartFileSearch").field(arg0).finish(),
            Self::FileSearchResult { query, matches } => f.debug_struct("FileSearchResult").field("query", query).field("matches", matches).finish(),
            Self::DiffResult(arg0) => f.debug_tuple("DiffResult").field(arg0).finish(),
            Self::InsertHistoryCell(_) => write!(f, "InsertHistoryCell(<HistoryCell>)"),
            Self::StartCommitAnimation => write!(f, "StartCommitAnimation"),
            Self::StopCommitAnimation => write!(f, "StopCommitAnimation"),
            Self::CommitTick => write!(f, "CommitTick"),
            Self::UpdateReasoningEffort(arg0) => f.debug_tuple("UpdateReasoningEffort").field(arg0).finish(),
            Self::UpdateModel(arg0) => f.debug_tuple("UpdateModel").field(arg0).finish(),
            Self::PersistModelSelection { model, effort } => f.debug_struct("PersistModelSelection").field("model", model).field("effort", effort).finish(),
            Self::OpenReasoningPopup { model, presets } => f.debug_struct("OpenReasoningPopup").field("model", model).field("presets", presets).finish(),
            Self::UpdateAskForApprovalPolicy(arg0) => f.debug_tuple("UpdateAskForApprovalPolicy").field(arg0).finish(),
            Self::UpdateSandboxPolicy(arg0) => f.debug_tuple("UpdateSandboxPolicy").field(arg0).finish(),
            Self::ConversationHistory(arg0) => f.debug_tuple("ConversationHistory").field(arg0).finish(),
            Self::OpenReviewBranchPicker(arg0) => f.debug_tuple("OpenReviewBranchPicker").field(arg0).finish(),
            Self::OpenReviewCommitPicker(arg0) => f.debug_tuple("OpenReviewCommitPicker").field(arg0).finish(),
            Self::OpenReviewCustomPrompt => write!(f, "OpenReviewCustomPrompt"),
            Self::FullScreenApprovalRequest(arg0) => f.debug_tuple("FullScreenApprovalRequest").field(arg0).finish(),
        }
    }
}
