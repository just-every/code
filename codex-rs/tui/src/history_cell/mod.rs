use crate::diff_render::create_diff_summary_with_width;
use crate::exec_command::strip_bash_lc_and_escape;
use crate::insert_history::word_wrap_lines;
use crate::sanitize::Mode as SanitizeMode;
use crate::sanitize::Options as SanitizeOptions;
use crate::sanitize::sanitize_for_tui;
use crate::slash_command::SlashCommand;
use crate::text_formatting::format_json_compact;
use crate::util::buffer::{fill_rect, write_line};
use ::image::DynamicImage;
use ::image::ImageReader;
use base64::Engine;
use codex_ansi_escape::ansi_escape_line;
use codex_common::create_config_summary_entries;
use codex_common::elapsed::format_duration;
use codex_core::config::Config;
use codex_core::config_types::ReasoningEffort;
use codex_core::parse_command::ParsedCommand;
use codex_core::plan_tool::PlanItemArg;
use codex_core::plan_tool::StepStatus;
use codex_core::plan_tool::UpdatePlanArgs;
use codex_core::protocol::FileChange;
use codex_core::protocol::McpInvocation;
use codex_core::protocol::SessionConfiguredEvent;
use codex_core::protocol::TokenUsage;
use codex_protocol::num_format::format_with_separators;
use mcp_types::EmbeddedResourceResource;
use mcp_types::ResourceLink;
use ratatui::prelude::*;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::widgets::Block;
use ratatui::widgets::Borders;
use ratatui::widgets::Padding;
use ratatui::widgets::Paragraph;
use ratatui::widgets::WidgetRef;
use ratatui::widgets::Wrap;
use shlex::Shlex;

use std::collections::HashMap;
use std::collections::HashSet;
use std::io::Cursor;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use tracing::error;

mod animated;
mod explore;
mod image;
mod loading;
mod plain;
mod plan_update;
mod reasoning;
mod semantic;
mod text;
mod tool;
mod upgrade;
mod wait_status;

pub(crate) use animated::AnimatedWelcomeCell;
pub(crate) use explore::{ExploreAggregationCell, ExploreEntryStatus};
pub(crate) use image::ImageOutputCell;
pub(crate) use loading::LoadingCell;
pub(crate) use plain::PlainHistoryCell;
pub(crate) use plan_update::PlanUpdateCell;
pub(crate) use reasoning::CollapsibleReasoningCell;
pub(crate) use tool::{RunningToolCallCell, RunningToolCallState, ToolCallCell, ToolCallStatus};
pub(crate) use upgrade::UpgradeNoticeCell;
pub(crate) use wait_status::WaitStatusCell;

// ==================== Core Types ====================

#[derive(Clone)]
pub(crate) struct CommandOutput {
    pub(crate) exit_code: i32,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

#[derive(Clone, Copy)]
pub(crate) enum PatchEventType {
    ApprovalRequest,
    ApplyBegin { auto_approved: bool },
}

// ==================== HistoryCellType ====================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HistoryCellType {
    Plain,
    User,
    Assistant,
    Reasoning,
    Error,
    Exec { kind: ExecKind, status: ExecStatus },
    Tool { status: ToolStatus },
    Patch { kind: PatchKind },
    PlanUpdate,
    BackgroundEvent,
    Notice,
    Diff,
    Image,
    AnimatedWelcome,
    Loading,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExecKind {
    Read,
    Search,
    List,
    Run,
}

// Unified action classification for exec commands
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExecAction {
    Read,
    Search,
    List,
    Run,
}

pub(crate) fn action_enum_from_parsed(
    parsed: &[codex_core::parse_command::ParsedCommand],
) -> ExecAction {
    use codex_core::parse_command::ParsedCommand;
    for p in parsed {
        match p {
            ParsedCommand::Read { .. } => return ExecAction::Read,
            ParsedCommand::Search { .. } => return ExecAction::Search,
            ParsedCommand::ListFiles { .. } => return ExecAction::List,
            _ => {}
        }
    }
    ExecAction::Run
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExecStatus {
    Running,
    Success,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolStatus {
    Running,
    Success,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PatchKind {
    Proposed,
    ApplyBegin,
    ApplySuccess,
    ApplyFailure,
}

// ==================== HistoryCell Trait ====================

/// Represents an event to display in the conversation history.
/// Returns its `Vec<Line<'static>>` representation to make it easier
/// to display in a scrollable list.
pub(crate) trait HistoryCell {
    fn display_lines(&self) -> Vec<Line<'static>>;
    /// A required, explicit type descriptor for the history cell.
    fn kind(&self) -> HistoryCellType;

    /// Allow downcasting to concrete types
    fn as_any(&self) -> &dyn std::any::Any {
        // Default implementation that doesn't support downcasting
        // Concrete types that need downcasting should override this
        &() as &dyn std::any::Any
    }
    /// Allow mutable downcasting to concrete types
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;

    /// Get display lines with empty lines trimmed from beginning and end.
    /// This ensures consistent spacing when cells are rendered together.
    fn display_lines_trimmed(&self) -> Vec<Line<'static>> {
        trim_empty_lines(self.display_lines())
    }

    fn desired_height(&self, width: u16) -> u16 {
        Paragraph::new(Text::from(self.display_lines_trimmed()))
            .wrap(Wrap { trim: false })
            .line_count(width)
            .try_into()
            .unwrap_or(0)
    }

    fn render_with_skip(&self, area: Rect, buf: &mut Buffer, skip_rows: u16) {
        // Check if this cell has custom rendering
        if self.has_custom_render() {
            // Allow custom renders to handle top skipping explicitly
            self.custom_render_with_skip(area, buf, skip_rows);
            return;
        }

        // Default path: render the full text and use Paragraph.scroll to skip
        // vertical rows AFTER wrapping. Slicing lines before wrapping causes
        // incorrect blank space when lines wrap across multiple rows.
        // IMPORTANT: Explicitly clear the entire area first. While some containers
        // clear broader regions, custom widgets that shrink or scroll can otherwise
        // leave residual glyphs to the right of shorter lines or from prior frames.
        // We paint spaces with the current theme background to guarantee a clean slate.
        // Assistant messages use a subtly tinted background: theme background
        // moved 5% toward the theme info color for a gentle distinction.
        let cell_bg = match self.kind() {
            HistoryCellType::Assistant => crate::colors::assistant_bg(),
            _ => crate::colors::background(),
        };
        let bg_style = Style::default().bg(cell_bg).fg(crate::colors::text());
        if matches!(self.kind(), HistoryCellType::Assistant) {
            fill_rect(buf, area, Some(' '), bg_style);
        }

        // Ensure the entire allocated area is painted with the theme background
        // by attaching a background-styled Block to the Paragraph as well.
        let lines = self.display_lines_trimmed();
        let text = Text::from(lines);

        let bg_block = Block::default().style(Style::default().bg(cell_bg));
        Paragraph::new(text)
            .block(bg_block)
            .wrap(Wrap { trim: false })
            .scroll((skip_rows, 0))
            .style(Style::default().bg(cell_bg))
            .render(area, buf);
    }

    /// Returns true if this cell has custom rendering (e.g., animations)
    fn has_custom_render(&self) -> bool {
        false // Default: most cells use display_lines
    }

    /// Custom render implementation for cells that need it
    fn custom_render(&self, _area: Rect, _buf: &mut Buffer) {
        // Default: do nothing (cells with custom rendering will override)
    }
    /// Custom render with support for skipping top rows
    fn custom_render_with_skip(&self, area: Rect, buf: &mut Buffer, _skip_rows: u16) {
        // Default: fall back to non-skipping custom render
        self.custom_render(area, buf);
    }

    /// Returns true if this cell is currently animating and needs redraws
    fn is_animating(&self) -> bool {
        false // Default: most cells don't animate
    }

    /// Returns true if this is a loading cell that should be removed when streaming starts
    #[allow(dead_code)]
    fn is_loading_cell(&self) -> bool {
        false // Default: most cells are not loading cells
    }

    /// Trigger fade-out animation (for AnimatedWelcomeCell)
    fn trigger_fade(&self) {
        // Default: do nothing (only AnimatedWelcomeCell implements this)
    }

    /// Check if this cell should be removed (e.g., fully faded out)
    fn should_remove(&self) -> bool {
        false // Default: most cells should not be removed
    }

    /// Returns the gutter symbol for this cell type
    /// Returns None if no symbol should be displayed
    fn gutter_symbol(&self) -> Option<&'static str> {
        match self.kind() {
            HistoryCellType::Plain => None,
            HistoryCellType::User => Some("›"),
            // Restore assistant gutter icon
            HistoryCellType::Assistant => Some("•"),
            HistoryCellType::Reasoning => None,
            HistoryCellType::Error => Some("✖"),
            HistoryCellType::Tool { status } => Some(match status {
                ToolStatus::Running => "⚙",
                ToolStatus::Success => "✔",
                ToolStatus::Failed => "✖",
            }),
            HistoryCellType::Exec { kind, status } => {
                // Show ❯ only for Run executions; hide for read/search/list summaries
                match (kind, status) {
                    (ExecKind::Run, ExecStatus::Error) => Some("✖"),
                    (ExecKind::Run, _) => Some("❯"),
                    _ => None,
                }
            }
            HistoryCellType::Patch { .. } => Some("↯"),
            // Plan updates supply their own gutter glyph dynamically.
            HistoryCellType::PlanUpdate => None,
            HistoryCellType::BackgroundEvent => Some("»"),
            HistoryCellType::Notice => Some("★"),
            HistoryCellType::Diff => Some("↯"),
            HistoryCellType::Image => None,
            HistoryCellType::AnimatedWelcome => None,
            HistoryCellType::Loading => None,
        }
    }
}

// Allow Box<dyn HistoryCell> to implement HistoryCell
impl HistoryCell for Box<dyn HistoryCell> {
    fn as_any(&self) -> &dyn std::any::Any {
        self.as_ref().as_any()
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self.as_mut().as_any_mut()
    }
    fn kind(&self) -> HistoryCellType {
        self.as_ref().kind()
    }

    fn display_lines(&self) -> Vec<Line<'static>> {
        self.as_ref().display_lines()
    }

    fn display_lines_trimmed(&self) -> Vec<Line<'static>> {
        self.as_ref().display_lines_trimmed()
    }

    fn desired_height(&self, width: u16) -> u16 {
        self.as_ref().desired_height(width)
    }

    fn render_with_skip(&self, area: Rect, buf: &mut Buffer, skip_rows: u16) {
        self.as_ref().render_with_skip(area, buf, skip_rows)
    }

    fn has_custom_render(&self) -> bool {
        self.as_ref().has_custom_render()
    }

    fn custom_render(&self, area: Rect, buf: &mut Buffer) {
        self.as_ref().custom_render(area, buf)
    }

    fn is_animating(&self) -> bool {
        self.as_ref().is_animating()
    }

    fn is_loading_cell(&self) -> bool {
        self.as_ref().is_loading_cell()
    }

    fn trigger_fade(&self) {
        self.as_ref().trigger_fade()
    }

    fn should_remove(&self) -> bool {
        self.as_ref().should_remove()
    }

    fn gutter_symbol(&self) -> Option<&'static str> {
        self.as_ref().gutter_symbol()
    }
}

// ==================== ExploreAggregationCell ====================
// Collapses consecutive Read/Search/List commands into a single "Exploring" cell
// while commands are executing, updating the entry status once the command finishes.

pub(crate) fn clean_wait_command(raw: &str) -> String {
    let trimmed = raw.trim();
    let Some((first_token, rest)) = split_token(trimmed) else {
        return trimmed.to_string();
    };
    if !looks_like_shell(first_token) {
        return trimmed.to_string();
    }
    let rest = rest.trim_start();
    let Some((second_token, remainder)) = split_token(rest) else {
        return trimmed.to_string();
    };
    if second_token != "-lc" {
        return trimmed.to_string();
    }
    let mut command = remainder.trim_start();
    if command.len() >= 2 {
        let bytes = command.as_bytes();
        let first_char = bytes[0] as char;
        let last_char = bytes[bytes.len().saturating_sub(1)] as char;
        if (first_char == '"' && last_char == '"') || (first_char == '\'' && last_char == '\'') {
            command = &command[1..command.len().saturating_sub(1)];
        }
    }
    if command.is_empty() {
        trimmed.to_string()
    } else {
        command.to_string()
    }
}

fn split_token(input: &str) -> Option<(&str, &str)> {
    let s = input.trim_start();
    if s.is_empty() {
        return None;
    }
    if let Some(idx) = s.find(char::is_whitespace) {
        let (token, rest) = s.split_at(idx);
        Some((token, rest))
    } else {
        Some((s, ""))
    }
}

fn looks_like_shell(token: &str) -> bool {
    let trimmed = token.trim_matches('"').trim_matches('\'');
    let basename = trimmed
        .rsplit('/')
        .next()
        .unwrap_or(trimmed)
        .to_ascii_lowercase();
    matches!(
        basename.as_str(),
        "bash"
            | "bash.exe"
            | "sh"
            | "sh.exe"
            | "zsh"
            | "zsh.exe"
            | "dash"
            | "dash.exe"
            | "ksh"
            | "ksh.exe"
            | "busybox"
    )
}

// Remove formatting-only pipes (sed/head/tail) when we already provide a line-range
// annotation alongside the command summary. Keeps the core command intact for display.
// ==================== ExecCell ====================

#[derive(Clone, PartialEq, Eq)]
struct ExecWaitNote {
    text: String,
    is_error: bool,
}

#[derive(Clone, Default)]
struct ExecWaitState {
    total_wait: Option<Duration>,
    run_duration: Option<Duration>,
    waiting: bool,
    notes: Vec<ExecWaitNote>,
}

pub(crate) struct ExecCell {
    pub(crate) command: Vec<String>,
    pub(crate) parsed: Vec<ParsedCommand>,
    pub(crate) output: Option<CommandOutput>,
    pub(crate) start_time: Option<Instant>,
    pub(crate) stream_preview: Option<CommandOutput>,
    // Caches to avoid recomputing expensive line construction for completed execs
    cached_display_lines: std::cell::RefCell<Option<Vec<Line<'static>>>>,
    cached_pre_lines: std::cell::RefCell<Option<Vec<Line<'static>>>>,
    cached_out_lines: std::cell::RefCell<Option<Vec<Line<'static>>>>,
    // Cached per-width layout (wrapped rows + totals) while content is stable
    cached_layout: std::cell::RefCell<Option<Rc<ExecLayoutCache>>>,
    cached_command_lines: std::cell::RefCell<Option<Vec<Line<'static>>>>,
    cached_wait_extras: std::cell::RefCell<Option<Vec<Line<'static>>>>,
    parsed_meta: Option<ParsedExecMetadata>,
    has_bold_command: bool,
    wait_state: std::cell::RefCell<ExecWaitState>,
}

#[derive(Clone)]
struct ExecLayoutCache {
    width: u16,
    pre_lines: Vec<Line<'static>>,
    out_lines: Vec<Line<'static>>,
    pre_total: u16,
    out_block_total: u16,
}

#[derive(Clone)]
struct ParsedExecMetadata {
    action: ExecAction,
    ctx_path: Option<String>,
    search_paths: HashSet<String>,
}

impl ParsedExecMetadata {
    fn from_commands(parsed: &[ParsedCommand]) -> Self {
        let action = action_enum_from_parsed(parsed);
        let ctx_path = first_context_path(parsed);
        let mut search_paths: HashSet<String> = HashSet::new();
        for pc in parsed {
            if let ParsedCommand::Search { path: Some(p), .. } = pc {
                search_paths.insert(p.to_string());
            }
        }
        Self {
            action,
            ctx_path,
            search_paths,
        }
    }
}

// ==================== AssistantMarkdownCell ====================
// Stores raw assistant markdown and rebuilds on demand (e.g., theme/syntax changes)

pub(crate) struct AssistantMarkdownCell {
    // Raw markdown used to rebuild when theme/syntax changes
    pub(crate) raw: String,
    // Optional stream/item id that produced this finalized cell
    pub(crate) id: Option<String>,
    // Pre-rendered lines (first line is a hidden "codex" header)
    pub(crate) lines: Vec<Line<'static>>, // includes hidden header "codex"
    // Cached per-width wrap plan to avoid re-segmentation and re-measure
    cached_layout: std::cell::RefCell<Option<AssistantLayoutCache>>,
}

impl AssistantMarkdownCell {
    #[allow(dead_code)]
    pub(crate) fn new(raw: String, cfg: &codex_core::config::Config) -> Self {
        Self::new_with_id(raw, None, cfg)
    }

    pub(crate) fn new_with_id(
        raw: String,
        id: Option<String>,
        cfg: &codex_core::config::Config,
    ) -> Self {
        let mut me = Self {
            raw,
            id,
            lines: Vec::new(),
            cached_layout: std::cell::RefCell::new(None),
        };
        me.rebuild(cfg);
        me
    }
    pub(crate) fn rebuild(&mut self, cfg: &codex_core::config::Config) {
        let mut out: Vec<Line<'static>> = Vec::new();
        out.push(Line::from("codex"));
        crate::markdown::append_markdown_with_bold_first(&self.raw, &mut out, cfg);
        // Apply bright text to body like streaming finalize
        let bright = crate::colors::text_bright();
        for line in out.iter_mut().skip(1) {
            line.style = line.style.patch(Style::default().fg(bright));
        }
        self.lines = out;
        // Invalidate cached layout on rebuild (theme/lines changed)
        *self.cached_layout.borrow_mut() = None;
    }
}

// Cached layout for AssistantMarkdownCell (per width)
#[derive(Clone)]
pub(crate) struct AssistantLayoutCache {
    width: u16,
    segs: Vec<AssistantSeg>,
    seg_rows: Vec<u16>,
    total_rows_with_padding: u16,
}

impl AssistantLayoutCache {
    pub(crate) fn total_rows(&self) -> u16 {
        self.total_rows_with_padding
    }
}

#[derive(Clone, Debug)]
enum AssistantSeg {
    Text(Vec<Line<'static>>),
    Bullet(Vec<Line<'static>>),
    Code {
        lines: Vec<Line<'static>>,
        lang_label: Option<String>,
        max_line_width: u16,
    },
}

impl AssistantMarkdownCell {
    pub(crate) fn ensure_layout(&self, width: u16) -> AssistantLayoutCache {
        if let Some(cache) = self.cached_layout.borrow().as_ref() {
            if cache.width == width {
                return cache.clone();
            }
        }

        let text_wrap_width = width;
        let mut segs: Vec<AssistantSeg> = Vec::new();
        let mut text_buf: Vec<Line<'static>> = Vec::new();
        let mut iter = self.display_lines_trimmed().into_iter().peekable();
        let measure_line = |line: &Line<'_>| -> u16 {
            line.spans
                .iter()
                .map(|s| unicode_width::UnicodeWidthStr::width(s.content.as_ref()))
                .sum::<usize>()
                .min(u16::MAX as usize) as u16
        };

        while let Some(line) = iter.next() {
            if crate::render::line_utils::is_code_block_painted(&line) {
                if !text_buf.is_empty() {
                    let wrapped = word_wrap_lines(&text_buf, text_wrap_width);
                    segs.push(AssistantSeg::Text(wrapped));
                    text_buf.clear();
                }

                let mut chunk = vec![line];
                while let Some(next) = iter.peek() {
                    if crate::render::line_utils::is_code_block_painted(next) {
                        chunk.push(iter.next().unwrap());
                    } else {
                        break;
                    }
                }

                let mut lang_label: Option<String> = None;
                let mut content_lines: Vec<Line<'static>> = Vec::new();
                for (idx, candidate) in chunk.into_iter().enumerate() {
                    if idx == 0 {
                        let flat: String =
                            candidate.spans.iter().map(|s| s.content.as_ref()).collect();
                        if let Some(s) = flat.strip_prefix("⟦LANG:") {
                            if let Some(end) = s.find('⟧') {
                                lang_label = Some(s[..end].to_string());
                                continue;
                            }
                        }
                    }
                    content_lines.push(candidate);
                }

                while content_lines
                    .first()
                    .is_some_and(|l| crate::render::line_utils::is_blank_line_spaces_only(l))
                {
                    let _ = content_lines.remove(0);
                }
                while content_lines
                    .last()
                    .is_some_and(|l| crate::render::line_utils::is_blank_line_spaces_only(l))
                {
                    let _ = content_lines.pop();
                }

                if content_lines.is_empty() {
                    continue;
                }

                let max_line_width = content_lines
                    .iter()
                    .map(|l| measure_line(l))
                    .max()
                    .unwrap_or(0);

                segs.push(AssistantSeg::Code {
                    lines: content_lines,
                    lang_label,
                    max_line_width,
                });
                continue;
            }

            if text_wrap_width > 4 && is_horizontal_rule_line(&line) {
                if !text_buf.is_empty() {
                    let wrapped = word_wrap_lines(&text_buf, text_wrap_width);
                    segs.push(AssistantSeg::Text(wrapped));
                    text_buf.clear();
                }
                let hr = Line::from(Span::styled(
                    std::iter::repeat('─')
                        .take(text_wrap_width as usize)
                        .collect::<String>(),
                    Style::default().fg(crate::colors::assistant_hr()),
                ));
                segs.push(AssistantSeg::Bullet(vec![hr]));
                continue;
            }

            if text_wrap_width > 4 {
                if let Some((indent_spaces, bullet_char)) = detect_bullet_prefix(&line) {
                    if !text_buf.is_empty() {
                        let wrapped = word_wrap_lines(&text_buf, text_wrap_width);
                        segs.push(AssistantSeg::Text(wrapped));
                        text_buf.clear();
                    }
                    segs.push(AssistantSeg::Bullet(wrap_bullet_line(
                        line,
                        indent_spaces,
                        &bullet_char,
                        text_wrap_width,
                    )));
                    continue;
                }
            }

            text_buf.push(line);
        }

        if !text_buf.is_empty() {
            let wrapped = word_wrap_lines(&text_buf, text_wrap_width);
            segs.push(AssistantSeg::Text(wrapped));
            text_buf.clear();
        }

        let mut seg_rows: Vec<u16> = Vec::with_capacity(segs.len());
        let mut total: u16 = 0;
        for seg in &segs {
            let rows = match seg {
                AssistantSeg::Text(lines) | AssistantSeg::Bullet(lines) => lines.len() as u16,
                AssistantSeg::Code { lines, .. } => lines.len() as u16 + 2,
            };
            seg_rows.push(rows);
            total = total.saturating_add(rows);
        }
        total = total.saturating_add(2);

        let cache = AssistantLayoutCache {
            width,
            segs,
            seg_rows,
            total_rows_with_padding: total,
        };
        *self.cached_layout.borrow_mut() = Some(cache.clone());
        cache
    }

    pub(crate) fn render_with_layout(
        &self,
        plan: &AssistantLayoutCache,
        area: Rect,
        buf: &mut Buffer,
        skip_rows: u16,
    ) {
        let cell_bg = crate::colors::assistant_bg();
        let bg_style = Style::default().bg(cell_bg);
        fill_rect(buf, area, Some(' '), bg_style);

        if area.width == 0 || area.height == 0 {
            return;
        }

        let segs = &plan.segs;
        let seg_rows = &plan.seg_rows;
        let mut remaining_skip = skip_rows;
        let mut cur_y = area.y;
        let end_y = area.y.saturating_add(area.height);

        if remaining_skip == 0 && cur_y < end_y {
            cur_y = cur_y.saturating_add(1);
        }
        remaining_skip = remaining_skip.saturating_sub(1);

        for (seg_idx, seg) in segs.iter().enumerate() {
            if cur_y >= end_y {
                break;
            }
            let rows = seg_rows.get(seg_idx).copied().unwrap_or(0);
            if remaining_skip >= rows {
                remaining_skip -= rows;
                continue;
            }

            match seg {
                AssistantSeg::Text(lines) | AssistantSeg::Bullet(lines) => {
                    let total = lines.len() as u16;
                    if total == 0 {
                        continue;
                    }
                    let start = usize::from(remaining_skip);
                    let visible = total.saturating_sub(remaining_skip);
                    let avail = end_y.saturating_sub(cur_y);
                    let draw_count = visible.min(avail);
                    if draw_count == 0 {
                        remaining_skip = 0;
                        continue;
                    }
                    for line in lines.iter().skip(start).take(draw_count as usize) {
                        if cur_y >= end_y {
                            break;
                        }
                        write_line(buf, area.x, cur_y, area.width, line, bg_style);
                        cur_y = cur_y.saturating_add(1);
                    }
                    remaining_skip = 0;
                }
                AssistantSeg::Code {
                    lines,
                    lang_label,
                    max_line_width,
                } => {
                    let avail = end_y.saturating_sub(cur_y);
                    if avail == 0 {
                        break;
                    }

                    let full_height = lines.len() as u16 + 2;
                    let card_w = max_line_width.saturating_add(6).min(area.width.max(6));

                    let temp_area = Rect::new(0, 0, card_w, full_height);
                    let mut temp_buf = Buffer::empty(temp_area);
                    let code_bg = crate::colors::code_block_bg();
                    let blk = Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(crate::colors::border()))
                        .style(Style::default().bg(code_bg))
                        .padding(Padding {
                            left: 2,
                            right: 2,
                            top: 0,
                            bottom: 0,
                        });
                    let blk = if let Some(lang) = lang_label {
                        blk.title(Span::styled(
                            format!(" {} ", lang),
                            Style::default().fg(crate::colors::text_dim()),
                        ))
                    } else {
                        blk
                    };
                    let inner_rect = blk.inner(temp_area);
                    blk.clone().render(temp_area, &mut temp_buf);
                    for (idx, line) in lines.iter().enumerate() {
                        let target_y = inner_rect.y.saturating_add(idx as u16);
                        if target_y >= inner_rect.y.saturating_add(inner_rect.height) {
                            break;
                        }
                        write_line(
                            &mut temp_buf,
                            inner_rect.x,
                            target_y,
                            inner_rect.width,
                            line,
                            Style::default().bg(code_bg),
                        );
                    }

                    let start_row = remaining_skip.min(full_height);
                    let draw_rows = avail.min(full_height.saturating_sub(remaining_skip));
                    if draw_rows == 0 {
                        remaining_skip = 0;
                        continue;
                    }

                    for row_offset in 0..usize::from(draw_rows) {
                        let src_y = start_row + row_offset as u16;
                        let dest_y = cur_y.saturating_add(row_offset as u16);
                        if dest_y >= end_y {
                            break;
                        }
                        for col in 0..usize::from(card_w) {
                            let dest_x = area.x + col as u16;
                            if dest_x >= area.x.saturating_add(area.width) {
                                break;
                            }
                            let cell = temp_buf[(col as u16, src_y)].clone();
                            buf[(dest_x, dest_y)] = cell;
                        }
                    }
                    cur_y = cur_y.saturating_add(draw_rows);
                    remaining_skip = 0;
                }
            }
        }

        if remaining_skip == 0 && cur_y < end_y {
            cur_y = cur_y.saturating_add(1);
        } else {
            remaining_skip = remaining_skip.saturating_sub(1);
        }
        let _ = (cur_y, remaining_skip);
    }
}

impl HistoryCell for AssistantMarkdownCell {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn kind(&self) -> HistoryCellType {
        HistoryCellType::Assistant
    }
    fn display_lines(&self) -> Vec<Line<'static>> {
        // Hide the header line, mirroring PlainHistoryCell behavior for Assistant
        if self.lines.len() > 1 {
            self.lines[1..].to_vec()
        } else {
            Vec::new()
        }
    }
    fn has_custom_render(&self) -> bool {
        true
    }
    fn desired_height(&self, width: u16) -> u16 {
        self.ensure_layout(width).total_rows_with_padding
    }
    fn custom_render_with_skip(&self, area: Rect, buf: &mut Buffer, skip_rows: u16) {
        let plan = self.ensure_layout(area.width);
        self.render_with_layout(&plan, area, buf, skip_rows);
    }
}

impl HistoryCell for ExecCell {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn kind(&self) -> HistoryCellType {
        let kind = match self.parsed_action() {
            ExecAction::Read => ExecKind::Read,
            ExecAction::Search => ExecKind::Search,
            ExecAction::List => ExecKind::List,
            ExecAction::Run => ExecKind::Run,
        };
        let status = match &self.output {
            None => ExecStatus::Running,
            Some(o) if o.exit_code == 0 => ExecStatus::Success,
            Some(_) => ExecStatus::Error,
        };
        HistoryCellType::Exec { kind, status }
    }
    fn gutter_symbol(&self) -> Option<&'static str> {
        match self.kind() {
            HistoryCellType::Exec {
                kind: ExecKind::Run,
                status,
            } => {
                if matches!(status, ExecStatus::Error) {
                    Some("✖")
                } else if self.has_bold_command {
                    Some("❯")
                } else {
                    None
                }
            }
            HistoryCellType::Exec { .. } => None,
            _ => None,
        }
    }
    fn display_lines(&self) -> Vec<Line<'static>> {
        // Fallback textual representation (used for height measurement only when custom rendering).
        // For completed executions, cache the computed lines since they are immutable.
        if let Some(cached) = self.cached_display_lines.borrow().as_ref() {
            return cached.clone();
        }
        let lines = exec_command_lines(
            &self.command,
            &self.parsed,
            self.output.as_ref(),
            self.stream_preview.as_ref(),
            self.start_time,
        );
        if self.output.is_some() {
            *self.cached_display_lines.borrow_mut() = Some(lines.clone());
        }
        lines
    }
    fn has_custom_render(&self) -> bool {
        true
    }
    fn is_animating(&self) -> bool {
        self.output.is_none() && self.start_time.is_some()
    }
    fn desired_height(&self, width: u16) -> u16 {
        let (pre_total, _out_block_total, out_total_with_status) = self.ensure_wrap_totals(width);
        pre_total.saturating_add(out_total_with_status)
    }
    fn custom_render_with_skip(&self, area: Rect, buf: &mut Buffer, skip_rows: u16) {
        let plan = self.ensure_layout(area.width);
        let plan_ref = plan.as_ref();

        let pre_total = plan_ref.pre_total;
        let out_block_total = plan_ref.out_block_total;

        let pre_skip = skip_rows.min(pre_total);
        let after_pre_skip = skip_rows.saturating_sub(pre_total);
        let block_skip = after_pre_skip.min(out_block_total);
        let after_block_skip = after_pre_skip.saturating_sub(block_skip);

        let pre_height = pre_total.saturating_sub(pre_skip).min(area.height);
        let mut remaining_height = area.height.saturating_sub(pre_height);

        let block_height = out_block_total
            .saturating_sub(block_skip)
            .min(remaining_height);
        remaining_height = remaining_height.saturating_sub(block_height);

        let status_line_to_render =
            if self.output.is_none() && after_block_skip == 0 && remaining_height > 0 {
                self.streaming_status_line()
            } else {
                None
            };
        let status_height = status_line_to_render.is_some().then_some(1).unwrap_or(0);

        let mut cur_y = area.y;

        if pre_height > 0 {
            let pre_area = Rect {
                x: area.x,
                y: cur_y,
                width: area.width,
                height: pre_height,
            };
            let bg_style = Style::default()
                .bg(crate::colors::background())
                .fg(crate::colors::text());
            fill_rect(buf, pre_area, Some(' '), bg_style);
            for (idx, line) in plan_ref
                .pre_lines
                .iter()
                .skip(pre_skip as usize)
                .take(pre_height as usize)
                .enumerate()
            {
                let y = pre_area.y.saturating_add(idx as u16);
                if y >= pre_area.y.saturating_add(pre_area.height) {
                    break;
                }
                write_line(buf, pre_area.x, y, pre_area.width, line, bg_style);
            }
            cur_y = cur_y.saturating_add(pre_height);
        }

        if block_height > 0 && area.width > 0 {
            let out_area = Rect {
                x: area.x,
                y: cur_y,
                width: area.width,
                height: block_height,
            };
            let bg_style = Style::default()
                .bg(crate::colors::background())
                .fg(crate::colors::text_dim());
            fill_rect(buf, out_area, Some(' '), bg_style);
            let block = Block::default()
                .borders(Borders::LEFT)
                .border_style(
                    Style::default()
                        .fg(crate::colors::border_dim())
                        .bg(crate::colors::background()),
                )
                .style(Style::default().bg(crate::colors::background()))
                .padding(Padding {
                    left: 1,
                    right: 0,
                    top: 0,
                    bottom: 0,
                });
            let inner_rect = block.inner(out_area);
            block.render(out_area, buf);

            if inner_rect.width > 0 {
                for (idx, line) in plan_ref
                    .out_lines
                    .iter()
                    .skip(block_skip as usize)
                    .take(block_height as usize)
                    .enumerate()
                {
                    let y = inner_rect.y.saturating_add(idx as u16);
                    if y >= inner_rect.y.saturating_add(inner_rect.height) {
                        break;
                    }
                    write_line(buf, inner_rect.x, y, inner_rect.width, line, bg_style);
                }
            }
            cur_y = cur_y.saturating_add(block_height);
        }

        if let Some(line) = status_line_to_render {
            if status_height > 0 {
                let status_area = Rect {
                    x: area.x,
                    y: cur_y,
                    width: area.width,
                    height: status_height,
                };
                let bg_style = Style::default().bg(crate::colors::background());
                fill_rect(buf, status_area, Some(' '), bg_style);
                write_line(
                    buf,
                    status_area.x,
                    status_area.y,
                    status_area.width,
                    &line,
                    bg_style,
                );
            }
        }
    }
}

impl ExecCell {
    fn invalidate_render_caches(&self) {
        self.cached_display_lines.borrow_mut().take();
        self.cached_pre_lines.borrow_mut().take();
        self.cached_out_lines.borrow_mut().take();
        self.cached_layout.borrow_mut().take();
        self.cached_wait_extras.borrow_mut().take();
    }

    fn parsed_action(&self) -> ExecAction {
        self.parsed_meta
            .as_ref()
            .map(|meta| meta.action)
            .unwrap_or(ExecAction::Run)
    }

    pub(crate) fn set_waiting(&self, waiting: bool) {
        let mut state = self.wait_state.borrow_mut();
        if state.waiting != waiting {
            state.waiting = waiting;
            drop(state);
            self.invalidate_render_caches();
        }
    }

    pub(crate) fn set_wait_total(&self, total: Option<Duration>) {
        let mut state = self.wait_state.borrow_mut();
        if state.total_wait != total {
            state.total_wait = total;
            drop(state);
            self.invalidate_render_caches();
        }
    }

    pub(crate) fn set_run_duration(&self, duration: Option<Duration>) {
        let mut state = self.wait_state.borrow_mut();
        if state.run_duration != duration {
            state.run_duration = duration;
            drop(state);
            self.invalidate_render_caches();
        }
    }

    pub(crate) fn wait_total(&self) -> Option<Duration> {
        self.wait_state.borrow().total_wait
    }

    pub(crate) fn clear_wait_notes(&self) {
        let mut state = self.wait_state.borrow_mut();
        if state.notes.is_empty() {
            return;
        }
        state.notes.clear();
        drop(state);
        self.invalidate_render_caches();
    }

    pub(crate) fn push_wait_note(&self, text: &str, is_error: bool) {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }
        let mut state = self.wait_state.borrow_mut();
        if state
            .notes
            .last()
            .map(|note| note.text == trimmed && note.is_error == is_error)
            .unwrap_or(false)
        {
            return;
        }
        state.notes.push(ExecWaitNote {
            text: trimmed.to_string(),
            is_error,
        });
        drop(state);
        self.invalidate_render_caches();
    }

    pub(crate) fn set_wait_notes(&self, notes: &[(String, bool)]) {
        let mut state = self.wait_state.borrow_mut();
        let mut changed = state.notes.len() != notes.len();
        if !changed {
            for (existing, (text, is_error)) in state.notes.iter().zip(notes.iter()) {
                if existing.text != text.trim() || existing.is_error != *is_error {
                    changed = true;
                    break;
                }
            }
        }
        if !changed {
            return;
        }
        state.notes = notes
            .iter()
            .map(|(text, is_error)| ExecWaitNote {
                text: text.trim().to_string(),
                is_error: *is_error,
            })
            .filter(|note| !note.text.is_empty())
            .collect();
        drop(state);
        self.invalidate_render_caches();
    }

    fn wait_note_lines(&self, state: &ExecWaitState) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        for note in &state.notes {
            let mut line = Line::from(note.text.clone());
            let mut style = Style::default().fg(if note.is_error {
                crate::colors::error()
            } else {
                crate::colors::text_dim()
            });
            if note.is_error {
                style = style.add_modifier(Modifier::BOLD);
            }
            for span in line.spans.iter_mut() {
                span.style = style;
            }
            lines.push(line);
        }
        lines
    }

    fn wait_state_snapshot(&self) -> ExecWaitState {
        self.wait_state.borrow().clone()
    }

    fn wait_summary_line(&self, state: &ExecWaitState) -> Option<Line<'static>> {
        if state.waiting {
            return None;
        }
        if let Some(run_duration) = state.run_duration {
            if run_duration.is_zero() {
                return None;
            }
            let text = format!("Ran for {}", format_duration(run_duration));
            return Some(Line::styled(
                text,
                Style::default().fg(crate::colors::text_dim()),
            ));
        }
        let total = state.total_wait?;
        if total.is_zero() {
            return None;
        }
        let text = format!("Waited {}", format_duration(total));
        Some(Line::styled(
            text,
            Style::default().fg(crate::colors::text_dim()),
        ))
    }

    fn wait_extras(&self, state: &ExecWaitState) -> Vec<Line<'static>> {
        if let Some(cached) = self.cached_wait_extras.borrow().as_ref() {
            return cached.clone();
        }
        let mut extra_lines: Vec<Line<'static>> = Vec::new();
        if let Some(summary_line) = self.wait_summary_line(state) {
            extra_lines.push(summary_line);
        }
        extra_lines.extend(self.wait_note_lines(state));
        if self.output.is_some() && !extra_lines.is_empty() {
            *self.cached_wait_extras.borrow_mut() = Some(extra_lines.clone());
        }
        extra_lines
    }

    #[cfg(test)]
    fn has_bold_command(&self) -> bool {
        self.has_bold_command
    }

    pub(crate) fn replace_command_metadata(
        &mut self,
        command: Vec<String>,
        parsed: Vec<ParsedCommand>,
    ) {
        self.command = command;
        self.parsed = parsed;
        self.has_bold_command = command_has_bold_token(&self.command);
        self.cached_command_lines.borrow_mut().take();
        self.cached_wait_extras.borrow_mut().take();
        self.parsed_meta = if self.parsed.is_empty() {
            None
        } else {
            Some(ParsedExecMetadata::from_commands(&self.parsed))
        };
        self.invalidate_render_caches();
    }
    /// Compute wrapped row totals for the preamble and the output at the given width.
    /// Delegates to the per-width layout cache to avoid redundant reflow work.
    fn ensure_wrap_totals(&self, width: u16) -> (u16, u16, u16) {
        let layout = self.ensure_layout(width);
        let status_height = if self.output.is_none() {
            self.streaming_status_line().map(|_| 1).unwrap_or(0)
        } else {
            0
        };
        (
            layout.pre_total,
            layout.out_block_total,
            layout.out_block_total.saturating_add(status_height),
        )
    }

    fn ensure_layout(&self, width: u16) -> Rc<ExecLayoutCache> {
        if let Some(layout) = self.cached_layout.borrow().as_ref() {
            if layout.width == width {
                return layout.clone();
            }
        }

        let (pre_lines_raw, out_lines_raw, _status_line_opt) = self.exec_render_parts();
        let pre_trimmed = trim_empty_lines(pre_lines_raw);
        let out_trimmed = trim_empty_lines(out_lines_raw);

        let pre_wrap_width = width;
        let out_wrap_width = width.saturating_sub(2);

        let pre_wrapped = if pre_wrap_width == 0 {
            Vec::new()
        } else {
            word_wrap_lines(&pre_trimmed, pre_wrap_width)
        };
        let out_wrapped = if out_wrap_width == 0 {
            Vec::new()
        } else {
            word_wrap_lines(&out_trimmed, out_wrap_width)
        };

        let clamp_len = |len: usize| -> u16 { len.min(u16::MAX as usize) as u16 };
        let pre_total = clamp_len(pre_wrapped.len());
        let out_block_total = clamp_len(out_wrapped.len());

        let layout = Rc::new(ExecLayoutCache {
            width,
            pre_lines: pre_wrapped,
            out_lines: out_wrapped,
            pre_total,
            out_block_total,
        });
        *self.cached_layout.borrow_mut() = Some(layout.clone());
        layout
    }
    // Build separate segments: (preamble lines, output lines)
    fn exec_render_parts(
        &self,
    ) -> (
        Vec<Line<'static>>,
        Vec<Line<'static>>,
        Option<Line<'static>>,
    ) {
        if let (Some(pre), Some(out)) = (
            self.cached_pre_lines.borrow().as_ref(),
            self.cached_out_lines.borrow().as_ref(),
        ) {
            if self.output.is_some() {
                return (pre.clone(), out.clone(), None);
            }
            if self.stream_preview.is_some() {
                let wait_state = self.wait_state_snapshot();
                let status_label = if wait_state.waiting {
                    "Waiting"
                } else {
                    "Running"
                };
                let status = self.streaming_status_line_for_label(status_label);
                return (pre.clone(), out.clone(), status);
            }
        }

        let wait_state = self.wait_state_snapshot();
        let status_label = if wait_state.waiting {
            "Waiting"
        } else {
            "Running"
        };

        let (pre, mut out, status) = if self.parsed.is_empty() {
            if let (Some(pre_cached), Some(out_cached)) = (
                self.cached_pre_lines.borrow().as_ref(),
                self.cached_out_lines.borrow().as_ref(),
            ) {
                let status_cached = if self.output.is_none() {
                    self.streaming_status_line_for_label(status_label)
                } else {
                    None
                };
                return (pre_cached.clone(), out_cached.clone(), status_cached);
            }

            self.exec_render_parts_generic(status_label)
        } else {
            if self.output.is_some() {
                if let (Some(pre_cached), Some(out_cached)) = (
                    self.cached_pre_lines.borrow().as_ref(),
                    self.cached_out_lines.borrow().as_ref(),
                ) {
                    return (pre_cached.clone(), out_cached.clone(), None);
                }
            }

            match self.parsed_meta.as_ref() {
                Some(meta) => exec_render_parts_parsed_with_meta(
                    &self.parsed,
                    meta,
                    self.output.as_ref(),
                    self.stream_preview.as_ref(),
                    self.start_time,
                    status_label,
                ),
                None => exec_render_parts_parsed(
                    &self.parsed,
                    self.output.as_ref(),
                    self.stream_preview.as_ref(),
                    self.start_time,
                    status_label,
                ),
            }
        };

        if self.output.is_some() {
            let extra_lines = self.wait_extras(&wait_state);
            if !extra_lines.is_empty() {
                let is_blank_line = |line: &Line<'static>| {
                    line.spans
                        .iter()
                        .all(|span| span.content.as_ref().trim().is_empty())
                };
                let is_error_line = |line: &Line<'static>| {
                    line.spans
                        .first()
                        .map(|span| span.content.as_ref().starts_with("Error (exit code"))
                        .unwrap_or(false)
                };
                let insert_at = if let Some(pos) = out.iter().position(is_error_line) {
                    pos
                } else {
                    out.len()
                };

                let mut block: Vec<Line<'static>> = Vec::new();
                if insert_at > 0 && !is_blank_line(&out[insert_at - 1]) {
                    block.push(Line::from(""));
                }
                block.extend(extra_lines.into_iter());
                if insert_at < out.len() {
                    if !is_blank_line(&out[insert_at]) {
                        block.push(Line::from(""));
                    }
                } else {
                    block.push(Line::from(""));
                }

                out.splice(insert_at..insert_at, block);
            }
            *self.cached_pre_lines.borrow_mut() = Some(pre.clone());
            *self.cached_out_lines.borrow_mut() = Some(out.clone());
        } else if self.output.is_none() {
            *self.cached_pre_lines.borrow_mut() = Some(pre.clone());
            *self.cached_out_lines.borrow_mut() = Some(out.clone());
        }
        (pre, out, status)
    }

    pub(crate) fn update_stream_preview(&mut self, stdout: &str, stderr: &str) {
        if stdout.is_empty() && stderr.is_empty() {
            if self.stream_preview.is_none() {
                return;
            }
            self.stream_preview = None;
        } else {
            self.stream_preview = Some(CommandOutput {
                exit_code: STREAMING_EXIT_CODE,
                stdout: stdout.to_string(),
                stderr: stderr.to_string(),
            });
        }
        self.invalidate_render_caches();
    }

    fn exec_render_parts_generic(
        &self,
        status_label: &str,
    ) -> (
        Vec<Line<'static>>,
        Vec<Line<'static>>,
        Option<Line<'static>>,
    ) {
        let mut pre = self.generic_command_lines();
        let display_output = self.output.as_ref().or(self.stream_preview.as_ref());
        let mut out = output_lines(display_output, false, false);
        let has_output = !trim_empty_lines(out.clone()).is_empty();

        if self.output.is_none() && has_output {
            if let Some(last) = pre.last_mut() {
                last.spans.insert(
                    0,
                    Span::styled("┌ ", Style::default().fg(crate::colors::text_dim())),
                );
            }
        }

        let mut status = None;
        if self.output.is_none() {
            let status_line = self.streaming_status_line_for_label(status_label);
            if status_line.is_some() {
                if let Some(last) = out.last() {
                    let is_blank = last
                        .spans
                        .iter()
                        .all(|sp| sp.content.as_ref().trim().is_empty());
                    if is_blank {
                        out.pop();
                    }
                }
            }
            status = status_line;
        }

        (pre, out, status)
    }

    fn generic_command_lines(&self) -> Vec<Line<'static>> {
        if let Some(cached) = self.cached_command_lines.borrow().as_ref() {
            return cached.clone();
        }

        let command_escaped = strip_bash_lc_and_escape(&self.command);
        let formatted = format_inline_script_for_display(&command_escaped);
        let normalized = normalize_shell_command_display(&formatted);
        let command_display = insert_line_breaks_after_double_ampersand(&normalized);

        let mut highlighted_cmd =
            crate::syntax_highlight::highlight_code_block(&command_display, Some("bash"));
        for (idx, line) in highlighted_cmd.iter_mut().enumerate() {
            emphasize_shell_command_name(line);
            if idx > 0 {
                line.spans.insert(
                    0,
                    Span::styled("  ", Style::default().fg(crate::colors::text())),
                );
            }
        }

        let owned: Vec<Line<'static>> = highlighted_cmd;
        *self.cached_command_lines.borrow_mut() = Some(owned.clone());
        owned
    }

    fn streaming_status_line(&self) -> Option<Line<'static>> {
        if self.output.is_some() {
            return None;
        }
        let wait_state = self.wait_state_snapshot();
        let status_label = if wait_state.waiting {
            "Waiting"
        } else {
            "Running"
        };
        self.streaming_status_line_for_label(status_label)
    }

    fn streaming_status_line_for_label(&self, status_label: &str) -> Option<Line<'static>> {
        if self.output.is_some() {
            return None;
        }

        if self.parsed.is_empty() {
            let mut message = format!("{status_label}...");
            if let Some(start) = self.start_time {
                let elapsed = start.elapsed();
                if !elapsed.is_zero() {
                    message = format!("{message} ({})", format_duration(elapsed));
                }
            }
            return Some(running_status_line(message));
        }

        let meta = match self.parsed_meta.as_ref() {
            Some(meta) => meta,
            None => return None,
        };
        if !matches!(meta.action, ExecAction::Run) {
            return None;
        }

        let mut message = match meta.ctx_path.as_deref() {
            Some(p) => format!("{status_label}... in {p}"),
            None => format!("{status_label}..."),
        };
        if let Some(start) = self.start_time {
            let elapsed = start.elapsed();
            if !elapsed.is_zero() {
                message = format!("{message} ({})", format_duration(elapsed));
            }
        }
        Some(running_status_line(message))
    }
}

// ==================== DiffCell ====================

pub(crate) struct DiffCell {
    pub(crate) lines: Vec<Line<'static>>,
}

impl HistoryCell for DiffCell {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn kind(&self) -> HistoryCellType {
        HistoryCellType::Diff
    }
    fn display_lines(&self) -> Vec<Line<'static>> {
        self.lines.clone()
    }
    fn has_custom_render(&self) -> bool {
        true
    }
    fn custom_render_with_skip(&self, area: Rect, buf: &mut Buffer, mut skip_rows: u16) {
        // Render a two-column diff with a 1-col marker gutter and 1-col padding
        // so wrapped lines hang under their first content column.
        // Hard clear the entire area: write spaces + background so any
        // previously longer content does not bleed into shorter frames.
        let bg = Style::default().bg(crate::colors::background());
        fill_rect(buf, area, Some(' '), bg);

        // Center the sign in the two-column gutter by leaving one leading
        // space and drawing the sign in the second column.
        let marker_col_x = area.x.saturating_add(2); // two spaces then '+'/'-'
        let content_x = area.x.saturating_add(4); // two spaces before sign + one after sign
        let content_w = area.width.saturating_sub(4);
        let mut cur_y = area.y;

        // Helper to classify a line and extract marker and content
        let classify = |l: &Line<'_>| -> (Option<char>, Line<'static>, Style) {
            let text: String = l
                .spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>();
            let default_style = Style::default().fg(crate::colors::text());
            if text.starts_with("+") && !text.starts_with("+++") {
                let content = text.chars().skip(1).collect::<String>();
                (
                    Some('+'),
                    Line::from(content).style(Style::default().fg(crate::colors::success())),
                    default_style,
                )
            } else if text.starts_with("-") && !text.starts_with("---") {
                let content = text.chars().skip(1).collect::<String>();
                (
                    Some('-'),
                    Line::from(content).style(Style::default().fg(crate::colors::error())),
                    default_style,
                )
            } else if text.starts_with("@@") {
                (
                    None,
                    Line::from(text).style(Style::default().fg(crate::colors::primary())),
                    default_style,
                )
            } else {
                (None, Line::from(text), default_style)
            }
        };

        'outer: for line in &self.lines {
            // Measure this line at wrapped width
            let (_marker, content_line, _sty) = classify(line);
            let content_text = Text::from(vec![content_line.clone()]);
            let rows: u16 = Paragraph::new(content_text.clone())
                .wrap(Wrap { trim: false })
                .line_count(content_w)
                .try_into()
                .unwrap_or(0);

            let mut local_skip = 0u16;
            if skip_rows > 0 {
                if skip_rows >= rows {
                    skip_rows -= rows;
                    continue 'outer;
                } else {
                    local_skip = skip_rows;
                    skip_rows = 0;
                }
            }

            // Remaining height available
            if cur_y >= area.y.saturating_add(area.height) {
                break;
            }
            let avail = area.y.saturating_add(area.height) - cur_y;
            let draw_h = rows.saturating_sub(local_skip).min(avail);
            if draw_h == 0 {
                continue;
            }

            // Draw content with hanging indent (left margin = 2)
            let content_area = Rect {
                x: content_x,
                y: cur_y,
                width: content_w,
                height: draw_h,
            };
            let block = Block::default().style(bg);
            Paragraph::new(content_text)
                .block(block)
                .wrap(Wrap { trim: false })
                .scroll((local_skip, 0))
                .style(bg)
                .render(content_area, buf);

            // Draw marker on the first visible visual row of this logical line
            let (marker, _content_line2, _) = classify(line);
            if let Some(m) = marker {
                if local_skip == 0 && area.width >= 1 {
                    let color = if m == '+' {
                        crate::colors::success()
                    } else {
                        crate::colors::error()
                    };
                    let style = Style::default().fg(color).bg(crate::colors::background());
                    buf.set_string(marker_col_x, cur_y, m.to_string(), style);
                }
            }

            cur_y = cur_y.saturating_add(draw_h);
            if cur_y >= area.y.saturating_add(area.height) {
                break;
            }
        }
    }
}

// ==================== MergedExecCell ====================
// Represents multiple completed exec results merged into one cell while preserving
// the bordered, dimmed output styling for each command's stdout/stderr preview.

pub(crate) struct MergedExecCell {
    // Sequence of (preamble lines, output lines) for each completed exec
    segments: Vec<(Vec<Line<'static>>, Vec<Line<'static>>)>,
    // Choose icon/behavior based on predominant action kind for gutter
    kind: ExecKind,
}

impl MergedExecCell {
    pub(crate) fn exec_kind(&self) -> ExecKind {
        self.kind
    }
    pub(crate) fn from_exec(exec: &ExecCell) -> Self {
        let (pre, out, _) = exec.exec_render_parts();
        let kind = match exec.parsed_action() {
            ExecAction::Read => ExecKind::Read,
            ExecAction::Search => ExecKind::Search,
            ExecAction::List => ExecKind::List,
            ExecAction::Run => ExecKind::Run,
        };
        Self {
            segments: vec![(pre, out)],
            kind,
        }
    }
    pub(crate) fn push_exec(&mut self, exec: &ExecCell) {
        let (pre, out, _) = exec.exec_render_parts();
        self.segments.push((pre, out));
    }

    // Build an aggregated preamble for Read segments by concatenating
    // all per-exec preambles and coalescing contiguous ranges for the
    // same file. Returns None for non-Read kinds.
    fn aggregated_read_preamble_lines(&self) -> Option<Vec<Line<'static>>> {
        if self.kind != ExecKind::Read {
            return None;
        }
        use ratatui::text::Span;
        // Concatenate per-segment preambles (without their headers), but KEEP ONLY
        // read-like entries. Then normalize the connector so only the very first
        // visible line uses a corner marker and subsequent lines use two spaces.
        // Finally, coalesce contiguous ranges for the same file.

        // Local helper: parse a read range line of the form
        // "└ <file> (lines A to B)" or "  <file> (lines A to B)".
        fn parse_read_line(line: &Line<'_>) -> Option<(String, u32, u32)> {
            if line.spans.is_empty() {
                return None;
            }
            let first = line.spans[0].content.as_ref();
            if !(first == "└ " || first == "  ") {
                return None;
            }
            let rest: String = line
                .spans
                .iter()
                .skip(1)
                .map(|s| s.content.as_ref())
                .collect();
            if let Some(idx) = rest.rfind(" (lines ") {
                let fname = rest[..idx].to_string();
                let tail = &rest[idx + 1..];
                if tail.starts_with("(lines ") && tail.ends_with(")") {
                    let inner = &tail[7..tail.len().saturating_sub(1)];
                    if let Some((a, b)) = inner.split_once(" to ") {
                        if let (Ok(s), Ok(e)) = (a.trim().parse::<u32>(), b.trim().parse::<u32>()) {
                            return Some((fname, s, e));
                        }
                    }
                }
            }
            None
        }

        // Heuristic: identify search-like lines (e.g., "… in dir/" or " (in dir)") so
        // they can be dropped from a Read aggregation if they slipped in.
        fn is_search_like(line: &Line<'_>) -> bool {
            let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            let t = text.trim();
            t.contains(" (in ")
                || t.rsplit_once(" in ")
                    .map(|(_, rhs)| rhs.trim_end().ends_with('/'))
                    .unwrap_or(false)
        }

        let mut kept: Vec<Line<'static>> = Vec::new();
        for (seg_idx, (pre_raw, _)) in self.segments.iter().enumerate() {
            let mut pre = trim_empty_lines(pre_raw.clone());
            if !pre.is_empty() {
                pre.remove(0);
            } // drop per-exec header
            // Filter: keep definite read-range lines; drop obvious search-like lines.
            for l in pre.into_iter() {
                if is_search_like(&l) {
                    continue;
                }
                // Prefer lines that parse as read ranges; otherwise allow if they are not search-like.
                let keep = parse_read_line(&l).is_some() || seg_idx == 0; // be permissive for first segment
                if !keep {
                    continue;
                }
                kept.push(l);
            }
        }

        if kept.is_empty() {
            return Some(kept);
        }

        // Normalize connector: first visible line uses "└ ", later lines use "  ".
        if let Some(first) = kept.first_mut() {
            let flat: String = first.spans.iter().map(|s| s.content.as_ref()).collect();
            let has_connector = flat.trim_start().starts_with("└ ");
            if !has_connector {
                first.spans.insert(
                    0,
                    Span::styled("└ ", Style::default().fg(crate::colors::text_dim())),
                );
            }
        }
        for l in kept.iter_mut().skip(1) {
            if let Some(sp0) = l.spans.get_mut(0) {
                if sp0.content.as_ref() == "└ " {
                    sp0.content = "  ".into();
                    // Keep dim styling for alignment consistency
                    sp0.style = sp0.style.add_modifier(Modifier::DIM);
                }
            }
        }

        // Merge adjacent/overlapping ranges in-place
        coalesce_read_ranges_in_lines_local(&mut kept);
        Some(kept)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_core::parse_command::ParsedCommand;

    #[test]
    fn action_enum_from_parsed_variants() {
        // Read
        let parsed = vec![ParsedCommand::Read {
            name: "foo.txt".into(),
            cmd: "sed -n '1,10p' foo.txt".into(),
        }];
        assert!(matches!(action_enum_from_parsed(&parsed), ExecAction::Read));
        // Search
        let parsed = vec![ParsedCommand::Search {
            query: Some("term".into()),
            path: Some("src".into()),
            cmd: "rg term src".into(),
        }];
        assert!(matches!(
            action_enum_from_parsed(&parsed),
            ExecAction::Search
        ));
        // Listå
        let parsed = vec![ParsedCommand::ListFiles {
            cmd: "ls -la".into(),
            path: Some(".".into()),
        }];
        assert!(matches!(action_enum_from_parsed(&parsed), ExecAction::List));
        // Default → Run
        let parsed = vec![ParsedCommand::Unknown {
            cmd: "echo hi".into(),
        }];
        assert!(matches!(action_enum_from_parsed(&parsed), ExecAction::Run));
        // Empty → Run
        let parsed: Vec<ParsedCommand> = vec![];
        assert!(matches!(action_enum_from_parsed(&parsed), ExecAction::Run));
    }

    #[test]
    fn build_preview_lines_preserves_tabbed_columns() {
        let sample = "AGENTS.md\t\tdocs\t\t\tpackage.json\r\n\
build-fast.sh\t\tflake.lock\t\tpnpm-lock.yaml\r\n\
CHANGELOG.md\t\tflake.nix\t\tpnpm-workspace.yaml\r\n\
cliff.toml\t\tFormula\t\t\tREADME.md\r\n\
codex-cli\t\thomebrew-tap\t\trelease-notes\r\n\
codex-rs\t\tLICENSE\t\t\tscripts\r\n\
config.toml.example\tNOTICE\r\n";

        let lines = build_preview_lines(sample, false);
        assert!(lines.len() >= 6);

        let first_line: String = lines[0]
            .spans
            .iter()
            .map(|span| span.content.to_string())
            .collect();
        assert!(first_line.contains("AGENTS.md"));
        assert!(first_line.contains("docs"));
        assert!(first_line.contains("package.json"));
        assert!(
            first_line.contains("  docs"),
            "expected spaces between ls columns: {first_line:?}"
        );
    }

    #[test]
    fn format_inline_python_breaks_semicolons() {
        let command = vec![
            "python".to_string(),
            "-c".to_string(),
            "import os; print('hi')".to_string(),
        ];
        let escaped = strip_bash_lc_and_escape(&command);
        let formatted = format_inline_python_for_display(&escaped);
        eprintln!("Formatted output:\n{}", formatted);
        assert!(formatted.contains("python -c '\n") || formatted.contains("python -c \""), "Should contain python -c with quote");
        assert!(formatted.contains("import os"), "Should contain import statement");
        assert!(formatted.contains("print('hi')") || formatted.contains("print("), "Should contain print statement");
    }

    #[test]
    fn format_inline_python_preserves_simple_snippet() {
        let command = vec![
            "python".to_string(),
            "-c".to_string(),
            "print('hi')".to_string(),
        ];
        let escaped = strip_bash_lc_and_escape(&command);
        let formatted = format_inline_python_for_display(&escaped);
        assert_eq!(formatted, escaped);
    }

    #[test]
    fn inspect_python_heredoc_strip() {
        let command = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "python3 - <<'PY'\nimport os\nroot = '/tmp'\nprint(root)\nPY".to_string(),
        ];
        let escaped = strip_bash_lc_and_escape(&command);
        assert!(escaped.contains("\n"));
        assert!(escaped.contains("<<'PY'"));
    }

    #[test]
    fn format_inline_python_formats_heredoc() {
        let sanitized = "python3 - <<'PY' from pathlib import Path root = Path(.) candidates = [] for path in root.rglob(*.py): try: size = path.stat().st_size except (PermissionError, FileNotFoundError): continue candidates.append((size, path)) candidates.sort(reverse=True) for size, path in candidates[:10]: print(f{size:>9} bytes - {path}) PY";
        let formatted = format_inline_python_for_display(sanitized);
        assert!(formatted.contains("'<<PY'\n"));
        assert!(formatted.contains("from pathlib import Path"));
        assert!(formatted.contains("root = Path(.)"));
        assert!(formatted.contains("    candidates = []"));
        assert!(formatted.contains("    for path in root.rglob(*.py):"));
        assert!(formatted.contains("        try:"));
        assert!(formatted.contains("            size = path.stat().st_size"));
        assert!(formatted.contains("        except (PermissionError, FileNotFoundError):"));
        assert!(formatted.contains("            continue"));
        assert!(formatted.contains("    candidates.append((size, path))"));
        assert!(formatted.contains("    candidates.sort(reverse=True)"));
        assert!(formatted.contains("    for size, path in candidates[:10]:"));
        assert!(formatted.contains("        print(f{size:>9} bytes - {path})"));
        assert!(formatted.trim_end().ends_with("PY"));
    }

    #[test]
    fn format_inline_python_splits_chained_assignments() {
        let sanitized = "python3 - <<'PY' import os py_count = 0 total_size = 0 for root, _, files in os.walk(.): for name in files: if name.endswith(.py): py_count += 1 total_size += os.path.getsize(os.path.join(root, name)) print(Total Python files:, py_count) print(Approx total size (KB):, round(total_size / 1024, 1)) PY";
        let formatted = format_inline_python_for_display(sanitized);
        assert!(formatted.contains("    py_count = 0"));
        assert!(formatted.contains("    total_size = 0"));
        assert!(formatted.contains("            py_count += 1"));
        assert!(
            formatted
                .contains("            total_size += os.path.getsize(os.path.join(root, name))")
        );
        assert!(formatted.contains("        print(Total Python files:, py_count)"));
        assert!(
            formatted
                .contains("        print(Approx total size (KB):, round(total_size / 1024, 1))")
        );
    }

    #[test]
    fn format_inline_node_script_indents_blocks() {
        let sanitized = "node -e 'const fs = require(\'fs\'); let count = 0; [\'a.js\', \'b.js\'].forEach(file => { count += 1; console.log(file); }); if (count > 0) { console.log(`Total: ${count}`); }'";
        let formatted = format_inline_script_for_display(sanitized);
        assert!(formatted.contains("node -e '\n"));
        assert!(formatted.contains("    const fs = require(fs);"));
        assert!(formatted.contains("    let count = 0;"));
        assert!(
            formatted
                .contains("    [a.js, b.js].forEach(file => { count += 1; console.log(file); });")
        );
        assert!(formatted.contains("    if (count > 0) { console.log(`Total: ${count}`); }"));
    }

    #[test]
    fn format_inline_shell_script_breaks_on_semicolons() {
        let sanitized = "bash -c 'set -e; echo start; for f in *.rs; do echo $f; done'";
        let formatted = format_inline_script_for_display(sanitized);
        assert!(formatted.contains("bash -c '\n"));
        assert!(formatted.contains("    set -e;"));
        assert!(formatted.contains("    echo start;"));
        assert!(formatted.contains("    for f in *.rs"));
        assert!(formatted.contains("    do echo $f;"));
        assert!(formatted.contains("    done"));
        assert!(formatted.trim_end().ends_with("'"));
    }

    #[test]
    fn merged_exec_cell_push_and_kind() {
        // Build two completed ExecCell instances for Read
        let parsed = vec![ParsedCommand::Read {
            name: "foo.txt".into(),
            cmd: "sed -n '1,10p' foo.txt".into(),
        }];
        let e1 = new_completed_exec_command(
            vec!["sed".into(), "-n".into(), "1,10p".into(), "foo.txt".into()],
            parsed.clone(),
            CommandOutput {
                exit_code: 0,
                stdout: "ok".into(),
                stderr: String::new(),
            },
        );
        let e2 = new_completed_exec_command(
            vec!["sed".into(), "-n".into(), "11,20p".into(), "foo.txt".into()],
            parsed,
            CommandOutput {
                exit_code: 0,
                stdout: "ok2".into(),
                stderr: String::new(),
            },
        );
        let mut merged = MergedExecCell::from_exec(&e1);
        assert!(matches!(merged.exec_kind(), ExecKind::Read));
        // Push the second and ensure it keeps kind and adds a segment
        let before = merged.segments.len();
        merged.push_exec(&e2);
        assert_eq!(merged.segments.len(), before + 1);
        assert!(matches!(merged.exec_kind(), ExecKind::Read));
    }

    #[test]
    fn parse_read_line_annotation_handles_common_tools() {
        // sed -n 'A,Bp'
        assert_eq!(
            parse_read_line_annotation("sed -n '5,42p' foo.txt"),
            Some("(lines 5 to 42)".into())
        );
        // head -n N
        assert_eq!(
            parse_read_line_annotation("head -n 100 foo.txt"),
            Some("(lines 1 to 100)".into())
        );
        // bare head => default 10
        assert_eq!(
            parse_read_line_annotation("git show HEAD:file | head"),
            Some("(lines 1 to 10)".into())
        );
        // tail -n +K
        assert_eq!(
            parse_read_line_annotation("tail -n +20 foo.txt"),
            Some("(from 20 to end)".into())
        );
        // tail -n N
        assert_eq!(
            parse_read_line_annotation("tail -n 50 foo.txt"),
            Some("(last 50 lines)".into())
        );
        // bare tail => default 10
        assert_eq!(
            parse_read_line_annotation("git show HEAD:file | tail"),
            Some("(last 10 lines)".into())
        );
        // Unrelated command
        assert_eq!(parse_read_line_annotation("cat foo.txt"), None);
    }

    #[test]
    fn strip_redundant_pipes_when_annotated() {
        let cmd = "git show upstream/main:codex-rs/core/src/codex.rs | sed -n '2160,2640p'";
        let (ann, _) = parse_read_line_annotation_with_range(cmd);
        assert!(ann.is_some());
        let cleaned = strip_redundant_line_filter_pipes(cmd);
        assert!(cleaned.starts_with("git show upstream/main:codex-rs/core/src/codex.rs"));
        assert!(!cleaned.contains("| sed -n"));

        let cmd2 = "nl -ba core/src/parse_command.rs | sed -n '1200,1720p'";
        let (ann2, _) = parse_read_line_annotation_with_range(cmd2);
        assert!(ann2.is_some());
        let cleaned2 = strip_redundant_line_filter_pipes(cmd2);
        assert_eq!(cleaned2, "nl -ba core/src/parse_command.rs");

        let cmd3 = "git show HEAD:file | head";
        let (ann3, _) = parse_read_line_annotation_with_range(cmd3);
        assert!(ann3.is_some());
        let cleaned3 = strip_redundant_line_filter_pipes(cmd3);
        assert_eq!(cleaned3, "git show HEAD:file");
    }

    #[test]
    fn bold_detection_sets_flag_for_long_commands() {
        let cell = new_active_exec_command(
            vec!["bash".into(), "-lc".into(), "cargo build".into()],
            vec![ParsedCommand::Unknown {
                cmd: "cargo build".into(),
            }],
        );
        assert!(cell.has_bold_command());
    }

    #[test]
    fn short_commands_do_not_set_bold_flag() {
        let cell = new_active_exec_command(
            vec!["bash".into(), "-lc".into(), "ls".into()],
            vec![ParsedCommand::Unknown { cmd: "ls".into() }],
        );
        assert!(!cell.has_bold_command());
    }

    #[test]
    fn gutter_symbol_shows_for_long_run_commands() {
        let cell = new_active_exec_command(
            vec!["bash".into(), "-lc".into(), "cargo build".into()],
            vec![ParsedCommand::Unknown {
                cmd: "cargo build".into(),
            }],
        );
        assert_eq!(cell.gutter_symbol(), Some("❯"));
    }

    #[test]
    fn completed_exec_preserves_gutter_symbol_for_long_commands() {
        let cell = new_completed_exec_command(
            vec!["bash".into(), "-lc".into(), "cargo build".into()],
            vec![ParsedCommand::Unknown {
                cmd: "cargo build".into(),
            }],
            CommandOutput {
                exit_code: 0,
                stdout: "ok".into(),
                stderr: String::new(),
            },
        );
        assert_eq!(cell.gutter_symbol(), Some("❯"));
    }

    #[test]
    fn shell_wrappers_still_preserve_gutter_symbol() {
        let cell = new_active_exec_command(
            vec!["/bin/sh".into(), "-lc".into(), "cargo build".into()],
            vec![ParsedCommand::Unknown {
                cmd: "cargo build".into(),
            }],
        );
        assert_eq!(cell.gutter_symbol(), Some("❯"));
    }
}

impl HistoryCell for MergedExecCell {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn kind(&self) -> HistoryCellType {
        HistoryCellType::Exec {
            kind: self.kind,
            status: ExecStatus::Success,
        }
    }
    fn desired_height(&self, width: u16) -> u16 {
        // Match custom_render_with_skip exactly:
        // - Shared header row for non-Run kinds (1)
        // - For each segment:
        //   - Measure preamble after dropping the per-segment header when present
        //     and normalizing the leading "└ " prefix at full `width`.
        //   - Measure output inside a left-bordered block with left padding,
        //     which reduces the effective wrapping width by 2 columns.
        let mut total: u16 = if self.kind == ExecKind::Run { 0 } else { 1 };
        let pre_wrap_width = width;
        let out_wrap_width = width.saturating_sub(2);

        if let Some(agg_pre) = self.aggregated_read_preamble_lines() {
            let pre_rows: u16 = Paragraph::new(Text::from(agg_pre))
                .wrap(Wrap { trim: false })
                .line_count(pre_wrap_width)
                .try_into()
                .unwrap_or(0);
            total = total.saturating_add(pre_rows);
            for (_pre_raw, out_raw) in &self.segments {
                let out = trim_empty_lines(out_raw.clone());
                let out_rows: u16 = Paragraph::new(Text::from(out))
                    .wrap(Wrap { trim: false })
                    .line_count(out_wrap_width)
                    .try_into()
                    .unwrap_or(0);
                total = total.saturating_add(out_rows);
            }
            return total;
        }

        let mut added_corner = false;
        for (pre_raw, out_raw) in &self.segments {
            // Build preamble like the renderer: trim, drop first header line, ensure prefix
            let mut pre = trim_empty_lines(pre_raw.clone());
            if self.kind != ExecKind::Run && !pre.is_empty() {
                pre.remove(0);
            }
            if self.kind != ExecKind::Run {
                if let Some(first) = pre.first_mut() {
                    let flat: String = first.spans.iter().map(|s| s.content.as_ref()).collect();
                    let has_corner = flat.trim_start().starts_with("└ ");
                    let has_spaced_corner = flat.trim_start().starts_with("  └ ");
                    if !added_corner {
                        if !(has_corner || has_spaced_corner) {
                            first.spans.insert(
                                0,
                                Span::styled("└ ", Style::default().fg(crate::colors::text_dim())),
                            );
                        }
                        added_corner = true;
                    } else {
                        // For subsequent segments, ensure no leading corner; use two spaces instead.
                        if let Some(sp0) = first.spans.get_mut(0) {
                            if sp0.content.as_ref() == "└ " {
                                sp0.content = "  ".into();
                                sp0.style = sp0.style.add_modifier(Modifier::DIM);
                            }
                        }
                    }
                }
            }
            let out = trim_empty_lines(out_raw.clone());
            let pre_rows: u16 = Paragraph::new(Text::from(pre))
                .wrap(Wrap { trim: false })
                .line_count(pre_wrap_width)
                .try_into()
                .unwrap_or(0);
            let out_rows: u16 = Paragraph::new(Text::from(out))
                .wrap(Wrap { trim: false })
                .line_count(out_wrap_width)
                .try_into()
                .unwrap_or(0);
            total = total.saturating_add(pre_rows).saturating_add(out_rows);
        }

        total
    }
    fn display_lines(&self) -> Vec<Line<'static>> {
        // Fallback textual form: concatenate all preambles + outputs with blank separators.
        let mut out: Vec<Line<'static>> = Vec::new();
        for (i, (pre, body)) in self.segments.iter().enumerate() {
            if i > 0 {
                out.push(Line::from(""));
            }
            out.extend(trim_empty_lines(pre.clone()));
            out.extend(trim_empty_lines(body.clone()));
        }
        out
    }
    fn has_custom_render(&self) -> bool {
        true
    }
    fn custom_render_with_skip(&self, area: Rect, buf: &mut Buffer, mut skip_rows: u16) {
        // Shared header for non-Run kinds (e.g., "Read") then each segment's command + output.
        let bg = Style::default()
            .bg(crate::colors::background())
            .fg(crate::colors::text());
        // Hard clear area first
        fill_rect(buf, area, Some(' '), bg);

        // Build one header line based on exec kind
        let header_line = match self.kind {
            ExecKind::Read => Some(Line::styled(
                "Read",
                Style::default().fg(crate::colors::text()),
            )),
            ExecKind::Search => Some(Line::styled(
                "Search",
                Style::default().fg(crate::colors::text_dim()),
            )),
            ExecKind::List => Some(Line::styled(
                "List",
                Style::default().fg(crate::colors::text()),
            )),
            ExecKind::Run => None,
        };

        let mut cur_y = area.y;
        let end_y = area.y.saturating_add(area.height);

        // Render or skip header line
        if let Some(header_line) = header_line {
            if skip_rows == 0 {
                if cur_y < end_y {
                    let txt = Text::from(vec![header_line]);
                    Paragraph::new(txt)
                        .block(Block::default().style(bg))
                        .wrap(Wrap { trim: false })
                        .render(
                            Rect {
                                x: area.x,
                                y: cur_y,
                                width: area.width,
                                height: 1,
                            },
                            buf,
                        );
                    cur_y = cur_y.saturating_add(1);
                }
            } else {
                skip_rows = skip_rows.saturating_sub(1);
            }
        }

        // Helper: ensure only the very first preamble line across all segments gets the corner
        let mut added_corner: bool = false;
        let mut ensure_prefix = |lines: &mut Vec<Line<'static>>| {
            if self.kind == ExecKind::Run {
                return;
            }
            if let Some(first) = lines.first_mut() {
                let flat: String = first.spans.iter().map(|s| s.content.as_ref()).collect();
                let has_corner = flat.trim_start().starts_with("└ ");
                let has_spaced_corner = flat.trim_start().starts_with("  └ ");
                if !added_corner {
                    if !(has_corner || has_spaced_corner) {
                        first.spans.insert(
                            0,
                            Span::styled("└ ", Style::default().fg(crate::colors::text_dim())),
                        );
                    }
                    added_corner = true;
                } else {
                    // For subsequent segments, replace any leading corner with two spaces
                    if let Some(sp0) = first.spans.get_mut(0) {
                        if sp0.content.as_ref() == "└ " {
                            sp0.content = "  ".into();
                            sp0.style = sp0.style.add_modifier(Modifier::DIM);
                        }
                    }
                }
            }
        };

        // Special aggregated rendering for Read: collapse file ranges
        if self.kind == ExecKind::Read {
            // Build aggregated preamble once
            let agg_pre = self.aggregated_read_preamble_lines().unwrap_or_else(|| {
                // Fallback: concatenate per-segment preambles
                let mut all: Vec<Line<'static>> = Vec::new();
                for (i, (pre_raw, _)) in self.segments.iter().enumerate() {
                    let mut pre = trim_empty_lines(pre_raw.clone());
                    if !pre.is_empty() {
                        pre.remove(0);
                    }
                    if i == 0 {
                        // ensure leading corner (legacy for Read aggregation)
                        if let Some(first) = pre.first_mut() {
                            let flat: String =
                                first.spans.iter().map(|s| s.content.as_ref()).collect();
                            let already = flat.trim_start().starts_with("└ ")
                                || flat.trim_start().starts_with("  └ ");
                            if !already {
                                first.spans.insert(
                                    0,
                                    Span::styled(
                                        "└ ",
                                        Style::default().fg(crate::colors::text_dim()),
                                    ),
                                );
                            }
                        }
                    }
                    all.extend(pre);
                }
                all
            });

            // Header was already handled above (including skip accounting).
            // Render aggregated preamble next using the current skip_rows.
            let pre_text = Text::from(agg_pre);
            let pre_wrap_width = area.width;
            let pre_total: u16 = Paragraph::new(pre_text.clone())
                .wrap(Wrap { trim: false })
                .line_count(pre_wrap_width)
                .try_into()
                .unwrap_or(0);
            if cur_y < end_y {
                let pre_skip = skip_rows.min(pre_total);
                let pre_remaining = pre_total.saturating_sub(pre_skip);
                let pre_height = pre_remaining.min(end_y.saturating_sub(cur_y));
                if pre_height > 0 {
                    Paragraph::new(pre_text)
                        .block(Block::default().style(bg))
                        .wrap(Wrap { trim: false })
                        .scroll((pre_skip, 0))
                        .style(bg)
                        .render(
                            Rect {
                                x: area.x,
                                y: cur_y,
                                width: area.width,
                                height: pre_height,
                            },
                            buf,
                        );
                    cur_y = cur_y.saturating_add(pre_height);
                }
                skip_rows = skip_rows.saturating_sub(pre_skip);
            }

            // Render each segment's output only
            let out_wrap_width = area.width.saturating_sub(2);
            for (_pre_raw, out_raw) in self.segments.iter() {
                if cur_y >= end_y {
                    break;
                }
                let out = trim_empty_lines(out_raw.clone());
                let out_text = Text::from(out.clone());
                let out_total: u16 = Paragraph::new(out_text.clone())
                    .wrap(Wrap { trim: false })
                    .line_count(out_wrap_width)
                    .try_into()
                    .unwrap_or(0);
                let out_skip = skip_rows.min(out_total);
                let out_remaining = out_total.saturating_sub(out_skip);
                let out_height = out_remaining.min(end_y.saturating_sub(cur_y));
                if out_height > 0 {
                    let out_area = Rect {
                        x: area.x,
                        y: cur_y,
                        width: area.width,
                        height: out_height,
                    };
                    let block = Block::default()
                        .borders(Borders::LEFT)
                        .border_style(
                            Style::default()
                                .fg(crate::colors::border_dim())
                                .bg(crate::colors::background()),
                        )
                        .style(Style::default().bg(crate::colors::background()))
                        .padding(Padding {
                            left: 1,
                            right: 0,
                            top: 0,
                            bottom: 0,
                        });
                    Paragraph::new(out_text)
                        .block(block)
                        .wrap(Wrap { trim: false })
                        .scroll((out_skip, 0))
                        .style(
                            Style::default()
                                .bg(crate::colors::background())
                                .fg(crate::colors::text_dim()),
                        )
                        .render(out_area, buf);
                    cur_y = cur_y.saturating_add(out_height);
                }
                skip_rows = skip_rows.saturating_sub(out_skip);
            }
            return;
        }

        for (pre_raw, out_raw) in self.segments.iter() {
            if cur_y >= end_y {
                break;
            }
            // Drop the per-segment header line (first element)
            let mut pre = trim_empty_lines(pre_raw.clone());
            if self.kind != ExecKind::Run && !pre.is_empty() {
                pre.remove(0);
            }
            // Normalize command prefix for generic execs (only on the first segment)
            ensure_prefix(&mut pre);

            let out = trim_empty_lines(out_raw.clone());

            // Measure with same widths as ExecCell
            let pre_text = Text::from(pre.clone());
            let out_text = Text::from(out.clone());
            let pre_wrap_width = area.width;
            let out_wrap_width = area.width.saturating_sub(2);
            let pre_total: u16 = Paragraph::new(pre_text.clone())
                .wrap(Wrap { trim: false })
                .line_count(pre_wrap_width)
                .try_into()
                .unwrap_or(0);
            let out_total: u16 = Paragraph::new(out_text.clone())
                .wrap(Wrap { trim: false })
                .line_count(out_wrap_width)
                .try_into()
                .unwrap_or(0);

            // Apply skip to pre, then out
            let pre_skip = skip_rows.min(pre_total);
            let out_skip = skip_rows.saturating_sub(pre_total).min(out_total);

            // Render pre
            let pre_remaining = pre_total.saturating_sub(pre_skip);
            let pre_height = pre_remaining.min(end_y.saturating_sub(cur_y));
            if pre_height > 0 {
                Paragraph::new(pre_text)
                    .block(Block::default().style(bg))
                    .wrap(Wrap { trim: false })
                    .scroll((pre_skip, 0))
                    .style(bg)
                    .render(
                        Rect {
                            x: area.x,
                            y: cur_y,
                            width: area.width,
                            height: pre_height,
                        },
                        buf,
                    );
                cur_y = cur_y.saturating_add(pre_height);
            }

            if cur_y >= end_y {
                break;
            }
            // Render out as bordered, dim block
            let out_remaining = out_total.saturating_sub(out_skip);
            let out_height = out_remaining.min(end_y.saturating_sub(cur_y));
            if out_height > 0 {
                let out_area = Rect {
                    x: area.x,
                    y: cur_y,
                    width: area.width,
                    height: out_height,
                };
                let block = Block::default()
                    .borders(Borders::LEFT)
                    .border_style(
                        Style::default()
                            .fg(crate::colors::border_dim())
                            .bg(crate::colors::background()),
                    )
                    .style(Style::default().bg(crate::colors::background()))
                    .padding(Padding {
                        left: 1,
                        right: 0,
                        top: 0,
                        bottom: 0,
                    });
                Paragraph::new(out_text)
                    .block(block)
                    .wrap(Wrap { trim: false })
                    .scroll((out_skip, 0))
                    .style(
                        Style::default()
                            .bg(crate::colors::background())
                            .fg(crate::colors::text_dim()),
                    )
                    .render(out_area, buf);
                cur_y = cur_y.saturating_add(out_height);
            }

            // Consume skip rows used in this segment
            let consumed = pre_total + out_total;
            skip_rows = skip_rows.saturating_sub(consumed);
        }
    }
}

fn exec_render_parts_parsed_with_meta(
    parsed_commands: &[ParsedCommand],
    meta: &ParsedExecMetadata,
    output: Option<&CommandOutput>,
    stream_preview: Option<&CommandOutput>,
    start_time: Option<Instant>,
    status_label: &str,
) -> (
    Vec<Line<'static>>,
    Vec<Line<'static>>,
    Option<Line<'static>>,
) {
    let action = meta.action;
    let ctx_path = meta.ctx_path.as_deref();
    let suppress_run_header = matches!(action, ExecAction::Run) && output.is_some();
    let mut pre: Vec<Line<'static>> = Vec::new();
    let mut running_status: Option<Line<'static>> = None;
    if !suppress_run_header {
        match output {
            None => match action {
                ExecAction::Read => pre.push(Line::styled(
                    "Read",
                    Style::default().fg(crate::colors::text()),
                )),
                ExecAction::Search => pre.push(Line::styled(
                    "Search",
                    Style::default().fg(crate::colors::text_dim()),
                )),
                ExecAction::List => pre.push(Line::styled(
                    "List",
                    Style::default().fg(crate::colors::text()),
                )),
                ExecAction::Run => {
                    let mut message = match &ctx_path {
                        Some(p) => format!("{}... in {p}", status_label),
                        None => format!("{}...", status_label),
                    };
                    if let Some(start) = start_time {
                        let elapsed = start.elapsed();
                        message = format!("{message} ({})", format_duration(elapsed));
                    }
                    running_status = Some(running_status_line(message));
                }
            },
            Some(o) if o.exit_code == 0 => {
                let done = match action {
                    ExecAction::Read => "Read".to_string(),
                    ExecAction::Search => "Search".to_string(),
                    ExecAction::List => "List".to_string(),
                    ExecAction::Run => match &ctx_path {
                        Some(p) => format!("Ran in {}", p),
                        None => "Ran".to_string(),
                    },
                };
                if matches!(
                    action,
                    ExecAction::Read | ExecAction::Search | ExecAction::List
                ) {
                    pre.push(Line::styled(
                        done,
                        Style::default().fg(crate::colors::text_dim()),
                    ));
                } else {
                    pre.push(Line::styled(
                        done,
                        Style::default()
                            .fg(crate::colors::text_bright())
                            .add_modifier(Modifier::BOLD),
                    ));
                }
            }
            Some(_) => {
                let done = match action {
                    ExecAction::Read => "Read".to_string(),
                    ExecAction::Search => "Search".to_string(),
                    ExecAction::List => "List".to_string(),
                    ExecAction::Run => match &ctx_path {
                        Some(p) => format!("Ran in {}", p),
                        None => "Ran".to_string(),
                    },
                };
                if matches!(
                    action,
                    ExecAction::Read | ExecAction::Search | ExecAction::List
                ) {
                    pre.push(Line::styled(
                        done,
                        Style::default().fg(crate::colors::text_dim()),
                    ));
                } else {
                    pre.push(Line::styled(
                        done,
                        Style::default()
                            .fg(crate::colors::text_bright())
                            .add_modifier(Modifier::BOLD),
                    ));
                }
            }
        }
    }

    // Reuse the same parsed-content rendering as new_parsed_command
    let search_paths = &meta.search_paths;
    // Compute output preview first to know whether to draw the downward corner.
    let show_stdout = matches!(action, ExecAction::Run);
    let display_output = output.or(stream_preview);
    let mut out = output_lines(display_output, !show_stdout, false);
    let mut any_content_emitted = false;
    // Determine allowed label(s) for this cell's primary action
    let expected_label: Option<&'static str> = match action {
        ExecAction::Read => Some("Read"),
        ExecAction::Search => Some("Search"),
        ExecAction::List => Some("List"),
        ExecAction::Run => None, // run: allow a set of labels
    };
    let use_content_connectors = !(matches!(action, ExecAction::Run) && output.is_none());

    for parsed in parsed_commands.iter() {
        let (label, content) = match parsed {
            ParsedCommand::Read { name, cmd, .. } => {
                let mut c = name.clone();
                if let Some(ann) = parse_read_line_annotation(cmd) {
                    c = format!("{} {}", c, ann);
                }
                ("Read".to_string(), c)
            }
            ParsedCommand::ListFiles { cmd: _, path } => match path {
                Some(p) => {
                    if search_paths.contains(p) {
                        (String::new(), String::new())
                    } else {
                        let display_p = if p.ends_with('/') {
                            p.to_string()
                        } else {
                            format!("{}/", p)
                        };
                        ("List".to_string(), format!("{}", display_p))
                    }
                }
                None => ("List".to_string(), "./".to_string()),
            },
            ParsedCommand::Search { query, path, cmd } => {
                // Make search terms human-readable:
                // - Unescape any backslash-escaped character (e.g., "\?" -> "?")
                // - Close unbalanced pairs for '(' and '{' to avoid dangling text in UI
                let prettify_term = |s: &str| -> String {
                    // General unescape: remove backslashes that escape the next char
                    let mut out = String::with_capacity(s.len());
                    let mut iter = s.chars();
                    while let Some(ch) = iter.next() {
                        if ch == '\\' {
                            if let Some(next) = iter.next() {
                                out.push(next);
                            } else {
                                out.push('\\');
                            }
                        } else {
                            out.push(ch);
                        }
                    }

                    // Balance parentheses
                    let opens_paren = out.matches("(").count();
                    let closes_paren = out.matches(")").count();
                    for _ in 0..opens_paren.saturating_sub(closes_paren) {
                        out.push(')');
                    }

                    // Balance curly braces
                    let opens_curly = out.matches("{").count();
                    let closes_curly = out.matches("}").count();
                    for _ in 0..opens_curly.saturating_sub(closes_curly) {
                        out.push('}');
                    }

                    out
                };
                let fmt_query = |q: &str| -> String {
                    let mut parts: Vec<String> = q
                        .split('|')
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                        .map(prettify_term)
                        .collect();
                    match parts.len() {
                        0 => String::new(),
                        1 => parts.remove(0),
                        2 => format!("{} and {}", parts[0], parts[1]),
                        _ => {
                            let last = parts.last().cloned().unwrap_or_default();
                            let head = &parts[..parts.len() - 1];
                            format!("{} and {}", head.join(", "), last)
                        }
                    }
                };
                match (query, path) {
                    (Some(q), Some(p)) => {
                        let display_p = if p.ends_with('/') {
                            p.to_string()
                        } else {
                            format!("{}/", p)
                        };
                        (
                            "Search".to_string(),
                            format!("{} in {}", fmt_query(q), display_p),
                        )
                    }
                    (Some(q), None) => ("Search".to_string(), format!("{}", fmt_query(q))),
                    (None, Some(p)) => {
                        let display_p = if p.ends_with('/') {
                            p.to_string()
                        } else {
                            format!("{}/", p)
                        };
                        ("Search".to_string(), format!(" in {}", display_p))
                    }
                    (None, None) => ("Search".to_string(), cmd.clone()),
                }
            }
            ParsedCommand::ReadCommand { cmd } => ("Run".to_string(), cmd.clone()),
            // Upstream variants not present in our core parser are ignored or treated as generic runs
            ParsedCommand::Unknown { cmd } => {
                // Suppress separator helpers like `echo ---` which are used
                // internally to delimit chunks when reading files.
                let t = cmd.trim();
                let lower = t.to_lowercase();
                if lower.starts_with("echo") && lower.contains("---") {
                    (String::new(), String::new()) // drop from preamble
                } else {
                    ("Run".to_string(), format_inline_script_for_display(cmd))
                }
            } // Noop variant not present in our core parser
              // ParsedCommand::Noop { .. } => continue,
        };
        // Enforce per-action grouping: only keep entries matching this cell's action.
        if let Some(exp) = expected_label {
            if label != exp {
                continue;
            }
        } else if !(label == "Run" || label == "Search") {
            // For generic "run" header, keep common run-like labels only.
            continue;
        }
        if label.is_empty() && content.is_empty() {
            continue;
        }
        for line_text in content.lines() {
            if line_text.is_empty() {
                continue;
            }
            let prefix = if !any_content_emitted {
                if suppress_run_header || !use_content_connectors {
                    ""
                } else {
                    "└ "
                }
            } else if suppress_run_header || !use_content_connectors {
                ""
            } else {
                "  "
            };
            let mut spans: Vec<Span<'static>> = Vec::new();
            if !prefix.is_empty() {
                spans.push(Span::styled(
                    prefix,
                    Style::default().add_modifier(Modifier::DIM),
                ));
            }
            match label.as_str() {
                "Search" => {
                    let remaining = line_text.to_string();
                    let (terms_part, path_part) = if let Some(idx) = remaining.rfind(" (in ") {
                        (
                            remaining[..idx].to_string(),
                            Some(remaining[idx..].to_string()),
                        )
                    } else if let Some(idx) = remaining.rfind(" in ") {
                        let suffix = &remaining[idx + 1..];
                        if suffix.trim_end().ends_with('/') {
                            (
                                remaining[..idx].to_string(),
                                Some(remaining[idx..].to_string()),
                            )
                        } else {
                            (remaining.clone(), None)
                        }
                    } else {
                        (remaining.clone(), None)
                    };
                    let tmp = terms_part.clone();
                    let chunks: Vec<String> = if tmp.contains(", ") {
                        tmp.split(", ").map(|s| s.to_string()).collect()
                    } else {
                        vec![tmp.clone()]
                    };
                    for (i, chunk) in chunks.iter().enumerate() {
                        if i > 0 {
                            spans.push(Span::styled(
                                ", ",
                                Style::default().fg(crate::colors::text_dim()),
                            ));
                        }
                        if let Some((left, right)) = chunk.rsplit_once(" and ") {
                            if !left.is_empty() {
                                spans.push(Span::styled(
                                    left.to_string(),
                                    Style::default().fg(crate::colors::text()),
                                ));
                                spans.push(Span::styled(
                                    " and ",
                                    Style::default().fg(crate::colors::text_dim()),
                                ));
                                spans.push(Span::styled(
                                    right.to_string(),
                                    Style::default().fg(crate::colors::text()),
                                ));
                            } else {
                                spans.push(Span::styled(
                                    chunk.to_string(),
                                    Style::default().fg(crate::colors::text()),
                                ));
                            }
                        } else {
                            spans.push(Span::styled(
                                chunk.to_string(),
                                Style::default().fg(crate::colors::text()),
                            ));
                        }
                    }
                    if let Some(p) = path_part {
                        spans.push(Span::styled(
                            p,
                            Style::default().fg(crate::colors::text_dim()),
                        ));
                    }
                }
                "Read" => {
                    if let Some(idx) = line_text.find(" (") {
                        let (fname, rest) = line_text.split_at(idx);
                        spans.push(Span::styled(
                            fname.to_string(),
                            Style::default().fg(crate::colors::text()),
                        ));
                        spans.push(Span::styled(
                            rest.to_string(),
                            Style::default().fg(crate::colors::text_dim()),
                        ));
                    } else {
                        spans.push(Span::styled(
                            line_text.to_string(),
                            Style::default().fg(crate::colors::text()),
                        ));
                    }
                }
                "List" => {
                    spans.push(Span::styled(
                        line_text.to_string(),
                        Style::default().fg(crate::colors::text()),
                    ));
                }
                _ => {
                    // Apply shell syntax highlighting to executed command lines.
                    // We highlight the single logical line as bash and append its spans inline.
                    let normalized = normalize_shell_command_display(line_text);
                    let display_line = insert_line_breaks_after_double_ampersand(&normalized);
                    let mut hl =
                        crate::syntax_highlight::highlight_code_block(&display_line, Some("bash"));
                    if let Some(mut first_line) = hl.pop() {
                        emphasize_shell_command_name(&mut first_line);
                        spans.extend(first_line.spans.into_iter());
                    } else {
                        spans.push(Span::styled(
                            display_line,
                            Style::default().fg(crate::colors::text()),
                        ));
                    }
                }
            }
            pre.push(Line::from(spans));
            any_content_emitted = true;
        }
    }

    // If this is a List cell and nothing emitted (e.g., suppressed due to matching Search path),
    // still show a single contextual line so users can see where we listed.
    if matches!(action, ExecAction::List) && !any_content_emitted {
        let display_p = match &ctx_path {
            Some(p) if !p.is_empty() => {
                if p.ends_with('/') {
                    p.to_string()
                } else {
                    format!("{p}/")
                }
            }
            _ => "./".to_string(),
        };
        pre.push(Line::from(vec![
            Span::styled("└ ", Style::default().add_modifier(Modifier::DIM)),
            Span::styled(
                format!("{display_p}"),
                Style::default().fg(crate::colors::text()),
            ),
        ]));
    }

    // Collapse adjacent Read ranges for the same file inside a single exec's preamble
    coalesce_read_ranges_in_lines_local(&mut pre);

    // Output: show stdout only for real run commands; errors always included
    // Collapse adjacent Read ranges for the same file inside a single exec's preamble
    coalesce_read_ranges_in_lines_local(&mut pre);

    if running_status.is_some() {
        if let Some(last) = out.last() {
            let is_blank = last
                .spans
                .iter()
                .all(|sp| sp.content.as_ref().trim().is_empty());
            if is_blank {
                out.pop();
            }
        }
    }

    (pre, out, running_status)
}

fn exec_render_parts_parsed(
    parsed_commands: &[ParsedCommand],
    output: Option<&CommandOutput>,
    stream_preview: Option<&CommandOutput>,
    start_time: Option<Instant>,
    status_label: &str,
) -> (
    Vec<Line<'static>>,
    Vec<Line<'static>>,
    Option<Line<'static>>,
) {
    let meta = ParsedExecMetadata::from_commands(parsed_commands);
    exec_render_parts_parsed_with_meta(
        parsed_commands,
        &meta,
        output,
        stream_preview,
        start_time,
        status_label,
    )
}

// Local helper: coalesce "<file> (lines A to B)" entries when contiguous.
fn coalesce_read_ranges_in_lines_local(lines: &mut Vec<Line<'static>>) {
    use ratatui::style::Modifier;
    use ratatui::style::Style;
    use ratatui::text::Span;
    // Nothing to do for empty/single line vectors
    if lines.len() <= 1 {
        return;
    }

    // Parse a content line of the form
    //   "└ <file> (lines A to B)" or "  <file> (lines A to B)"
    // into (filename, start, end, prefix, original_index).
    fn parse_read_line_with_index(
        idx: usize,
        line: &Line<'_>,
    ) -> Option<(String, u32, u32, String, usize)> {
        if line.spans.is_empty() {
            return None;
        }
        let prefix = line.spans[0].content.to_string();
        if !(prefix == "└ " || prefix == "  ") {
            return None;
        }
        let rest: String = line
            .spans
            .iter()
            .skip(1)
            .map(|s| s.content.as_ref())
            .collect();
        if let Some(i) = rest.rfind(" (lines ") {
            let fname = rest[..i].to_string();
            let tail = &rest[i + 1..];
            if tail.starts_with("(lines ") && tail.ends_with(")") {
                let inner = &tail[7..tail.len() - 1];
                if let Some((s1, s2)) = inner.split_once(" to ") {
                    if let (Ok(a), Ok(b)) = (s1.trim().parse::<u32>(), s2.trim().parse::<u32>()) {
                        return Some((fname, a, b, prefix, idx));
                    }
                }
            }
        }
        None
    }

    // Collect read ranges grouped by filename, preserving first-seen order.
    // Also track the earliest prefix to reuse when emitting a single line per file.
    #[derive(Default)]
    struct FileRanges {
        prefix: String,
        first_index: usize,
        ranges: Vec<(u32, u32)>,
    }

    let mut files: Vec<(String, FileRanges)> = Vec::new();
    let mut non_read_lines: Vec<Line<'static>> = Vec::new();

    for (idx, line) in lines.iter().enumerate() {
        if let Some((fname, a, b, prefix, orig_idx)) = parse_read_line_with_index(idx, line) {
            // Insert or update entry for this file, preserving encounter order
            if let Some((_name, fr)) = files.iter_mut().find(|(n, _)| n == &fname) {
                fr.ranges.push((a.min(b), a.max(b)));
                // Keep earliest index as stable ordering anchor
                if orig_idx < fr.first_index {
                    fr.first_index = orig_idx;
                }
            } else {
                files.push((
                    fname,
                    FileRanges {
                        prefix,
                        first_index: orig_idx,
                        ranges: vec![(a.min(b), a.max(b))],
                    },
                ));
            }
        } else {
            non_read_lines.push(line.clone());
        }
    }

    if files.is_empty() {
        return;
    }

    // For each file: merge overlapping/touching ranges; then sort ascending and emit one line.
    fn merge_and_sort(mut v: Vec<(u32, u32)>) -> Vec<(u32, u32)> {
        if v.len() <= 1 {
            return v;
        }
        v.sort_by_key(|(s, _)| *s);
        let mut out: Vec<(u32, u32)> = Vec::with_capacity(v.len());
        let mut cur = v[0];
        for &(s, e) in v.iter().skip(1) {
            if s <= cur.1.saturating_add(1) {
                // touching or overlap
                cur.1 = cur.1.max(e);
            } else {
                out.push(cur);
                cur = (s, e);
            }
        }
        out.push(cur);
        out
    }

    // Rebuild the lines vector: keep header (if present) and any non-read lines,
    // then append one consolidated line per file in first-seen order by index.
    let mut rebuilt: Vec<Line<'static>> = Vec::with_capacity(lines.len());

    // Heuristic: preserve an initial header line that does not start with a connector.
    if !lines.is_empty() {
        if lines[0]
            .spans
            .first()
            .map(|s| s.content.as_ref() != "└ " && s.content.as_ref() != "  ")
            .unwrap_or(false)
        {
            rebuilt.push(lines[0].clone());
        }
    }

    // Sort files by their first appearance index to keep stable ordering with other files.
    files.sort_by_key(|(_n, fr)| fr.first_index);

    for (name, mut fr) in files.into_iter() {
        fr.ranges = merge_and_sort(fr.ranges);
        // Build range annotation: " (lines S1 to E1, S2 to E2, ...)"
        let mut ann = String::new();
        ann.push_str(" (");
        ann.push_str("lines ");
        for (i, (s, e)) in fr.ranges.iter().enumerate() {
            if i > 0 {
                ann.push_str(", ");
            }
            ann.push_str(&format!("{} to {}", s, e));
        }
        ann.push(')');

        let spans: Vec<Span<'static>> = vec![
            Span::styled(fr.prefix, Style::default().add_modifier(Modifier::DIM)),
            Span::styled(name, Style::default().fg(crate::colors::text())),
            Span::styled(ann, Style::default().fg(crate::colors::text_dim())),
        ];
        rebuilt.push(Line::from(spans));
    }

    // Append any other non-read lines (rare for Read sections, but safe)
    // Note: keep their original order after consolidated entries
    rebuilt.extend(non_read_lines.into_iter());

    *lines = rebuilt;
}

impl WidgetRef for &ExecCell {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        Paragraph::new(Text::from(self.display_lines_trimmed()))
            .wrap(Wrap { trim: false })
            .style(Style::default().bg(crate::colors::background()))
            .render(area, buf);
    }
}

/// Return the emoji followed by a hair space (U+200A) and a normal space.
/// This creates a reasonable gap across different terminals,
/// in particular Terminal.app and iTerm, which render too tightly with just a single normal space.
///
/// Improvements here could be to condition this behavior on terminal,
/// or possibly on emoji.
// Removed unused helpers padded_emoji and padded_emoji_with.

pub(crate) fn new_completed_wait_tool_call(target: String, duration: Duration) -> WaitStatusCell {
    let mut duration_str = format_duration(duration);
    if duration_str.ends_with(" 00s") {
        duration_str.truncate(duration_str.len().saturating_sub(4));
    }

    let header = crate::history::WaitStatusHeader {
        title: "Waited".to_string(),
        title_tone: crate::history::TextTone::Success,
        summary: Some(duration_str),
        summary_tone: crate::history::TextTone::Dim,
    };

    let mut details: Vec<crate::history::WaitStatusDetail> = Vec::new();
    if !target.is_empty() {
        details.push(crate::history::WaitStatusDetail {
            label: "for".to_string(),
            value: Some(target),
            tone: crate::history::TextTone::Dim,
        });
    }

    let state = crate::history::WaitStatusState {
        id: crate::history::HistoryId::ZERO,
        header,
        details,
    };

    WaitStatusCell::new(state)
}

// ==================== StreamingContentCell ====================
// For live streaming content that's being actively rendered

pub(crate) struct StreamingContentCell {
    pub(crate) id: Option<String>,
    lines: Vec<Line<'static>>,
    // Show an ellipsis on a new line while streaming is in progress
    pub(crate) show_ellipsis: bool,
    // Cached per-width wrap plan to avoid re-segmentation; invalidated on extend
    cached_layout: std::cell::RefCell<Option<AssistantLayoutCache>>, // reuse same struct
}

impl HistoryCell for StreamingContentCell {
    // IMPORTANT: We must support immutable downcasting here. The TUI replaces
    // an in‑progress StreamingContentCell with a finalized AssistantMarkdownCell
    // by searching history via `c.as_any().downcast_ref::<StreamingContentCell>()`
    // and matching on the stream `id`. If this returns a dummy type (default impl)
    // instead of `self`, the lookup fails and the final cannot find the streaming
    // cell — leading to duplicates (final gets appended instead of replaced).
    // See: chatwidget.rs::insert_final_answer_with_id and related logs
    // ("final-answer: append new AssistantMarkdownCell (no prior cell)").
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn kind(&self) -> HistoryCellType {
        HistoryCellType::Assistant
    }
    fn has_custom_render(&self) -> bool {
        true
    }
    fn desired_height(&self, width: u16) -> u16 {
        let plan = self.ensure_stream_layout(width);
        let mut total = plan.total_rows_with_padding;
        if self.show_ellipsis {
            total = total.saturating_add(1);
        }
        total
    }
    fn custom_render_with_skip(&self, area: Rect, buf: &mut Buffer, skip_rows: u16) {
        // Render with a 1-row top and bottom padding, all using the assistant bg tint.
        let cell_bg = crate::colors::assistant_bg();
        let bg_style = Style::default().bg(cell_bg);

        // Hard clear area with assistant background
        fill_rect(buf, area, Some(' '), bg_style);

        // Build or reuse cached segments for this width
        let plan = self.ensure_stream_layout(area.width);
        let text_wrap_width = area.width;
        let mut segs = plan.segs.clone();
        let mut seg_rows = plan.seg_rows.clone();
        if self.show_ellipsis {
            // Animated three-dot indicator with a rotating middle dot (·):
            // frames: "...", "·..", ".·.", "..·", "...".
            // Keep it subtle and only show during streaming like the old ellipsis.
            const FRAMES: [&str; 5] = ["...", "·..", ".·.", "..·", "..."];
            let frame_idx = (SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                / 200) as usize
                % FRAMES.len();
            let frame = FRAMES[frame_idx];

            let ellipsis_line = Line::styled(
                frame.to_string(),
                Style::default().fg(crate::colors::text_dim()),
            );
            let wrapped = word_wrap_lines(&[ellipsis_line], text_wrap_width);
            seg_rows.push(wrapped.len() as u16);
            segs.push(AssistantSeg::Text(wrapped));
        }

        // Streaming-style top padding row
        let mut remaining_skip = skip_rows;
        let mut cur_y = area.y;
        let end_y = area.y.saturating_add(area.height);
        if remaining_skip == 0 && cur_y < end_y {
            cur_y = cur_y.saturating_add(1);
        }
        remaining_skip = remaining_skip.saturating_sub(1);

        // Helpers
        #[derive(Debug, Clone)]
        enum Seg {
            Text(Vec<Line<'static>>),
            Bullet(Vec<Line<'static>>),
            Code(Vec<Line<'static>>),
        }
        use unicode_width::UnicodeWidthStr as UW;
        let measure_line =
            |l: &Line<'_>| -> usize { l.spans.iter().map(|s| UW::width(s.content.as_ref())).sum() };
        let mut draw_segment = |seg: &Seg, y: &mut u16, skip: &mut u16| {
            if *y >= end_y {
                return;
            }
            match seg {
                Seg::Text(lines) => {
                    let txt = Text::from(lines.clone());
                    let total: u16 = Paragraph::new(txt.clone())
                        .wrap(Wrap { trim: false })
                        .line_count(text_wrap_width)
                        .try_into()
                        .unwrap_or(0);
                    if *skip >= total {
                        *skip -= total;
                        return;
                    }
                    let avail = end_y.saturating_sub(*y);
                    let draw_h = (total.saturating_sub(*skip)).min(avail);
                    if draw_h == 0 {
                        return;
                    }
                    let rect = Rect {
                        x: area.x,
                        y: *y,
                        width: area.width,
                        height: draw_h,
                    };
                    Paragraph::new(txt)
                        .block(Block::default().style(bg_style))
                        .wrap(Wrap { trim: false })
                        .scroll((*skip, 0))
                        .style(bg_style)
                        .render(rect, buf);
                    *y = y.saturating_add(draw_h);
                    *skip = 0;
                }
                Seg::Bullet(lines) => {
                    let total = lines.len() as u16;
                    if *skip >= total {
                        *skip -= total;
                        return;
                    }
                    let avail = end_y.saturating_sub(*y);
                    let draw_h = (total.saturating_sub(*skip)).min(avail);
                    if draw_h == 0 {
                        return;
                    }
                    let rect = Rect {
                        x: area.x,
                        y: *y,
                        width: area.width,
                        height: draw_h,
                    };
                    let txt = Text::from(lines.clone());
                    Paragraph::new(txt)
                        .block(Block::default().style(bg_style))
                        .scroll((*skip, 0))
                        .style(bg_style)
                        .render(rect, buf);
                    *y = y.saturating_add(draw_h);
                    *skip = 0;
                }
                Seg::Code(lines_in) => {
                    if lines_in.is_empty() {
                        return;
                    }
                    // Extract optional language sentinel and drop it from the content lines
                    let mut lang_label: Option<String> = None;
                    let mut lines = lines_in.clone();
                    if let Some(first) = lines.first() {
                        let flat: String = first.spans.iter().map(|s| s.content.as_ref()).collect();
                        if let Some(s) = flat.strip_prefix("⟦LANG:") {
                            if let Some(end) = s.find('⟧') {
                                lang_label = Some(s[..end].to_string());
                                lines.remove(0);
                            }
                        }
                    }
                    if lines.is_empty() {
                        return;
                    }
                    // Determine target width of the code card (respect content width)
                    let max_w = lines.iter().map(|l| measure_line(l)).max().unwrap_or(0) as u16;
                    let inner_w = max_w.max(1);
                    // Borders (2) + inner left/right padding (4 total for two spaces each)
                    let card_w = inner_w.saturating_add(6).min(area.width.max(6));
                    // Include top/bottom border only (2); no inner vertical padding
                    let total = lines.len() as u16 + 2;
                    if *skip >= total {
                        *skip -= total;
                        return;
                    }
                    let avail = end_y.saturating_sub(*y);
                    if avail == 0 {
                        return;
                    }
                    // Compute visible slice of the card (accounting for inner padding rows)
                    let mut local_skip = *skip;
                    let mut top_border = 1u16;
                    if local_skip > 0 {
                        let drop = local_skip.min(top_border);
                        top_border -= drop;
                        local_skip -= drop;
                    }
                    let code_skip = local_skip.min(lines.len() as u16);
                    local_skip -= code_skip;
                    let mut bottom_border = 1u16;
                    if local_skip > 0 {
                        let drop = local_skip.min(bottom_border);
                        bottom_border -= drop;
                    }
                    let visible = top_border + (lines.len() as u16 - code_skip) + bottom_border;
                    let draw_h = visible.min(avail);
                    if draw_h == 0 {
                        return;
                    }
                    // Align card to content area (no outer left/right stripes)
                    let content_x = area.x;
                    let rect_x = content_x;
                    // Draw bordered block for the visible rows
                    let rect = Rect {
                        x: rect_x,
                        y: *y,
                        width: card_w,
                        height: draw_h,
                    };
                    let code_bg = crate::colors::code_block_bg();
                    let mut blk = Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(crate::colors::border()))
                        .style(Style::default().bg(code_bg))
                        .padding(Padding {
                            left: 2,
                            right: 2,
                            top: 0,
                            bottom: 0,
                        });
                    if let Some(lang) = &lang_label {
                        blk = blk.title(Span::styled(
                            format!(" {} ", lang),
                            Style::default().fg(crate::colors::text_dim()),
                        ));
                    }
                    let blk_for_inner = blk.clone();
                    blk.render(rect, buf);
                    // Inner paragraph area (exclude borders)
                    let inner_rect = blk_for_inner.inner(rect);
                    let inner_h = inner_rect.height.min(rect.height);
                    if inner_h > 0 {
                        let slice_start = code_skip as usize;
                        let txt = Text::from(lines[slice_start..].to_vec());
                        Paragraph::new(txt)
                            .style(Style::default().bg(code_bg))
                            .block(Block::default().style(Style::default().bg(code_bg)))
                            .render(inner_rect, buf);
                    }
                    // No outside padding stripes.
                    *y = y.saturating_add(draw_h);
                    *skip = 0;
                }
            }
        };

        for (i, seg) in segs.iter().enumerate() {
            if cur_y >= end_y {
                break;
            }
            // Fast-skip full segments using precomputed rows
            let rows = seg_rows.get(i).copied().unwrap_or(0);
            if remaining_skip >= rows {
                remaining_skip -= rows;
                continue;
            }
            let seg_draw = match seg {
                AssistantSeg::Text(lines) => Seg::Text(lines.clone()),
                AssistantSeg::Bullet(lines) => Seg::Bullet(lines.clone()),
                AssistantSeg::Code { lines, .. } => Seg::Code(lines.clone()),
            };
            draw_segment(&seg_draw, &mut cur_y, &mut remaining_skip);
        }
        // Bottom padding row (blank): area already cleared
        if remaining_skip == 0 && cur_y < end_y {
            cur_y = cur_y.saturating_add(1);
        } else {
            remaining_skip = remaining_skip.saturating_sub(1);
        }
        // Mark as used to satisfy unused_assignments lint
        let _ = (cur_y, remaining_skip);
    }
    fn display_lines(&self) -> Vec<Line<'static>> {
        // Hide a leading title header line (e.g., "codex") if present.
        // This mirrors AssistantMarkdownCell behavior so streaming and final
        // cells render identically with the header suppressed.
        let has_leading_header = self
            .lines
            .first()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
                    .trim()
                    .eq_ignore_ascii_case("codex")
            })
            .unwrap_or(false);

        if has_leading_header {
            if self.lines.len() == 1 {
                Vec::new()
            } else {
                self.lines[1..].to_vec()
            }
        } else {
            self.lines.clone()
        }
    }
}

impl StreamingContentCell {
    pub(crate) fn extend_lines(&mut self, mut new_lines: Vec<Line<'static>>) {
        if new_lines.is_empty() {
            return;
        }
        self.lines.append(&mut new_lines);
        // Invalidate cached plan so next render recomputes incrementally for current width
        *self.cached_layout.borrow_mut() = None;
    }

    pub(crate) fn retint(&mut self, old: &crate::theme::Theme, new: &crate::theme::Theme) {
        retint_lines_in_place(&mut self.lines, old, new);
        *self.cached_layout.borrow_mut() = None;
    }

    fn ensure_stream_layout(&self, width: u16) -> AssistantLayoutCache {
        if let Some(cache) = self.cached_layout.borrow().as_ref() {
            if cache.width == width {
                return cache.clone();
            }
        }
        // Reuse the same segmentation logic as Assistant, operating on current
        // lines. IMPORTANT: AssistantMarkdownCell::display_lines() hides the
        // first line as a synthetic header (e.g., "codex"). When we borrow its
        // layout engine, we must ensure a header line is present so the real
        // first content line is not dropped. Previously we removed the header
        // and passed only body lines, which caused the first content line to be
        // cut off during streaming and only appear once finalized.
        let mut body_lines = self.lines.clone();
        let mut had_header = false;
        if let Some(first) = body_lines.first() {
            let flat: String = first.spans.iter().map(|s| s.content.as_ref()).collect();
            if flat.trim().eq_ignore_ascii_case("codex") {
                had_header = true;
            }
        }
        // Prepend a hidden header if missing so Assistant layout doesn't drop
        // the first real content line. The header itself is suppressed by both
        // streaming and finalized render paths, so it never visibly appears.
        if !had_header {
            body_lines.insert(0, ratatui::text::Line::from("codex"));
        }
        let tmp = AssistantMarkdownCell {
            raw: String::new(),
            id: None,
            // We do not prepend a header; segmentation should be based on body only.
            lines: body_lines,
            cached_layout: std::cell::RefCell::new(None),
        };
        let cache = tmp.ensure_layout(width);
        *self.cached_layout.borrow_mut() = Some(cache.clone());
        cache
    }
}

// Detect lines that start with a markdown bullet produced by our renderer and return (indent, bullet)
fn detect_bullet_prefix(line: &ratatui::text::Line<'_>) -> Option<(usize, String)> {
    // Treat these as unordered bullets, plus checkbox glyphs for task lists.
    let bullets = ["-", "•", "◦", "·", "∘", "⋅", "☐", "✔"];
    let spans = &line.spans;
    if spans.is_empty() {
        return None;
    }
    // First span may be leading spaces
    let mut idx = 0;
    let mut indent = 0usize;
    if let Some(s) = spans.get(0) {
        let t = s.content.as_ref();
        if !t.is_empty() && t.chars().all(|c| c == ' ') {
            indent = t.chars().count();
            idx = 1;
        }
    }
    // Next must be a bullet-like prefix with an accompanying space. Accept either
    // a separate single-space span after the marker OR a trailing space baked
    // into the bullet span (e.g., checkboxes like "☐ ").
    let bullet_span = spans.get(idx)?;
    let mut bullet_text = bullet_span.content.as_ref().to_string();
    let has_following_space_span = spans
        .get(idx + 1)
        .map(|s| s.content.as_ref() == " ")
        .unwrap_or(false);
    let has_trailing_space_in_bullet = bullet_text.ends_with(' ');
    if !(has_following_space_span || has_trailing_space_in_bullet) {
        return None;
    }
    if has_trailing_space_in_bullet {
        bullet_text.pop();
    }
    if bullets.contains(&bullet_text.as_str()) {
        return Some((indent, bullet_text));
    }
    // Ordered list: e.g., "1.", "12.", etc.
    if bullet_text.len() >= 2
        && bullet_text.ends_with('.')
        && bullet_text[..bullet_text.len() - 1]
            .chars()
            .all(|c| c.is_ascii_digit())
    {
        return Some((indent, bullet_text));
    }
    // Fallback: derive from flattened text if span structure is unexpected.
    // This guards against upstream changes that merge or split the bullet/space spans.
    let flat: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    let mut chars = flat.chars().peekable();
    let mut indent_count = 0usize;
    while matches!(chars.peek(), Some(' ')) {
        chars.next();
        indent_count += 1;
    }
    // Capture token up to first whitespace
    let mut token = String::new();
    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() {
            break;
        }
        token.push(ch);
        chars.next();
        // Limit token length to avoid scanning entire lines on odd inputs
        if token.len() > 8 {
            break;
        }
    }
    // Require at least one whitespace after the token
    let has_space = matches!(chars.peek(), Some(c) if c.is_whitespace());
    if has_space {
        let bullets = ["-", "•", "◦", "·", "∘", "⋅", "☐", "✔"]; // same set
        if bullets.contains(&token.as_str())
            || (token.len() >= 2
                && token.ends_with('.')
                && token[..token.len() - 1].chars().all(|c| c.is_ascii_digit()))
        {
            return Some((indent_count, token));
        }
    }
    None
}

// Wrap a bullet line with a hanging indent so wrapped lines align under the content start.
fn wrap_bullet_line(
    mut line: ratatui::text::Line<'static>,
    indent_spaces: usize,
    bullet: &str,
    width: u16,
) -> Vec<ratatui::text::Line<'static>> {
    use ratatui::style::Style;
    use ratatui::text::Span;
    use unicode_width::UnicodeWidthStr as UWStr;

    // Apply a 1-col safety margin to reduce secondary wraps from Paragraph,
    // which can occur due to terminal-specific width differences (e.g.,
    // ambiguous-width glyphs, grapheme clusters). This keeps our prewrapped
    // bullet lines comfortably within the final render width.
    let width = width.saturating_sub(1) as usize;
    let mut spans = std::mem::take(&mut line.spans);
    // If the line contains OSC 8 hyperlinks (ESC ]8), avoid character-level
    // rewrapping to prevent breaking escape sequences. Fall back to default
    // Paragraph wrapping for this line by returning it unchanged.
    if spans.iter().any(|s| s.content.as_ref().contains('\u{1b}')) {
        line.spans = spans;
        return vec![line];
    }
    let mut i = 0usize;
    // Consume leading spaces span
    if i < spans.len() {
        let t = spans[i].content.as_ref();
        if t.chars().all(|c| c == ' ') {
            i += 1;
        }
    }
    // Consume bullet span and optional following single-space span. Support
    // cases where the bullet span already contains a trailing space (e.g., "☐ ").
    let bullet_style = if i < spans.len() {
        spans[i].style
    } else {
        Style::default()
    };
    if i < spans.len() {
        let bullet_span_text = spans[i].content.as_ref().to_string();
        i += 1; // consume bullet span
        if !bullet_span_text.ends_with(' ') && i < spans.len() && spans[i].content.as_ref() == " " {
            i += 1; // consume separate following space span
        }
    }

    // Remaining spans comprise the content; build grapheme clusters with style
    use unicode_segmentation::UnicodeSegmentation;
    let rest_spans = spans.drain(i..).collect::<Vec<_>>();
    let mut clusters: Vec<(String, Style)> = Vec::new();
    for sp in &rest_spans {
        let st = sp.style;
        for g in sp.content.as_ref().graphemes(true) {
            clusters.push((g.to_string(), st));
        }
    }

    // Some renderers may leave extra literal spaces between the bullet and the
    // first non-space glyph as part of the content (instead of a distinct
    // single-space span). Detect and incorporate those spaces into the hanging
    // indent, then drop them from the visible content so continuation lines
    // align perfectly under the start of the sentence.
    let mut leading_content_spaces: usize = 0;
    while leading_content_spaces < clusters.len()
        && (clusters[leading_content_spaces].0 == " "
            || clusters[leading_content_spaces].0 == "\u{3000}")
    {
        leading_content_spaces += 1;
    }

    // Prefix widths (display columns)
    let bullet_cols = UWStr::width(bullet);
    // Use a single space after the bullet so nested lists do not
    // render with an extra space ("·  item" -> "· item"). Keep the
    // hanging indent consistent so wrapped lines align under the
    // start of the bullet content.
    let gap_after_bullet = 1usize;
    let extra_gap = leading_content_spaces; // absorb any extra content-leading spaces
    let first_prefix = indent_spaces + bullet_cols + gap_after_bullet + extra_gap;
    let cont_prefix = indent_spaces + bullet_cols + gap_after_bullet + extra_gap; // keep continuation aligned

    let mut out: Vec<ratatui::text::Line<'static>> = Vec::new();
    let mut pos = leading_content_spaces;
    let mut first = true;
    while pos < clusters.len() {
        let avail_cols = if first {
            width.saturating_sub(first_prefix)
        } else {
            width.saturating_sub(cont_prefix)
        } as usize;
        let avail_cols = avail_cols.max(1);

        // Greedy take up to avail_cols, preferring to break at a preceding space cluster.
        let mut taken = 0usize; // number of clusters consumed
        let mut cols = 0usize; // display columns consumed
        let mut last_space_idx: Option<usize> = None; // index into clusters
        while pos + taken < clusters.len() {
            let (ref g, _) = clusters[pos + taken];
            let w = UWStr::width(g.as_str());
            if cols.saturating_add(w) > avail_cols {
                break;
            }
            cols += w;
            if g == " " || g == "\u{3000}" {
                last_space_idx = Some(pos + taken);
            }
            taken += 1;
            if cols == avail_cols {
                break;
            }
        }

        // Choose cut position:
        // - If the entire remaining content fits into this visual line, do NOT
        //   split at the last space — keep the final word on this line.
        // - Otherwise, prefer breaking at the last space within range; fall back
        //   to a hard break when no space is present (e.g., a long token).
        let (cut_end, next_start) = if pos + taken >= clusters.len() {
            (pos + taken, pos + taken)
        } else if let Some(space_idx) = last_space_idx {
            // Trim any spaces following the break point for next line start
            let mut next = space_idx;
            // cut_end excludes the space
            let mut cut = space_idx;
            // Also trim any trailing spaces before the break in this segment
            while cut > pos && clusters[cut - 1].0 == " " {
                cut -= 1;
            }
            // Advance next past contiguous spaces
            while next < clusters.len() && clusters[next].0 == " " {
                next += 1;
            }
            (cut, next)
        } else {
            // No space seen in range – hard break (very long word or first token longer than width)
            (pos + taken, pos + taken)
        };

        // If cut_end did not advance (e.g., segment starts with spaces), skip spaces and continue
        if cut_end <= pos {
            let mut p = pos;
            while p < clusters.len() && clusters[p].0 == " " {
                p += 1;
            }
            if p == pos {
                // safety: ensure forward progress
                p = pos + 1;
            }
            pos = p;
            continue;
        }

        let slice = &clusters[pos..cut_end];
        let mut seg_spans: Vec<Span<'static>> = Vec::new();
        // Build prefix spans
        if first {
            if indent_spaces > 0 {
                seg_spans.push(Span::raw(" ".repeat(indent_spaces)));
            }
            seg_spans.push(Span::styled(bullet.to_string(), bullet_style));
            // Two-space gap after bullet for readability and hanging indent
            seg_spans.push(Span::raw("  "));
        } else {
            seg_spans.push(Span::raw(" ".repeat(cont_prefix)));
        }
        // Build content spans coalescing same-style runs
        let mut cur_style = None::<Style>;
        let mut buf = String::new();
        for (g, st) in slice.iter() {
            if cur_style.map(|cs| cs == *st).unwrap_or(false) {
                buf.push_str(g);
            } else {
                if !buf.is_empty() {
                    seg_spans.push(Span::styled(std::mem::take(&mut buf), cur_style.unwrap()));
                }
                cur_style = Some(*st);
                buf.push_str(g);
            }
        }
        if !buf.is_empty() {
            seg_spans.push(Span::styled(buf, cur_style.unwrap()));
        }
        out.push(ratatui::text::Line::from(seg_spans));
        pos = next_start;
        first = false;
    }

    if out.is_empty() {
        // Ensure at least prefix-only line (edge case empty content)
        let mut seg_spans: Vec<Span<'static>> = Vec::new();
        if indent_spaces > 0 {
            seg_spans.push(Span::raw(" ".repeat(indent_spaces)));
        }
        seg_spans.push(Span::styled(bullet.to_string(), bullet_style));
        out.push(ratatui::text::Line::from(seg_spans));
    }

    out
}

// Wrap a line with a hanging indent of `indent_spaces + hang_cols` columns, without
// rendering a bullet glyph. This is used for the special case where we suppress the
// initial "-" bullet on the first assistant line, but still want continuation lines
// to align under where the content would begin (i.e., as if there were a bullet +
// two-space gap).

fn is_horizontal_rule_line(line: &ratatui::text::Line<'_>) -> bool {
    let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    let t = text.trim();
    if t.is_empty() {
        return false;
    }
    let chars: Vec<char> = t.chars().collect();
    // Allow optional spaces between characters
    let only = |ch: char| chars.iter().all(|c| *c == ch || c.is_whitespace());
    (only('-') && chars.iter().filter(|c| **c == '-').count() >= 3)
        || (only('*') && chars.iter().filter(|c| **c == '*').count() >= 3)
        || (only('_') && chars.iter().filter(|c| **c == '_').count() >= 3)
}

// Bold the first sentence (up to the first '.', '!' or '?' in the first non-empty line),
// or the entire first non-empty line if no terminator is present. Newlines already split lines.
// removed bold_first_sentence; renderer handles first sentence styling
/*
fn bold_first_sentence(mut lines: Vec<Line<'static>>) -> Vec<Line<'static>> {
    use ratatui::text::Span;
    use ratatui::style::Modifier;

    // Find the first non-empty line index
    let first_idx = match lines.iter().position(|l| {
        let s: String = l.spans.iter().map(|sp| sp.content.as_ref()).collect();
        !s.trim().is_empty()
    }) {
        Some(i) => i,
        None => return lines,
    };

    // Build the plain text of that line
    let line_text: String = lines[first_idx]
        .spans
        .iter()
        .map(|sp| sp.content.as_ref())
        .collect();

    // If the first non-space character is a bullet (•), do not bold.
    if line_text.chars().skip_while(|c| c.is_whitespace()).next() == Some('•') {
        return lines;
    }

    // Heuristic: pick first sentence terminator that is not part of a filename or common
    // abbreviation (e.g., "e.g.", "i.e."). Treat '.', '!' or '?' as terminators when
    // followed by whitespace/end or a closing quote then whitespace/end. Skip when the
    // next character is a letter/number (e.g., within filenames like example.sh).
    let mut boundary: Option<usize> = None; // char index inclusive
    let chars: Vec<char> = line_text.chars().collect();
    let len_chars = chars.len();
    for i in 0..len_chars {
        let ch = chars[i];
        if ch == '.' || ch == '!' || ch == '?' || ch == ':' {
            let next = chars.get(i + 1).copied();
            // Skip if next is alphanumeric (likely filename/identifier like example.sh)
            if matches!(next, Some(c) if c.is_ascii_alphanumeric()) { continue; }
            // Skip common abbreviation endings like "e.g." or "i.e." (match last 4 chars)
            if i >= 3 {
                let tail: String = chars[i - 3..=i].iter().collect::<String>().to_lowercase();
                if tail == "e.g." || tail == "i.e." { continue; }
            }
            // Accept if end of line,
            // or next is whitespace,
            // or next is quote then whitespace/end
            let ok = match next {
                None => true,
                Some(c) if c.is_whitespace() => true,
                Some('"') | Some('\'') => {
                    let n2 = chars.get(i + 2).copied();
                    n2.is_none() || n2.map(|c| c.is_whitespace()).unwrap_or(false)
                }
                _ => false,
            };
            if ok { boundary = Some(i); break; }
        }
    }

    // Bold up to and including the terminator.
    let bold_upto = boundary.map(|i| i + 1);

    // If there's no terminator or there's no additional content after it in the message,
    // do not bold (single-sentence message).
    if let Some(limit) = bold_upto {
        let mut has_more_in_line = false;
        // allow trailing quote right after terminator
        let mut idx = limit;
        if let Some('"') | Some('\'') = chars.get(idx) { idx += 1; }
        if idx < len_chars {
            has_more_in_line = chars[idx..].iter().any(|c| !c.is_whitespace());
        }
        let has_more_below = if !has_more_in_line {
            lines.iter().skip(first_idx + 1).any(|l| {
                let s: String = l.spans.iter().map(|sp| sp.content.as_ref()).collect();
                !s.trim().is_empty()
            })
        } else { true };
        if !has_more_below {
            return lines; // single-sentence message: leave as-is
        }
    } else {
        // No terminator at all → treat as single sentence; leave as-is
        return lines;
    }

    // Rebuild spans for the line with bold applied up to bold_upto (in chars)
    let mut new_spans: Vec<Span<'static>> = Vec::new();
    let mut consumed_chars: usize = 0;
    for sp in lines[first_idx].spans.drain(..) {
        let content = sp.content.into_owned();
        let len = content.chars().count();
        if bold_upto.is_none() {
            // Entire line bold
            let mut st = sp.style;
            st.add_modifier.insert(Modifier::BOLD);
            st.fg = Some(crate::colors::text_bright());
            new_spans.push(Span::styled(content, st));
            consumed_chars += len;
            continue;
        }
        let limit = bold_upto.unwrap();
        if consumed_chars >= limit {
            // After bold range, preserve original styling (do not strip bold)
            new_spans.push(Span::styled(content, sp.style));
            consumed_chars += len;
        } else if consumed_chars + len <= limit {
            // Entire span within bold range
            let mut st = sp.style;
            st.add_modifier.insert(Modifier::BOLD);
            st.fg = Some(crate::colors::text_bright());
            new_spans.push(Span::styled(content, st));
            consumed_chars += len;
        } else {
            // Split this span at the boundary
            let split_at = limit - consumed_chars; // chars into this span
            let mut iter = content.chars();
            let bold_part: String = iter.by_ref().take(split_at).collect();
            let rest_part: String = iter.collect();
            let mut bold_style = sp.style;
            bold_style.add_modifier.insert(Modifier::BOLD);
            bold_style.fg = Some(crate::colors::text_bright());
            if !bold_part.is_empty() { new_spans.push(Span::styled(bold_part, bold_style)); }
            if !rest_part.is_empty() { new_spans.push(Span::styled(rest_part, sp.style)); }
            consumed_chars += len;
        }
    }
    lines[first_idx].spans = new_spans;

    // Recolor markdown bullet glyphs inside assistant content to text_dim.
    // Applies to common unordered bullets produced by our renderer: •, ◦, ·, ∘, ⋅
    let bullet_set: [&str; 5] = ["•", "◦", "·", "∘", "⋅"];
    for line in lines.iter_mut() {
        let mut updated: Vec<Span<'static>> = Vec::with_capacity(line.spans.len());
        for sp in line.spans.drain(..) {
            let content_ref = sp.content.as_ref();
            if bullet_set.contains(&content_ref) {
                let mut st = sp.style;
                st.fg = Some(crate::colors::text_dim());
                updated.push(Span::styled(sp.content, st));
            } else {
                updated.push(sp);
            }
        }
        line.spans = updated;
    }

    lines
}
*/

// ==================== Helper Functions ====================

// Unified preview format: show first 2 and last 5 non-empty lines with an ellipsis between.
const PREVIEW_HEAD_LINES: usize = 2;
const PREVIEW_TAIL_LINES: usize = 5;
const STREAMING_EXIT_CODE: i32 = i32::MIN;

/// Normalize common TTY overwrite sequences within a text block so that
/// progress lines using carriage returns, backspaces, or ESC[K erase behave as
/// expected when rendered in a pure-buffered UI (no cursor movement).
pub(crate) fn normalize_overwrite_sequences(input: &str) -> String {
    // Process per line, but keep CR/BS/CSI semantics within logical lines.
    // Treat "\n" as committing a line and resetting the cursor.
    let mut out = String::with_capacity(input.len());
    let mut line: Vec<char> = Vec::new(); // visible chars only
    let mut cursor: usize = 0; // column in visible chars

    // Helper to flush current line to out
    let flush_line = |line: &mut Vec<char>, cursor: &mut usize, out: &mut String| {
        if !line.is_empty() {
            out.push_str(&line.iter().collect::<String>());
        }
        out.push('\n');
        line.clear();
        *cursor = 0;
    };

    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        match ch {
            '\n' => {
                flush_line(&mut line, &mut cursor, &mut out);
                i += 1;
            }
            '\r' => {
                // Carriage return: move cursor to column 0
                cursor = 0;
                i += 1;
            }
            '\u{0008}' => {
                // Backspace: move left one column if possible
                if cursor > 0 {
                    cursor -= 1;
                }
                i += 1;
            }
            '\u{001B}' => {
                // CSI: ESC [ ... <cmd>
                if i + 1 < chars.len() && chars[i + 1] == '[' {
                    // Find final byte (alphabetic)
                    let mut j = i + 2;
                    while j < chars.len() && !chars[j].is_alphabetic() {
                        j += 1;
                    }
                    if j < chars.len() {
                        let cmd = chars[j];
                        // Extract numeric prefix (first parameter only)
                        let num: usize = chars[i + 2..j]
                            .iter()
                            .take_while(|c| c.is_ascii_digit())
                            .collect::<String>()
                            .parse()
                            .unwrap_or(0);

                        match cmd {
                            // Erase in Line: 0/None = cursor..end, 1 = start..cursor, 2 = entire line
                            'K' => {
                                let n = num; // default 0 when absent
                                match n {
                                    0 => {
                                        if cursor < line.len() {
                                            line.truncate(cursor);
                                        }
                                    }
                                    1 => {
                                        // Replace from start to cursor with spaces to keep remaining columns stable
                                        let end = cursor.min(line.len());
                                        for k in 0..end {
                                            line[k] = ' ';
                                        }
                                        // Trim leading spaces if the whole line became spaces
                                        while line.last().map_or(false, |c| *c == ' ') {
                                            line.pop();
                                        }
                                    }
                                    2 => {
                                        line.clear();
                                        cursor = 0;
                                    }
                                    _ => {}
                                }
                                i = j + 1;
                                continue;
                            }
                            // Cursor horizontal absolute (1-based)
                            'G' => {
                                let pos = num.saturating_sub(1);
                                cursor = pos.min(line.len());
                                i = j + 1;
                                continue;
                            }
                            // Cursor forward/backward
                            'C' => {
                                cursor = cursor.saturating_add(num);
                                i = j + 1;
                                continue;
                            }
                            'D' => {
                                cursor = cursor.saturating_sub(num);
                                i = j + 1;
                                continue;
                            }
                            _ => {
                                // Unknown/unsupported CSI (incl. SGR 'm'): keep styling intact by
                                // copying the entire sequence verbatim into the output so ANSI
                                // parsing can apply later, but do not affect cursor position.
                                // First, splice current visible buffer into out to preserve order
                                if !line.is_empty() {
                                    out.push_str(&line.iter().collect::<String>());
                                    line.clear();
                                    cursor = 0;
                                }
                                for k in i..=j {
                                    out.push(chars[k]);
                                }
                                i = j + 1;
                                continue;
                            }
                        }
                    } else {
                        // Malformed CSI: drop it entirely by exiting the loop
                        break;
                    }
                } else {
                    // Other ESC sequences (e.g., OSC): pass through verbatim without affecting cursor
                    // Copy ESC and advance one; do not attempt to parse full OSC payload here.
                    if !line.is_empty() {
                        out.push_str(&line.iter().collect::<String>());
                        line.clear();
                        cursor = 0;
                    }
                    out.push(ch);
                    i += 1;
                }
            }
            _ => {
                // Put visible char at cursor, expanding with spaces if needed
                if cursor < line.len() {
                    line[cursor] = ch;
                } else {
                    while line.len() < cursor {
                        line.push(' ');
                    }
                    line.push(ch);
                }
                cursor += 1;
                i += 1;
            }
        }
    }
    // Flush any remaining visible text
    if !line.is_empty() {
        out.push_str(&line.iter().collect::<String>());
    }
    out
}

fn build_preview_lines(text: &str, _include_left_pipe: bool) -> Vec<Line<'static>> {
    // Prefer UI‑themed JSON highlighting when the (ANSI‑stripped) text parses as JSON.
    let stripped_plain = sanitize_for_tui(
        text,
        SanitizeMode::Plain,
        SanitizeOptions {
            expand_tabs: true,
            tabstop: 4,
            debug_markers: false,
        },
    );
    if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&stripped_plain) {
        let pretty =
            serde_json::to_string_pretty(&json_val).unwrap_or_else(|_| json_val.to_string());
        let highlighted = crate::syntax_highlight::highlight_code_block(&pretty, Some("json"));
        return select_preview_from_lines(&highlighted, PREVIEW_HEAD_LINES, PREVIEW_TAIL_LINES);
    }

    // Otherwise, compact valid JSON (without ANSI) to improve wrap, or pass original through.
    let processed = format_json_compact(text).unwrap_or_else(|| text.to_string());
    let processed = normalize_overwrite_sequences(&processed);
    let processed = sanitize_for_tui(
        &processed,
        SanitizeMode::AnsiPreserving,
        SanitizeOptions {
            expand_tabs: true,
            tabstop: 4,
            debug_markers: false,
        },
    );
    let non_empty: Vec<&str> = processed.lines().filter(|line| !line.is_empty()).collect();

    enum Seg<'a> {
        Line(&'a str),
        Ellipsis,
    }
    let segments: Vec<Seg> = if non_empty.len() <= PREVIEW_HEAD_LINES + PREVIEW_TAIL_LINES {
        non_empty.iter().map(|s| Seg::Line(s)).collect()
    } else {
        let mut v: Vec<Seg> = Vec::with_capacity(PREVIEW_HEAD_LINES + PREVIEW_TAIL_LINES + 1);
        // Head
        for i in 0..PREVIEW_HEAD_LINES {
            v.push(Seg::Line(non_empty[i]));
        }
        v.push(Seg::Ellipsis);
        // Tail
        let start = non_empty.len().saturating_sub(PREVIEW_TAIL_LINES);
        for s in &non_empty[start..] {
            v.push(Seg::Line(s));
        }
        v
    };

    fn ansi_line_with_theme_bg(s: &str) -> Line<'static> {
        let mut ln = ansi_escape_line(s);
        for sp in ln.spans.iter_mut() {
            sp.style.bg = None;
        }
        ln
    }

    let mut out: Vec<Line<'static>> = Vec::new();
    for seg in segments {
        match seg {
            Seg::Line(line) => out.push(ansi_line_with_theme_bg(line)),
            Seg::Ellipsis => out.push(Line::from("⋮".dim())),
        }
    }
    out
}

fn output_lines(
    output: Option<&CommandOutput>,
    only_err: bool,
    include_angle_pipe: bool,
) -> Vec<Line<'static>> {
    let CommandOutput {
        exit_code,
        stdout,
        stderr,
    } = match output {
        Some(o) => o,
        None => return Vec::new(),
    };

    let mut lines: Vec<Line<'static>> = Vec::new();
    let is_streaming_preview = *exit_code == STREAMING_EXIT_CODE;

    if !only_err && !stdout.is_empty() {
        lines.extend(build_preview_lines(stdout, include_angle_pipe));
    }

    if !stderr.is_empty() && (is_streaming_preview || *exit_code != 0) {
        if !lines.is_empty() {
            lines.push(Line::from(""));
        }
        if !is_streaming_preview {
            lines.push(Line::styled(
                format!("Error (exit code {})", exit_code),
                Style::default().fg(crate::colors::error()),
            ));
        }
        let stderr_norm = sanitize_for_tui(
            &normalize_overwrite_sequences(stderr),
            SanitizeMode::AnsiPreserving,
            SanitizeOptions {
                expand_tabs: true,
                tabstop: 4,
                debug_markers: false,
            },
        );
        for line in stderr_norm.lines().filter(|line| !line.is_empty()) {
            lines.push(ansi_escape_line(line).style(Style::default().fg(crate::colors::error())));
        }
    }

    if !lines.is_empty() {
        lines.push(Line::from(""));
    }

    lines
}

fn format_mcp_invocation(invocation: McpInvocation) -> Line<'static> {
    let provider_name = pretty_provider_name(&invocation.server);
    let invocation_str = if let Some(args) = invocation.arguments {
        format!("{}.{}({})", provider_name, invocation.tool, args)
    } else {
        format!("{}.{}()", provider_name, invocation.tool)
    };

    Line::styled(
        invocation_str,
        Style::default()
            .fg(crate::colors::text_dim())
            .add_modifier(Modifier::ITALIC),
    )
}

fn pretty_provider_name(id: &str) -> String {
    // Special case common providers with human-friendly names
    match id {
        "brave-search" => "brave",
        "screenshot-website-fast" => "screenshot",
        "read-website-fast" => "readweb",
        "sequential-thinking" => "think",
        "discord-bot" => "discord",
        _ => id,
    }
    .to_string()
}

// ==================== Factory Functions ====================

pub(crate) fn new_background_event(message: String) -> PlainHistoryCell {
    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from("event".dim()));
    let msg_norm = normalize_overwrite_sequences(&message);
    lines.extend(msg_norm.lines().map(|line| ansi_escape_line(line).dim()));
    // No empty line at end - trimming and spacing handled by renderer
    PlainHistoryCell::new(lines, HistoryCellType::BackgroundEvent)
}

pub(crate) fn new_session_info(
    config: &Config,
    event: SessionConfiguredEvent,
    is_first_event: bool,
    latest_version: Option<&str>,
) -> PlainHistoryCell {
    let SessionConfiguredEvent {
        model,
        session_id: _,
        history_log_id: _,
        history_entry_count: _,
        ..
    } = event;

    if is_first_event {
        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.push(Line::from("notice".dim()));
        lines.extend(popular_commands_lines(latest_version));
        PlainHistoryCell::new(lines, HistoryCellType::Notice)
    } else if config.model == model {
        PlainHistoryCell::new(Vec::new(), HistoryCellType::Notice)
    } else {
        let lines = vec![
            Line::from("model changed:")
                .fg(crate::colors::keyword())
                .bold(),
            Line::from(format!("requested: {}", config.model)),
            Line::from(format!("used: {model}")),
            // No empty line at end - trimming and spacing handled by renderer
        ];
        PlainHistoryCell::new(lines, HistoryCellType::Notice)
    }
}

/// Build the common lines for the "Popular commands" section (without the leading
/// "notice" marker). Shared between the initial session info and the startup prelude.
fn popular_commands_lines(_latest_version: Option<&str>) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::styled(
        "Popular commands:",
        Style::default().fg(crate::colors::text_bright()),
    ));
    lines.push(Line::from(vec![
        Span::styled("/agents", Style::default().fg(crate::colors::primary())),
        Span::from(" - "),
        Span::from(SlashCommand::Agents.description())
            .style(Style::default().add_modifier(Modifier::DIM)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("/model", Style::default().fg(crate::colors::primary())),
        Span::from(" - "),
        Span::from(SlashCommand::Model.description())
            .style(Style::default().add_modifier(Modifier::DIM)),
        Span::styled(
            " NEW with GPT-5-Codex!",
            Style::default().fg(crate::colors::primary()),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("/chrome", Style::default().fg(crate::colors::primary())),
        Span::from(" - "),
        Span::from(SlashCommand::Chrome.description())
            .style(Style::default().add_modifier(Modifier::DIM)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("/plan", Style::default().fg(crate::colors::primary())),
        Span::from(" - "),
        Span::from(SlashCommand::Plan.description())
            .style(Style::default().add_modifier(Modifier::DIM)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("/code", Style::default().fg(crate::colors::primary())),
        Span::from(" - "),
        Span::from(SlashCommand::Code.description())
            .style(Style::default().add_modifier(Modifier::DIM)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("/branch", Style::default().fg(crate::colors::primary())),
        Span::from(" - "),
        Span::from(SlashCommand::Branch.description())
            .style(Style::default().add_modifier(Modifier::DIM)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("/limits", Style::default().fg(crate::colors::primary())),
        Span::from(" - "),
        Span::from(SlashCommand::Limits.description())
            .style(Style::default().add_modifier(Modifier::DIM)),
        Span::styled(" NEW", Style::default().fg(crate::colors::primary())),
    ]));
    lines.push(Line::from(vec![
        Span::styled("/undo", Style::default().fg(crate::colors::primary())),
        Span::from(" - "),
        Span::from(SlashCommand::Undo.description())
            .style(Style::default().add_modifier(Modifier::DIM)),
        Span::styled(" NEW", Style::default().fg(crate::colors::primary())),
    ]));
    lines.push(Line::from(vec![
        Span::styled("/review", Style::default().fg(crate::colors::primary())),
        Span::from(" - "),
        Span::from(SlashCommand::Review.description())
            .style(Style::default().add_modifier(Modifier::DIM)),
        Span::styled(" NEW", Style::default().fg(crate::colors::primary())),
    ]));

    lines
}

/// Create a notice cell that shows the "Popular commands" immediately.
/// If `connecting_mcp` is true, include a dim status line to inform users
/// that external MCP servers are being connected in the background.
pub(crate) fn new_upgrade_prelude(latest_version: Option<&str>) -> Option<UpgradeNoticeCell> {
    if !crate::updates::upgrade_ui_enabled() {
        return None;
    }
    let latest = latest_version?.trim();
    if latest.is_empty() {
        return None;
    }

    let current = codex_version::version();
    if latest == current {
        return None;
    }

    Some(UpgradeNoticeCell::new(
        current.to_string(),
        latest.to_string(),
    ))
}

pub(crate) fn new_popular_commands_notice(
    _connecting_mcp: bool,
    latest_version: Option<&str>,
) -> PlainHistoryCell {
    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from("notice".dim()));
    lines.extend(popular_commands_lines(latest_version));
    // Connecting status is now rendered as a separate BackgroundEvent cell
    // with its own gutter icon and spacing. Keep this notice focused.
    PlainHistoryCell::new(lines, HistoryCellType::Notice)
}

/// Background status cell shown during startup while external MCP servers
/// are being connected. Uses the standard background-event gutter (»)
/// and inserts a blank line above the message for visual separation from
/// the Popular commands block.
pub(crate) fn new_connecting_mcp_status() -> PlainHistoryCell {
    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from("event".dim()));
    // Explicit blank line above the status message
    lines.push(Line::from(String::new()));
    lines.push(Line::from(Span::styled(
        "Connecting MCP servers…",
        Style::default().fg(crate::colors::text_dim()),
    )));
    PlainHistoryCell::new(lines, HistoryCellType::BackgroundEvent)
}

pub(crate) fn new_user_prompt(message: String) -> PlainHistoryCell {
    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from("user"));
    // Sanitize user-provided text for terminal safety and stable layout:
    // - Normalize common TTY overwrite sequences (\r, \x08, ESC[K)
    // - Expand tabs to spaces with a fixed tab stop so wrapping is deterministic
    // - Parse ANSI sequences into spans so we never emit raw control bytes
    let normalized = normalize_overwrite_sequences(&message);
    let sanitized = sanitize_for_tui(
        &normalized,
        SanitizeMode::AnsiPreserving,
        SanitizeOptions {
            expand_tabs: true,
            tabstop: 4,
            debug_markers: false,
        },
    );
    // Build content lines with ANSI converted to styled spans
    let content: Vec<Line<'static>> = sanitized.lines().map(|l| ansi_escape_line(l)).collect();
    let content = trim_empty_lines(content);
    lines.extend(content);
    // No empty line at end - trimming and spacing handled by renderer
    PlainHistoryCell::new(lines, HistoryCellType::User)
}

/// Render a queued user message that will be sent in the next turn.
/// Visually identical to a normal user cell, but the header shows a
/// small "(queued)" suffix so it’s clear it hasn’t been executed yet.
pub(crate) fn new_queued_user_prompt(message: String) -> PlainHistoryCell {
    use ratatui::style::Style;
    use ratatui::text::Span;
    let mut lines: Vec<Line<'static>> = Vec::new();
    // Header: "user (queued)"
    lines.push(Line::from(vec![
        Span::from("user "),
        Span::from("(queued)").style(Style::default().fg(crate::colors::text_dim())),
    ]));
    // Normalize and render body like normal user messages
    let normalized = normalize_overwrite_sequences(&message);
    let sanitized = sanitize_for_tui(
        &normalized,
        SanitizeMode::AnsiPreserving,
        SanitizeOptions {
            expand_tabs: true,
            tabstop: 4,
            debug_markers: false,
        },
    );
    let content: Vec<Line<'static>> = sanitized.lines().map(|l| ansi_escape_line(l)).collect();
    let content = trim_empty_lines(content);
    lines.extend(content);
    PlainHistoryCell::new(lines, HistoryCellType::User)
}

/// Expand horizontal tabs to spaces using a fixed tab stop.
/// This prevents terminals from applying their own tab expansion after
/// ratatui has computed layout, which can otherwise cause glyphs to appear
/// to "hang" or smear until overwritten.
// Tab expansion and control stripping are centralized in crate::sanitize

#[allow(dead_code)]
pub(crate) fn new_text_line(line: Line<'static>) -> PlainHistoryCell {
    PlainHistoryCell::new(vec![line], HistoryCellType::Notice)
}

pub(crate) fn new_streaming_content(lines: Vec<Line<'static>>) -> StreamingContentCell {
    StreamingContentCell {
        id: None,
        lines,
        show_ellipsis: true,
        cached_layout: std::cell::RefCell::new(None),
    }
}

pub(crate) fn new_streaming_content_with_id(
    id: Option<String>,
    lines: Vec<Line<'static>>,
) -> StreamingContentCell {
    StreamingContentCell {
        id,
        lines,
        show_ellipsis: true,
        cached_layout: std::cell::RefCell::new(None),
    }
}

pub(crate) fn new_animated_welcome() -> AnimatedWelcomeCell {
    AnimatedWelcomeCell::new()
}

#[allow(dead_code)]
pub(crate) fn new_loading_cell(message: String) -> LoadingCell {
    LoadingCell::new(message)
}

pub(crate) fn new_active_exec_command(
    command: Vec<String>,
    parsed: Vec<ParsedCommand>,
) -> ExecCell {
    new_exec_cell(command, parsed, None)
}

pub(crate) fn new_completed_exec_command(
    command: Vec<String>,
    parsed: Vec<ParsedCommand>,
    output: CommandOutput,
) -> ExecCell {
    new_exec_cell(command, parsed, Some(output))
}

fn command_has_bold_token(command: &[String]) -> bool {
    let command_escaped = strip_bash_lc_and_escape(command);
    let normalized = normalize_shell_command_display(&command_escaped);
    let trimmed = normalized.trim_start();
    if trimmed.is_empty() {
        return false;
    }
    trimmed.chars().take_while(|ch| !ch.is_whitespace()).count() > 4
}

fn new_exec_cell(
    command: Vec<String>,
    parsed: Vec<ParsedCommand>,
    output: Option<CommandOutput>,
) -> ExecCell {
    let start_time = if output.is_none() {
        Some(Instant::now())
    } else {
        None
    };
    let has_bold_command = command_has_bold_token(&command);
    let parsed_meta = if parsed.is_empty() {
        None
    } else {
        Some(ParsedExecMetadata::from_commands(&parsed))
    };
    ExecCell {
        command,
        parsed,
        output,
        start_time,
        stream_preview: None,
        cached_display_lines: std::cell::RefCell::new(None),
        cached_pre_lines: std::cell::RefCell::new(None),
        cached_out_lines: std::cell::RefCell::new(None),
        cached_layout: std::cell::RefCell::new(None),
        cached_command_lines: std::cell::RefCell::new(None),
        cached_wait_extras: std::cell::RefCell::new(None),
        parsed_meta,
        has_bold_command,
        wait_state: std::cell::RefCell::new(ExecWaitState::default()),
    }
}

fn exec_command_lines(
    command: &[String],
    parsed: &[ParsedCommand],
    output: Option<&CommandOutput>,
    stream_preview: Option<&CommandOutput>,
    start_time: Option<Instant>,
) -> Vec<Line<'static>> {
    match parsed.is_empty() {
        true => new_exec_command_generic(command, output, stream_preview, start_time),
        false => new_parsed_command(parsed, output, stream_preview, start_time),
    }
}

// Legacy helper removed in favor of ExecAction (action_enum_from_parsed)

fn first_context_path(parsed_commands: &[ParsedCommand]) -> Option<String> {
    for parsed in parsed_commands.iter() {
        match parsed {
            ParsedCommand::ListFiles { path, .. } => {
                if let Some(p) = path {
                    return Some(p.clone());
                }
            }
            ParsedCommand::Search { path, .. } => {
                if let Some(p) = path {
                    return Some(p.clone());
                }
            }
            _ => {}
        }
    }
    None
}

fn parse_read_line_annotation_with_range(cmd: &str) -> (Option<String>, Option<(u32, u32)>) {
    let lower = cmd.to_lowercase();
    // Try sed -n '<start>,<end>p'
    if lower.contains("sed") && lower.contains("-n") {
        // Look for a token like 123,456p possibly quoted
        for raw in cmd.split(|c: char| c.is_whitespace() || c == '"' || c == '\'') {
            let token = raw.trim();
            if token.ends_with('p') {
                let core = &token[..token.len().saturating_sub(1)];
                if let Some((a, b)) = core.split_once(',') {
                    if let (Ok(start), Ok(end)) = (a.trim().parse::<u32>(), b.trim().parse::<u32>())
                    {
                        return (
                            Some(format!("(lines {} to {})", start, end)),
                            Some((start, end)),
                        );
                    }
                }
            }
        }
    }
    // head -n N => lines 1..N
    if lower.contains("head") && lower.contains("-n") {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        for i in 0..parts.len() {
            if parts[i] == "-n" && i + 1 < parts.len() {
                if let Ok(n) = parts[i + 1]
                    .trim_matches('"')
                    .trim_matches('\'')
                    .parse::<u32>()
                {
                    return (Some(format!("(lines 1 to {})", n)), Some((1, n)));
                }
            }
        }
    }
    // bare `head` => default 10 lines
    if lower.contains("head") && !lower.contains("-n") {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.iter().any(|p| *p == "head") {
            return (Some("(lines 1 to 10)".to_string()), Some((1, 10)));
        }
    }
    // tail -n +K => from K to end; tail -n N => last N lines
    if lower.contains("tail") && lower.contains("-n") {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        for i in 0..parts.len() {
            if parts[i] == "-n" && i + 1 < parts.len() {
                let val = parts[i + 1].trim_matches('"').trim_matches('\'');
                if let Some(rest) = val.strip_prefix('+') {
                    if let Ok(k) = rest.parse::<u32>() {
                        return (Some(format!("(from {} to end)", k)), Some((k, u32::MAX)));
                    }
                } else if let Ok(n) = val.parse::<u32>() {
                    return (Some(format!("(last {} lines)", n)), None);
                }
            }
        }
    }
    // bare `tail` => default 10 lines
    if lower.contains("tail") && !lower.contains("-n") {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.iter().any(|p| *p == "tail") {
            return (Some("(last 10 lines)".to_string()), None);
        }
    }
    (None, None)
}

fn parse_read_line_annotation(cmd: &str) -> Option<String> {
    parse_read_line_annotation_with_range(cmd).0
}

#[allow(dead_code)]
fn strip_redundant_line_filter_pipes(cmd: &str) -> String {
    let (annotation, _) = parse_read_line_annotation_with_range(cmd);
    if annotation.is_none() {
        return cmd.to_string();
    }

    if let Some(idx) = cmd.rfind('|') {
        let head = cmd[..idx].trim_end();
        head.to_string()
    } else {
        cmd.to_string()
    }
}

fn normalize_shell_command_display(cmd: &str) -> String {
    let first_non_ws = cmd
        .char_indices()
        .find(|(_, ch)| !ch.is_whitespace())
        .map(|(idx, _)| idx);
    let Some(start) = first_non_ws else {
        return cmd.to_string();
    };
    if cmd[start..].starts_with("./") {
        let mut normalized = String::with_capacity(cmd.len().saturating_sub(2));
        normalized.push_str(&cmd[..start]);
        normalized.push_str(&cmd[start + 2..]);
        normalized
    } else {
        cmd.to_string()
    }
}

fn insert_line_breaks_after_double_ampersand(cmd: &str) -> String {
    if !cmd.contains("&&") {
        return cmd.to_string();
    }

    let mut result = String::with_capacity(cmd.len() + 8);
    let mut i = 0;
    let mut in_single = false;
    let mut in_double = false;

    while i < cmd.len() {
        let ch = cmd[i..].chars().next().expect("valid char boundary");
        let ch_len = ch.len_utf8();

        match ch {
            '\'' if !in_double => {
                in_single = !in_single;
                result.push(ch);
                i += ch_len;
                continue;
            }
            '"' if !in_single => {
                in_double = !in_double;
                result.push(ch);
                i += ch_len;
                continue;
            }
            '&' if !in_single && !in_double => {
                let next_idx = i + ch_len;
                if next_idx < cmd.len() {
                    if let Some(next_ch) = cmd[next_idx..].chars().next() {
                        if next_ch == '&' {
                            result.push('&');
                            result.push('&');
                            i = next_idx + next_ch.len_utf8();
                            while i < cmd.len() {
                                let ahead = cmd[i..].chars().next().expect("valid char boundary");
                                if ahead.is_whitespace() {
                                    i += ahead.len_utf8();
                                    continue;
                                }
                                break;
                            }
                            if i < cmd.len() {
                                result.push('\n');
                            }
                            continue;
                        }
                    }
                }
            }
            _ => {}
        }

        result.push(ch);
        i += ch_len;
    }

    result
}

fn emphasize_shell_command_name(line: &mut Line<'static>) {
    let mut emphasized = false;
    let mut rebuilt: Vec<Span<'static>> = Vec::with_capacity(line.spans.len());

    for span in line.spans.drain(..) {
        if emphasized {
            rebuilt.push(span);
            continue;
        }

        let style = span.style;
        let content_owned = span.content.into_owned();

        if content_owned.trim().is_empty() {
            rebuilt.push(Span::styled(content_owned, style));
            continue;
        }

        let mut token_start: Option<usize> = None;
        for (idx, ch) in content_owned.char_indices() {
            if !ch.is_whitespace() {
                token_start = Some(idx);
                break;
            }
        }

        let Some(start) = token_start else {
            rebuilt.push(Span::styled(content_owned, style));
            continue;
        };

        let mut end = content_owned.len();
        for (offset, ch) in content_owned[start..].char_indices() {
            if ch.is_whitespace() {
                end = start + offset;
                break;
            }
        }

        let before = &content_owned[..start];
        let token = &content_owned[start..end];
        let after = &content_owned[end..];

        if !before.is_empty() {
            rebuilt.push(Span::styled(before.to_string(), style));
        }

        if token.chars().count() <= 4 {
            rebuilt.push(Span::styled(token.to_string(), style));
        } else {
            let bright_style = style
                .fg(crate::colors::text_bright())
                .add_modifier(Modifier::BOLD);
            rebuilt.push(Span::styled(token.to_string(), bright_style));
        }

        if !after.is_empty() {
            rebuilt.push(Span::styled(after.to_string(), style));
        }

        emphasized = true;
    }

    if emphasized {
        line.spans = rebuilt;
    } else if !rebuilt.is_empty() {
        line.spans = rebuilt;
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn format_inline_python_for_display(command_escaped: &str) -> String {
    try_format_inline_python(command_escaped).unwrap_or_else(|| command_escaped.to_string())
}

fn format_inline_script_for_display(command_escaped: &str) -> String {
    if let Some(formatted) = try_format_inline_python(command_escaped) {
        return formatted;
    }
    if let Some(formatted) = format_inline_node_for_display(command_escaped) {
        return formatted;
    }
    if let Some(formatted) = format_inline_shell_for_display(command_escaped) {
        return formatted;
    }
    command_escaped.to_string()
}

fn try_format_inline_python(command_escaped: &str) -> Option<String> {
    if let Some(formatted) = format_python_dash_c(command_escaped) {
        return Some(formatted);
    }
    if let Some(formatted) = format_python_heredoc(command_escaped) {
        return Some(formatted);
    }
    None
}

fn format_python_dash_c(command_escaped: &str) -> Option<String> {
    let tokens: Vec<String> = Shlex::new(command_escaped).collect();
    if tokens.len() < 3 {
        return None;
    }

    let python_idx = tokens
        .iter()
        .position(|token| is_python_invocation_token(token))?;

    let c_idx = tokens
        .iter()
        .enumerate()
        .skip(python_idx + 1)
        .find_map(|(idx, token)| if token == "-c" { Some(idx) } else { None })?;

    let script_idx = c_idx + 1;
    if script_idx >= tokens.len() {
        return None;
    }

    let script_raw = tokens[script_idx].as_str();
    if script_raw.is_empty() {
        return None;
    }

    let script_block = build_python_script_block(script_raw)?;

    let mut parts: Vec<String> = Vec::with_capacity(tokens.len());
    for (idx, token) in tokens.iter().enumerate() {
        if idx == script_idx {
            parts.push(script_block.clone());
        } else {
            parts.push(escape_token_for_display(token));
        }
    }

    Some(parts.join(" "))
}

fn build_python_script_block(script: &str) -> Option<String> {
    let normalized = script.replace("\r\n", "\n");
    let lines: Vec<String> = if normalized.contains('\n') {
        normalized
            .lines()
            .map(|line| line.trim_end().to_string())
            .collect()
    } else if script_has_semicolon_outside_quotes(&normalized) {
        split_semicolon_statements(&normalized)
    } else {
        return None;
    };

    let meaningful: Vec<String> = merge_from_import_lines(lines)
        .into_iter()
        .map(|line| line.trim_end().to_string())
        .filter(|line| !line.trim().is_empty())
        .collect();

    if meaningful.len() <= 1 {
        return None;
    }

    let indented = indent_python_lines(meaningful);

    let mut block = String::from("'\n");
    for line in indented {
        block.push_str("    ");
        let escaped = escape_single_quotes_for_shell(line.as_str());
        block.push_str(escaped.as_str());
        block.push('\n');
    }
    block.push('\'');
    Some(block)
}

fn format_python_heredoc(command_escaped: &str) -> Option<String> {
    let tokens: Vec<String> = Shlex::new(command_escaped).collect();
    if tokens.len() < 3 {
        return None;
    }

    let python_idx = tokens
        .iter()
        .position(|token| is_python_invocation_token(token))?;

    let heredoc_idx = tokens
        .iter()
        .enumerate()
        .skip(python_idx + 1)
        .find_map(|(idx, token)| heredoc_delimiter(token).map(|delim| (idx, delim)))?;

    let (marker_idx, terminator) = heredoc_idx;
    let closing_idx = tokens
        .iter()
        .enumerate()
        .skip(marker_idx + 1)
        .rev()
        .find_map(|(idx, token)| (token == &terminator).then_some(idx))?;

    if closing_idx <= marker_idx + 1 {
        return None;
    }

    let script_tokens = &tokens[marker_idx + 1..closing_idx];
    if script_tokens.is_empty() {
        return None;
    }

    let script_lines = split_heredoc_script_lines(script_tokens);
    if script_lines.is_empty() {
        return None;
    }

    let script_lines = indent_python_lines(merge_from_import_lines(script_lines));

    let header_tokens: Vec<String> = tokens[..=marker_idx]
        .iter()
        .map(|t| escape_token_for_display(t))
        .collect();

    let mut result = header_tokens.join(" ");
    if !result.ends_with('\n') {
        result.push('\n');
    }

    for line in script_lines {
        result.push_str("    ");
        let escaped = escape_single_quotes_for_shell(line.trim_end());
        result.push_str(escaped.as_str());
        result.push('\n');
    }

    result.push_str(&escape_token_for_display(&tokens[closing_idx]));

    if closing_idx + 1 < tokens.len() {
        let tail: Vec<String> = tokens[closing_idx + 1..]
            .iter()
            .map(|t| escape_token_for_display(t))
            .collect();
        if !tail.is_empty() {
            result.push(' ');
            result.push_str(&tail.join(" "));
        }
    }

    Some(result)
}

fn heredoc_delimiter(token: &str) -> Option<String> {
    if !token.starts_with("<<") {
        return None;
    }
    let mut delim = token.trim_start_matches("<<").to_string();
    if delim.is_empty() {
        return None;
    }
    if delim.starts_with('"') && delim.ends_with('"') && delim.len() >= 2 {
        delim = delim[1..delim.len() - 1].to_string();
    } else if delim.starts_with('\'') && delim.ends_with('\'') && delim.len() >= 2 {
        delim = delim[1..delim.len() - 1].to_string();
    }
    if delim.is_empty() { None } else { Some(delim) }
}

fn split_heredoc_script_lines(script_tokens: &[String]) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut paren_depth = 0i32;
    let mut bracket_depth = 0i32;
    let mut brace_depth = 0i32;
    let mut current_has_assignment = false;

    for (idx, token) in script_tokens.iter().enumerate() {
        if !current.is_empty() && paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 {
            let token_lower = token.to_ascii_lowercase();
            let current_first = current.first().map(|s| s.to_ascii_lowercase());
            let should_flush_before = is_statement_boundary_token(token)
                && !(token_lower == "import" && current_first.as_deref() == Some("from"));
            if should_flush_before {
                let line = current.join(" ");
                lines.push(line.trim().to_string());
                current.clear();
                current_has_assignment = false;
            }
        }

        current.push(token.clone());
        adjust_bracket_depth(
            token,
            &mut paren_depth,
            &mut bracket_depth,
            &mut brace_depth,
        );

        if is_assignment_operator(token) {
            current_has_assignment = true;
        }

        let next = script_tokens.get(idx + 1);
        let mut should_break = false;
        let mut break_here = false;

        if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 {
            if next.is_none() {
                should_break = true;
            } else {
                let next_token = next.unwrap();
                if is_statement_boundary_token(next_token) {
                    should_break = true;
                } else if current
                    .first()
                    .map(|s| s.as_str() == "import" || s.as_str() == "from")
                    .unwrap_or(false)
                {
                    if current.len() > 1 && next_token != "as" && next_token != "," {
                        should_break = true;
                    }
                } else if current_has_assignment
                    && !is_assignment_operator(token)
                    && next_token
                        .chars()
                        .next()
                        .map(|ch| ch.is_ascii_alphanumeric() || ch == '_')
                        .unwrap_or(false)
                    && !next_token.contains('(')
                {
                    should_break = true;
                }

                let token_trimmed = token.trim_matches(|c| c == ')' || c == ']' || c == '}');
                if token_trimmed.ends_with(':') {
                    break_here = true;
                }

                let lowered = token.trim().to_ascii_lowercase();
                if matches!(lowered.as_str(), "return" | "break" | "continue" | "pass") {
                    break_here = true;
                }

                if let Some(next_token) = next {
                    let next_str = next_token.as_str();
                    if token.ends_with(')')
                        && (next_str.contains('.')
                            || next_str.contains('=')
                            || next_str.starts_with("print"))
                    {
                        break_here = true;
                    }
                }
            }
        }

        if break_here {
            let line = current.join(" ");
            lines.push(line.trim().to_string());
            current.clear();
            current_has_assignment = false;
            continue;
        }

        if should_break {
            let line = current.join(" ");
            lines.push(line.trim().to_string());
            current.clear();
            current_has_assignment = false;
        }
    }

    if !current.is_empty() {
        let line = current.join(" ");
        lines.push(line.trim().to_string());
    }

    lines.into_iter().filter(|line| !line.is_empty()).collect()
}

fn is_statement_boundary_token(token: &str) -> bool {
    matches!(
        token,
        "import"
            | "from"
            | "def"
            | "class"
            | "if"
            | "elif"
            | "else"
            | "for"
            | "while"
            | "try"
            | "except"
            | "with"
            | "return"
            | "raise"
            | "pass"
            | "continue"
            | "break"
    ) || token.starts_with("print")
}

fn indent_python_lines(lines: Vec<String>) -> Vec<String> {
    let mut indented: Vec<String> = Vec::with_capacity(lines.len());
    let mut indent_level: usize = 0;
    let mut pending_dedent_after_flow = false;

    for raw in lines {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            indented.push(String::new());
            continue;
        }

        let lowered_first = trimmed
            .split_whitespace()
            .next()
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();

        if pending_dedent_after_flow
            && !matches!(
                lowered_first.as_str(),
                "elif" | "else" | "except" | "finally"
            )
        {
            if indent_level > 0 {
                indent_level -= 1;
            }
        }
        pending_dedent_after_flow = false;

        if matches!(
            lowered_first.as_str(),
            "elif" | "else" | "except" | "finally"
        ) {
            if indent_level > 0 {
                indent_level -= 1;
            }
        }

        let mut line = String::with_capacity(trimmed.len() + indent_level * 4);
        for _ in 0..indent_level {
            line.push_str("    ");
        }
        line.push_str(trimmed);
        indented.push(line);

        if trimmed.ends_with(':')
            && !matches!(
                lowered_first.as_str(),
                "return" | "break" | "continue" | "pass" | "raise"
            )
        {
            indent_level += 1;
        } else if matches!(
            lowered_first.as_str(),
            "return" | "break" | "continue" | "pass" | "raise"
        ) {
            pending_dedent_after_flow = true;
        }
    }

    indented
}

fn merge_from_import_lines(lines: Vec<String>) -> Vec<String> {
    let mut merged: Vec<String> = Vec::with_capacity(lines.len());
    let mut idx = 0;
    while idx < lines.len() {
        let line = lines[idx].trim().to_string();
        if line.starts_with("from ")
            && idx + 1 < lines.len()
            && lines[idx + 1].trim_start().starts_with("import ")
        {
            let combined = format!("{} {}", line.trim_end(), lines[idx + 1].trim_start());
            merged.push(combined);
            idx += 2;
        } else {
            merged.push(line);
            idx += 1;
        }
    }
    merged
}

fn is_assignment_operator(token: &str) -> bool {
    matches!(
        token,
        "=" | "+=" | "-=" | "*=" | "/=" | "//=" | "%=" | "^=" | "|=" | "&=" | "**=" | "<<=" | ">>="
    )
}

fn is_shell_executable(token: &str) -> bool {
    let trimmed = token.trim_matches(|c| c == '\'' || c == '"');
    let lowered = Path::new(trimmed)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(trimmed)
        .to_ascii_lowercase();
    matches!(
        lowered.as_str(),
        "bash"
            | "bash.exe"
            | "sh"
            | "sh.exe"
            | "dash"
            | "dash.exe"
            | "zsh"
            | "zsh.exe"
            | "ksh"
            | "ksh.exe"
            | "busybox"
    )
}

fn escape_single_quotes_for_shell(s: &str) -> String {
    if !s.contains('\'') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + 8);
    let mut iter = s.split('\'');
    if let Some(first) = iter.next() {
        out.push_str(first);
    }
    for segment in iter {
        out.push_str("'\\''");
        out.push_str(segment);
    }
    out
}

fn is_node_invocation_token(token: &str) -> bool {
    let trimmed = token.trim_matches(|c| c == '\'' || c == '"');
    let base = Path::new(trimmed)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(trimmed)
        .to_ascii_lowercase();
    matches!(base.as_str(), "node" | "node.exe" | "nodejs" | "nodejs.exe")
}

fn format_node_script(tokens: &[String], script_idx: usize, script: &str) -> Option<String> {
    let block = build_js_script_block(script)?;
    let mut parts: Vec<String> = Vec::with_capacity(tokens.len());
    for (idx, token) in tokens.iter().enumerate() {
        if idx == script_idx {
            parts.push(block.clone());
        } else {
            parts.push(escape_token_for_display(token));
        }
    }
    Some(parts.join(" "))
}

fn build_js_script_block(script: &str) -> Option<String> {
    let normalized = script.replace("\r\n", "\n");
    let lines: Vec<String> = if normalized.contains('\n') {
        normalized
            .lines()
            .map(|line| line.trim_end().to_string())
            .collect()
    } else {
        split_js_statements(&normalized)
    };

    let meaningful: Vec<String> = lines
        .into_iter()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();

    if meaningful.len() <= 1 {
        return None;
    }

    let indented = indent_js_lines(meaningful);
    let mut block = String::from("'\n");
    for line in indented {
        block.push_str("    ");
        let escaped = escape_single_quotes_for_shell(line.as_str());
        block.push_str(escaped.as_str());
        block.push('\n');
    }
    block.push('\'');
    Some(block)
}

fn split_js_statements(script: &str) -> Vec<String> {
    let mut segments: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut in_backtick = false;
    let mut escape = false;
    let mut paren_depth = 0i32;
    let mut brace_depth = 0i32;
    let mut bracket_depth = 0i32;

    for ch in script.chars() {
        if escape {
            current.push(ch);
            escape = false;
            continue;
        }

        match ch {
            '\\' if in_single || in_double || in_backtick => {
                escape = true;
                current.push(ch);
                continue;
            }
            '\'' if !in_double && !in_backtick => {
                in_single = !in_single;
                current.push(ch);
                continue;
            }
            '"' if !in_single && !in_backtick => {
                in_double = !in_double;
                current.push(ch);
                continue;
            }
            '`' if !in_single && !in_double => {
                in_backtick = !in_backtick;
                current.push(ch);
                continue;
            }
            _ => {}
        }

        if !(in_single || in_double || in_backtick) {
            match ch {
                '{' => brace_depth += 1,
                '}' => {
                    if brace_depth > 0 {
                        brace_depth -= 1;
                    }
                }
                '(' => paren_depth += 1,
                ')' => {
                    if paren_depth > 0 {
                        paren_depth -= 1;
                    }
                }
                '[' => bracket_depth += 1,
                ']' => {
                    if bracket_depth > 0 {
                        bracket_depth -= 1;
                    }
                }
                ';' if brace_depth == 0 && paren_depth == 0 && bracket_depth == 0 => {
                    current.push(ch);
                    let seg = current.trim().to_string();
                    if !seg.is_empty() {
                        segments.push(seg);
                    }
                    current.clear();
                    continue;
                }
                '\n' if brace_depth == 0 && paren_depth == 0 && bracket_depth == 0 => {
                    let seg = current.trim().to_string();
                    if !seg.is_empty() {
                        segments.push(seg);
                    }
                    current.clear();
                    continue;
                }
                _ => {}
            }
        }

        current.push(ch);
    }

    let seg = current.trim().to_string();
    if !seg.is_empty() {
        segments.push(seg);
    }
    segments
}

fn indent_js_lines(lines: Vec<String>) -> Vec<String> {
    let mut indented: Vec<String> = Vec::with_capacity(lines.len());
    let mut indent_level: usize = 0;

    for raw in lines {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            indented.push(String::new());
            continue;
        }

        let mut leading_closers = 0usize;
        let mut cut = trimmed.len();
        for (idx, ch) in trimmed.char_indices() {
            match ch {
                '}' | ']' => {
                    leading_closers += 1;
                    cut = idx + ch.len_utf8();
                    continue;
                }
                _ => {
                    cut = idx;
                    break;
                }
            }
        }

        if leading_closers > 0 && cut >= trimmed.len() {
            cut = trimmed.len();
        }

        if leading_closers > 0 {
            indent_level = indent_level.saturating_sub(leading_closers);
        }

        let remainder = trimmed[cut..].trim_start();
        let mut line = String::with_capacity(remainder.len() + indent_level * 4);
        for _ in 0..indent_level {
            line.push_str("    ");
        }
        if remainder.is_empty() && cut < trimmed.len() {
            line.push_str(trimmed);
        } else {
            line.push_str(remainder);
        }
        indented.push(line);

        let (opens, closes) = js_brace_deltas(trimmed);
        indent_level = indent_level + opens;
        indent_level = indent_level.saturating_sub(closes);
    }

    indented
}

fn js_brace_deltas(line: &str) -> (usize, usize) {
    let mut opens = 0usize;
    let mut closes = 0usize;
    let mut in_single = false;
    let mut in_double = false;
    let mut in_backtick = false;
    let mut escape = false;

    for ch in line.chars() {
        if escape {
            escape = false;
            continue;
        }
        match ch {
            '\\' if in_single || in_double || in_backtick => {
                escape = true;
            }
            '\'' if !in_double && !in_backtick => in_single = !in_single,
            '"' if !in_single && !in_backtick => in_double = !in_double,
            '`' if !in_single && !in_double => in_backtick = !in_backtick,
            '{' if !(in_single || in_double || in_backtick) => opens += 1,
            '}' if !(in_single || in_double || in_backtick) => closes += 1,
            _ => {}
        }
    }

    (opens, closes)
}

fn is_shell_invocation_token(token: &str) -> bool {
    is_shell_executable(token)
}

fn format_shell_script(tokens: &[String], script_idx: usize, script: &str) -> Option<String> {
    let block = build_shell_script_block(script)?;
    let mut parts: Vec<String> = Vec::with_capacity(tokens.len());
    for (idx, token) in tokens.iter().enumerate() {
        if idx == script_idx {
            parts.push(block.clone());
        } else {
            parts.push(escape_token_for_display(token));
        }
    }
    Some(parts.join(" "))
}

fn build_shell_script_block(script: &str) -> Option<String> {
    let normalized = script.replace("\r\n", "\n");
    let segments = split_shell_statements(&normalized);
    let meaningful: Vec<String> = segments
        .into_iter()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();
    if meaningful.len() <= 1 {
        return None;
    }
    let indented = indent_shell_lines(meaningful);
    let mut block = String::from("'\n");
    for line in indented {
        block.push_str("    ");
        let escaped = escape_single_quotes_for_shell(line.as_str());
        block.push_str(escaped.as_str());
        block.push('\n');
    }
    block.push('\'');
    Some(block)
}

fn split_shell_statements(script: &str) -> Vec<String> {
    let mut segments: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut escape = false;

    let chars: Vec<char> = script.chars().collect();
    let mut idx = 0;
    while idx < chars.len() {
        let ch = chars[idx];
        if escape {
            current.push(ch);
            escape = false;
            idx += 1;
            continue;
        }
        match ch {
            '\\' if in_single || in_double => {
                escape = true;
                current.push(ch);
                idx += 1;
                continue;
            }
            '\'' if !in_double => {
                in_single = !in_single;
                current.push(ch);
                idx += 1;
                continue;
            }
            '"' if !in_single => {
                in_double = !in_double;
                current.push(ch);
                idx += 1;
                continue;
            }
            ';' if !(in_single || in_double) => {
                current.push(ch);
                segments.push(current.trim().to_string());
                current.clear();
                idx += 1;
                continue;
            }
            '&' | '|' if !(in_single || in_double) => {
                let current_op = ch;
                if idx + 1 < chars.len() && chars[idx + 1] == current_op {
                    if !current.trim().is_empty() {
                        segments.push(current.trim().to_string());
                    }
                    segments.push(format!("{}{}", current_op, current_op));
                    current.clear();
                    idx += 2;
                    continue;
                }
            }
            '\n' if !(in_single || in_double) => {
                segments.push(current.trim().to_string());
                current.clear();
                idx += 1;
                continue;
            }
            _ => {}
        }
        current.push(ch);
        idx += 1;
    }

    if !current.trim().is_empty() {
        segments.push(current.trim().to_string());
    }

    segments
}

fn indent_shell_lines(lines: Vec<String>) -> Vec<String> {
    let mut indented: Vec<String> = Vec::with_capacity(lines.len());
    let mut indent_level: usize = 0;

    for raw in lines {
        if raw == "&&" || raw == "||" {
            let mut line = String::new();
            for _ in 0..indent_level {
                line.push_str("    ");
            }
            line.push_str(raw.as_str());
            indented.push(line);
            continue;
        }

        let trimmed = raw.trim();
        if trimmed.is_empty() {
            indented.push(String::new());
            continue;
        }

        if trimmed.starts_with("fi") || trimmed.starts_with("done") || trimmed.starts_with("esac") {
            indent_level = indent_level.saturating_sub(1);
        }

        let mut line = String::new();
        for _ in 0..indent_level {
            line.push_str("    ");
        }
        line.push_str(trimmed);
        indented.push(line);

        if trimmed.ends_with("do")
            || trimmed.ends_with("then")
            || trimmed.ends_with("{")
            || trimmed.starts_with("case ")
        {
            indent_level += 1;
        }
    }

    indented
}

fn adjust_bracket_depth(token: &str, paren: &mut i32, bracket: &mut i32, brace: &mut i32) {
    for ch in token.chars() {
        match ch {
            '(' => *paren += 1,
            ')' => *paren -= 1,
            '[' => *bracket += 1,
            ']' => *bracket -= 1,
            '{' => *brace += 1,
            '}' => *brace -= 1,
            _ => {}
        }
    }
    *paren = (*paren).max(0);
    *bracket = (*bracket).max(0);
    *brace = (*brace).max(0);
}

fn is_python_invocation_token(token: &str) -> bool {
    if token.is_empty() || token.contains('=') {
        return false;
    }

    let trimmed = token.trim_matches(|c| c == '\'' || c == '"');
    let base = Path::new(trimmed)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(trimmed)
        .to_ascii_lowercase();

    if !base.starts_with("python") {
        return false;
    }

    let suffix = &base["python".len()..];
    suffix.is_empty()
        || suffix
            .chars()
            .all(|ch| ch.is_ascii_digit() || ch == '.' || ch == 'w')
}

fn escape_token_for_display(token: &str) -> String {
    if is_shell_word(token) {
        token.to_string()
    } else {
        let mut escaped = String::from("'");
        for ch in token.chars() {
            if ch == '\'' {
                escaped.push_str("'\\''");
            } else {
                escaped.push(ch);
            }
        }
        escaped.push('\'');
        escaped
    }
}

fn is_shell_word(token: &str) -> bool {
    token.chars().all(|ch| {
        matches!(
            ch,
            'a'..='z'
                | 'A'..='Z'
                | '0'..='9'
                | '_'
                | '-'
                | '.'
                | '/'
                | ':'
                | ','
                | '@'
                | '%'
                | '+'
                | '='
                | '['
                | ']'
        )
    })
}

fn script_has_semicolon_outside_quotes(script: &str) -> bool {
    let mut in_single = false;
    let mut in_double = false;
    let mut escape = false;

    for ch in script.chars() {
        if escape {
            escape = false;
            continue;
        }
        match ch {
            '\\' if in_single || in_double => {
                escape = true;
            }
            '\'' if !in_double => {
                in_single = !in_single;
            }
            '"' if !in_single => {
                in_double = !in_double;
            }
            ';' if !in_single && !in_double => return true,
            _ => {}
        }
    }

    false
}

fn split_semicolon_statements(script: &str) -> Vec<String> {
    let mut segments: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut escape = false;

    for ch in script.chars() {
        if escape {
            current.push(ch);
            escape = false;
            continue;
        }

        match ch {
            '\\' if in_single || in_double => {
                escape = true;
                current.push(ch);
            }
            '\'' if !in_double => {
                in_single = !in_single;
                current.push(ch);
            }
            '"' if !in_single => {
                in_double = !in_double;
                current.push(ch);
            }
            ';' if !in_single && !in_double => {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    segments.push(trimmed.to_string());
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    let trimmed = current.trim();
    if !trimmed.is_empty() {
        segments.push(trimmed.to_string());
    }

    segments
}

fn running_status_line(message: String) -> Line<'static> {
    Line::from(vec![
        Span::styled("└ ", Style::default().fg(crate::colors::border_dim())),
        Span::styled(message, Style::default().fg(crate::colors::text_dim())),
    ])
}

fn new_parsed_command(
    parsed_commands: &[ParsedCommand],
    output: Option<&CommandOutput>,
    stream_preview: Option<&CommandOutput>,
    start_time: Option<Instant>,
) -> Vec<Line<'static>> {
    let meta = ParsedExecMetadata::from_commands(parsed_commands);
    let action = meta.action;
    let ctx_path = meta.ctx_path.as_deref();
    let suppress_run_header = matches!(action, ExecAction::Run) && output.is_some();
    let mut lines: Vec<Line> = Vec::new();
    let mut running_status: Option<Line<'static>> = None;
    if !suppress_run_header {
        match output {
            None => {
                if matches!(action, ExecAction::Run) {
                    let mut message = match &ctx_path {
                        Some(p) => format!("Running... in {p}"),
                        None => "Running...".to_string(),
                    };
                    if let Some(start) = start_time {
                        let elapsed = start.elapsed();
                        message = format!("{message} ({})", format_duration(elapsed));
                    }
                    running_status = Some(running_status_line(message));
                } else {
                    let duration_suffix = if let Some(start) = start_time {
                        let elapsed = start.elapsed();
                        format!(" ({})", format_duration(elapsed))
                    } else {
                        String::new()
                    };
                    let header = match action {
                        ExecAction::Read => "Read",
                        ExecAction::Search => "Search",
                        ExecAction::List => "List",
                        ExecAction::Run => unreachable!(),
                    };
                    lines.push(Line::styled(
                        format!("{header}{duration_suffix}"),
                        Style::default().fg(crate::colors::text_dim()),
                    ));
                }
            }
            Some(o) if o.exit_code == 0 => {
                if matches!(
                    action,
                    ExecAction::Read | ExecAction::Search | ExecAction::List
                ) {
                    lines.push(Line::styled(
                        match action {
                            ExecAction::Read => "Read",
                            ExecAction::Search => "Search",
                            ExecAction::List => "List",
                            ExecAction::Run => unreachable!(),
                        },
                        Style::default().fg(crate::colors::text()),
                    ));
                } else {
                    let done = match ctx_path {
                        Some(p) => format!("Ran in {p}"),
                        None => "Ran".to_string(),
                    };
                    lines.push(Line::styled(
                        done,
                        Style::default()
                            .fg(crate::colors::text_bright())
                            .add_modifier(Modifier::BOLD),
                    ));
                }
            }
            Some(_o) => {
                if matches!(
                    action,
                    ExecAction::Read | ExecAction::Search | ExecAction::List
                ) {
                    lines.push(Line::styled(
                        match action {
                            ExecAction::Read => "Read",
                            ExecAction::Search => "Search",
                            ExecAction::List => "List",
                            ExecAction::Run => unreachable!(),
                        },
                        Style::default().fg(crate::colors::text()),
                    ));
                } else {
                    let done = match ctx_path {
                        Some(p) => format!("Ran in {p}"),
                        None => "Ran".to_string(),
                    };
                    lines.push(Line::styled(
                        done,
                        Style::default()
                            .fg(crate::colors::text_bright())
                            .add_modifier(Modifier::BOLD),
                    ));
                }
            }
        }
    }

    // Collect any paths referenced by search commands to suppress redundant directory lines
    let search_paths = &meta.search_paths;

    // We'll emit only content lines here; the header above already communicates the action.
    // Use a single leading "└ " for the very first content line, then indent subsequent ones,
    // except when we're showing an inline running status for ExecAction::Run.
    let mut any_content_emitted = false;
    let use_content_connectors = !(matches!(action, ExecAction::Run) && output.is_none());

    // Restrict displayed entries to the primary action for this cell.
    // For the generic "run" header, allow Run/Test/Lint/Format entries.
    let expected_label: Option<&'static str> = match action {
        ExecAction::Read => Some("Read"),
        ExecAction::Search => Some("Search"),
        ExecAction::List => Some("List"),
        ExecAction::Run => None,
    };

    for parsed in parsed_commands.iter() {
        // Produce a logical label and content string without icons
        let (label, content) = match parsed {
            ParsedCommand::Read { name, cmd, .. } => {
                let mut c = name.clone();
                if let Some(ann) = parse_read_line_annotation(cmd) {
                    c = format!("{c} {ann}");
                }
                ("Read".to_string(), c)
            }
            ParsedCommand::ListFiles { cmd: _, path } => match path {
                Some(p) => {
                    if search_paths.contains(p) {
                        (String::new(), String::new()) // suppressed
                    } else {
                        let display_p = if p.ends_with('/') {
                            p.to_string()
                        } else {
                            format!("{p}/")
                        };
                        ("List".to_string(), format!("{display_p}"))
                    }
                }
                None => ("List".to_string(), "./".to_string()),
            },
            ParsedCommand::Search { query, path, cmd } => {
                // Format query for display: unescape backslash-escapes and close common unbalanced delimiters
                let prettify_term = |s: &str| -> String {
                    // General unescape: turn "\X" into "X" for any X
                    let mut out = String::with_capacity(s.len());
                    let mut iter = s.chars();
                    while let Some(ch) = iter.next() {
                        if ch == '\\' {
                            if let Some(next) = iter.next() {
                                out.push(next);
                            } else {
                                out.push('\\');
                            }
                        } else {
                            out.push(ch);
                        }
                    }
                    // Balance parentheses
                    let opens_paren = out.matches("(").count();
                    let closes_paren = out.matches(")").count();
                    for _ in 0..opens_paren.saturating_sub(closes_paren) {
                        out.push(')');
                    }
                    // Balance curly braces
                    let opens_curly = out.matches("{").count();
                    let closes_curly = out.matches("}").count();
                    for _ in 0..opens_curly.saturating_sub(closes_curly) {
                        out.push('}');
                    }
                    out
                };
                let fmt_query = |q: &str| -> String {
                    let mut parts: Vec<String> = q
                        .split('|')
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                        .map(prettify_term)
                        .collect();
                    match parts.len() {
                        0 => String::new(),
                        1 => parts.remove(0),
                        2 => format!("{} and {}", parts[0], parts[1]),
                        _ => {
                            let last = parts.last().cloned().unwrap_or_default();
                            let head = &parts[..parts.len() - 1];
                            format!("{} and {}", head.join(", "), last)
                        }
                    }
                };
                match (query, path) {
                    (Some(q), Some(p)) => {
                        let display_p = if p.ends_with('/') {
                            p.to_string()
                        } else {
                            format!("{p}/")
                        };
                        (
                            "Search".to_string(),
                            format!("{} in {}", fmt_query(q), display_p),
                        )
                    }
                    (Some(q), None) => ("Search".to_string(), format!("{}", fmt_query(q))),
                    (None, Some(p)) => {
                        let display_p = if p.ends_with('/') {
                            p.to_string()
                        } else {
                            format!("{p}/")
                        };
                        ("Search".to_string(), format!(" in {}", display_p))
                    }
                    (None, None) => ("Search".to_string(), cmd.clone()),
                }
            }
            ParsedCommand::ReadCommand { cmd } => ("Run".to_string(), cmd.clone()),
            // Upstream-only variants handled as generic runs in this fork
            ParsedCommand::Unknown { cmd } => {
                let t = cmd.trim();
                let lower = t.to_lowercase();
                if lower.starts_with("echo") && lower.contains("---") {
                    (String::new(), String::new())
                } else {
                    ("Run".to_string(), format_inline_script_for_display(cmd))
                }
            } // ParsedCommand::Noop { .. } => continue,
        };

        // Keep only entries that match the primary action grouping.
        if let Some(exp) = expected_label {
            if label != exp {
                continue;
            }
        } else if !(label == "Run" || label == "Search") {
            continue;
        }

        // Skip suppressed entries
        if label.is_empty() && content.is_empty() {
            continue;
        }

        // Split content into lines and push without repeating the action label
        for line_text in content.lines() {
            if line_text.is_empty() {
                continue;
            }
            let prefix = if !any_content_emitted {
                if suppress_run_header || !use_content_connectors {
                    ""
                } else {
                    "└ "
                }
            } else if suppress_run_header || !use_content_connectors {
                ""
            } else {
                "  "
            };
            let mut spans: Vec<Span<'static>> = Vec::new();
            if !prefix.is_empty() {
                spans.push(Span::styled(
                    prefix,
                    Style::default().add_modifier(Modifier::DIM),
                ));
            }

            match label.as_str() {
                // Highlight searched terms in normal text color; keep connectors/path dim
                "Search" => {
                    let remaining = line_text.to_string();
                    // Split off optional path suffix. Support both " (in ...)" and " in <dir>/" forms.
                    let (terms_part, path_part) = if let Some(idx) = remaining.rfind(" (in ") {
                        (
                            remaining[..idx].to_string(),
                            Some(remaining[idx..].to_string()),
                        )
                    } else if let Some(idx) = remaining.rfind(" in ") {
                        let suffix = &remaining[idx + 1..]; // keep leading space for styling
                        // Heuristic: treat as path if it ends with '/'
                        if suffix.trim_end().ends_with('/') {
                            (
                                remaining[..idx].to_string(),
                                Some(remaining[idx..].to_string()),
                            )
                        } else {
                            (remaining.clone(), None)
                        }
                    } else {
                        (remaining.clone(), None)
                    };
                    // Tokenize terms by ", " and " and " while preserving separators
                    let tmp = terms_part.clone();
                    // First, split by ", "
                    let chunks: Vec<String> = if tmp.contains(", ") {
                        tmp.split(", ").map(|s| s.to_string()).collect()
                    } else {
                        vec![tmp.clone()]
                    };
                    for (i, chunk) in chunks.iter().enumerate() {
                        if i > 0 {
                            // Add comma separator between items (dim)
                            spans.push(Span::styled(
                                ", ",
                                Style::default().fg(crate::colors::text_dim()),
                            ));
                        }
                        // Within each chunk, if it contains " and ", split into left and right with dimmed " and "
                        if let Some((left, right)) = chunk.rsplit_once(" and ") {
                            if !left.is_empty() {
                                spans.push(Span::styled(
                                    left.to_string(),
                                    Style::default().fg(crate::colors::text()),
                                ));
                                spans.push(Span::styled(
                                    " and ",
                                    Style::default().fg(crate::colors::text_dim()),
                                ));
                                spans.push(Span::styled(
                                    right.to_string(),
                                    Style::default().fg(crate::colors::text()),
                                ));
                            } else {
                                spans.push(Span::styled(
                                    chunk.to_string(),
                                    Style::default().fg(crate::colors::text()),
                                ));
                            }
                        } else {
                            spans.push(Span::styled(
                                chunk.to_string(),
                                Style::default().fg(crate::colors::text()),
                            ));
                        }
                    }
                    if let Some(p) = path_part {
                        // Dim the entire path portion including the " in " or " (in " prefix
                        spans.push(Span::styled(
                            p,
                            Style::default().fg(crate::colors::text_dim()),
                        ));
                    }
                }
                // Highlight filenames in Read; keep line ranges dim
                "Read" => {
                    if let Some(idx) = line_text.find(" (") {
                        let (fname, rest) = line_text.split_at(idx);
                        spans.push(Span::styled(
                            fname.to_string(),
                            Style::default().fg(crate::colors::text()),
                        ));
                        spans.push(Span::styled(
                            rest.to_string(),
                            Style::default().fg(crate::colors::text_dim()),
                        ));
                    } else {
                        spans.push(Span::styled(
                            line_text.to_string(),
                            Style::default().fg(crate::colors::text()),
                        ));
                    }
                }
                // List: highlight directory names
                "List" => {
                    spans.push(Span::styled(
                        line_text.to_string(),
                        Style::default().fg(crate::colors::text()),
                    ));
                }
                _ => {
                    // For executed commands (Run/Test/Lint/etc.), use shell syntax highlighting.
                    let normalized = normalize_shell_command_display(line_text);
                    let display_line = insert_line_breaks_after_double_ampersand(&normalized);
                    let mut hl =
                        crate::syntax_highlight::highlight_code_block(&display_line, Some("bash"));
                    if let Some(mut first_line) = hl.pop() {
                        emphasize_shell_command_name(&mut first_line);
                        spans.extend(first_line.spans.into_iter());
                    } else {
                        spans.push(Span::styled(
                            display_line,
                            Style::default().fg(crate::colors::text()),
                        ));
                    }
                }
            }

            lines.push(Line::from(spans));
            any_content_emitted = true;
        }
    }

    // If this is a List cell and the loop above produced no content (e.g.,
    // the list path was suppressed because a Search referenced the same path),
    // emit a single contextual line so the location is always visible.
    if matches!(action, ExecAction::List) && !any_content_emitted {
        let display_p = match ctx_path {
            Some(p) if !p.is_empty() => {
                if p.ends_with('/') {
                    p.to_string()
                } else {
                    format!("{p}/")
                }
            }
            _ => "./".to_string(),
        };
        lines.push(Line::from(vec![
            Span::styled("└ ", Style::default().add_modifier(Modifier::DIM)),
            Span::styled(
                format!("{display_p}"),
                Style::default().fg(crate::colors::text()),
            ),
        ]));
        // no-op: avoid unused assignment warning; the variable's value is not consumed later
    }

    // Show stdout for real run commands; keep read/search/list concise unless error
    let show_stdout = matches!(action, ExecAction::Run);
    let use_angle_pipe = show_stdout; // add "> " prefix for run output
    let display_output = output.or(stream_preview);
    let mut preview_lines = output_lines(display_output, !show_stdout, use_angle_pipe);
    if let Some(status_line) = running_status {
        if let Some(last) = preview_lines.last() {
            let is_blank = last
                .spans
                .iter()
                .all(|sp| sp.content.as_ref().trim().is_empty());
            if is_blank {
                preview_lines.pop();
            }
        }
        preview_lines.push(status_line);
    }
    lines.extend(preview_lines);
    lines.push(Line::from(""));
    lines
}

fn new_exec_command_generic(
    command: &[String],
    output: Option<&CommandOutput>,
    stream_preview: Option<&CommandOutput>,
    start_time: Option<Instant>,
) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let command_escaped = strip_bash_lc_and_escape(command);
    let normalized = normalize_shell_command_display(&command_escaped);
    let command_display = insert_line_breaks_after_double_ampersand(&normalized);
    // Highlight the command as bash and then append a dimmed duration to the
    // first visual line while running.
    let mut highlighted_cmd =
        crate::syntax_highlight::highlight_code_block(&command_display, Some("bash"));

    for (idx, line) in highlighted_cmd.iter_mut().enumerate() {
        emphasize_shell_command_name(line);
        if idx > 0 {
            line.spans.insert(
                0,
                Span::styled("  ", Style::default().fg(crate::colors::text())),
            );
        }
    }

    let render_running_header = output.is_none();
    let display_output = output.or(stream_preview);
    let mut running_status = None;
    if render_running_header {
        let mut message = "Running...".to_string();
        if let Some(start) = start_time {
            let elapsed = start.elapsed();
            message = format!("{message} ({})", format_duration(elapsed));
        }
        running_status = Some(running_status_line(message));
    }

    if output.is_some() {
        for line in highlighted_cmd.iter_mut() {
            for span in line.spans.iter_mut() {
                span.style = span.style.fg(crate::colors::text_bright());
            }
        }
    }

    lines.extend(highlighted_cmd);

    let mut preview_lines = output_lines(display_output, false, true);
    if let Some(status_line) = running_status {
        if let Some(last) = preview_lines.last() {
            let is_blank = last
                .spans
                .iter()
                .all(|sp| sp.content.as_ref().trim().is_empty());
            if is_blank {
                preview_lines.pop();
            }
        }
        preview_lines.push(status_line);
    }

    lines.extend(preview_lines);
    lines
}

#[allow(dead_code)]
pub(crate) fn new_active_mcp_tool_call(invocation: McpInvocation) -> ToolCallCell {
    let title_line = Line::styled("Working", Style::default().fg(crate::colors::info()));
    let lines: Vec<Line> = vec![
        title_line,
        format_mcp_invocation(invocation),
        Line::from(""),
    ];
    ToolCallCell::new(lines, ToolCallStatus::Running)
}

#[allow(dead_code)]
pub(crate) fn new_active_custom_tool_call(tool_name: String, args: Option<String>) -> ToolCallCell {
    let title_line = Line::styled("Working", Style::default().fg(crate::colors::info()));
    let invocation_str = if let Some(args) = args {
        format!("{}({})", tool_name, args)
    } else {
        format!("{}()", tool_name)
    };

    let lines: Vec<Line> = vec![
        title_line,
        Line::styled(
            invocation_str,
            Style::default()
                .fg(crate::colors::text_dim())
                .add_modifier(Modifier::ITALIC),
        ),
        Line::from(""),
    ];
    ToolCallCell::new(lines, ToolCallStatus::Running)
}

// Friendly present-participle titles for running browser tools
fn browser_running_title(tool_name: &str) -> &'static str {
    match tool_name {
        "browser_click" => "Clicking...",
        "browser_type" => "Typing...",
        "browser_key" => "Sending key...",
        "browser_javascript" => "Running JavaScript...",
        "browser_scroll" => "Scrolling...",
        "browser_open" => "Opening...",
        "browser_close" => "Closing...",
        "browser_status" => "Checking status...",
        "browser_history" => "Navigating...",
        "browser_inspect" => "Inspecting...",
        "browser_console" => "Reading console...",
        "browser_move" => "Moving...",
        _ => "Working...",
    }
}

pub(crate) fn new_running_browser_tool_call(
    tool_name: String,
    args: Option<String>,
) -> RunningToolCallCell {
    // Parse args JSON and use compact humanized form when possible
    let mut arg_lines: Vec<Line<'static>> = Vec::new();
    if let Some(args_str) = args {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&args_str) {
            if let Some(lines) = format_browser_args_humanized(&tool_name, &json) {
                arg_lines.extend(lines);
            } else {
                arg_lines.extend(format_browser_args_line(&json));
            }
        }
    }
    let arg_lines = semantic::lines_from_ratatui(arg_lines);
    RunningToolCallCell::new(RunningToolCallState::new(
        browser_running_title(&tool_name).to_string(),
        SystemTime::now(),
        arg_lines,
        false,
        false,
        None,
    ))
}

fn custom_tool_running_title(tool_name: &str) -> String {
    if tool_name == "wait" {
        return "Waiting".to_string();
    }
    if tool_name.starts_with("agent_") {
        // Reuse agent title and append ellipsis
        format!("{}...", agent_tool_title(tool_name))
    } else if tool_name.starts_with("browser_") {
        browser_running_title(tool_name).to_string()
    } else {
        // TitleCase from snake_case and append ellipsis
        let pretty = tool_name
            .split('_')
            .filter(|s| !s.is_empty())
            .map(|s| {
                let mut chars = s.chars();
                match chars.next() {
                    Some(f) => format!("{}{}", f.to_uppercase(), chars.as_str()),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        format!("{}...", pretty)
    }
}

pub(crate) fn new_running_custom_tool_call(
    tool_name: String,
    args: Option<String>,
) -> RunningToolCallCell {
    // Parse args JSON and format as key/value lines
    let mut arg_lines: Vec<Line<'static>> = Vec::new();
    let mut wait_has_target = false;
    let mut wait_has_call_id = false;
    let mut wait_cap_ms = None;
    if let Some(args_str) = args {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&args_str) {
            if tool_name == "wait" {
                wait_cap_ms = json.get("timeout_ms").and_then(|v| v.as_u64());
                if let Some(for_what) = json.get("for").and_then(|v| v.as_str()) {
                    let cleaned = clean_wait_command(for_what);
                    let mut spans = vec![Span::styled(
                        "└ for ",
                        Style::default().fg(crate::colors::text_dim()),
                    )];
                    spans.push(Span::styled(
                        cleaned,
                        Style::default().fg(crate::colors::text_dim()),
                    ));
                    arg_lines.push(Line::from(spans));
                    wait_has_target = true;
                }
                if let Some(cid) = json.get("call_id").and_then(|v| v.as_str()) {
                    if !wait_has_target {
                        arg_lines.push(Line::from(vec![
                            Span::styled(
                                "└ call_id: ",
                                Style::default().fg(crate::colors::text_dim()),
                            ),
                            Span::styled(
                                cid.to_string(),
                                Style::default().fg(crate::colors::text()),
                            ),
                        ]));
                    }
                    wait_has_call_id = true;
                }
            } else {
                arg_lines.extend(format_browser_args_line(&json));
            }
        } else {
            arg_lines.push(Line::from(vec![
                Span::styled("└ args: ", Style::default().fg(crate::colors::text_dim())),
                Span::styled(args_str, Style::default().fg(crate::colors::text())),
            ]));
        }
    }
    let arg_lines = semantic::lines_from_ratatui(arg_lines);
    RunningToolCallCell::new(RunningToolCallState::new(
        custom_tool_running_title(&tool_name),
        SystemTime::now(),
        arg_lines,
        wait_has_target,
        wait_has_call_id,
        wait_cap_ms,
    ))
}

/// Running web search call (native Responses web_search)
pub(crate) fn new_running_web_search(query: Option<String>) -> RunningToolCallCell {
    let mut arg_lines: Vec<Line<'static>> = Vec::new();
    if let Some(q) = query {
        arg_lines.push(Line::from(vec![
            Span::styled("└ query: ", Style::default().fg(crate::colors::text_dim())),
            Span::styled(q, Style::default().fg(crate::colors::text())),
        ]));
    }
    let arg_lines = semantic::lines_from_ratatui(arg_lines);
    RunningToolCallCell::new(RunningToolCallState::new(
        "Web Search...".to_string(),
        SystemTime::now(),
        arg_lines,
        false,
        false,
        None,
    ))
}

pub(crate) fn new_running_mcp_tool_call(invocation: McpInvocation) -> RunningToolCallCell {
    // Represent as provider.tool(...) on one dim line beneath a generic running header with timer
    let line = format_mcp_invocation(invocation);
    RunningToolCallCell::new(RunningToolCallState::new(
        "Working...".to_string(),
        SystemTime::now(),
        semantic::lines_from_ratatui(vec![line]),
        false,
        false,
        None,
    ))
}

pub(crate) fn new_completed_custom_tool_call(
    tool_name: String,
    args: Option<String>,
    duration: Duration,
    success: bool,
    result: String,
) -> ToolCallCell {
    // Special rendering for browser_* tools
    if tool_name.starts_with("browser_") {
        return new_completed_browser_tool_call(tool_name, args, duration, success, result);
    }
    // Special rendering for agent_* tools
    if tool_name.starts_with("agent_") {
        return new_completed_agent_tool_call(tool_name, args, duration, success, result);
    }
    let duration = format_duration(duration);
    let status_str = if success { "Complete" } else { "Error" };
    let title_line = if success {
        Line::from(vec![
            Span::styled(status_str, Style::default().fg(crate::colors::success())),
            format!(", duration: {duration}").dim(),
        ])
    } else {
        Line::from(vec![
            Span::styled(status_str, Style::default().fg(crate::colors::error())),
            format!(", duration: {duration}").dim(),
        ])
    };

    let invocation_str = if let Some(args) = args {
        format!("{}({})", tool_name, args)
    } else {
        format!("{}()", tool_name)
    };

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(title_line);
    lines.push(Line::styled(
        invocation_str,
        Style::default()
            .fg(crate::colors::text_dim())
            .add_modifier(Modifier::ITALIC),
    ));

    if !result.is_empty() {
        lines.push(Line::from(""));
        let mut preview = build_preview_lines(&result, true);
        preview = preview
            .into_iter()
            .map(|l| l.style(Style::default().fg(crate::colors::text_dim())))
            .collect();
        lines.extend(preview);
    }

    lines.push(Line::from(""));
    ToolCallCell::new(
        lines,
        if success {
            ToolCallStatus::Success
        } else {
            ToolCallStatus::Failed
        },
    )
}

/// Completed web_fetch tool call with markdown rendering of the `markdown` field.
// Web fetch preview sizing: show 10 lines at the start and 5 at the end.
const WEB_FETCH_HEAD_LINES: usize = 10;
const WEB_FETCH_TAIL_LINES: usize = 5;

pub(crate) fn new_completed_web_fetch_tool_call(
    cfg: &Config,
    args: Option<String>,
    duration: Duration,
    success: bool,
    result: String,
) -> WebFetchToolCell {
    let duration = format_duration(duration);
    let status_str = if success { "Complete" } else { "Error" };
    let title_line = if success {
        Line::from(vec![
            Span::styled(status_str, Style::default().fg(crate::colors::success())),
            format!(", duration: {duration}").dim(),
        ])
    } else {
        Line::from(vec![
            Span::styled(status_str, Style::default().fg(crate::colors::error())),
            format!(", duration: {duration}").dim(),
        ])
    };

    let invocation_str = if let Some(args) = args {
        format!("{}({})", "web_fetch", args)
    } else {
        format!("{}()", "web_fetch")
    };

    // Header/preamble (no border)
    let mut pre_lines: Vec<Line<'static>> = Vec::new();
    pre_lines.push(title_line);
    pre_lines.push(Line::styled(
        invocation_str,
        Style::default()
            .fg(crate::colors::text_dim())
            .add_modifier(Modifier::ITALIC),
    ));

    // Try to parse JSON and extract the markdown field
    let mut appended_markdown = false;
    let mut body_lines: Vec<Line<'static>> = Vec::new();
    if !result.is_empty() {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&result) {
            if let Some(md) = value.get("markdown").and_then(|v| v.as_str()) {
                // Build a smarter sectioned preview from the raw markdown.
                let mut sect = build_web_fetch_sectioned_preview(md, cfg);
                dim_webfetch_emphasis_and_links(&mut sect);
                body_lines.extend(sect);
                appended_markdown = true;
            }
        }
    }

    // Fallback: compact preview if JSON parse failed or no markdown present
    if !appended_markdown && !result.is_empty() {
        // Fallback to plain text/JSON preview with ANSI preserved.
        let mut pv =
            select_preview_from_plain_text(&result, WEB_FETCH_HEAD_LINES, WEB_FETCH_TAIL_LINES);
        dim_webfetch_emphasis_and_links(&mut pv);
        body_lines.extend(pv);
    }

    // Spacer below header and below body to match exec styling
    pre_lines.push(Line::from(""));
    if !body_lines.is_empty() {
        body_lines.push(Line::from(""));
    }

    WebFetchToolCell {
        pre_lines,
        body_lines,
        state: if success {
            ToolCallStatus::Success
        } else {
            ToolCallStatus::Failed
        },
    }
}

// Helper: choose first `head` and last `tail` non-empty lines from a styled line list
fn select_preview_from_lines(
    lines: &[Line<'static>],
    head: usize,
    tail: usize,
) -> Vec<Line<'static>> {
    fn is_non_empty(l: &Line<'_>) -> bool {
        let s: String = l.spans.iter().map(|sp| sp.content.as_ref()).collect();
        !s.trim().is_empty()
    }
    let non_empty_idx: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| if is_non_empty(l) { Some(i) } else { None })
        .collect();
    if non_empty_idx.len() <= head + tail {
        return lines.to_vec();
    }
    let mut out: Vec<Line<'static>> = Vec::new();
    for &i in non_empty_idx.iter().take(head) {
        out.push(lines[i].clone());
    }
    out.push(Line::from("⋮".dim()));
    for &i in non_empty_idx
        .iter()
        .rev()
        .take(tail)
        .collect::<Vec<_>>()
        .iter()
        .rev()
    {
        out.push(lines[*i].clone());
    }
    out
}

// Helper: like build_preview_lines but parameterized and preserving ANSI
fn select_preview_from_plain_text(text: &str, head: usize, tail: usize) -> Vec<Line<'static>> {
    let processed = format_json_compact(text).unwrap_or_else(|| text.to_string());
    let processed = normalize_overwrite_sequences(&processed);
    let processed = sanitize_for_tui(
        &processed,
        SanitizeMode::AnsiPreserving,
        SanitizeOptions {
            expand_tabs: true,
            tabstop: 4,
            debug_markers: false,
        },
    );
    let non_empty: Vec<&str> = processed.lines().filter(|line| !line.is_empty()).collect();
    fn ansi_line_with_theme_bg(s: &str) -> Line<'static> {
        let mut ln = ansi_escape_line(s);
        for sp in ln.spans.iter_mut() {
            sp.style.bg = None;
        }
        ln
    }
    let mut out: Vec<Line<'static>> = Vec::new();
    if non_empty.len() <= head + tail {
        for s in non_empty {
            out.push(ansi_line_with_theme_bg(s));
        }
        return out;
    }
    for s in non_empty.iter().take(head) {
        out.push(ansi_line_with_theme_bg(s));
    }
    out.push(Line::from("⋮".dim()));
    let start = non_empty.len().saturating_sub(tail);
    for s in &non_empty[start..] {
        out.push(ansi_line_with_theme_bg(s));
    }
    out
}

// ==================== WebFetchToolCell ====================

pub(crate) struct WebFetchToolCell {
    pre_lines: Vec<Line<'static>>,  // header/invocation
    body_lines: Vec<Line<'static>>, // bordered, dim preview
    state: ToolCallStatus,
}

impl HistoryCell for WebFetchToolCell {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn kind(&self) -> HistoryCellType {
        HistoryCellType::Tool {
            status: match self.state {
                ToolCallStatus::Running => ToolStatus::Running,
                ToolCallStatus::Success => ToolStatus::Success,
                ToolCallStatus::Failed => ToolStatus::Failed,
            },
        }
    }
    fn display_lines(&self) -> Vec<Line<'static>> {
        // Fallback textual representation used only for measurement outside custom render
        let mut v = Vec::new();
        v.extend(self.pre_lines.clone());
        v.extend(self.body_lines.clone());
        v
    }
    fn has_custom_render(&self) -> bool {
        true
    }
    fn desired_height(&self, width: u16) -> u16 {
        let pre_text = Text::from(trim_empty_lines(self.pre_lines.clone()));
        let body_text = Text::from(trim_empty_lines(self.body_lines.clone()));
        let pre_total: u16 = Paragraph::new(pre_text)
            .wrap(Wrap { trim: false })
            .line_count(width)
            .try_into()
            .unwrap_or(0);
        let body_total: u16 = Paragraph::new(body_text)
            .wrap(Wrap { trim: false })
            .line_count(width.saturating_sub(2))
            .try_into()
            .unwrap_or(0);
        pre_total.saturating_add(body_total)
    }
    fn custom_render_with_skip(&self, area: Rect, buf: &mut Buffer, skip_rows: u16) {
        // Measure with the same widths we will render with.
        let pre_text = Text::from(trim_empty_lines(self.pre_lines.clone()));
        let body_text = Text::from(trim_empty_lines(self.body_lines.clone()));
        let pre_wrap_width = area.width;
        let body_wrap_width = area.width.saturating_sub(2);
        let pre_total: u16 = Paragraph::new(pre_text.clone())
            .wrap(Wrap { trim: false })
            .line_count(pre_wrap_width)
            .try_into()
            .unwrap_or(0);
        let body_total: u16 = Paragraph::new(body_text.clone())
            .wrap(Wrap { trim: false })
            .line_count(body_wrap_width)
            .try_into()
            .unwrap_or(0);

        let pre_skip = skip_rows.min(pre_total);
        let body_skip = skip_rows.saturating_sub(pre_total).min(body_total);

        let pre_remaining = pre_total.saturating_sub(pre_skip);
        let pre_height = pre_remaining.min(area.height);
        let body_available = area.height.saturating_sub(pre_height);
        let body_remaining = body_total.saturating_sub(body_skip);
        let body_height = body_available.min(body_remaining);

        // Render preamble
        if pre_height > 0 {
            let pre_area = Rect {
                x: area.x,
                y: area.y,
                width: area.width,
                height: pre_height,
            };
            let bg_style = Style::default()
                .bg(crate::colors::background())
                .fg(crate::colors::text());
            fill_rect(buf, pre_area, Some(' '), bg_style);
            let pre_block =
                Block::default().style(Style::default().bg(crate::colors::background()));
            Paragraph::new(pre_text)
                .block(pre_block)
                .wrap(Wrap { trim: false })
                .scroll((pre_skip, 0))
                .style(Style::default().bg(crate::colors::background()))
                .render(pre_area, buf);
        }

        // Render body with left border + dim text
        if body_height > 0 {
            let body_area = Rect {
                x: area.x,
                y: area.y.saturating_add(pre_height),
                width: area.width,
                height: body_height,
            };
            let bg_style = Style::default()
                .bg(crate::colors::background())
                .fg(crate::colors::text_dim());
            fill_rect(buf, body_area, Some(' '), bg_style);
            let block = Block::default()
                .borders(Borders::LEFT)
                .border_style(
                    Style::default()
                        .fg(crate::colors::border_dim())
                        .bg(crate::colors::background()),
                )
                .style(Style::default().bg(crate::colors::background()))
                .padding(Padding {
                    left: 1,
                    right: 0,
                    top: 0,
                    bottom: 0,
                });
            Paragraph::new(body_text)
                .block(block)
                .wrap(Wrap { trim: false })
                .scroll((body_skip, 0))
                .style(
                    Style::default()
                        .bg(crate::colors::background())
                        .fg(crate::colors::text_dim()),
                )
                .render(body_area, buf);
        }
    }
}

// Build sectioned preview for web_fetch markdown:
// - First 2 non-empty lines
// - Up to 5 sections: a heading line (starts with #) plus the next 4 lines
// - Last 2 non-empty lines
// Ellipses (⋮) are inserted between groups. All content is rendered as markdown.
fn build_web_fetch_sectioned_preview(md: &str, cfg: &Config) -> Vec<Line<'static>> {
    let lines: Vec<&str> = md.lines().collect();

    // Collect first 1 and last 1 non-empty lines (by raw markdown lines)
    let first_non_empty: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| if l.trim().is_empty() { None } else { Some(i) })
        .take(1)
        .collect();
    let last_non_empty_rev: Vec<usize> = lines
        .iter()
        .enumerate()
        .rev()
        .filter_map(|(i, l)| if l.trim().is_empty() { None } else { Some(i) })
        .take(1)
        .collect();
    let mut last_non_empty = last_non_empty_rev.clone();
    last_non_empty.reverse();

    // Find up to 5 heading indices outside code fences
    let mut in_code = false;
    let mut section_heads: Vec<usize> = Vec::new();
    let mut i = 0;
    while i < lines.len() && section_heads.len() < 5 {
        let l = lines[i];
        let trimmed = l.trim_start();
        // Toggle code fence state
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_code = !in_code;
            i += 1;
            continue;
        }
        if !in_code {
            // Heading: 1-6 leading # followed by a space
            let mut level = 0usize;
            for ch in trimmed.chars() {
                if ch == '#' {
                    level += 1;
                } else {
                    break;
                }
            }
            if level >= 1 && level <= 6 {
                if trimmed.chars().nth(level).map_or(false, |c| c == ' ') {
                    section_heads.push(i);
                }
            }
        }
        i += 1;
    }

    // Helper to render a slice of raw markdown lines
    let render_slice = |start: usize, end_excl: usize, out: &mut Vec<Line<'static>>| {
        if start >= end_excl || start >= lines.len() {
            return;
        }
        let end = end_excl.min(lines.len());
        let segment = lines[start..end].join("\n");
        let mut seg_lines: Vec<Line<'static>> = Vec::new();
        crate::markdown::append_markdown(&segment, &mut seg_lines, cfg);
        // Trim leading/trailing empties per segment to keep things tight
        out.extend(trim_empty_lines(seg_lines));
    };

    let mut out: Vec<Line<'static>> = Vec::new();

    // First 2 lines
    if !first_non_empty.is_empty() {
        let start = first_non_empty[0];
        let end = first_non_empty
            .last()
            .copied()
            .unwrap_or(start)
            .saturating_add(1);
        render_slice(start, end, &mut out);
    }

    // Sections
    if !section_heads.is_empty() {
        if !out.is_empty() {
            out.push(Line::from("⋮".dim()));
        }
        for (idx, &h) in section_heads.iter().enumerate() {
            // heading + next 4 lines (total up to 5)
            let end = (h + 5).min(lines.len());
            render_slice(h, end, &mut out);
            if idx + 1 < section_heads.len() {
                out.push(Line::from("⋮".dim()));
            }
        }
    }

    // Last 2 lines
    if !last_non_empty.is_empty() {
        // Avoid duplicating lines if they overlap with earlier content
        let last_start = *last_non_empty.first().unwrap_or(&0);
        if !out.is_empty() {
            out.push(Line::from("⋮".dim()));
        }
        let last_end = last_non_empty
            .last()
            .copied()
            .unwrap_or(last_start)
            .saturating_add(1);
        render_slice(last_start, last_end, &mut out);
    }

    if out.is_empty() {
        // Fallback: if nothing matched, show head/tail preview
        let mut all_md_lines: Vec<Line<'static>> = Vec::new();
        crate::markdown::append_markdown(md, &mut all_md_lines, cfg);
        return select_preview_from_lines(
            &all_md_lines,
            WEB_FETCH_HEAD_LINES,
            WEB_FETCH_TAIL_LINES,
        );
    }

    out
}

// Post-process rendered markdown lines to dim emphasis, lists, and links for web_fetch only.
fn dim_webfetch_emphasis_and_links(lines: &mut Vec<Line<'static>>) {
    use ratatui::style::Modifier;
    let text_dim = crate::colors::text_dim();
    let code_bg = crate::colors::code_block_bg();
    // Recompute the link color logic used by the markdown renderer to detect link spans
    let link_fg = crate::colors::mix_toward(crate::colors::text(), crate::colors::primary(), 0.35);
    for line in lines.iter_mut() {
        // Heuristic list detection on the plain text form
        let s: String = line.spans.iter().map(|sp| sp.content.as_ref()).collect();
        let t = s.trim_start();
        let is_list = t.starts_with('-')
            || t.starts_with('*')
            || t.starts_with('+')
            || t.starts_with('•')
            || t.starts_with('·')
            || t.starts_with('⋅')
            || t.chars().take_while(|c| c.is_ascii_digit()).count() > 0
                && (t.chars().skip_while(|c| c.is_ascii_digit()).next() == Some('.')
                    || t.chars().skip_while(|c| c.is_ascii_digit()).next() == Some(')'));

        for sp in line.spans.iter_mut() {
            // Skip code block spans (have a solid code background)
            if sp.style.bg == Some(code_bg) {
                continue;
            }
            let style = &mut sp.style;
            let is_bold = style.add_modifier.contains(Modifier::BOLD);
            let is_under = style.add_modifier.contains(Modifier::UNDERLINED);
            let is_link_colored = style.fg == Some(link_fg);
            if is_list || is_bold || is_under || is_link_colored {
                style.fg = Some(text_dim);
            }
        }
    }
}

// Map `browser_*` tool names to friendly titles
fn browser_tool_title(tool_name: &str) -> &'static str {
    match tool_name {
        "browser_click" => "Browser Click",
        "browser_type" => "Browser Type",
        "browser_key" => "Browser Key",
        "browser_javascript" => "Browser JavaScript",
        "browser_scroll" => "Browser Scroll",
        "browser_open" => "Browser Open",
        "browser_close" => "Browser Close",
        "browser_status" => "Browser Status",
        "browser_history" => "Browser History",
        "browser_inspect" => "Browser Inspect",
        "browser_console" => "Browser Console",
        "browser_cdp" => "Browser CDP",
        "browser_move" => "Browser Move",
        _ => "Browser Tool",
    }
}

fn format_browser_args_line(args: &serde_json::Value) -> Vec<Line<'static>> {
    use serde_json::Value;
    let mut lines: Vec<Line<'static>> = Vec::new();

    let dim = |s: &str| {
        Span::styled(
            s.to_string(),
            Style::default().fg(crate::colors::text_dim()),
        )
    };
    let text = |s: String| Span::styled(s, Style::default().fg(crate::colors::text()));

    // Helper to one-line, truncated representation for values
    fn short(v: &serde_json::Value, key: &str) -> String {
        match v {
            serde_json::Value::String(s) => {
                let one = s.replace('\n', " ");
                let max = if key == "code" { 80 } else { 80 };
                if one.chars().count() > max {
                    let truncated: String = one.chars().take(max).collect();
                    format!("{}…", truncated)
                } else {
                    one
                }
            }
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::Bool(b) => b.to_string(),
            serde_json::Value::Array(a) => format!("[{} items]", a.len()),
            serde_json::Value::Object(o) => format!("{{{} keys}}", o.len()),
            serde_json::Value::Null => "null".to_string(),
        }
    }

    match args {
        Value::Object(map) => {
            // Preserve insertion order (serde_json in this crate preserves order via feature)
            for (k, v) in map {
                let val = short(v, k);
                lines.push(Line::from(vec![
                    dim("└ "),
                    dim(&format!("{}: ", k)),
                    text(val),
                ]));
            }
        }
        Value::Null => {}
        other => {
            lines.push(Line::from(vec![dim("└ args: "), text(other.to_string())]));
        }
    }
    lines
}

// Attempt a compact, humanized one-line summary for browser tools.
// Returns Some(lines) when a concise form is available for the given tool, else None.
fn format_browser_args_humanized(
    tool_name: &str,
    args: &serde_json::Value,
) -> Option<Vec<Line<'static>>> {
    use serde_json::Value;
    let text = |s: String| Span::styled(s, Style::default().fg(crate::colors::text()));

    // Helper: format coordinate pair as integers (pixels)
    let fmt_xy = |x: f64, y: f64| -> String {
        let xi = x.round() as i64;
        let yi = y.round() as i64;
        format!("({xi}, {yi})")
    };

    match (tool_name, args) {
        ("browser_click", Value::Object(map)) => {
            // Expect optional `type`, and x/y for absolute. Only compact when both x and y provided.
            let ty = map
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("click")
                .to_lowercase();
            let (x, y) = match (
                map.get("x").and_then(|v| v.as_f64()),
                map.get("y").and_then(|v| v.as_f64()),
            ) {
                (Some(x), Some(y)) => (x, y),
                _ => return None,
            };
            let msg = format!("└ {ty} at {}", fmt_xy(x, y));
            Some(vec![Line::from(text(msg))])
        }
        ("browser_move", Value::Object(map)) => {
            // Prefer absolute x/y → "to (x, y)"; otherwise relative dx/dy → "by (dx, dy)".
            if let (Some(x), Some(y)) = (
                map.get("x").and_then(|v| v.as_f64()),
                map.get("y").and_then(|v| v.as_f64()),
            ) {
                let msg = format!("└ to {}", fmt_xy(x, y));
                return Some(vec![Line::from(text(msg))]);
            }
            if let (Some(dx), Some(dy)) = (
                map.get("dx").and_then(|v| v.as_f64()),
                map.get("dy").and_then(|v| v.as_f64()),
            ) {
                let msg = format!("└ by {}", fmt_xy(dx, dy));
                return Some(vec![Line::from(text(msg))]);
            }
            None
        }
        _ => None,
    }
}

fn new_completed_browser_tool_call(
    tool_name: String,
    args: Option<String>,
    duration: Duration,
    success: bool,
    result: String,
) -> ToolCallCell {
    let title = browser_tool_title(&tool_name);
    let duration = format_duration(duration);

    // Title styled by status with duration dimmed
    let title_line = if success {
        Line::from(vec![
            Span::styled(
                title,
                Style::default()
                    .fg(crate::colors::success())
                    .add_modifier(Modifier::BOLD),
            ),
            format!(", duration: {duration}").dim(),
        ])
    } else {
        Line::from(vec![
            Span::styled(
                title,
                Style::default()
                    .fg(crate::colors::error())
                    .add_modifier(Modifier::BOLD),
            ),
            format!(", duration: {duration}").dim(),
        ])
    };

    // Parse args JSON (if provided)
    let mut arg_lines: Vec<Line<'static>> = Vec::new();
    if let Some(args_str) = args {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&args_str) {
            if let Some(lines) = format_browser_args_humanized(&tool_name, &json) {
                arg_lines.extend(lines);
            } else {
                arg_lines.extend(format_browser_args_line(&json));
            }
        }
    }

    // Result lines (preview format)
    let mut result_lines: Vec<Line<'static>> = Vec::new();
    if !result.is_empty() {
        let preview = build_preview_lines(&result, true)
            .into_iter()
            .map(|l| l.style(Style::default().fg(crate::colors::text_dim())))
            .collect::<Vec<_>>();
        result_lines.extend(preview);
    }

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(title_line);
    lines.extend(arg_lines);
    if !result_lines.is_empty() {
        lines.push(Line::from(""));
        lines.extend(result_lines);
    }
    lines.push(Line::from(""));

    ToolCallCell::new(
        lines,
        if success {
            ToolCallStatus::Success
        } else {
            ToolCallStatus::Failed
        },
    )
}

// Map `agent_*` tool names to friendly titles
fn agent_tool_title(tool_name: &str) -> String {
    match tool_name {
        "agent_run" => "Agent Run".to_string(),
        "agent_check" => "Agent Check".to_string(),
        "agent_result" => "Agent Result".to_string(),
        "agent_cancel" => "Agent Cancel".to_string(),
        "agent_wait" => "Agent Wait".to_string(),
        "agent_list" => "Agent List".to_string(),
        other => {
            // Fallback: pretty-print unknown agent_* tools as "Agent <TitleCase>"
            if let Some(rest) = other.strip_prefix("agent_") {
                let title = rest
                    .split('_')
                    .filter(|s| !s.is_empty())
                    .map(|s| {
                        let mut chars = s.chars();
                        match chars.next() {
                            Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                            None => String::new(),
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                format!("Agent {}", title)
            } else {
                "Agent Tool".to_string()
            }
        }
    }
}

fn new_completed_agent_tool_call(
    tool_name: String,
    args: Option<String>,
    duration: Duration,
    success: bool,
    result: String,
) -> ToolCallCell {
    let title = agent_tool_title(&tool_name);
    let duration = format_duration(duration);

    // Title styled by status with duration dimmed
    let title_line = if success {
        Line::from(vec![
            Span::styled(
                title,
                Style::default()
                    .fg(crate::colors::success())
                    .add_modifier(Modifier::BOLD),
            ),
            format!(", duration: {duration}").dim(),
        ])
    } else {
        Line::from(vec![
            Span::styled(
                title,
                Style::default()
                    .fg(crate::colors::error())
                    .add_modifier(Modifier::BOLD),
            ),
            format!(", duration: {duration}").dim(),
        ])
    };

    // Parse args JSON (if provided)
    let mut arg_lines: Vec<Line<'static>> = Vec::new();
    if let Some(args_str) = args {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&args_str) {
            arg_lines.extend(format_browser_args_line(&json));
        }
    }

    // Result lines (preview format)
    let mut result_lines: Vec<Line<'static>> = Vec::new();
    if !result.is_empty() {
        let preview = build_preview_lines(&result, true)
            .into_iter()
            .map(|l| l.style(Style::default().fg(crate::colors::text_dim())))
            .collect::<Vec<_>>();
        result_lines.extend(preview);
    }

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(title_line);
    lines.extend(arg_lines);
    if !result_lines.is_empty() {
        lines.push(Line::from(""));
        lines.extend(result_lines);
    }
    lines.push(Line::from(""));

    ToolCallCell::new(
        lines,
        if success {
            ToolCallStatus::Success
        } else {
            ToolCallStatus::Failed
        },
    )
}

// Try to create an image cell if the MCP result contains an image
fn try_new_completed_mcp_tool_call_with_image_output(
    result: &Result<mcp_types::CallToolResult, String>,
) -> Option<ImageOutputCell> {
    match result {
        Ok(mcp_types::CallToolResult { content, .. }) => {
            if let Some(mcp_types::ContentBlock::ImageContent(image)) = content.first() {
                let raw_data = match base64::engine::general_purpose::STANDARD.decode(&image.data) {
                    Ok(data) => data,
                    Err(e) => {
                        error!("Failed to decode image data: {e}");
                        return None;
                    }
                };
                let reader = match ImageReader::new(Cursor::new(raw_data)).with_guessed_format() {
                    Ok(reader) => reader,
                    Err(e) => {
                        error!("Failed to guess image format: {e}");
                        return None;
                    }
                };

                let image = match reader.decode() {
                    Ok(image) => image,
                    Err(e) => {
                        error!("Image decoding failed: {e}");
                        return None;
                    }
                };

                Some(ImageOutputCell::new(image))
            } else {
                None
            }
        }
        _ => None,
    }
}

pub(crate) fn new_completed_mcp_tool_call(
    _num_cols: usize,
    invocation: McpInvocation,
    duration: Duration,
    success: bool,
    result: Result<mcp_types::CallToolResult, String>,
) -> Box<dyn HistoryCell> {
    if let Some(cell) = try_new_completed_mcp_tool_call_with_image_output(&result) {
        return Box::new(cell);
    }

    let duration = format_duration(duration);
    let status_str = if success { "Complete" } else { "Error" };
    let title_line = if success {
        Line::from(vec![
            Span::styled(status_str, Style::default().fg(crate::colors::success())),
            format!(", duration: {duration}").dim(),
        ])
    } else {
        Line::from(vec![
            Span::styled(status_str, Style::default().fg(crate::colors::error())),
            format!(", duration: {duration}").dim(),
        ])
    };

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(title_line);
    lines.push(format_mcp_invocation(invocation));

    match result {
        Ok(mcp_types::CallToolResult { content, .. }) => {
            if !content.is_empty() {
                lines.push(Line::from(""));

                for tool_call_result in content {
                    match tool_call_result {
                        mcp_types::ContentBlock::TextContent(text) => {
                            let mut preview = build_preview_lines(&text.text, true);
                            preview = preview
                                .into_iter()
                                .map(|l| l.style(Style::default().fg(crate::colors::text_dim())))
                                .collect();
                            lines.extend(preview);
                        }
                        mcp_types::ContentBlock::ImageContent(_) => {
                            lines.push(Line::from("<image content>".to_string()))
                        }
                        mcp_types::ContentBlock::AudioContent(_) => {
                            lines.push(Line::from("<audio content>".to_string()))
                        }
                        mcp_types::ContentBlock::EmbeddedResource(resource) => {
                            let uri = match resource.resource {
                                EmbeddedResourceResource::TextResourceContents(text) => text.uri,
                                EmbeddedResourceResource::BlobResourceContents(blob) => blob.uri,
                            };
                            lines.push(Line::from(format!("embedded resource: {uri}")));
                        }
                        mcp_types::ContentBlock::ResourceLink(ResourceLink { uri, .. }) => {
                            lines.push(Line::from(format!("link: {uri}")));
                        }
                    }
                }
            }

            lines.push(Line::from(""));
        }
        Err(e) => {
            lines.push(Line::from(vec![
                Span::styled(
                    "Error: ",
                    Style::default()
                        .fg(crate::colors::error())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(e, Style::default().fg(crate::colors::error())),
            ]));
            lines.push(Line::from(""));
        }
    }

    Box::new(ToolCallCell::new(
        lines,
        if success {
            ToolCallStatus::Success
        } else {
            ToolCallStatus::Failed
        },
    ))
}

pub(crate) fn new_error_event(message: String) -> PlainHistoryCell {
    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::styled(
        "error",
        Style::default()
            .fg(crate::colors::error())
            .add_modifier(Modifier::BOLD),
    ));
    let msg_norm = normalize_overwrite_sequences(&message);
    lines.extend(
        msg_norm
            .lines()
            .map(|line| ansi_escape_line(line).style(Style::default().fg(crate::colors::error()))),
    );
    // No empty line at end - trimming and spacing handled by renderer
    PlainHistoryCell::new(lines, HistoryCellType::Error)
}

pub(crate) fn new_diff_output(diff_output: String) -> DiffCell {
    // Parse the diff output into lines
    let mut lines = vec![Line::from("/diff").fg(crate::colors::keyword())];
    for line in diff_output.lines() {
        if line.starts_with('+') && !line.starts_with("+++") {
            lines.push(Line::from(line.to_string()).fg(crate::colors::success()));
        } else if line.starts_with('-') && !line.starts_with("---") {
            lines.push(Line::from(line.to_string()).fg(crate::colors::error()));
        } else if line.starts_with("@@") {
            lines.push(Line::from(line.to_string()).fg(crate::colors::info()));
        } else {
            lines.push(Line::from(line.to_string()));
        }
    }
    lines.push(Line::from(""));
    DiffCell { lines }
}

pub(crate) fn new_reasoning_output(reasoning_effort: &ReasoningEffort) -> PlainHistoryCell {
    let lines = vec![
        Line::from(""),
        Line::from("Reasoning Effort")
            .fg(crate::colors::keyword())
            .bold(),
        Line::from(format!("Value: {}", reasoning_effort)),
    ];
    PlainHistoryCell::new(lines, HistoryCellType::Notice)
}

pub(crate) fn new_model_output(model: &str, effort: ReasoningEffort) -> PlainHistoryCell {
    let lines = vec![
        Line::from(""),
        Line::from("Model Selection")
            .fg(crate::colors::keyword())
            .bold(),
        Line::from(format!("Model: {}", model)),
        Line::from(format!("Reasoning Effort: {}", effort)),
    ];
    PlainHistoryCell::new(lines, HistoryCellType::Notice)
}

// Continue with more factory functions...
// I'll add the rest in the next part to keep this manageable
pub(crate) fn new_status_output(
    config: &Config,
    total_usage: &TokenUsage,
    last_usage: &TokenUsage,
) -> PlainHistoryCell {
    let mut lines: Vec<Line<'static>> = Vec::new();

    lines.push(Line::from("/status").fg(crate::colors::keyword()));
    lines.push(Line::from(""));

    // 🔧 Configuration
    lines.push(Line::from(vec!["🔧 ".into(), "Configuration".bold()]));

    // Prepare config summary with custom prettification
    let summary_entries = create_config_summary_entries(config);
    let summary_map: HashMap<String, String> = summary_entries
        .iter()
        .map(|(key, value)| (key.to_string(), value.clone()))
        .collect();

    let lookup = |key: &str| -> String { summary_map.get(key).unwrap_or(&String::new()).clone() };
    let title_case = |s: &str| -> String {
        s.split_whitespace()
            .map(|w| {
                let mut chars = w.chars();
                match chars.next() {
                    None => String::new(),
                    Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    };

    // Format model name with proper capitalization
    let formatted_model = if config.model.to_lowercase().starts_with("gpt-") {
        format!("GPT{}", &config.model[3..])
    } else {
        config.model.clone()
    };
    lines.push(Line::from(vec![
        "  • Name: ".into(),
        formatted_model.into(),
    ]));
    let provider_disp = pretty_provider_name(&config.model_provider_id);
    lines.push(Line::from(vec![
        "  • Provider: ".into(),
        provider_disp.into(),
    ]));

    // Only show Reasoning fields if present in config summary
    let reff = lookup("reasoning effort");
    if !reff.is_empty() {
        lines.push(Line::from(vec![
            "  • Reasoning Effort: ".into(),
            title_case(&reff).into(),
        ]));
    }
    let rsum = lookup("reasoning summaries");
    if !rsum.is_empty() {
        lines.push(Line::from(vec![
            "  • Reasoning Summaries: ".into(),
            title_case(&rsum).into(),
        ]));
    }

    lines.push(Line::from(""));

    // 🔐 Authentication
    lines.push(Line::from(vec!["🔐 ".into(), "Authentication".bold()]));
    {
        use codex_login::AuthMode;
        use codex_login::CodexAuth;
        use codex_login::OPENAI_API_KEY_ENV_VAR;
        use codex_login::try_read_auth_json;

        // Determine effective auth mode the core would choose
        let auth_result = CodexAuth::from_codex_home(
            &config.codex_home,
            AuthMode::ChatGPT,
            &config.responses_originator_header,
        );

        match auth_result {
            Ok(Some(auth)) => match auth.mode {
                AuthMode::ApiKey => {
                    // Prefer suffix from auth.json; fall back to env var if needed
                    let suffix =
                        try_read_auth_json(&codex_login::get_auth_file(&config.codex_home))
                            .ok()
                            .and_then(|a| a.openai_api_key)
                            .or_else(|| std::env::var(OPENAI_API_KEY_ENV_VAR).ok())
                            .map(|k| {
                                let n = k.len().saturating_sub(4);
                                k[n..].to_string()
                            })
                            .unwrap_or_else(|| "????".to_string());
                    lines.push(Line::from(format!("  • Method: API key (…{suffix})")));
                }
                AuthMode::ChatGPT => {
                    let account_id = auth
                        .get_account_id()
                        .unwrap_or_else(|| "unknown".to_string());
                    lines.push(Line::from(format!(
                        "  • Method: ChatGPT account (account_id: {account_id})"
                    )));
                }
            },
            _ => {
                lines.push(Line::from("  • Method: unauthenticated"));
            }
        }
    }

    lines.push(Line::from(""));

    // 📊 Token Usage
    lines.push(Line::from(vec!["📊 ".into(), "Token Usage".bold()]));
    // Input: <input> [+ <cached> cached]
    let mut input_line_spans: Vec<Span<'static>> = vec![
        "  • Input: ".into(),
        format_with_separators(last_usage.non_cached_input()).into(),
    ];
    if last_usage.cached_input_tokens > 0 {
        input_line_spans.push(
            format!(
                " (+ {} cached)",
                format_with_separators(last_usage.cached_input_tokens)
            )
            .into(),
        );
    }
    lines.push(Line::from(input_line_spans));
    // Output: <output>
    lines.push(Line::from(vec![
        "  • Output: ".into(),
        format_with_separators(last_usage.output_tokens).into(),
    ]));
    // Total: <total>
    lines.push(Line::from(vec![
        "  • Total: ".into(),
        format_with_separators(last_usage.blended_total()).into(),
    ]));
    lines.push(Line::from(vec![
        "  • Session total: ".into(),
        format_with_separators(total_usage.blended_total()).into(),
    ]));

    // 📐 Model Limits
    let context_window = config.model_context_window;
    let max_output_tokens = config.model_max_output_tokens;
    let auto_compact_limit = config.model_auto_compact_token_limit;

    if context_window.is_some() || max_output_tokens.is_some() || auto_compact_limit.is_some() {
        lines.push(Line::from(""));
        lines.push(Line::from(vec!["📐 ".into(), "Model Limits".bold()]));

        if let Some(context_window) = context_window {
            let used = last_usage.tokens_in_context_window().min(context_window);
            let percent_full = if context_window > 0 {
                ((used as f64 / context_window as f64) * 100.0).min(100.0)
            } else {
                0.0
            };
            lines.push(Line::from(format!(
                "  • Context window: {} used of {} ({:.0}% full)",
                format_with_separators(used),
                format_with_separators(context_window),
                percent_full
            )));
        }

        if let Some(max_output_tokens) = max_output_tokens {
            lines.push(Line::from(format!(
                "  • Max output tokens: {}",
                format_with_separators(max_output_tokens)
            )));
        }

        match auto_compact_limit {
            Some(limit) if limit > 0 => {
                let limit_u64 = limit as u64;
                let remaining = limit_u64.saturating_sub(total_usage.total_tokens);
                lines.push(Line::from(format!(
                    "  • Auto-compact threshold: {} ({} remaining)",
                    format_with_separators(limit_u64),
                    format_with_separators(remaining)
                )));
                if total_usage.total_tokens > limit_u64 {
                    lines.push(Line::from(
                        "    • Compacting will trigger on the next turn".dim(),
                    ));
                }
            }
            _ => {
                if let Some(window) = context_window {
                    if window > 0 {
                        let used = last_usage.tokens_in_context_window();
                        let remaining = window.saturating_sub(used);
                        let percent_left = if window == 0 {
                            0.0
                        } else {
                            (remaining as f64 / window as f64) * 100.0
                        };
                        lines.push(Line::from(format!(
                            "  • Context window: {} used of {} ({:.0}% left)",
                            format_with_separators(used),
                            format_with_separators(window),
                            percent_left
                        )));
                        lines.push(Line::from(format!(
                            "  • {} tokens before overflow",
                            format_with_separators(remaining)
                        )));
                        lines.push(Line::from(
                            "  • Auto-compaction runs after overflow errors".to_string(),
                        ));
                    } else {
                        lines.push(Line::from(
                            "  • Auto-compaction runs after overflow errors".to_string(),
                        ));
                    }
                } else {
                    lines.push(Line::from(
                        "  • Auto-compaction runs after overflow errors".to_string(),
                    ));
                }
            }
        }
    }

    PlainHistoryCell::new(lines, HistoryCellType::Notice)
}

pub(crate) fn new_warning_event(message: String) -> PlainHistoryCell {
    let warn_style = Style::default().fg(crate::colors::warning());
    let mut lines: Vec<Line<'static>> = Vec::with_capacity(2);
    lines.push(Line::from("notice"));
    lines.push(Line::from(vec![Span::styled(
        format!("⚠ {message}"),
        warn_style,
    )]));
    PlainHistoryCell::new(lines, HistoryCellType::Notice)
}

pub(crate) fn new_prompts_output() -> PlainHistoryCell {
    let lines: Vec<Line<'static>> = vec![
        Line::from("/prompts").fg(crate::colors::keyword()),
        Line::from(""),
        Line::from(" 1. Explain this codebase"),
        Line::from(" 2. Summarize recent commits"),
        Line::from(" 3. Implement {feature}"),
        Line::from(" 4. Find and fix a bug in @filename"),
        Line::from(" 5. Write tests for @filename"),
        Line::from(" 6. Improve documentation in @filename"),
        Line::from(""),
    ];
    PlainHistoryCell::new(lines, HistoryCellType::Notice)
}

fn plan_progress_icon(total: usize, completed: usize) -> &'static str {
    if total == 0 || completed == 0 {
        "○"
    } else if completed >= total {
        "●"
    } else if completed.saturating_mul(3) <= total {
        "◔"
    } else if completed.saturating_mul(3) < total.saturating_mul(2) {
        "◑"
    } else {
        "◕"
    }
}

pub(crate) fn new_plan_update(update: UpdatePlanArgs) -> PlanUpdateCell {
    let UpdatePlanArgs { name, plan } = update;

    let mut lines: Vec<Line<'static>> = Vec::new();
    let total = plan.len();
    let completed = plan
        .iter()
        .filter(|p| matches!(p.status, StepStatus::Completed))
        .count();
    let icon = plan_progress_icon(total, completed);
    let is_complete = total > 0 && completed >= total;
    let header_color = if is_complete {
        crate::colors::success()
    } else {
        crate::colors::info()
    };

    let width: usize = 10;
    let filled = if total > 0 {
        (completed * width + total / 2) / total
    } else {
        0
    };
    let empty = width.saturating_sub(filled);

    // Build header without leading icon; icon will render in the gutter
    let mut header: Vec<Span> = Vec::new();
    let title = name
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("Plan");
    header.push(Span::styled(
        title.to_string(),
        Style::default()
            .fg(header_color)
            .add_modifier(Modifier::BOLD),
    ));
    header.push(Span::raw(" ["));
    if filled > 0 {
        header.push(Span::styled(
            "█".repeat(filled),
            Style::default().fg(crate::colors::success()),
        ));
    }
    if empty > 0 {
        header.push(Span::styled(
            "░".repeat(empty),
            Style::default().add_modifier(Modifier::DIM),
        ));
    }
    header.push(Span::raw("] "));
    header.push(Span::raw(format!("{completed}/{total}")));
    lines.push(Line::from(header));

    // Steps styled as checkbox items
    if plan.is_empty() {
        lines.push(Line::from("(no steps provided)".dim().italic()));
    } else {
        for (idx, PlanItemArg { step, status }) in plan.into_iter().enumerate() {
            let (box_span, text_span) = match status {
                StepStatus::Completed => (
                    Span::styled("✔", Style::default().fg(crate::colors::success())),
                    Span::styled(
                        step,
                        Style::default().add_modifier(Modifier::CROSSED_OUT | Modifier::DIM),
                    ),
                ),
                StepStatus::InProgress => (
                    Span::raw("□"),
                    Span::styled(step, Style::default().fg(crate::colors::info())),
                ),
                StepStatus::Pending => (
                    Span::raw("□"),
                    Span::styled(step, Style::default().add_modifier(Modifier::DIM)),
                ),
            };
            let prefix = if idx == 0 {
                Span::raw("└ ")
            } else {
                Span::raw("  ")
            };
            lines.push(Line::from(vec![
                prefix,
                box_span,
                Span::raw(" "),
                text_span,
            ]));
        }
    }

    PlanUpdateCell::new(lines, icon, is_complete)
}

pub(crate) fn new_patch_event(
    event_type: PatchEventType,
    changes: HashMap<PathBuf, FileChange>,
) -> PatchSummaryCell {
    let title = match event_type {
        PatchEventType::ApprovalRequest => "proposed patch".to_string(),
        PatchEventType::ApplyBegin { .. } => "Updated".to_string(),
    };
    let kind = match event_type {
        PatchEventType::ApprovalRequest => PatchKind::Proposed,
        PatchEventType::ApplyBegin { .. } => PatchKind::ApplyBegin,
    };
    PatchSummaryCell {
        title,
        changes,
        event_type,
        kind,
        cached: std::cell::RefCell::new(None),
    }
}

pub(crate) fn new_patch_apply_failure(stderr: String) -> PlainHistoryCell {
    let mut lines: Vec<Line<'static>> = vec![
        Line::from("❌ Patch application failed")
            .fg(crate::colors::error())
            .bold(),
        Line::from(""),
    ];

    let norm = normalize_overwrite_sequences(&stderr);
    let norm = sanitize_for_tui(
        &norm,
        SanitizeMode::AnsiPreserving,
        SanitizeOptions {
            expand_tabs: true,
            tabstop: 4,
            debug_markers: false,
        },
    );
    for line in norm.lines() {
        if !line.is_empty() {
            lines.push(ansi_escape_line(line).fg(crate::colors::error()));
        }
    }

    lines.push(Line::from(""));
    PlainHistoryCell::new(
        lines,
        HistoryCellType::Patch {
            kind: PatchKind::ApplyFailure,
        },
    )
}

// ==================== PatchSummaryCell ====================
// Renders patch summary + details with width-aware hanging indents so wrapped
// diff lines align under their code indentation.

pub(crate) struct PatchSummaryCell {
    pub(crate) title: String,
    pub(crate) changes: HashMap<PathBuf, FileChange>,
    pub(crate) event_type: PatchEventType,
    pub(crate) kind: PatchKind,
    // Cache width-specific rendered lines to avoid repeated filesystem reads
    // and pre-wrapping work inside create_diff_summary_with_width.
    cached: std::cell::RefCell<Option<PatchLayoutCache>>,
}

#[derive(Clone)]
struct PatchLayoutCache {
    width: u16,
    lines: Vec<Line<'static>>,
}

impl PatchSummaryCell {
    fn ensure_lines(&self, width: u16) -> Vec<Line<'static>> {
        if let Some(c) = self.cached.borrow().as_ref() {
            if c.width == width {
                return c.lines.clone();
            }
        }
        let lines: Vec<Line<'static>> = create_diff_summary_with_width(
            &self.title,
            &self.changes,
            self.event_type,
            Some(width as usize),
        )
        .into_iter()
        .collect();
        *self.cached.borrow_mut() = Some(PatchLayoutCache {
            width,
            lines: lines.clone(),
        });
        lines
    }
}

impl HistoryCell for PatchSummaryCell {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn kind(&self) -> HistoryCellType {
        HistoryCellType::Patch { kind: self.kind }
    }

    // We compute lines based on width at render time; provide a conservative
    // default for non-width callers (not normally used in our pipeline).
    fn display_lines(&self) -> Vec<Line<'static>> {
        self.ensure_lines(80)
    }

    fn has_custom_render(&self) -> bool {
        true
    }

    fn desired_height(&self, width: u16) -> u16 {
        let lines = self.ensure_lines(width);
        Paragraph::new(Text::from(lines))
            .wrap(Wrap { trim: false })
            .line_count(width)
            .try_into()
            .unwrap_or(0)
    }

    fn custom_render_with_skip(&self, area: Rect, buf: &mut Buffer, skip_rows: u16) {
        let text = Text::from(self.ensure_lines(area.width));
        let bg_block = Block::default().style(Style::default().bg(crate::colors::background()));
        Paragraph::new(text)
            .block(bg_block)
            .wrap(Wrap { trim: false })
            .scroll((skip_rows, 0))
            .style(Style::default().bg(crate::colors::background()))
            .render(area, buf);
    }
}

// new_patch_apply_success was removed in favor of in-place header mutation and type update in chatwidget

// ==================== Spacing Helper ====================

/// Check if a line appears to be a title/header (like "codex", "user", "thinking", etc.)
fn is_title_line(line: &Line) -> bool {
    // Check if the line has special formatting that indicates it's a title
    if line.spans.is_empty() {
        return false;
    }

    // Get the text content of the line
    let text: String = line
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>()
        .trim()
        .to_lowercase();

    // Check for common title patterns (fallback heuristic only; primary logic uses explicit cell types)
    matches!(
        text.as_str(),
        "codex"
            | "user"
            | "thinking"
            | "event"
            | "tool"
            | "/diff"
            | "/status"
            | "/prompts"
            | "reasoning effort"
            | "error"
    ) || text.starts_with("⚡")
        || text.starts_with("⚙")
        || text.starts_with("✓")
        || text.starts_with("✗")
        || text.starts_with("↯")
        || text.starts_with("proposed patch")
        || text.starts_with("applying patch")
        || text.starts_with("updating")
        || text.starts_with("updated")
}

/// Check if a line is empty (no content or just whitespace)
fn is_empty_line(line: &Line) -> bool {
    if line.spans.is_empty() {
        return true;
    }
    // Consider a line empty when all spans have only whitespace
    line.spans
        .iter()
        .all(|s| s.content.as_ref().trim().is_empty())
}

/// Trim empty lines from the beginning and end of a Vec<Line>.
/// Also normalizes internal spacing - no more than 1 empty line between content.
/// This ensures consistent spacing when cells are rendered together.
pub(crate) fn trim_empty_lines(mut lines: Vec<Line<'static>>) -> Vec<Line<'static>> {
    // Remove ALL leading empty lines
    while lines.first().map_or(false, is_empty_line) {
        lines.remove(0);
    }

    // Remove ALL trailing empty lines
    while lines.last().map_or(false, is_empty_line) {
        lines.pop();
    }

    // Normalize internal spacing - no more than 1 empty line in a row
    let mut result = Vec::new();
    let mut prev_was_empty = false;

    for line in lines {
        let is_empty = is_empty_line(&line);

        // Skip consecutive empty lines
        if is_empty && prev_was_empty {
            continue;
        }

        // Special case: If this is an empty line right after a title, skip it
        if is_empty && result.len() == 1 && result.first().map_or(false, is_title_line) {
            continue;
        }

        result.push(line);
        prev_was_empty = is_empty;
    }

    result
}

/// Retint a set of pre-rendered lines by mapping colors from the previous
/// theme palette to the new one. This pragmatically applies a theme change
/// to already materialized `Line` structures without rebuilding them from
/// semantic sources.
pub(crate) fn retint_lines_in_place(
    lines: &mut Vec<Line<'static>>,
    old: &crate::theme::Theme,
    new: &crate::theme::Theme,
) {
    use ratatui::style::Color;
    fn map_color(c: Color, old: &crate::theme::Theme, new: &crate::theme::Theme) -> Color {
        if c == old.text {
            return new.text;
        }
        if c == old.text_dim {
            return new.text_dim;
        }
        if c == old.text_bright {
            return new.text_bright;
        }
        if c == old.primary {
            return new.primary;
        }
        if c == old.success {
            return new.success;
        }
        if c == old.error {
            return new.error;
        }
        if c == old.info {
            return new.info;
        }
        if c == old.border {
            return new.border;
        }
        if c == old.foreground {
            return new.foreground;
        }
        if c == old.background {
            return new.background;
        }

        match c {
            Color::White => return new.text_bright,
            Color::Gray | Color::DarkGray => return new.text_dim,
            Color::Black => return new.text,
            Color::Red | Color::LightRed => return new.error,
            Color::Green | Color::LightGreen => return new.success,
            Color::Yellow | Color::LightYellow => return new.warning,
            Color::Blue | Color::LightBlue | Color::Cyan | Color::LightCyan => return new.info,
            Color::Magenta | Color::LightMagenta => return new.primary,
            _ => {}
        }

        c
    }

    for line in lines.iter_mut() {
        let mut st = line.style;
        if let Some(fg) = st.fg {
            st.fg = Some(map_color(fg, old, new));
        }
        if let Some(bg) = st.bg {
            st.bg = Some(map_color(bg, old, new));
        }
        if let Some(uc) = st.underline_color {
            st.underline_color = Some(map_color(uc, old, new));
        }
        line.style = st;

        let mut new_spans: Vec<Span<'static>> = Vec::with_capacity(line.spans.len());
        for s in line.spans.drain(..) {
            let mut st = s.style;
            if let Some(fg) = st.fg {
                st.fg = Some(map_color(fg, old, new));
            }
            if let Some(bg) = st.bg {
                st.bg = Some(map_color(bg, old, new));
            }
            if let Some(uc) = st.underline_color {
                st.underline_color = Some(map_color(uc, old, new));
            }
            new_spans.push(Span::styled(s.content, st));
        }
        line.spans = new_spans;
    }
}

fn format_inline_node_for_display(command_escaped: &str) -> Option<String> {
    let tokens: Vec<String> = Shlex::new(command_escaped).collect();
    if tokens.len() < 2 {
        return None;
    }

    let node_idx = tokens
        .iter()
        .position(|token| is_node_invocation_token(token))?;

    let mut idx = node_idx + 1;
    while idx < tokens.len() {
        match tokens[idx].as_str() {
            "-e" | "--eval" | "-p" | "--print" => {
                let script_idx = idx + 1;
                if script_idx >= tokens.len() {
                    return None;
                }
                return format_node_script(&tokens, script_idx, tokens[script_idx].as_str());
            }
            "--" => break,
            _ => idx += 1,
        }
    }

    None
}

fn format_inline_shell_for_display(command_escaped: &str) -> Option<String> {
    let tokens: Vec<String> = Shlex::new(command_escaped).collect();
    if tokens.len() < 3 {
        return None;
    }

    let shell_idx = tokens.iter().position(|t| is_shell_invocation_token(t))?;

    let flag_idx = shell_idx + 1;
    if flag_idx >= tokens.len() {
        return None;
    }

    let flag = tokens[flag_idx].as_str();
    if flag != "-c" && flag != "-lc" {
        return None;
    }

    let script_idx = flag_idx + 1;
    if script_idx >= tokens.len() {
        return None;
    }

    format_shell_script(&tokens, script_idx, tokens[script_idx].as_str())
}
