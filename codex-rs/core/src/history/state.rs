use crate::parse_command::ParsedCommand;
use crate::protocol::{FileChange, RateLimitSnapshot, RateLimitWindow, TokenUsage};
use codex_protocol::plan_tool::StepStatus;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum HistoryRecord {
    PlainMessage(PlainMessageState),
    WaitStatus(WaitStatusState),
    Loading(LoadingState),
    RunningTool(RunningToolState),
    ToolCall(ToolCallState),
    PlanUpdate(PlanUpdateState),
    UpgradeNotice(UpgradeNoticeState),
    Reasoning(ReasoningState),
    Exec(ExecRecord),
    MergedExec(MergedExecRecord),
    AssistantStream(AssistantStreamState),
    AssistantMessage(AssistantMessageState),
    Diff(DiffRecord),
    Image(ImageRecord),
    Explore(ExploreRecord),
    RateLimits(RateLimitsRecord),
    Patch(PatchRecord),
    BackgroundEvent(BackgroundEventRecord),
    Notice(NoticeRecord),
}

#[derive(Clone, Debug, PartialEq)]
pub enum HistoryDomainEvent {
    /// Insert a record at `index`. Prefer this over `HistoryEvent::Insert` so
    /// callers can work with domain-specific structs without manually setting
    /// `HistoryId`.
    Insert {
        index: usize,
        record: HistoryDomainRecord,
    },
    /// Replace the record at `index`.
    Replace {
        index: usize,
        record: HistoryDomainRecord,
    },
    /// Remove the record at `index`.
    Remove { index: usize },
    /// Push incremental exec stream output onto an existing record.
    UpdateExecStream {
        index: usize,
        stdout_chunk: Option<ExecStreamChunk>,
        stderr_chunk: Option<ExecStreamChunk>,
    },
    UpsertAssistantStream {
        stream_id: String,
        preview_markdown: String,
        delta: Option<AssistantStreamDelta>,
        metadata: Option<MessageMetadata>,
    },
    UpdateExecWait {
        index: usize,
        total_wait: Option<Duration>,
        wait_active: bool,
        notes: Vec<ExecWaitNote>,
    },
    StartExec {
        index: usize,
        call_id: Option<String>,
        command: Vec<String>,
        parsed: Vec<ParsedCommand>,
        action: ExecAction,
        started_at: SystemTime,
        working_dir: Option<PathBuf>,
        env: Vec<(String, String)>,
        tags: Vec<String>,
    },
    FinishExec {
        id: Option<HistoryId>,
        call_id: Option<String>,
        status: ExecStatus,
        exit_code: Option<i32>,
        completed_at: Option<SystemTime>,
        stdout_tail: Option<String>,
        stderr_tail: Option<String>,
        wait_total: Option<Duration>,
        wait_active: bool,
        wait_notes: Vec<ExecWaitNote>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum HistoryDomainRecord {
    Plain(PlainMessageState),
    WaitStatus(WaitStatusState),
    Loading(LoadingState),
    RunningTool(RunningToolState),
    ToolCall(ToolCallState),
    PlanUpdate(PlanUpdateState),
    UpgradeNotice(UpgradeNoticeState),
    Reasoning(ReasoningState),
    BackgroundEvent(BackgroundEventRecord),
    RateLimits(RateLimitsRecord),
    Exec(ExecRecord),
    MergedExec(MergedExecRecord),
    AssistantStream(AssistantStreamState),
    AssistantMessage(AssistantMessageState),
    Patch(PatchRecord),
    Image(ImageRecord),
    Diff(DiffRecord),
    Explore(ExploreRecord),
    Notice(NoticeRecord),
}

impl From<HistoryRecord> for HistoryDomainRecord {
    fn from(record: HistoryRecord) -> Self {
        match record {
            HistoryRecord::PlainMessage(state) => HistoryDomainRecord::Plain(state),
            HistoryRecord::WaitStatus(state) => HistoryDomainRecord::WaitStatus(state),
            HistoryRecord::Loading(state) => HistoryDomainRecord::Loading(state),
            HistoryRecord::RunningTool(state) => HistoryDomainRecord::RunningTool(state),
            HistoryRecord::ToolCall(state) => HistoryDomainRecord::ToolCall(state),
            HistoryRecord::PlanUpdate(state) => HistoryDomainRecord::PlanUpdate(state),
            HistoryRecord::UpgradeNotice(state) => HistoryDomainRecord::UpgradeNotice(state),
            HistoryRecord::Reasoning(state) => HistoryDomainRecord::Reasoning(state),
            HistoryRecord::Exec(state) => HistoryDomainRecord::Exec(state),
            HistoryRecord::MergedExec(state) => HistoryDomainRecord::MergedExec(state),
            HistoryRecord::AssistantStream(state) => HistoryDomainRecord::AssistantStream(state),
            HistoryRecord::AssistantMessage(state) => HistoryDomainRecord::AssistantMessage(state),
            HistoryRecord::Diff(state) => HistoryDomainRecord::Diff(state),
            HistoryRecord::Image(state) => HistoryDomainRecord::Image(state),
            HistoryRecord::Explore(state) => HistoryDomainRecord::Explore(state),
            HistoryRecord::RateLimits(state) => HistoryDomainRecord::RateLimits(state),
            HistoryRecord::Patch(state) => HistoryDomainRecord::Patch(state),
            HistoryRecord::BackgroundEvent(state) => HistoryDomainRecord::BackgroundEvent(state),
            HistoryRecord::Notice(state) => HistoryDomainRecord::Notice(state),
        }
    }
}

impl From<PlainMessageState> for HistoryDomainRecord {
    fn from(state: PlainMessageState) -> Self {
        HistoryDomainRecord::Plain(state)
    }
}

impl From<WaitStatusState> for HistoryDomainRecord {
    fn from(state: WaitStatusState) -> Self {
        HistoryDomainRecord::WaitStatus(state)
    }
}

impl From<LoadingState> for HistoryDomainRecord {
    fn from(state: LoadingState) -> Self {
        HistoryDomainRecord::Loading(state)
    }
}

impl From<RunningToolState> for HistoryDomainRecord {
    fn from(state: RunningToolState) -> Self {
        HistoryDomainRecord::RunningTool(state)
    }
}

impl From<ToolCallState> for HistoryDomainRecord {
    fn from(state: ToolCallState) -> Self {
        HistoryDomainRecord::ToolCall(state)
    }
}

impl From<PlanUpdateState> for HistoryDomainRecord {
    fn from(state: PlanUpdateState) -> Self {
        HistoryDomainRecord::PlanUpdate(state)
    }
}

impl From<UpgradeNoticeState> for HistoryDomainRecord {
    fn from(state: UpgradeNoticeState) -> Self {
        HistoryDomainRecord::UpgradeNotice(state)
    }
}

impl From<ReasoningState> for HistoryDomainRecord {
    fn from(state: ReasoningState) -> Self {
        HistoryDomainRecord::Reasoning(state)
    }
}

impl From<BackgroundEventRecord> for HistoryDomainRecord {
    fn from(state: BackgroundEventRecord) -> Self {
        HistoryDomainRecord::BackgroundEvent(state)
    }
}

impl From<RateLimitsRecord> for HistoryDomainRecord {
    fn from(state: RateLimitsRecord) -> Self {
        HistoryDomainRecord::RateLimits(state)
    }
}

impl From<ExecRecord> for HistoryDomainRecord {
    fn from(state: ExecRecord) -> Self {
        HistoryDomainRecord::Exec(state)
    }
}

impl From<AssistantStreamState> for HistoryDomainRecord {
    fn from(state: AssistantStreamState) -> Self {
        HistoryDomainRecord::AssistantStream(state)
    }
}

impl From<AssistantMessageState> for HistoryDomainRecord {
    fn from(state: AssistantMessageState) -> Self {
        HistoryDomainRecord::AssistantMessage(state)
    }
}

impl From<PatchRecord> for HistoryDomainRecord {
    fn from(state: PatchRecord) -> Self {
        HistoryDomainRecord::Patch(state)
    }
}

impl From<ImageRecord> for HistoryDomainRecord {
    fn from(state: ImageRecord) -> Self {
        HistoryDomainRecord::Image(state)
    }
}

impl From<DiffRecord> for HistoryDomainRecord {
    fn from(state: DiffRecord) -> Self {
        HistoryDomainRecord::Diff(state)
    }
}

impl From<ExploreRecord> for HistoryDomainRecord {
    fn from(state: ExploreRecord) -> Self {
        HistoryDomainRecord::Explore(state)
    }
}

impl From<NoticeRecord> for HistoryDomainRecord {
    fn from(state: NoticeRecord) -> Self {
        HistoryDomainRecord::Notice(state)
    }
}

impl HistoryDomainRecord {
    fn into_history_record(self) -> HistoryRecord {
        match self {
            HistoryDomainRecord::Plain(mut state) => {
                state.id = HistoryId::ZERO;
                HistoryRecord::PlainMessage(state)
            }
            HistoryDomainRecord::WaitStatus(mut state) => {
                state.id = HistoryId::ZERO;
                HistoryRecord::WaitStatus(state)
            }
            HistoryDomainRecord::Loading(mut state) => {
                state.id = HistoryId::ZERO;
                HistoryRecord::Loading(state)
            }
            HistoryDomainRecord::RunningTool(mut state) => {
                state.id = HistoryId::ZERO;
                HistoryRecord::RunningTool(state)
            }
            HistoryDomainRecord::ToolCall(mut state) => {
                state.id = HistoryId::ZERO;
                HistoryRecord::ToolCall(state)
            }
            HistoryDomainRecord::PlanUpdate(mut state) => {
                state.id = HistoryId::ZERO;
                HistoryRecord::PlanUpdate(state)
            }
            HistoryDomainRecord::UpgradeNotice(mut state) => {
                state.id = HistoryId::ZERO;
                HistoryRecord::UpgradeNotice(state)
            }
            HistoryDomainRecord::Reasoning(mut state) => {
                state.id = HistoryId::ZERO;
                HistoryRecord::Reasoning(state)
            }
            HistoryDomainRecord::BackgroundEvent(mut state) => {
                state.id = HistoryId::ZERO;
                HistoryRecord::BackgroundEvent(state)
            }
            HistoryDomainRecord::RateLimits(mut state) => {
                state.id = HistoryId::ZERO;
                HistoryRecord::RateLimits(state)
            }
            HistoryDomainRecord::Exec(mut state) => {
                state.id = HistoryId::ZERO;
                HistoryRecord::Exec(state)
            }
            HistoryDomainRecord::MergedExec(mut state) => {
                state.id = HistoryId::ZERO;
                HistoryRecord::MergedExec(state)
            }
            HistoryDomainRecord::AssistantStream(mut state) => {
                state.id = HistoryId::ZERO;
                HistoryRecord::AssistantStream(state)
            }
            HistoryDomainRecord::AssistantMessage(mut state) => {
                state.id = HistoryId::ZERO;
                HistoryRecord::AssistantMessage(state)
            }
            HistoryDomainRecord::Patch(mut state) => {
                state.id = HistoryId::ZERO;
                HistoryRecord::Patch(state)
            }
            HistoryDomainRecord::Image(mut state) => {
                state.id = HistoryId::ZERO;
                HistoryRecord::Image(state)
            }
            HistoryDomainRecord::Diff(mut state) => {
                state.id = HistoryId::ZERO;
                HistoryRecord::Diff(state)
            }
            HistoryDomainRecord::Explore(mut state) => {
                state.id = HistoryId::ZERO;
                HistoryRecord::Explore(state)
            }
            HistoryDomainRecord::Notice(mut state) => {
                state.id = HistoryId::ZERO;
                HistoryRecord::Notice(state)
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlainMessageState {
    pub id: HistoryId,
    pub role: PlainMessageRole,
    pub kind: PlainMessageKind,
    pub header: Option<MessageHeader>,
    pub lines: Vec<MessageLine>,
    pub metadata: Option<MessageMetadata>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlainMessageKind {
    Plain,
    User,
    Assistant,
    Tool,
    Error,
    Background,
    Notice,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlainMessageRole {
    System,
    User,
    Assistant,
    Tool,
    Error,
    BackgroundEvent,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageHeader {
    pub label: String,
    pub badge: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageLine {
    pub kind: MessageLineKind,
    pub spans: Vec<InlineSpan>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageLineKind {
    Paragraph,
    Bullet { indent: u8, marker: BulletMarker },
    Code { language: Option<String> },
    Quote,
    Separator,
    Metadata,
    Blank,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BulletMarker {
    Dash,
    Numbered(u32),
    Custom(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InlineSpan {
    pub text: String,
    pub tone: TextTone,
    pub emphasis: TextEmphasis,
    pub entity: Option<TextEntity>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextTone {
    Default,
    Dim,
    Primary,
    Success,
    Warning,
    Error,
    Info,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextEmphasis {
    pub bold: bool,
    pub italic: bool,
    pub dim: bool,
    pub strike: bool,
    pub underline: bool,
}

impl Default for TextEmphasis {
    fn default() -> Self {
        Self {
            bold: false,
            italic: false,
            dim: false,
            strike: false,
            underline: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextEntity {
    Link { href: String },
    Code,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageMetadata {
    pub citations: Vec<String>,
    pub token_usage: Option<TokenUsage>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaitStatusState {
    pub id: HistoryId,
    pub header: WaitStatusHeader,
    pub details: Vec<WaitStatusDetail>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaitStatusHeader {
    pub title: String,
    pub title_tone: TextTone,
    pub summary: Option<String>,
    pub summary_tone: TextTone,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaitStatusDetail {
    pub label: String,
    pub value: Option<String>,
    pub tone: TextTone,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadingState {
    pub id: HistoryId,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallState {
    pub id: HistoryId,
    #[serde(default)]
    pub call_id: Option<String>,
    pub status: ToolStatus,
    pub title: String,
    pub duration: Option<Duration>,
    pub arguments: Vec<ToolArgument>,
    pub result_preview: Option<ToolResultPreview>,
    pub error_message: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolStatus {
    Running,
    Success,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolArgument {
    pub name: String,
    pub value: ArgumentValue,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArgumentValue {
    Text(String),
    Json(serde_json::Value),
    Secret,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolResultPreview {
    pub lines: Vec<String>,
    pub truncated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunningToolState {
    pub id: HistoryId,
    #[serde(default)]
    pub call_id: Option<String>,
    pub title: String,
    pub started_at: SystemTime,
    pub wait_cap_ms: Option<u64>,
    pub wait_has_target: bool,
    pub wait_has_call_id: bool,
    pub arguments: Vec<ToolArgument>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanUpdateState {
    pub id: HistoryId,
    pub name: String,
    pub icon: PlanIcon,
    pub progress: PlanProgress,
    pub steps: Vec<PlanStep>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanIcon {
    LightBulb,
    Rocket,
    Clipboard,
    Custom(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanProgress {
    pub completed: usize,
    pub total: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanStep {
    pub description: String,
    pub status: StepStatus,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpgradeNoticeState {
    pub id: HistoryId,
    pub current_version: String,
    pub latest_version: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReasoningState {
    pub id: HistoryId,
    pub sections: Vec<ReasoningSection>,
    pub effort: Option<ReasoningEffortLevel>,
    pub in_progress: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReasoningEffortLevel {
    Low,
    Medium,
    High,
    Custom(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReasoningSection {
    /// Optional heading rendered in bold at the top of the section.
    pub heading: Option<String>,
    /// Single-line preview used for collapsed summaries; derived from the first
    /// meaningful block (heading, bullet, paragraph, etc.).
    pub summary: Option<Vec<InlineSpan>>,
    /// Rich collection of blocks that fully describe the reasoning content.
    pub blocks: Vec<ReasoningBlock>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReasoningBlock {
    Paragraph(Vec<InlineSpan>),
    Bullet {
        indent: u8,
        marker: BulletMarker,
        spans: Vec<InlineSpan>,
    },
    Code {
        language: Option<String>,
        content: String,
    },
    Quote(Vec<InlineSpan>),
    Separator,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecRecord {
    pub id: HistoryId,
    #[serde(default)]
    pub call_id: Option<String>,
    pub command: Vec<String>,
    pub parsed: Vec<ParsedCommand>,
    pub action: ExecAction,
    pub status: ExecStatus,
    pub stdout_chunks: Vec<ExecStreamChunk>,
    pub stderr_chunks: Vec<ExecStreamChunk>,
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub wait_total: Option<Duration>,
    #[serde(default)]
    pub wait_active: bool,
    pub wait_notes: Vec<ExecWaitNote>,
    pub started_at: SystemTime,
    pub completed_at: Option<SystemTime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<PathBuf>,
    #[serde(default)]
    pub env: Vec<(String, String)>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MergedExecRecord {
    pub id: HistoryId,
    pub action: ExecAction,
    pub segments: Vec<ExecRecord>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecAction {
    Read,
    Search,
    List,
    Run,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecStatus {
    Running,
    Success,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecStreamChunk {
    pub offset: usize,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecWaitNote {
    pub message: String,
    pub tone: TextTone,
    pub timestamp: SystemTime,
}
fn stream_len(chunks: &[ExecStreamChunk]) -> usize {
    chunks
        .iter()
        .map(|chunk| chunk.offset.saturating_add(chunk.content.len()))
        .max()
        .unwrap_or(0)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssistantStreamDelta {
    pub delta: String,
    pub sequence: Option<u64>,
    pub received_at: SystemTime,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssistantStreamState {
    pub id: HistoryId,
    pub stream_id: String,
    pub preview_markdown: String,
    pub deltas: Vec<AssistantStreamDelta>,
    pub citations: Vec<String>,
    pub metadata: Option<MessageMetadata>,
    pub in_progress: bool,
    pub last_updated_at: SystemTime,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssistantMessageState {
    pub id: HistoryId,
    pub stream_id: Option<String>,
    pub markdown: String,
    pub citations: Vec<String>,
    pub metadata: Option<MessageMetadata>,
    pub token_usage: Option<TokenUsage>,
    pub created_at: SystemTime,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffRecord {
    pub id: HistoryId,
    pub title: String,
    pub hunks: Vec<DiffHunk>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffHunk {
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffLineKind {
    Context,
    Addition,
    Removal,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageRecord {
    pub id: HistoryId,
    pub source_path: Option<PathBuf>,
    pub alt_text: Option<String>,
    pub width: u16,
    pub height: u16,
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_len: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExploreRecord {
    pub id: HistoryId,
    pub entries: Vec<ExploreEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExploreEntry {
    pub action: ExecAction,
    pub summary: ExploreSummary,
    pub status: ExploreEntryStatus,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExploreSummary {
    Search {
        query: Option<String>,
        path: Option<String>,
    },
    List {
        path: Option<String>,
    },
    Read {
        display_path: String,
        annotation: Option<String>,
        range: Option<(u32, u32)>,
    },
    Command {
        display: String,
        annotation: Option<String>,
    },
    Fallback {
        text: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExploreEntryStatus {
    Running,
    Success,
    NotFound,
    Error { exit_code: Option<i32> },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RateLimitsRecord {
    pub id: HistoryId,
    pub snapshot: RateLimitSnapshot,
    pub legend: Vec<RateLimitLegendEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateLimitLegendEntry {
    pub label: String,
    pub description: String,
    pub tone: TextTone,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchRecord {
    pub id: HistoryId,
    pub patch_type: PatchEventType,
    pub changes: HashMap<PathBuf, FileChange>,
    pub failure: Option<PatchFailureMetadata>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PatchEventType {
    ApprovalRequest,
    ApplyBegin { auto_approved: bool },
    ApplySuccess,
    ApplyFailure,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchFailureMetadata {
    pub message: String,
    pub stdout_excerpt: Option<String>,
    pub stderr_excerpt: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackgroundEventRecord {
    pub id: HistoryId,
    pub title: String,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoticeRecord {
    pub id: HistoryId,
    pub title: Option<String>,
    pub body: Vec<MessageLine>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HistoryId(pub u64);

impl HistoryId {
    pub const ZERO: HistoryId = HistoryId(0);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderKeySnapshot {
    pub req: u64,
    pub out: i32,
    pub seq: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HistorySnapshot {
    pub records: Vec<HistoryRecord>,
    pub next_id: u64,
    #[serde(default)]
    pub exec_call_lookup: HashMap<String, HistoryId>,
    #[serde(default)]
    pub tool_call_lookup: HashMap<String, HistoryId>,
    #[serde(default)]
    pub stream_lookup: HashMap<String, HistoryId>,
    #[serde(default)]
    pub order: Vec<OrderKeySnapshot>,
    #[serde(default)]
    pub order_debug: Vec<Option<String>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HistoryState {
    pub records: Vec<HistoryRecord>,
    pub next_id: u64,
    #[serde(default)]
    pub exec_call_lookup: HashMap<String, HistoryId>,
    #[serde(default)]
    pub tool_call_lookup: HashMap<String, HistoryId>,
    #[serde(default)]
    pub stream_lookup: HashMap<String, HistoryId>,
    #[serde(skip)]
    id_index: HashMap<HistoryId, usize>,
}

#[allow(dead_code)]
impl HistoryState {
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
            next_id: 1,
            exec_call_lookup: HashMap::new(),
            tool_call_lookup: HashMap::new(),
            stream_lookup: HashMap::new(),
            id_index: HashMap::new(),
        }
    }

    pub fn push(&mut self, record: HistoryRecord) -> HistoryId {
        let id = self.next_history_id();
        let record = record.with_id(id);
        self.register_record(&record);
        self.records.push(record);
        self.rebuild_id_index();
        id
    }

    pub fn upsert_assistant_stream_state(
        &mut self,
        stream_id: &str,
        preview_markdown: String,
        delta: Option<AssistantStreamDelta>,
        metadata: Option<&MessageMetadata>,
    ) -> HistoryId {
        let event = HistoryDomainEvent::UpsertAssistantStream {
            stream_id: stream_id.to_string(),
            preview_markdown,
            delta,
            metadata: metadata.cloned(),
        };
        match self.apply_domain_event(event) {
            HistoryMutation::Inserted { id, .. } | HistoryMutation::Replaced { id, .. } => id,
            _ => HistoryId::ZERO,
        }
    }

    pub fn finalize_assistant_stream_state(
        &mut self,
        stream_id: Option<&str>,
        markdown: String,
        metadata: Option<&MessageMetadata>,
        token_usage: Option<&TokenUsage>,
    ) -> AssistantMessageState {
        let mut carried_citations: Vec<String> = Vec::new();
        let mut carried_metadata: Option<MessageMetadata> = None;
        if let Some(stream_id) = stream_id {
            if let Some(idx) = self.records.iter().position(|record| match record {
                HistoryRecord::AssistantStream(state) => state.stream_id == stream_id,
                _ => false,
            }) {
                if let Some(HistoryRecord::AssistantStream(state)) = self.remove(idx) {
                    if !state.citations.is_empty() {
                        carried_citations = state.citations;
                    }
                    if carried_metadata.is_none() {
                        carried_metadata = state.metadata;
                    }
                }
            }
        }

        let citations = metadata
            .map(|meta| meta.citations.clone())
            .unwrap_or(carried_citations);
        let metadata = metadata.cloned().or(carried_metadata);
        let token_usage = token_usage
            .cloned()
            .or_else(|| metadata.as_ref().and_then(|meta| meta.token_usage.clone()));

        let mut state = AssistantMessageState {
            id: HistoryId::ZERO,
            stream_id: stream_id.map(|s| s.to_string()),
            markdown,
            citations,
            metadata,
            token_usage,
            created_at: SystemTime::now(),
        };
        let id = self.next_history_id();
        state.id = id;
        self.records
            .push(HistoryRecord::AssistantMessage(state.clone()));
        self.rebuild_id_index();
        state
    }

    pub fn assistant_stream_state(&self, stream_id: &str) -> Option<&AssistantStreamState> {
        self.records.iter().find_map(|record| match record {
            HistoryRecord::AssistantStream(state) if state.stream_id == stream_id => Some(state),
            _ => None,
        })
    }

    pub fn insert(&mut self, index: usize, record: HistoryRecord) -> HistoryId {
        let id = self.next_history_id();
        let record = record.with_id(id);
        self.register_record(&record);
        self.records.insert(index, record);
        self.rebuild_id_index();
        id
    }

    pub fn remove(&mut self, index: usize) -> Option<HistoryRecord> {
        if index < self.records.len() {
            let record = self.records.remove(index);
            self.unregister_record(&record);
            self.rebuild_id_index();
            Some(record)
        } else {
            None
        }
    }

    pub fn get(&self, index: usize) -> Option<&HistoryRecord> {
        self.records.get(index)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut HistoryRecord> {
        self.records.get_mut(index)
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn snapshot(&self) -> HistorySnapshot {
        HistorySnapshot {
            records: self.records.clone(),
            next_id: self.next_id,
            exec_call_lookup: self.exec_call_lookup.clone(),
            tool_call_lookup: self.tool_call_lookup.clone(),
            stream_lookup: self.stream_lookup.clone(),
            order: Vec::new(),
            order_debug: Vec::new(),
        }
    }

    pub fn restore(&mut self, snapshot: &HistorySnapshot) {
        self.records = snapshot.records.clone();
        self.next_id = snapshot.next_id;
        self.exec_call_lookup = snapshot.exec_call_lookup.clone();
        self.tool_call_lookup = snapshot.tool_call_lookup.clone();
        self.stream_lookup = snapshot.stream_lookup.clone();
        if self.exec_call_lookup.is_empty()
            && self.tool_call_lookup.is_empty()
            && self.stream_lookup.is_empty()
        {
            self.rebuild_lookup_maps();
        }
        self.rebuild_id_index();
    }

    pub fn truncate_after(&mut self, id: HistoryId) -> Vec<HistoryRecord> {
        if id == HistoryId::ZERO {
            let removed = std::mem::take(&mut self.records);
            self.exec_call_lookup.clear();
            self.tool_call_lookup.clear();
            self.stream_lookup.clear();
            self.next_id = 1;
            return removed;
        }

        let Some(pos) = self.records.iter().position(|record| record.id() == id) else {
            return Vec::new();
        };

        if pos + 1 >= self.records.len() {
            return Vec::new();
        }

        let removed = self.records.split_off(pos + 1);
        for record in &removed {
            self.unregister_record(record);
        }
        self.recompute_next_id();
        self.rebuild_id_index();
        removed
    }

    pub fn history_id_for_exec_call(&self, call_id: &str) -> Option<HistoryId> {
        self.exec_call_lookup.get(call_id).copied()
    }

    pub fn history_id_for_tool_call(&self, call_id: &str) -> Option<HistoryId> {
        self.tool_call_lookup.get(call_id).copied()
    }

    pub fn history_id_for_stream(&self, stream_id: &str) -> Option<HistoryId> {
        self.stream_lookup.get(stream_id).copied()
    }

    pub fn index_of(&self, id: HistoryId) -> Option<usize> {
        if id == HistoryId::ZERO {
            return None;
        }
        self.id_index.get(&id).copied()
    }

    pub fn record(&self, id: HistoryId) -> Option<&HistoryRecord> {
        self.index_of(id).and_then(|idx| self.records.get(idx))
    }

    pub fn record_mut(&mut self, id: HistoryId) -> Option<&mut HistoryRecord> {
        self.index_of(id).and_then(|idx| self.records.get_mut(idx))
    }

    fn register_record(&mut self, record: &HistoryRecord) {
        match record {
            HistoryRecord::Exec(state) => {
                if let Some(call_id) = state.call_id.as_ref() {
                    self.exec_call_lookup.insert(call_id.clone(), state.id);
                }
            }
            HistoryRecord::MergedExec(state) => {
                for segment in &state.segments {
                    if let Some(call_id) = segment.call_id.as_ref() {
                        self.exec_call_lookup.insert(call_id.clone(), state.id);
                    }
                }
            }
            HistoryRecord::RunningTool(state) => {
                if let Some(call_id) = state.call_id.as_ref() {
                    self.tool_call_lookup.insert(call_id.clone(), state.id);
                }
            }
            HistoryRecord::ToolCall(state) => {
                if let Some(call_id) = state.call_id.as_ref() {
                    self.tool_call_lookup.insert(call_id.clone(), state.id);
                }
            }
            HistoryRecord::AssistantStream(state) => {
                self.stream_lookup.insert(state.stream_id.clone(), state.id);
            }
            _ => {}
        }
    }

    fn unregister_record(&mut self, record: &HistoryRecord) {
        match record {
            HistoryRecord::Exec(state) => {
                if let Some(call_id) = state.call_id.as_ref() {
                    if self
                        .exec_call_lookup
                        .get(call_id)
                        .is_some_and(|id| *id == state.id)
                    {
                        self.exec_call_lookup.remove(call_id);
                    }
                }
            }
            HistoryRecord::MergedExec(state) => {
                for segment in &state.segments {
                    if let Some(call_id) = segment.call_id.as_ref() {
                        if self
                            .exec_call_lookup
                            .get(call_id)
                            .is_some_and(|id| *id == state.id)
                        {
                            self.exec_call_lookup.remove(call_id);
                        }
                    }
                }
            }
            HistoryRecord::RunningTool(state) => {
                if let Some(call_id) = state.call_id.as_ref() {
                    if self
                        .tool_call_lookup
                        .get(call_id)
                        .is_some_and(|id| *id == state.id)
                    {
                        self.tool_call_lookup.remove(call_id);
                    }
                }
            }
            HistoryRecord::ToolCall(state) => {
                if let Some(call_id) = state.call_id.as_ref() {
                    if self
                        .tool_call_lookup
                        .get(call_id)
                        .is_some_and(|id| *id == state.id)
                    {
                        self.tool_call_lookup.remove(call_id);
                    }
                }
            }
            HistoryRecord::AssistantStream(state) => {
                if self
                    .stream_lookup
                    .get(&state.stream_id)
                    .is_some_and(|id| *id == state.id)
                {
                    self.stream_lookup.remove(&state.stream_id);
                }
            }
            _ => {}
        }
    }

    fn rebuild_lookup_maps(&mut self) {
        self.exec_call_lookup.clear();
        self.tool_call_lookup.clear();
        self.stream_lookup.clear();
        let snapshot = self.records.clone();
        for record in &snapshot {
            self.register_record(record);
        }
    }

    fn rebuild_id_index(&mut self) {
        self.id_index.clear();
        for (idx, record) in self.records.iter().enumerate() {
            let id = record.id();
            if id != HistoryId::ZERO {
                self.id_index.insert(id, idx);
            }
        }
    }

    fn next_history_id(&mut self) -> HistoryId {
        let id = HistoryId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        id
    }

    fn recompute_next_id(&mut self) {
        let next = self
            .records
            .iter()
            .map(|record| record.id().0)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        self.next_id = next;
    }

    pub fn apply_event(&mut self, event: HistoryEvent) -> HistoryMutation {
        let mutation = match event {
            HistoryEvent::Insert { index, record } => {
                let id = self.next_history_id();
                let record = record.with_id(id);
                let idx = index.min(self.records.len());
                self.records.insert(idx, record.clone());
                self.register_record(&record);
                HistoryMutation::Inserted {
                    index: idx,
                    id,
                    record,
                }
            }
            HistoryEvent::Replace { index, record } => {
                if let Some(existing) = self.records.get(index).cloned() {
                    let id = existing.id();
                    let record = record.with_id(id);
                    self.unregister_record(&existing);
                    self.records[index] = record.clone();
                    self.register_record(&record);
                    HistoryMutation::Replaced { index, id, record }
                } else {
                    HistoryMutation::Noop
                }
            }
            HistoryEvent::Remove { index } => {
                if index < self.records.len() {
                    let record = self.records.remove(index);
                    self.unregister_record(&record);
                    let id = record.id();
                    HistoryMutation::Removed { index, id, record }
                } else {
                    HistoryMutation::Noop
                }
            }
        };

        if !matches!(mutation, HistoryMutation::Noop) {
            self.rebuild_id_index();
        }

        mutation
    }

    pub fn apply_domain_event(&mut self, event: HistoryDomainEvent) -> HistoryMutation {
        match event {
            HistoryDomainEvent::Insert { index, record } => {
                let record = record.into_history_record();
                self.apply_event(HistoryEvent::Insert { index, record })
            }
            HistoryDomainEvent::Replace { index, record } => {
                let record = record.into_history_record();
                self.apply_event(HistoryEvent::Replace { index, record })
            }
            HistoryDomainEvent::Remove { index } => {
                self.apply_event(HistoryEvent::Remove { index })
            }
            HistoryDomainEvent::UpdateExecStream {
                index,
                stdout_chunk,
                stderr_chunk,
            } => {
                if let Some(HistoryRecord::Exec(existing)) = self.records.get(index).cloned() {
                    let mut updated = existing;
                    if let Some(chunk) = stdout_chunk {
                        updated.stdout_chunks.push(chunk);
                    }
                    if let Some(chunk) = stderr_chunk {
                        updated.stderr_chunks.push(chunk);
                    }
                    self.apply_event(HistoryEvent::Replace {
                        index,
                        record: HistoryRecord::Exec(updated),
                    })
                } else {
                    HistoryMutation::Noop
                }
            }
            HistoryDomainEvent::UpsertAssistantStream {
                stream_id,
                preview_markdown,
                delta,
                metadata,
            } => {
                let now = SystemTime::now();
                if let Some(idx) = self.records.iter().position(|record| {
                    matches!(record,
                        HistoryRecord::AssistantStream(state) if state.stream_id == stream_id)
                }) {
                    if let Some(HistoryRecord::AssistantStream(existing)) =
                        self.records.get(idx).cloned()
                    {
                        let mut updated = existing;
                        if let Some(delta) = delta {
                            updated.deltas.push(delta);
                        }
                        updated.preview_markdown = preview_markdown;
                        if let Some(meta) = metadata.clone() {
                            updated.citations = meta.citations.clone();
                            updated.metadata = Some(meta);
                        }
                        updated.in_progress = true;
                        updated.last_updated_at = now;
                        return self.apply_event(HistoryEvent::Replace {
                            index: idx,
                            record: HistoryRecord::AssistantStream(updated),
                        });
                    }
                }

                let mut deltas = Vec::new();
                if let Some(delta) = delta {
                    deltas.push(delta);
                }
                let citations = metadata
                    .as_ref()
                    .map(|meta| meta.citations.clone())
                    .unwrap_or_default();
                let assistant_state = AssistantStreamState {
                    id: HistoryId::ZERO,
                    stream_id,
                    preview_markdown,
                    deltas,
                    citations,
                    metadata,
                    in_progress: true,
                    last_updated_at: now,
                };
                let record = HistoryRecord::AssistantStream(assistant_state);
                self.apply_event(HistoryEvent::Insert {
                    index: self.records.len(),
                    record,
                })
            }
            HistoryDomainEvent::UpdateExecWait {
                index,
                total_wait,
                wait_active,
                notes,
            } => {
                if let Some(HistoryRecord::Exec(existing)) = self.records.get(index).cloned() {
                    let mut updated = existing;
                    updated.wait_total = total_wait;
                    updated.wait_active = wait_active;
                    updated.wait_notes = notes;
                    self.apply_event(HistoryEvent::Replace {
                        index,
                        record: HistoryRecord::Exec(updated),
                    })
                } else {
                    HistoryMutation::Noop
                }
            }
            HistoryDomainEvent::StartExec {
                index,
                call_id,
                command,
                parsed,
                action,
                started_at,
                working_dir,
                env,
                tags,
            } => {
                let insert_index = index.min(self.records.len());
                let record = ExecRecord {
                    id: HistoryId::ZERO,
                    call_id,
                    command,
                    parsed,
                    action,
                    status: ExecStatus::Running,
                    stdout_chunks: Vec::new(),
                    stderr_chunks: Vec::new(),
                    exit_code: None,
                    wait_total: None,
                    wait_active: false,
                    wait_notes: Vec::new(),
                    started_at,
                    completed_at: None,
                    working_dir,
                    env,
                    tags,
                };
                self.apply_event(HistoryEvent::Insert {
                    index: insert_index,
                    record: HistoryRecord::Exec(record),
                })
            }
            HistoryDomainEvent::FinishExec {
                id,
                call_id,
                status,
                exit_code,
                completed_at,
                wait_total,
                wait_active,
                wait_notes,
                stdout_tail,
                stderr_tail,
            } => {
                let mut target_idx = id.and_then(|hid| self.index_of(hid));
                if target_idx.is_none() {
                    if let Some(call_id) = call_id.as_ref() {
                        if let Some(mapped_id) = self.history_id_for_exec_call(call_id) {
                            target_idx = self.index_of(mapped_id);
                        }
                    }
                }

                if let Some(idx) = target_idx {
                    if let Some(HistoryRecord::Exec(existing)) = self.records.get(idx).cloned() {
                        let mut updated = existing;
                        updated.status = status;
                        updated.exit_code = exit_code;
                        updated.completed_at = completed_at;
                        updated.wait_total = wait_total;
                        updated.wait_active = wait_active;
                        updated.wait_notes = wait_notes;

                        if let Some(tail) = stdout_tail {
                            if !tail.is_empty() {
                                let offset = stream_len(&updated.stdout_chunks);
                                updated.stdout_chunks.push(ExecStreamChunk {
                                    offset,
                                    content: tail,
                                });
                            }
                        }
                        if let Some(tail) = stderr_tail {
                            if !tail.is_empty() {
                                let offset = stream_len(&updated.stderr_chunks);
                                updated.stderr_chunks.push(ExecStreamChunk {
                                    offset,
                                    content: tail,
                                });
                            }
                        }

                        self.apply_event(HistoryEvent::Replace {
                            index: idx,
                            record: HistoryRecord::Exec(updated),
                        })
                    } else {
                        HistoryMutation::Noop
                    }
                } else {
                    HistoryMutation::Noop
                }
            }
        }
    }
}

impl HistorySnapshot {
    pub fn with_order(
        mut self,
        order: Vec<OrderKeySnapshot>,
        order_debug: Vec<Option<String>>,
    ) -> Self {
        self.order = order;
        self.order_debug = order_debug;
        self
    }
}

#[allow(dead_code)]
trait WithId {
    fn with_id(self, id: HistoryId) -> HistoryRecord;
}

impl WithId for HistoryRecord {
    fn with_id(self, id: HistoryId) -> HistoryRecord {
        match self {
            HistoryRecord::PlainMessage(mut state) => {
                state.id = id;
                HistoryRecord::PlainMessage(state)
            }
            HistoryRecord::WaitStatus(mut state) => {
                state.id = id;
                HistoryRecord::WaitStatus(state)
            }
            HistoryRecord::Loading(mut state) => {
                state.id = id;
                HistoryRecord::Loading(state)
            }
            HistoryRecord::RunningTool(mut state) => {
                state.id = id;
                HistoryRecord::RunningTool(state)
            }
            HistoryRecord::ToolCall(mut state) => {
                state.id = id;
                HistoryRecord::ToolCall(state)
            }
            HistoryRecord::PlanUpdate(mut state) => {
                state.id = id;
                HistoryRecord::PlanUpdate(state)
            }
            HistoryRecord::UpgradeNotice(mut state) => {
                state.id = id;
                HistoryRecord::UpgradeNotice(state)
            }
            HistoryRecord::Reasoning(mut state) => {
                state.id = id;
                HistoryRecord::Reasoning(state)
            }
            HistoryRecord::Exec(mut state) => {
                state.id = id;
                HistoryRecord::Exec(state)
            }
            HistoryRecord::MergedExec(mut state) => {
                state.id = id;
                HistoryRecord::MergedExec(state)
            }
            HistoryRecord::AssistantStream(mut state) => {
                state.id = id;
                HistoryRecord::AssistantStream(state)
            }
            HistoryRecord::AssistantMessage(mut state) => {
                state.id = id;
                HistoryRecord::AssistantMessage(state)
            }
            HistoryRecord::Diff(mut state) => {
                state.id = id;
                HistoryRecord::Diff(state)
            }
            HistoryRecord::Image(mut state) => {
                state.id = id;
                HistoryRecord::Image(state)
            }
            HistoryRecord::Explore(mut state) => {
                state.id = id;
                HistoryRecord::Explore(state)
            }
            HistoryRecord::RateLimits(mut state) => {
                state.id = id;
                HistoryRecord::RateLimits(state)
            }
            HistoryRecord::Patch(mut state) => {
                state.id = id;
                HistoryRecord::Patch(state)
            }
            HistoryRecord::BackgroundEvent(mut state) => {
                state.id = id;
                HistoryRecord::BackgroundEvent(state)
            }
            HistoryRecord::Notice(mut state) => {
                state.id = id;
                HistoryRecord::Notice(state)
            }
        }
    }
}

impl HistoryRecord {
    pub fn id(&self) -> HistoryId {
        match self {
            HistoryRecord::PlainMessage(state) => state.id,
            HistoryRecord::WaitStatus(state) => state.id,
            HistoryRecord::Loading(state) => state.id,
            HistoryRecord::RunningTool(state) => state.id,
            HistoryRecord::ToolCall(state) => state.id,
            HistoryRecord::PlanUpdate(state) => state.id,
            HistoryRecord::UpgradeNotice(state) => state.id,
            HistoryRecord::Reasoning(state) => state.id,
            HistoryRecord::Exec(state) => state.id,
            HistoryRecord::MergedExec(state) => state.id,
            HistoryRecord::AssistantStream(state) => state.id,
            HistoryRecord::AssistantMessage(state) => state.id,
            HistoryRecord::Diff(state) => state.id,
            HistoryRecord::Image(state) => state.id,
            HistoryRecord::Explore(state) => state.id,
            HistoryRecord::RateLimits(state) => state.id,
            HistoryRecord::Patch(state) => state.id,
            HistoryRecord::BackgroundEvent(state) => state.id,
            HistoryRecord::Notice(state) => state.id,
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum HistoryEvent {
    Insert { index: usize, record: HistoryRecord },
    Replace { index: usize, record: HistoryRecord },
    Remove { index: usize },
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum HistoryMutation {
    Inserted {
        index: usize,
        id: HistoryId,
        record: HistoryRecord,
    },
    Replaced {
        index: usize,
        id: HistoryId,
        record: HistoryRecord,
    },
    Removed {
        index: usize,
        id: HistoryId,
        record: HistoryRecord,
    },
    Noop,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};

    fn plain_message(text: &str) -> HistoryRecord {
        HistoryRecord::PlainMessage(PlainMessageState {
            id: HistoryId::ZERO,
            role: PlainMessageRole::User,
            kind: PlainMessageKind::User,
            header: None,
            lines: vec![MessageLine {
                kind: MessageLineKind::Paragraph,
                spans: vec![InlineSpan {
                    text: text.to_string(),
                    tone: TextTone::Default,
                    emphasis: TextEmphasis::default(),
                    entity: None,
                }],
            }],
            metadata: None,
        })
    }

    fn zero_history_id(record: &mut HistoryRecord) {
        match record {
            HistoryRecord::PlainMessage(state) => state.id = HistoryId::ZERO,
            HistoryRecord::WaitStatus(state) => state.id = HistoryId::ZERO,
            HistoryRecord::Loading(state) => state.id = HistoryId::ZERO,
            HistoryRecord::RunningTool(state) => state.id = HistoryId::ZERO,
            HistoryRecord::ToolCall(state) => state.id = HistoryId::ZERO,
            HistoryRecord::PlanUpdate(state) => state.id = HistoryId::ZERO,
            HistoryRecord::UpgradeNotice(state) => state.id = HistoryId::ZERO,
            HistoryRecord::Reasoning(state) => state.id = HistoryId::ZERO,
            HistoryRecord::Exec(state) => state.id = HistoryId::ZERO,
            HistoryRecord::MergedExec(state) => state.id = HistoryId::ZERO,
            HistoryRecord::AssistantStream(state) => state.id = HistoryId::ZERO,
            HistoryRecord::AssistantMessage(state) => state.id = HistoryId::ZERO,
            HistoryRecord::Diff(state) => state.id = HistoryId::ZERO,
            HistoryRecord::Image(state) => state.id = HistoryId::ZERO,
            HistoryRecord::Explore(state) => state.id = HistoryId::ZERO,
            HistoryRecord::RateLimits(state) => state.id = HistoryId::ZERO,
            HistoryRecord::Patch(state) => state.id = HistoryId::ZERO,
            HistoryRecord::BackgroundEvent(state) => state.id = HistoryId::ZERO,
            HistoryRecord::Notice(state) => state.id = HistoryId::ZERO,
        }
    }

    #[test]
    fn exec_start_inserts_and_maps_id() {
        let mut state = HistoryState::new();
        let started_at = SystemTime::UNIX_EPOCH;
        let mutation = state.apply_domain_event(HistoryDomainEvent::StartExec {
            index: state.records.len(),
            call_id: Some("call-1".into()),
            command: vec!["echo".into(), "hi".into()],
            parsed: Vec::new(),
            action: ExecAction::Run,
            started_at,
            working_dir: Some(PathBuf::from("/tmp")),
            env: vec![("KEY".into(), "VAL".into())],
            tags: vec!["tag".into()],
        });

        let (new_id, index) = match mutation {
            HistoryMutation::Inserted { id, index, .. } => (id, index),
            other => panic!("unexpected mutation: {:?}", other),
        };

        assert_eq!(index, 0);
        assert_eq!(state.history_id_for_exec_call("call-1"), Some(new_id));
        let record = state.record(new_id).expect("exec record");
        match record {
            HistoryRecord::Exec(exec) => {
                assert_eq!(exec.command, vec!["echo", "hi"]);
                assert_eq!(exec.status, ExecStatus::Running);
                assert_eq!(exec.started_at, started_at);
                assert_eq!(exec.working_dir, Some(PathBuf::from("/tmp")));
                assert_eq!(exec.env, vec![("KEY".into(), "VAL".into())]);
                assert_eq!(exec.tags, vec![String::from("tag")]);
            }
            other => panic!("expected exec record, got {:?}", other),
        }
    }

    #[test]
    fn exec_finish_updates_status() {
        let mut state = HistoryState::new();
        let inserted_id = match state.apply_domain_event(HistoryDomainEvent::StartExec {
            index: state.records.len(),
            call_id: Some("call-2".into()),
            command: vec!["ls".into()],
            parsed: Vec::new(),
            action: ExecAction::Run,
            started_at: SystemTime::UNIX_EPOCH,
            working_dir: None,
            env: Vec::new(),
            tags: Vec::new(),
        }) {
            HistoryMutation::Inserted { id, .. } => id,
            other => panic!("unexpected mutation: {:?}", other),
        };

        let finish = state.apply_domain_event(HistoryDomainEvent::FinishExec {
            id: Some(inserted_id),
            call_id: None,
            status: ExecStatus::Success,
            exit_code: Some(0),
            completed_at: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(5)),
            wait_total: Some(Duration::from_secs(2)),
            wait_active: false,
            wait_notes: vec![ExecWaitNote {
                message: "done".into(),
                tone: TextTone::Info,
                timestamp: SystemTime::UNIX_EPOCH,
            }],
            stdout_tail: Some("output".into()),
            stderr_tail: Some("warn".into()),
        });

        assert!(matches!(finish, HistoryMutation::Replaced { id, .. } if id == inserted_id));

        let record = state.record(inserted_id).expect("exec record");
        match record {
            HistoryRecord::Exec(exec) => {
                assert_eq!(exec.status, ExecStatus::Success);
                assert_eq!(exec.exit_code, Some(0));
                assert_eq!(exec.wait_total, Some(Duration::from_secs(2)));
                assert_eq!(exec.wait_active, false);
                assert_eq!(exec.wait_notes.len(), 1);
                assert_eq!(
                    exec.stdout_chunks.last().map(|c| c.content.as_str()),
                    Some("output")
                );
                assert_eq!(
                    exec.stderr_chunks.last().map(|c| c.content.as_str()),
                    Some("warn")
                );
            }
            other => panic!("expected exec record, got {:?}", other),
        }
    }

    #[test]
    fn exec_start_snapshot_round_trip() {
        let mut state = HistoryState::new();
        let inserted_id = match state.apply_domain_event(HistoryDomainEvent::StartExec {
            index: state.records.len(),
            call_id: Some("call-3".into()),
            command: vec!["pwd".into()],
            parsed: Vec::new(),
            action: ExecAction::Run,
            started_at: SystemTime::UNIX_EPOCH,
            working_dir: Some(PathBuf::from("/work")),
            env: vec![("PWD".into(), "/work".into())],
            tags: vec![],
        }) {
            HistoryMutation::Inserted { id, .. } => id,
            other => panic!("unexpected mutation: {:?}", other),
        };

        let snapshot = state.snapshot();
        let mut restored = HistoryState::new();
        restored.restore(&snapshot);

        assert_eq!(
            restored.history_id_for_exec_call("call-3"),
            Some(inserted_id)
        );
        let record = restored.record(inserted_id).expect("restored exec");
        match record {
            HistoryRecord::Exec(exec) => {
                assert_eq!(exec.working_dir, Some(PathBuf::from("/work")));
                assert_eq!(exec.env, vec![("PWD".into(), "/work".into())]);
            }
            other => panic!("expected exec record, got {:?}", other),
        }
    }

    fn snapshot_and_restore_round_trip() {
        let mut state = HistoryState::new();
        let first_id = state.push(plain_message("first"));
        let second_id = state.push(plain_message("second"));
        assert_eq!(state.len(), 2);

        let snapshot = state.snapshot();

        let third_id = state.push(plain_message("third"));
        assert_eq!(state.len(), 3);
        assert!(third_id.0 > second_id.0);

        state.restore(&snapshot);
        assert_eq!(state.len(), 2);
        assert_eq!(state.next_id, snapshot.next_id);
        assert_eq!(state.records[0].id(), first_id);
        assert_eq!(state.records[1].id(), second_id);
    }

    #[test]
    fn truncate_after_removes_following_records() {
        let mut state = HistoryState::new();
        let first_id = state.push(plain_message("first"));
        let second_id = state.push(plain_message("second"));
        let third_id = state.push(plain_message("third"));
        assert!(first_id.0 < second_id.0 && second_id.0 < third_id.0);

        let removed = state.truncate_after(second_id);
        assert_eq!(removed.len(), 1);
        match &removed[0] {
            HistoryRecord::PlainMessage(st) => {
                assert_eq!(st.lines[0].spans[0].text, "third");
            }
            other => panic!("unexpected record removed: {:?}", other),
        }
        assert_eq!(state.len(), 2);
        assert_eq!(state.next_id, second_id.0.saturating_add(1));
    }

    #[test]
    fn truncate_after_zero_clears_state() {
        let mut state = HistoryState::new();
        state.push(plain_message("first"));
        state.push(plain_message("second"));

        let removed = state.truncate_after(HistoryId::ZERO);
        assert_eq!(removed.len(), 2);
        assert!(state.is_empty());
        assert_eq!(state.next_id, 1);
    }

    #[test]
    fn index_of_uses_cached_mapping() {
        let mut state = HistoryState::new();
        let first = state.push(plain_message("first"));
        let second = state.push(plain_message("second"));
        assert_eq!(state.index_of(first), Some(0));
        assert_eq!(state.index_of(second), Some(1));
        assert!(state.record(first).is_some());
        state.remove(0);
        assert_eq!(state.index_of(second), Some(0));
        assert!(state.index_of(first).is_none());
    }

    #[test]
    fn snapshot_json_round_trip() {
        let mut state = HistoryState::new();

        let user_id = state.push(plain_message("hello"));
        assert_ne!(user_id, HistoryId::ZERO);

        let exec_record = ExecRecord {
            id: HistoryId::ZERO,
            call_id: Some("call-123".into()),
            command: vec!["echo".into(), "hi".into()],
            parsed: Vec::new(),
            action: ExecAction::Run,
            status: ExecStatus::Success,
            stdout_chunks: vec![ExecStreamChunk {
                offset: 0,
                content: "hi".into(),
            }],
            stderr_chunks: Vec::new(),
            exit_code: Some(0),
            wait_total: Some(Duration::from_millis(10)),
            wait_active: false,
            wait_notes: vec![ExecWaitNote {
                message: "done".into(),
                tone: TextTone::Info,
                timestamp: SystemTime::UNIX_EPOCH,
            }],
            started_at: SystemTime::UNIX_EPOCH,
            completed_at: Some(SystemTime::UNIX_EPOCH),
            working_dir: None,
            env: Vec::new(),
            tags: Vec::new(),
        };
        let exec_id = state.push(HistoryRecord::Exec(exec_record));
        assert_ne!(exec_id, HistoryId::ZERO);

        let running_tool = RunningToolState {
            id: HistoryId::ZERO,
            call_id: Some("tool-1".into()),
            title: "Custom".into(),
            started_at: SystemTime::UNIX_EPOCH,
            wait_cap_ms: None,
            wait_has_target: false,
            wait_has_call_id: true,
            arguments: vec![ToolArgument {
                name: "arg".into(),
                value: ArgumentValue::Text("value".into()),
            }],
        };
        state.push(HistoryRecord::RunningTool(running_tool));

        let snapshot = state.snapshot();
        let json = serde_json::to_string(&snapshot).expect("snapshot serializes");
        let restored: HistorySnapshot = serde_json::from_str(&json).expect("snapshot deserializes");
        assert_eq!(snapshot, restored);

        let mut round_trip_state = HistoryState::new();
        round_trip_state.restore(&restored);

        assert_eq!(round_trip_state.records, state.records);
        assert_eq!(round_trip_state.exec_call_lookup, state.exec_call_lookup);
        assert_eq!(round_trip_state.tool_call_lookup, state.tool_call_lookup);
        assert_eq!(round_trip_state.stream_lookup, state.stream_lookup);
    }

    #[test]
    fn restore_rebuilds_lookup_and_index() {
        let mut state = HistoryState::new();

        let exec_id = state.push(HistoryRecord::Exec(ExecRecord {
            id: HistoryId::ZERO,
            call_id: Some("exec-call".into()),
            command: vec!["echo".into()],
            parsed: Vec::new(),
            action: ExecAction::Run,
            status: ExecStatus::Success,
            stdout_chunks: Vec::new(),
            stderr_chunks: Vec::new(),
            exit_code: Some(0),
            wait_total: None,
            wait_active: false,
            wait_notes: Vec::new(),
            started_at: SystemTime::UNIX_EPOCH,
            completed_at: Some(SystemTime::UNIX_EPOCH),
            working_dir: None,
            env: Vec::new(),
            tags: Vec::new(),
        }));

        let tool_id = state.push(HistoryRecord::ToolCall(ToolCallState {
            id: HistoryId::ZERO,
            call_id: Some("tool-call".into()),
            status: ToolStatus::Running,
            title: "tool".into(),
            duration: None,
            arguments: Vec::new(),
            result_preview: None,
            error_message: None,
        }));

        let stream_id = state.push(HistoryRecord::AssistantStream(AssistantStreamState {
            id: HistoryId::ZERO,
            stream_id: "stream-id".into(),
            preview_markdown: String::new(),
            deltas: Vec::new(),
            citations: Vec::new(),
            metadata: None,
            in_progress: true,
            last_updated_at: SystemTime::UNIX_EPOCH,
        }));

        let snapshot = state.snapshot();

        let mut restored = HistoryState::new();
        restored.restore(&snapshot);

        assert_eq!(
            restored.history_id_for_exec_call("exec-call"),
            Some(exec_id)
        );
        assert_eq!(
            restored.history_id_for_tool_call("tool-call"),
            Some(tool_id)
        );
        assert_eq!(restored.history_id_for_stream("stream-id"), Some(stream_id));
        assert_eq!(
            restored.index_of(exec_id),
            Some(
                snapshot
                    .records
                    .iter()
                    .position(|r| r.id() == exec_id)
                    .unwrap()
            )
        );
    }

    #[test]
    fn history_domain_record_round_trip_preserves_variants() {
        use std::mem::discriminant;

        let now = SystemTime::UNIX_EPOCH;
        let base_span = InlineSpan {
            text: "body".into(),
            tone: TextTone::Default,
            emphasis: TextEmphasis::default(),
            entity: None,
        };
        let metadata = MessageMetadata {
            citations: vec!["c1".into()],
            token_usage: Some(TokenUsage {
                input_tokens: 10,
                cached_input_tokens: 2,
                output_tokens: 5,
                reasoning_output_tokens: 1,
                total_tokens: 16,
            }),
        };

        let mut records: Vec<HistoryRecord> = Vec::new();

        let mut plain = plain_message("plain");
        if let HistoryRecord::PlainMessage(ref mut state) = plain {
            state.id = HistoryId(1);
        }
        records.push(plain);

        records.push(HistoryRecord::WaitStatus(WaitStatusState {
            id: HistoryId(2),
            header: WaitStatusHeader {
                title: "Wait".into(),
                title_tone: TextTone::Warning,
                summary: Some("pending".into()),
                summary_tone: TextTone::Info,
            },
            details: vec![WaitStatusDetail {
                label: "detail".into(),
                value: Some("value".into()),
                tone: TextTone::Default,
            }],
        }));

        records.push(HistoryRecord::Loading(LoadingState {
            id: HistoryId(3),
            message: "loading".into(),
        }));

        records.push(HistoryRecord::RunningTool(RunningToolState {
            id: HistoryId(4),
            call_id: Some("running-tool".into()),
            title: "Tool".into(),
            started_at: now,
            wait_cap_ms: Some(5000),
            wait_has_target: true,
            wait_has_call_id: true,
            arguments: vec![ToolArgument {
                name: "arg".into(),
                value: ArgumentValue::Text("value".into()),
            }],
        }));

        records.push(HistoryRecord::ToolCall(ToolCallState {
            id: HistoryId(5),
            call_id: Some("tool-call".into()),
            status: ToolStatus::Success,
            title: "ToolCall".into(),
            duration: Some(Duration::from_secs(2)),
            arguments: vec![ToolArgument {
                name: "arg".into(),
                value: ArgumentValue::Json(serde_json::json!({ "k": "v" })),
            }],
            result_preview: Some(ToolResultPreview {
                lines: vec!["ok".into()],
                truncated: false,
            }),
            error_message: None,
        }));

        records.push(HistoryRecord::PlanUpdate(PlanUpdateState {
            id: HistoryId(6),
            name: "Plan".into(),
            icon: PlanIcon::Rocket,
            progress: PlanProgress {
                completed: 1,
                total: 3,
            },
            steps: vec![PlanStep {
                description: "step".into(),
                status: StepStatus::InProgress,
            }],
        }));

        records.push(HistoryRecord::UpgradeNotice(UpgradeNoticeState {
            id: HistoryId(7),
            current_version: "1.0.0".into(),
            latest_version: "1.1.0".into(),
            message: "Upgrade available".into(),
        }));

        records.push(HistoryRecord::Reasoning(ReasoningState {
            id: HistoryId(8),
            sections: vec![ReasoningSection {
                heading: Some("Section".into()),
                summary: Some(vec![base_span.clone()]),
                blocks: vec![ReasoningBlock::Paragraph(vec![base_span.clone()])],
            }],
            effort: Some(ReasoningEffortLevel::Low),
            in_progress: false,
        }));

        records.push(HistoryRecord::Exec(ExecRecord {
            id: HistoryId(9),
            call_id: Some("exec".into()),
            command: vec!["echo".into()],
            parsed: Vec::new(),
            action: ExecAction::Run,
            status: ExecStatus::Running,
            stdout_chunks: vec![ExecStreamChunk {
                offset: 0,
                content: "out".into(),
            }],
            stderr_chunks: vec![ExecStreamChunk {
                offset: 0,
                content: "err".into(),
            }],
            exit_code: Some(0),
            wait_total: Some(Duration::from_secs(1)),
            wait_active: true,
            wait_notes: vec![ExecWaitNote {
                message: "note".into(),
                tone: TextTone::Info,
                timestamp: now,
            }],
            started_at: now,
            completed_at: Some(now),
            working_dir: None,
            env: Vec::new(),
            tags: Vec::new(),
        }));

        records.push(HistoryRecord::AssistantStream(AssistantStreamState {
            id: HistoryId(10),
            stream_id: "stream".into(),
            preview_markdown: "preview".into(),
            deltas: vec![AssistantStreamDelta {
                delta: "delta".into(),
                sequence: Some(1),
                received_at: now,
            }],
            citations: vec!["cite".into()],
            metadata: Some(metadata.clone()),
            in_progress: true,
            last_updated_at: now,
        }));

        records.push(HistoryRecord::AssistantMessage(AssistantMessageState {
            id: HistoryId(11),
            stream_id: Some("stream".into()),
            markdown: "final".into(),
            citations: vec!["cite".into()],
            metadata: Some(metadata.clone()),
            token_usage: metadata.token_usage.clone(),
            created_at: now,
        }));

        records.push(HistoryRecord::Diff(DiffRecord {
            id: HistoryId(12),
            title: "Diff".into(),
            hunks: vec![DiffHunk {
                header: "@@".into(),
                lines: vec![DiffLine {
                    kind: DiffLineKind::Addition,
                    content: "+ line".into(),
                }],
            }],
        }));

        records.push(HistoryRecord::Image(ImageRecord {
            id: HistoryId(13),
            source_path: Some(PathBuf::from("image.png")),
            alt_text: Some("An image".into()),
            width: 640,
            height: 480,
            sha256: Some("hash".into()),
            mime_type: Some("image/png".into()),
            byte_len: Some(2048),
        }));

        records.push(HistoryRecord::Explore(ExploreRecord {
            id: HistoryId(14),
            entries: vec![ExploreEntry {
                action: ExecAction::Read,
                summary: ExploreSummary::Read {
                    display_path: "file".into(),
                    annotation: Some("note".into()),
                    range: Some((1, 2)),
                },
                status: ExploreEntryStatus::Success,
            }],
        }));

        records.push(HistoryRecord::RateLimits(RateLimitsRecord {
            id: HistoryId(15),
            snapshot: RateLimitSnapshot {
                primary: Some(RateLimitWindow {
                    used_percent: 10.0,
                    window_minutes: Some(1),
                    resets_in_seconds: Some(30),
                }),
                secondary: Some(RateLimitWindow {
                    used_percent: 20.0,
                    window_minutes: Some(5),
                    resets_in_seconds: Some(60),
                }),
            },
            legend: vec![RateLimitLegendEntry {
                label: "primary".into(),
                description: "desc".into(),
                tone: TextTone::Info,
            }],
        }));

        let mut patch_changes = HashMap::new();
        patch_changes.insert(
            PathBuf::from("src/main.rs"),
            FileChange::Add {
                content: "fn main() {}".into(),
            },
        );
        records.push(HistoryRecord::Patch(PatchRecord {
            id: HistoryId(16),
            patch_type: PatchEventType::ApplyBegin {
                auto_approved: true,
            },
            changes: patch_changes,
            failure: None,
        }));

        records.push(HistoryRecord::BackgroundEvent(BackgroundEventRecord {
            id: HistoryId(17),
            title: "Background".into(),
            description: "desc".into(),
        }));

        records.push(HistoryRecord::Notice(NoticeRecord {
            id: HistoryId(18),
            title: Some("Notice".into()),
            body: vec![MessageLine {
                kind: MessageLineKind::Paragraph,
                spans: vec![base_span.clone()],
            }],
        }));

        for (idx, record) in records.iter().cloned().enumerate() {
            let domain = HistoryDomainRecord::from(record.clone());
            let rebuilt = domain.clone().into_history_record();

            let mut expected = record.clone();
            zero_history_id(&mut expected);

            assert_eq!(
                discriminant(&record),
                discriminant(&rebuilt),
                "variant mismatch at index {}",
                idx
            );
            assert_eq!(rebuilt, expected, "record mismatch at index {}", idx);
        }
    }
}
