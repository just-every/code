use codex_core::parse_command::ParsedCommand;
use codex_core::plan_tool::StepStatus;
use codex_core::protocol::{FileChange, RateLimitSnapshotEvent, TokenUsage};
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlainMessageState {
    pub id: HistoryId,
    pub role: PlainMessageRole,
    pub header: Option<MessageHeader>,
    pub lines: Vec<MessageLine>,
    pub metadata: Option<MessageMetadata>,
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
    pub heading: Option<String>,
    pub blocks: Vec<ReasoningBlock>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReasoningBlock {
    Paragraph(Vec<InlineSpan>),
    Bullet {
        indent: u8,
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
    pub command: Vec<String>,
    pub parsed: Vec<ParsedCommand>,
    pub action: ExecAction,
    pub status: ExecStatus,
    pub stdout_chunks: Vec<ExecStreamChunk>,
    pub stderr_chunks: Vec<ExecStreamChunk>,
    pub exit_code: Option<i32>,
    pub wait_notes: Vec<ExecWaitNote>,
    pub started_at: SystemTime,
    pub completed_at: Option<SystemTime>,
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssistantStreamState {
    pub id: HistoryId,
    pub stream_id: String,
    pub markdown: String,
    pub citations: Vec<String>,
    pub in_progress: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssistantMessageState {
    pub id: HistoryId,
    pub markdown: String,
    pub citations: Vec<String>,
    pub metadata: Option<MessageMetadata>,
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
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExploreRecord {
    pub id: HistoryId,
    pub title: String,
    pub entries: Vec<ExploreEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExploreEntry {
    pub label: String,
    pub status: ExploreEntryStatus,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExploreEntryStatus {
    Pending,
    Running,
    Success,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RateLimitsRecord {
    pub id: HistoryId,
    pub snapshot: RateLimitSnapshotEvent,
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
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PatchEventType {
    ApprovalRequest,
    ApplyBegin { auto_approved: bool },
    ApplySuccess,
    ApplyFailure,
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HistoryState {
    pub records: Vec<HistoryRecord>,
    pub next_id: u64,
}

#[allow(dead_code)]
impl HistoryState {
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
            next_id: 1,
        }
    }

    pub fn push(&mut self, record: HistoryRecord) -> HistoryId {
        let id = self.next_history_id();
        self.records.push(record.with_id(id));
        id
    }

    pub fn insert(&mut self, index: usize, record: HistoryRecord) -> HistoryId {
        let id = self.next_history_id();
        self.records.insert(index, record.with_id(id));
        id
    }

    pub fn remove(&mut self, index: usize) -> Option<HistoryRecord> {
        if index < self.records.len() {
            Some(self.records.remove(index))
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

    fn next_history_id(&mut self) -> HistoryId {
        let id = HistoryId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        id
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
