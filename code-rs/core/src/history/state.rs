use crate::plan_tool::StepStatus;
use crate::parse_command::ParsedCommand;
use crate::protocol::{FileChange, RateLimitSnapshotEvent, TokenUsage};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
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
    Context(ContextRecord),
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
    Remove {
        index: usize,
    },
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
    Context(ContextRecord),
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
            HistoryRecord::Context(state) => HistoryDomainRecord::Context(state),
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

impl From<ContextRecord> for HistoryDomainRecord {
    fn from(state: ContextRecord) -> Self {
        HistoryDomainRecord::Context(state)
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
            HistoryDomainRecord::Context(mut state) => {
                state.id = HistoryId::ZERO;
                HistoryRecord::Context(state)
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
    Code { language: Option<String>, content: String },
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

fn retained_stream_len(chunks: &[ExecStreamChunk]) -> usize {
    if chunks.is_empty() {
        return 0;
    }
    let first_offset = chunks.first().map(|chunk| chunk.offset).unwrap_or(0);
    stream_len(chunks).saturating_sub(first_offset)
}

fn retained_exec_record_bytes(record: &ExecRecord) -> usize {
    retained_stream_len(&record.stdout_chunks).saturating_add(retained_stream_len(&record.stderr_chunks))
}

fn truncated_prefix_len(chunks: &[ExecStreamChunk]) -> usize {
    chunks.first().map(|chunk| chunk.offset).unwrap_or(0)
}

fn truncate_exec_stream(chunks: &mut Vec<ExecStreamChunk>, truncate_at: usize) {
    while let Some(last) = chunks.last_mut() {
        let last_start = last.offset;
        let last_end = last_start.saturating_add(last.content.len());
        if truncate_at >= last_end {
            break;
        }
        if truncate_at <= last_start {
            chunks.pop();
            continue;
        }
        let keep = truncate_at.saturating_sub(last_start);
        last.content.truncate(keep);
        break;
    }
}

fn append_exec_chunk(chunks: &mut Vec<ExecStreamChunk>, chunk: ExecStreamChunk) {
    truncate_exec_stream(chunks, chunk.offset);
    if let Some(last) = chunks.last_mut() {
        let last_end = last.offset.saturating_add(last.content.len());
        if chunk.offset == last_end {
            last.content.push_str(&chunk.content);
            prune_exec_stream(chunks, MAX_EXEC_STREAM_RETAINED_BYTES);
            return;
        }
    }
    chunks.push(chunk);
    prune_exec_stream(chunks, MAX_EXEC_STREAM_RETAINED_BYTES);
}

fn prune_exec_stream(chunks: &mut Vec<ExecStreamChunk>, max_bytes: usize) {
    if chunks.is_empty() {
        return;
    }

    let retained = retained_stream_len(chunks);
    if retained <= max_bytes {
        return;
    }

    let mut bytes_to_drop = retained.saturating_sub(max_bytes);
    let mut drop_chunks = 0usize;

    while drop_chunks < chunks.len() {
        let chunk_len = chunks[drop_chunks].content.len();
        if bytes_to_drop >= chunk_len {
            bytes_to_drop = bytes_to_drop.saturating_sub(chunk_len);
            drop_chunks += 1;
        } else {
            break;
        }
    }

    if drop_chunks > 0 {
        chunks.drain(..drop_chunks);
    }

    if bytes_to_drop > 0 {
        if let Some(first) = chunks.first_mut() {
            let drain = bytes_to_drop.min(first.content.len());
            first.offset = first.offset.saturating_add(drain);
            first.content.drain(..drain);
        }
    }
}

fn append_assistant_delta(deltas: &mut Vec<AssistantStreamDelta>, delta: AssistantStreamDelta) -> usize {
    if let Some(last) = deltas.last_mut() {
        if delta.sequence.is_some() && delta.sequence == last.sequence {
            last.delta.push_str(&delta.delta);
            return prune_assistant_stream(deltas, MAX_ASSISTANT_STREAM_RETAINED_BYTES);
        }
        if delta.sequence.is_none() && last.sequence.is_none() {
            last.delta.push_str(&delta.delta);
            return prune_assistant_stream(deltas, MAX_ASSISTANT_STREAM_RETAINED_BYTES);
        }
    }
    deltas.push(delta);
    prune_assistant_stream(deltas, MAX_ASSISTANT_STREAM_RETAINED_BYTES)
}

fn prune_assistant_stream(deltas: &mut Vec<AssistantStreamDelta>, max_bytes: usize) -> usize {
    if max_bytes == 0 {
        let dropped = deltas.iter().map(|d| d.delta.len()).sum();
        deltas.clear();
        return dropped;
    }

    let mut total: usize = deltas.iter().map(|d| d.delta.len()).sum();
    if total <= max_bytes {
        return 0;
    }

    let mut truncated = 0usize;
    while total > max_bytes && !deltas.is_empty() {
        if let Some(first) = deltas.first_mut() {
            let first_len = first.delta.len();
            if total.saturating_sub(first_len) >= max_bytes {
                let removed = deltas.remove(0);
                truncated = truncated.saturating_add(removed.delta.len());
                total = total.saturating_sub(first_len);
                continue;
            }

            let mut drain = total - max_bytes;
            drain = drain.min(first_len);
            while drain < first_len && !first.delta.is_char_boundary(drain) {
                drain += 1;
            }

            if drain >= first_len {
                let removed = deltas.remove(0);
                truncated = truncated.saturating_add(removed.delta.len());
                total = total.saturating_sub(first_len);
                continue;
            }

            first.delta.drain(..drain);
            truncated = truncated.saturating_add(drain);
            total = total.saturating_sub(drain);
        }
    }
    truncated
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
    #[serde(default)]
    pub truncated_prefix_bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssistantMessageState {
    pub id: HistoryId,
    pub stream_id: Option<String>,
    pub markdown: String,
    pub citations: Vec<String>,
    pub metadata: Option<MessageMetadata>,
    pub token_usage: Option<TokenUsage>,
    /// True when this assistant output was emitted before the final Answer
    /// within the same turn (e.g., mid‑turn progress update). Used by the TUI
    /// to suppress gutter/bold styling.
    #[serde(default)]
    pub mid_turn: bool,
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
    Count {
        target: Option<String>,
        annotation: Option<String>,
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextDeltaField {
    Cwd,
    GitBranch,
    ReasoningEffort,
    BrowserSnapshot,
}

impl Default for ContextDeltaField {
    fn default() -> Self {
        ContextDeltaField::Cwd
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextDeltaRecord {
    pub field: ContextDeltaField,
    pub previous: Option<String>,
    pub current: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextBrowserSnapshotRecord {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub captured_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ContextRecord {
    pub id: HistoryId,
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    pub reasoning_effort: Option<String>,
    pub browser_session_active: bool,
    pub deltas: Vec<ContextDeltaRecord>,
    pub browser_snapshot: Option<ContextBrowserSnapshotRecord>,
    pub expanded: bool,
}

impl Default for ContextRecord {
    fn default() -> Self {
        Self {
            id: HistoryId::ZERO,
            cwd: None,
            git_branch: None,
            reasoning_effort: None,
            browser_session_active: false,
            deltas: Vec::new(),
            browser_snapshot: None,
            expanded: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HistoryId(pub u64);

impl HistoryId {
    pub const ZERO: HistoryId = HistoryId(0);
}

const EXEC_STREAM_CHUNK_THRESHOLD: usize = 2048;
const EXEC_STREAM_CHUNK_STEP: usize = 256;
const EXEC_STREAM_BYTE_THRESHOLD: usize = 8 * 1024 * 1024;
const EXEC_STREAM_BYTE_STEP: usize = 2 * 1024 * 1024;

/// Maximum per-stream payload we retain in memory for exec stdout/stderr.
/// Older bytes are truncated from the front once this threshold is exceeded.
pub const MAX_EXEC_STREAM_RETAINED_BYTES: usize = 32 * 1024 * 1024; // 32 MiB

/// Global cap across *all* exec streams we keep in memory. When exceeded, we
/// progressively trim the oldest exec records down to a small tail to keep RSS
/// bounded during long Auto Drive runs with many noisy commands.
pub const GLOBAL_EXEC_STREAM_RETAINED_BYTES: usize = 256 * 1024 * 1024; // 256 MiB total

/// Target tail to keep per stream when pruning old exec records to honor the
/// global cap. Chosen to preserve recent context without retaining full logs.
pub const EXEC_STREAM_PRUNE_TARGET_BYTES: usize = 512 * 1024; // 512 KiB per stream

/// Maximum bytes retained in memory for assistant streaming output.
pub const MAX_ASSISTANT_STREAM_RETAINED_BYTES: usize = 6 * 1024 * 1024; // 6 MiB

const ASSISTANT_STREAM_CHUNK_THRESHOLD: usize = 2048;
const ASSISTANT_STREAM_CHUNK_STEP: usize = 256;
const ASSISTANT_STREAM_BYTE_THRESHOLD: usize = 6 * 1024 * 1024;
const ASSISTANT_STREAM_BYTE_STEP: usize = 1 * 1024 * 1024;

#[derive(Default, Clone, Debug, PartialEq, Eq)]
struct StreamLogState {
    last_chunk_log: usize,
    last_byte_log: usize,
    total_chunks: usize,
    total_bytes: usize,
    last_truncated_log: usize,
    truncated_bytes: usize,
}

#[derive(Default, Clone, Debug, PartialEq, Eq)]
struct HistoryUsageTracker {
    exec: HashMap<HistoryId, StreamLogState>,
    assistant: HashMap<HistoryId, StreamLogState>,
}

#[derive(Default, Clone, Debug, PartialEq, Eq)]
struct UsageTrackerSnapshot {
    exec: Option<StreamLogState>,
    assistant: Option<StreamLogState>,
}

impl HistoryUsageTracker {
    fn reset(&mut self) {
        self.exec.clear();
        self.assistant.clear();
    }

    fn on_insert(&mut self, record: &HistoryRecord) {
        match record {
            HistoryRecord::Exec(state) => {
                if state.id != HistoryId::ZERO {
                    let entry = self.exec.entry(state.id).or_default();
                    let chunk_count = state.stdout_chunks.len().saturating_add(state.stderr_chunks.len());
                    let byte_count = stream_len(&state.stdout_chunks)
                        .saturating_add(stream_len(&state.stderr_chunks));
                    entry.total_chunks = entry.total_chunks.max(chunk_count);
                    entry.total_bytes = entry.total_bytes.max(byte_count);
                }
            }
            HistoryRecord::AssistantStream(state) => {
                if state.id != HistoryId::ZERO {
                    let entry = self.assistant.entry(state.id).or_default();
                    let chunk_count = state.deltas.len();
                    let byte_count: usize = state.deltas.iter().map(|delta| delta.delta.len()).sum();
                    entry.total_chunks = entry.total_chunks.max(chunk_count);
                    entry.total_bytes = entry.total_bytes.max(byte_count);
                }
            }
            _ => {}
        }
    }

    fn on_remove(&mut self, id: HistoryId) {
        self.exec.remove(&id);
        self.assistant.remove(&id);
    }

    fn add_exec_delta(&mut self, id: HistoryId, chunk_count: usize, byte_count: usize) {
        if id == HistoryId::ZERO {
            return;
        }
        let entry = self.exec.entry(id).or_default();
        entry.total_chunks = entry.total_chunks.saturating_add(chunk_count);
        entry.total_bytes = entry.total_bytes.saturating_add(byte_count);
    }

    fn add_assistant_delta(&mut self, id: HistoryId, byte_count: usize) {
        if id == HistoryId::ZERO {
            return;
        }
        let entry = self.assistant.entry(id).or_default();
        entry.total_chunks = entry.total_chunks.saturating_add(1);
        entry.total_bytes = entry.total_bytes.saturating_add(byte_count);
    }

    fn take_snapshot(&mut self, record: &HistoryRecord) -> UsageTrackerSnapshot {
        match record {
            HistoryRecord::Exec(state) => UsageTrackerSnapshot {
                exec: self.exec.remove(&state.id),
                assistant: None,
            },
            HistoryRecord::AssistantStream(state) => UsageTrackerSnapshot {
                exec: None,
                assistant: self.assistant.remove(&state.id),
            },
            _ => UsageTrackerSnapshot::default(),
        }
    }

    fn restore_snapshot(&mut self, record: &HistoryRecord, snapshot: UsageTrackerSnapshot) {
        if let Some(state) = snapshot.exec {
            if let HistoryRecord::Exec(exec_record) = record {
                if exec_record.id != HistoryId::ZERO {
                    self.exec.insert(exec_record.id, state);
                }
            }
        }
        if let Some(state) = snapshot.assistant {
            if let HistoryRecord::AssistantStream(stream_record) = record {
                if stream_record.id != HistoryId::ZERO {
                    self.assistant.insert(stream_record.id, state);
                }
            }
        }
    }

    fn observe_exec(&mut self, record: &ExecRecord, label: &'static str) {
        if record.id == HistoryId::ZERO {
            return;
        }
        let stdout_chunks = record.stdout_chunks.len();
        let stderr_chunks = record.stderr_chunks.len();
        let observed_chunks = stdout_chunks.saturating_add(stderr_chunks);
        let stdout_bytes = stream_len(&record.stdout_chunks);
        let stderr_bytes = stream_len(&record.stderr_chunks);
        let stdout_retained = retained_stream_len(&record.stdout_chunks);
        let stderr_retained = retained_stream_len(&record.stderr_chunks);
        let stdout_truncated = truncated_prefix_len(&record.stdout_chunks);
        let stderr_truncated = truncated_prefix_len(&record.stderr_chunks);
        let observed_bytes = stdout_bytes.saturating_add(stderr_bytes);
        let state = self.exec.entry(record.id).or_default();
        state.total_chunks = state.total_chunks.max(observed_chunks);
        state.total_bytes = state.total_bytes.max(observed_bytes);
        state.truncated_bytes = state
            .truncated_bytes
            .max(stdout_truncated.saturating_add(stderr_truncated));
        let total_chunks = state.total_chunks;
        let total_bytes = state.total_bytes;

        let mut should_log = false;
        if total_chunks >= EXEC_STREAM_CHUNK_THRESHOLD
            && total_chunks >= state.last_chunk_log.saturating_add(EXEC_STREAM_CHUNK_STEP)
        {
            state.last_chunk_log = total_chunks;
            should_log = true;
        }
        if total_bytes >= EXEC_STREAM_BYTE_THRESHOLD
            && total_bytes >= state.last_byte_log.saturating_add(EXEC_STREAM_BYTE_STEP)
        {
            state.last_byte_log = total_bytes;
            should_log = true;
        }

        let truncated_bytes = stdout_truncated.saturating_add(stderr_truncated);
        if truncated_bytes > state.last_truncated_log {
            state.last_truncated_log = truncated_bytes;
            should_log = true;
        }

        if should_log {
            let preview = command_preview(&record.command);
            tracing::warn!(
                target = "codex::history::memory",
                %label,
                history_id = record.id.0,
                status = ?record.status,
                stdout_chunks,
                stderr_chunks,
                stdout_bytes,
                stderr_bytes,
                stdout_retained,
                stderr_retained,
                stdout_truncated,
                stderr_truncated,
                total_chunks,
                total_bytes,
                command = %preview,
                "exec stream buffers accumulating many chunks or bytes"
            );
        }
    }

    fn observe_assistant(&mut self, state: &AssistantStreamState, label: &'static str) {
        if state.id == HistoryId::ZERO {
            return;
        }
        let chunk_count = state.deltas.len().max(self
            .assistant
            .get(&state.id)
            .map(|entry| entry.total_chunks)
            .unwrap_or(0));
        let byte_count: usize = state.deltas.iter().map(|delta| delta.delta.len()).sum();
        let tracker = self.assistant.entry(state.id).or_default();
        tracker.total_chunks = tracker.total_chunks.max(chunk_count);
        tracker.total_bytes = tracker.total_bytes.max(byte_count);

        let mut should_log = false;
        if chunk_count >= ASSISTANT_STREAM_CHUNK_THRESHOLD
            && chunk_count >= tracker
                .last_chunk_log
                .saturating_add(ASSISTANT_STREAM_CHUNK_STEP)
        {
            tracker.last_chunk_log = chunk_count;
            should_log = true;
        }
        if byte_count >= ASSISTANT_STREAM_BYTE_THRESHOLD
            && byte_count
                >= tracker
                    .last_byte_log
                    .saturating_add(ASSISTANT_STREAM_BYTE_STEP)
        {
            tracker.last_byte_log = byte_count;
            should_log = true;
        }

        if should_log {
            let preview = assistant_preview(state);
            tracing::warn!(
                target = "codex::history::memory",
                %label,
                history_id = state.id.0,
                delta_chunks = chunk_count,
                delta_bytes = byte_count,
                preview = %preview,
                "assistant stream retaining many deltas"
            );
        }
    }
}

fn command_preview(command: &[String]) -> String {
    if command.is_empty() {
        return "<empty command>".to_string();
    }
    let joined = command.join(" ");
    truncate_display(&joined)
}

fn assistant_preview(state: &AssistantStreamState) -> String {
    if !state.preview_markdown.trim().is_empty() {
        return truncate_display(state.preview_markdown.trim());
    }
    if let Some(last) = state.deltas.last() {
        if !last.delta.trim().is_empty() {
            return truncate_display(last.delta.trim());
        }
    }
    "<empty preview>".to_string()
}

fn truncate_display(input: &str) -> String {
    const MAX_CHARS: usize = 80;
    let mut chars = input.chars();
    let mut preview = String::new();
    for _ in 0..MAX_CHARS {
        if let Some(ch) = chars.next() {
            preview.push(ch);
        } else {
            break;
        }
    }
    if chars.next().is_some() {
        preview.push_str("...");
    }
    preview
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
    #[serde(skip)]
    usage_tracker: HistoryUsageTracker,
    #[serde(skip)]
    exec_stream_retained_total_bytes: usize,
    #[serde(skip)]
    exec_stream_retained_by_id: HashMap<HistoryId, usize>,
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
            usage_tracker: HistoryUsageTracker::default(),
            exec_stream_retained_total_bytes: 0,
            exec_stream_retained_by_id: HashMap::new(),
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
            HistoryMutation::Inserted { id, .. }
            | HistoryMutation::Replaced { id, .. } => id,
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

        // When we cannot associate this final with a live stream id, avoid
        // duplicating an identical assistant message that was already inserted
        // earlier (e.g. during streaming). Instead, refresh the existing record
        // in place so snapshots remain deduplicated.
        if let Some(stream_id) = stream_id {
            if let Some(idx) = self.records.iter().rposition(|record| match record {
                HistoryRecord::AssistantMessage(state) => {
                    state.stream_id.as_deref() == Some(stream_id)
                }
                _ => false,
            }) {
                if let HistoryRecord::AssistantMessage(existing) = &mut self.records[idx] {
                    existing.markdown = markdown;
                    existing.citations = citations.clone();
                    existing.metadata = metadata.clone();
                    existing.token_usage = token_usage.clone();
                    existing.created_at = SystemTime::now();
                    return existing.clone();
                }
            }
        }

        let mut state = AssistantMessageState {
            id: HistoryId::ZERO,
            stream_id: stream_id.map(|s| s.to_string()),
            markdown,
            citations,
            metadata,
            token_usage,
            mid_turn: false,
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
        let mut records = snapshot.records.clone();

        // Older snapshots may contain duplicate assistant messages with the same stream id
        // (e.g. streaming and final insertion of the same answer). Deduplicate by stream id
        // while preserving distinct messages that lack a stream id but share markdown text.
        let mut seen_streams: HashSet<String> = HashSet::new();
        records.retain(|record| match record {
            HistoryRecord::AssistantMessage(state) => {
                if let Some(stream_id) = &state.stream_id {
                    seen_streams.insert(stream_id.clone())
                } else {
                    true
                }
            }
            _ => true,
        });

        self.records = records;
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
        self.usage_tracker.reset();
        for record in &self.records {
            self.usage_tracker.on_insert(record);
        }
        self.rebuild_exec_stream_retained_totals();
    }

    fn rebuild_exec_stream_retained_totals(&mut self) {
        let mut total = 0usize;
        let mut by_id: HashMap<HistoryId, usize> = HashMap::new();

        for record in &self.records {
            let HistoryRecord::Exec(exec) = record else {
                continue;
            };
            if exec.id == HistoryId::ZERO {
                continue;
            }
            let retained = retained_exec_record_bytes(exec);
            by_id.insert(exec.id, retained);
            total = total.saturating_add(retained);
        }

        self.exec_stream_retained_total_bytes = total;
        self.exec_stream_retained_by_id = by_id;
    }

    fn set_exec_stream_retained(&mut self, id: HistoryId, retained: usize) {
        if id == HistoryId::ZERO {
            return;
        }
        let prev = self.exec_stream_retained_by_id.insert(id, retained).unwrap_or(0);
        match retained.cmp(&prev) {
            std::cmp::Ordering::Greater => {
                self.exec_stream_retained_total_bytes = self
                    .exec_stream_retained_total_bytes
                    .saturating_add(retained.saturating_sub(prev));
            }
            std::cmp::Ordering::Less => {
                self.exec_stream_retained_total_bytes = self
                    .exec_stream_retained_total_bytes
                    .saturating_sub(prev.saturating_sub(retained));
            }
            std::cmp::Ordering::Equal => {}
        }
    }

    fn remove_exec_stream_retained(&mut self, id: HistoryId) {
        if id == HistoryId::ZERO {
            return;
        }
        if let Some(prev) = self.exec_stream_retained_by_id.remove(&id) {
            self.exec_stream_retained_total_bytes = self
                .exec_stream_retained_total_bytes
                .saturating_sub(prev);
        }
    }

    pub fn truncate_after(&mut self, id: HistoryId) -> Vec<HistoryRecord> {
        if id == HistoryId::ZERO {
            let removed = std::mem::take(&mut self.records);
            self.exec_call_lookup.clear();
            self.tool_call_lookup.clear();
            self.stream_lookup.clear();
            self.next_id = 1;
            self.usage_tracker.reset();
            self.exec_stream_retained_total_bytes = 0;
            self.exec_stream_retained_by_id.clear();
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
                self.set_exec_stream_retained(state.id, retained_exec_record_bytes(state));
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
                self.stream_lookup
                    .insert(state.stream_id.clone(), state.id);
            }
            _ => {}
        }
        self.usage_tracker.on_insert(record);
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
                self.remove_exec_stream_retained(state.id);
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
        self.usage_tracker.on_remove(record.id());
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
                HistoryMutation::Inserted { index: idx, id, record }
            }
            HistoryEvent::Replace { index, record } => {
                if let Some(existing) = self.records.get(index).cloned() {
                    let id = existing.id();
                    let record = record.with_id(id);
                    let preserved_usage = self.usage_tracker.take_snapshot(&existing);
                    self.unregister_record(&existing);
                    self.records[index] = record.clone();
                    self.register_record(&record);
                    self.usage_tracker
                        .restore_snapshot(&record, preserved_usage);
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
                let (id, retained) = match self.records.get_mut(index) {
                    Some(HistoryRecord::Exec(existing)) => {
                        if let Some(chunk) = stdout_chunk {
                            let chunk_len = chunk.content.len();
                            self.usage_tracker.add_exec_delta(existing.id, 1, chunk_len);
                            append_exec_chunk(&mut existing.stdout_chunks, chunk);
                        }
                        if let Some(chunk) = stderr_chunk {
                            let chunk_len = chunk.content.len();
                            self.usage_tracker.add_exec_delta(existing.id, 1, chunk_len);
                            append_exec_chunk(&mut existing.stderr_chunks, chunk);
                        }
                        self.usage_tracker
                            .observe_exec(existing, "domain:update-exec-stream");
                        let retained = retained_exec_record_bytes(existing);
                        (existing.id, retained)
                    }
                    _ => return HistoryMutation::Noop,
                };

                self.set_exec_stream_retained(id, retained);

                self.enforce_exec_stream_global_limit();

                if let Some(record) = self.records.get(index).cloned() {
                    HistoryMutation::Replaced {
                        index,
                        id,
                        record,
                    }
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
                        if let Some(delta_clone) = delta.clone() {
                            self.usage_tracker
                                .add_assistant_delta(updated.id, delta_clone.delta.len());
                            let truncated = append_assistant_delta(&mut updated.deltas, delta_clone);
                            if truncated > 0 {
                                updated.truncated_prefix_bytes = updated
                                    .truncated_prefix_bytes
                                    .saturating_add(truncated);
                            }
                        }
                        updated.preview_markdown = preview_markdown.clone();
                        if let Some(meta) = metadata.clone() {
                            updated.citations = meta.citations.clone();
                            updated.metadata = Some(meta);
                        }
                        updated.in_progress = true;
                        updated.last_updated_at = now;
                        self.usage_tracker
                            .observe_assistant(&updated, "domain:assistant-stream");
                        let mutation = self.apply_event(HistoryEvent::Replace {
                            index: idx,
                            record: HistoryRecord::AssistantStream(updated),
                        });
                        if !matches!(mutation, HistoryMutation::Noop) {
                            return mutation;
                        }
                    }
                }

                let mut deltas = Vec::new();
                let mut truncated_prefix_bytes = 0usize;
                if let Some(delta_value) = delta {
                    if let Some(existing_id) = self.records
                        .iter()
                        .find_map(|record| match record {
                            HistoryRecord::AssistantStream(state) if state.stream_id == stream_id => {
                                Some(state.id)
                            }
                            _ => None,
                        })
                    {
                        self.usage_tracker
                            .add_assistant_delta(existing_id, delta_value.delta.len());
                    }
                    truncated_prefix_bytes = append_assistant_delta(&mut deltas, delta_value);
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
                    truncated_prefix_bytes,
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
                                self.usage_tracker
                                    .add_exec_delta(updated.id, 1, tail.len());
                                append_exec_chunk(
                                    &mut updated.stdout_chunks,
                                    ExecStreamChunk {
                                        offset,
                                        content: tail,
                                    },
                                );
                            }
                        }
                        if let Some(tail) = stderr_tail {
                            if !tail.is_empty() {
                                let offset = stream_len(&updated.stderr_chunks);
                                self.usage_tracker
                                    .add_exec_delta(updated.id, 1, tail.len());
                                append_exec_chunk(
                                    &mut updated.stderr_chunks,
                                    ExecStreamChunk {
                                        offset,
                                        content: tail,
                                    },
                                );
                            }
                        }

                        self.usage_tracker
                            .observe_exec(&updated, "domain:finish-exec");
                        let id = updated.id;
                        let mutation = self.apply_event(HistoryEvent::Replace {
                            index: idx,
                            record: HistoryRecord::Exec(updated),
                        });
                        if matches!(mutation, HistoryMutation::Noop) {
                            return mutation;
                        }

                        self.enforce_exec_stream_global_limit();

                        if let Some(record) = self.records.get(idx).cloned() {
                            HistoryMutation::Replaced {
                                index: idx,
                                id,
                                record,
                            }
                        } else {
                            HistoryMutation::Noop
                        }
                    } else {
                        HistoryMutation::Noop
                    }
                } else {
                    HistoryMutation::Noop
                }
            }
        }
    }

    /// Keep total in-memory exec stream payload bounded across all history
    /// records. When the cumulative retained bytes exceed
    /// `GLOBAL_EXEC_STREAM_RETAINED_BYTES`, progressively prune the oldest exec
    /// records down to `EXEC_STREAM_PRUNE_TARGET_BYTES` per stream.
    fn enforce_exec_stream_global_limit(&mut self) {
        if self.exec_stream_retained_total_bytes <= GLOBAL_EXEC_STREAM_RETAINED_BYTES {
            return;
        }

        let mut bytes_to_drop =
            self.exec_stream_retained_total_bytes - GLOBAL_EXEC_STREAM_RETAINED_BYTES;

        let retained_by_id = &mut self.exec_stream_retained_by_id;
        let total_retained = &mut self.exec_stream_retained_total_bytes;

        for record in &mut self.records {
            if bytes_to_drop == 0 {
                break;
            }

            let HistoryRecord::Exec(exec) = record else {
                continue;
            };

            let before = retained_by_id
                .get(&exec.id)
                .copied()
                .unwrap_or_else(|| retained_exec_record_bytes(exec));
            if before == 0 {
                continue;
            }

            prune_exec_stream(&mut exec.stdout_chunks, EXEC_STREAM_PRUNE_TARGET_BYTES);
            prune_exec_stream(&mut exec.stderr_chunks, EXEC_STREAM_PRUNE_TARGET_BYTES);

            let after = retained_exec_record_bytes(exec);
            let prev = retained_by_id.insert(exec.id, after).unwrap_or(0);
            match after.cmp(&prev) {
                std::cmp::Ordering::Greater => {
                    *total_retained = (*total_retained)
                        .saturating_add(after.saturating_sub(prev));
                }
                std::cmp::Ordering::Less => {
                    *total_retained = (*total_retained)
                        .saturating_sub(prev.saturating_sub(after));
                }
                std::cmp::Ordering::Equal => {}
            }

            let dropped = before.saturating_sub(after);
            bytes_to_drop = bytes_to_drop.saturating_sub(dropped);
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
            HistoryRecord::Context(mut state) => {
                state.id = id;
                HistoryRecord::Context(state)
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
            HistoryRecord::Context(state) => state.id,
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
    Inserted { index: usize, id: HistoryId, record: HistoryRecord },
    Replaced { index: usize, id: HistoryId, record: HistoryRecord },
    Removed { index: usize, id: HistoryId, record: HistoryRecord },
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
            HistoryRecord::Context(state) => state.id = HistoryId::ZERO,
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
                assert_eq!(exec.stdout_chunks.last().map(|c| c.content.as_str()), Some("output"));
                assert_eq!(exec.stderr_chunks.last().map(|c| c.content.as_str()), Some("warn"));
            }
            other => panic!("expected exec record, got {:?}", other),
        }
    }

    #[test]
    fn exec_stream_truncates_to_memory_cap() {
        let mut state = HistoryState::new();
        let inserted_id = match state.apply_domain_event(HistoryDomainEvent::StartExec {
            index: state.records.len(),
            call_id: Some("call-clip".into()),
            command: vec!["cat".into(), "large.log".into()],
            parsed: Vec::new(),
            action: ExecAction::Run,
            started_at: SystemTime::UNIX_EPOCH,
            working_dir: None,
            env: Vec::new(),
            tags: Vec::new(),
        }) {
            HistoryMutation::Inserted { id, .. } => id,
            other => panic!("unexpected mutation: {other:?}"),
        };

        let oversized = "x".repeat(MAX_EXEC_STREAM_RETAINED_BYTES + 1024);
        let exec_index = state.index_of(inserted_id).expect("exec index present");
        state.apply_domain_event(HistoryDomainEvent::UpdateExecStream {
            index: exec_index,
            stdout_chunk: Some(ExecStreamChunk { offset: 0, content: oversized.clone() }),
            stderr_chunk: None,
        });

        let exec_record = match state.record(inserted_id).expect("exec record") {
            HistoryRecord::Exec(record) => record.clone(),
            other => panic!("expected exec record, got {other:?}"),
        };

        let retained = retained_stream_len(&exec_record.stdout_chunks);
        assert_eq!(retained, MAX_EXEC_STREAM_RETAINED_BYTES);

        let truncated = truncated_prefix_len(&exec_record.stdout_chunks);
        assert_eq!(truncated, oversized.len() - MAX_EXEC_STREAM_RETAINED_BYTES);

        let mut flattened = String::new();
        let mut sorted = exec_record.stdout_chunks.clone();
        sorted.sort_by_key(|chunk| chunk.offset);
        for chunk in sorted {
            flattened.push_str(&chunk.content);
        }
        assert_eq!(flattened.len(), MAX_EXEC_STREAM_RETAINED_BYTES);
        let expected_tail = oversized[oversized.len() - MAX_EXEC_STREAM_RETAINED_BYTES..].to_string();
        assert_eq!(flattened, expected_tail);
    }

    #[test]
    fn finalize_assistant_updates_existing_records() {
        let mut state = HistoryState::new();

        // First finalize with a stream id to simulate the live streaming path.
        let first = state.finalize_assistant_stream_state(
            Some("stream-1"),
            "Hello world".to_string(),
            None,
            None,
        );
        assert_eq!(state.records.len(), 1);
        assert_eq!(first.stream_id.as_deref(), Some("stream-1"));

        // When updated content arrives with the same stream id, we update the existing
        // record in place.
        let updated = state.finalize_assistant_stream_state(
            Some("stream-1"),
            "Hello world!".to_string(),
            None,
            None,
        );
        assert_eq!(state.records.len(), 1);
        assert_eq!(updated.id, first.id);
        assert_eq!(updated.markdown, "Hello world!");

        // When the same content later arrives without a stream id (e.g. via replayed
        // response items), a new assistant message should be recorded.
        let second = state.finalize_assistant_stream_state(
            None,
            "Hello world!".to_string(),
            None,
            None,
        );

        assert_eq!(state.records.len(), 2);
        assert_ne!(second.id, first.id);
        assert_eq!(second.markdown, "Hello world!");
    }

    #[test]
    fn restore_deduplicates_assistant_messages() {
        let assistant = |id: u64| HistoryRecord::AssistantMessage(AssistantMessageState {
            id: HistoryId(id),
            stream_id: Some("stream-dup".to_string()),
            markdown: "Hello again".to_string(),
            citations: Vec::new(),
            metadata: None,
            token_usage: None,
            mid_turn: false,
            created_at: SystemTime::UNIX_EPOCH,
        });

        let snapshot = HistorySnapshot {
            records: vec![
                assistant(1),
                assistant(2),
                plain_message("keep me"),
            ],
            next_id: 3,
            exec_call_lookup: HashMap::new(),
            tool_call_lookup: HashMap::new(),
            stream_lookup: HashMap::new(),
            order: Vec::new(),
            order_debug: Vec::new(),
        };

        let mut state = HistoryState::new();
        state.restore(&snapshot);

        let assistant_count = state
            .records
            .iter()
            .filter(|record| matches!(record, HistoryRecord::AssistantMessage(_)))
            .count();
        assert_eq!(assistant_count, 1, "duplicate assistant messages should be removed");

        let remaining = state
            .records
            .iter()
            .find_map(|record| match record {
                HistoryRecord::AssistantMessage(state) => Some(state.clone()),
                _ => None,
            })
            .expect("assistant record");
        assert_eq!(remaining.id, HistoryId(1));
        assert_eq!(remaining.stream_id.as_deref(), Some("stream-dup"));
    }

    #[test]
    fn restore_preserves_distinct_messages_without_stream_id() {
        let assistant = |id: u64, text: &str| HistoryRecord::AssistantMessage(AssistantMessageState {
            id: HistoryId(id),
            stream_id: None,
            markdown: text.to_string(),
            citations: Vec::new(),
            metadata: None,
            token_usage: None,
            mid_turn: false,
            created_at: SystemTime::UNIX_EPOCH,
        });

        let snapshot = HistorySnapshot {
            records: vec![assistant(1, "Done."), assistant(2, "Done."), plain_message("next")],
            next_id: 3,
            exec_call_lookup: HashMap::new(),
            tool_call_lookup: HashMap::new(),
            stream_lookup: HashMap::new(),
            order: Vec::new(),
            order_debug: Vec::new(),
        };

        let mut state = HistoryState::new();
        state.restore(&snapshot);

        let assistant_count = state
            .records
            .iter()
            .filter(|record| matches!(record, HistoryRecord::AssistantMessage(_)))
            .count();
        assert_eq!(assistant_count, 2);
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

        assert_eq!(restored.history_id_for_exec_call("call-3"), Some(inserted_id));
        let record = restored.record(inserted_id).expect("restored exec");
        match record {
            HistoryRecord::Exec(exec) => {
                assert_eq!(exec.working_dir, Some(PathBuf::from("/work")));
                assert_eq!(exec.env, vec![("PWD".into(), "/work".into())]);
            }
            other => panic!("expected exec record, got {:?}", other),
        }
    }

    #[test]
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
            truncated_prefix_bytes: 0,
        }));

        let snapshot = state.snapshot();

        let mut restored = HistoryState::new();
        restored.restore(&snapshot);

        assert_eq!(restored.history_id_for_exec_call("exec-call"), Some(exec_id));
        assert_eq!(restored.history_id_for_tool_call("tool-call"), Some(tool_id));
        assert_eq!(restored.history_id_for_stream("stream-id"), Some(stream_id));
        assert_eq!(restored.index_of(exec_id), Some(snapshot.records.iter().position(|r| r.id() == exec_id).unwrap()));
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
            truncated_prefix_bytes: 0,
        }));

        records.push(HistoryRecord::AssistantMessage(AssistantMessageState {
            id: HistoryId(11),
            stream_id: Some("stream".into()),
            markdown: "final".into(),
            citations: vec!["cite".into()],
            metadata: Some(metadata.clone()),
            token_usage: metadata.token_usage.clone(),
            mid_turn: false,
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
            snapshot: RateLimitSnapshotEvent {
                primary_used_percent: 10.0,
                secondary_used_percent: 20.0,
                primary_to_secondary_ratio_percent: 50.0,
                primary_window_minutes: 1,
                secondary_window_minutes: 5,
                primary_reset_after_seconds: Some(30),
                secondary_reset_after_seconds: Some(60),
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
            patch_type: PatchEventType::ApplyBegin { auto_approved: true },
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
