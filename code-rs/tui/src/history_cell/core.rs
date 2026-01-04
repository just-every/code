use crate::history::compat::{ExecAction, ExecStatus, ToolStatus as HistoryToolStatus};
use crate::util::buffer::fill_rect;
use ratatui::prelude::*;
use ratatui::style::Style;
use ratatui::widgets::{Block, Paragraph, Widget, Wrap};

use super::formatting::trim_empty_lines;

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
    ApplySuccess,
    ApplyFailure,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HistoryCellType {
    Plain,
    User,
    Assistant,
    Reasoning,
    Error,
    Exec { kind: ExecKind, status: ExecStatus },
    Tool { status: ToolCellStatus },
    Patch { kind: PatchKind },
    PlanUpdate,
    BackgroundEvent,
    Notice,
    CompactionSummary,
    Diff,
    Image,
    Context,
    AnimatedWelcome,
    Loading,
}

pub(crate) fn gutter_symbol_for_kind(kind: HistoryCellType) -> Option<&'static str> {
    match kind {
        HistoryCellType::Plain => None,
        HistoryCellType::User => Some("›"),
        // Restore assistant gutter icon
        HistoryCellType::Assistant => Some("•"),
        HistoryCellType::Reasoning => None,
        HistoryCellType::Error => Some("✖"),
        HistoryCellType::Tool { status } => Some(match status {
            ToolCellStatus::Running => "⚙",
            ToolCellStatus::Success => "✔",
            ToolCellStatus::Failed => "✖",
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
        HistoryCellType::CompactionSummary => Some("📍"),
        HistoryCellType::Diff => Some("↯"),
        HistoryCellType::Image => None,
        HistoryCellType::Context => Some("◆"),
        HistoryCellType::AnimatedWelcome => None,
        HistoryCellType::Loading => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExecKind {
    Read,
    Search,
    List,
    Run,
}

impl From<ExecAction> for ExecKind {
    fn from(action: ExecAction) -> Self {
        match action {
            ExecAction::Read => ExecKind::Read,
            ExecAction::Search => ExecKind::Search,
            ExecAction::List => ExecKind::List,
            ExecAction::Run => ExecKind::Run,
        }
    }
}

impl From<ExecKind> for ExecAction {
    fn from(kind: ExecKind) -> Self {
        match kind {
            ExecKind::Read => ExecAction::Read,
            ExecKind::Search => ExecAction::Search,
            ExecKind::List => ExecAction::List,
            ExecKind::Run => ExecAction::Run,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolCellStatus {
    Running,
    Success,
    Failed,
}

impl From<HistoryToolStatus> for ToolCellStatus {
    fn from(status: HistoryToolStatus) -> Self {
        match status {
            HistoryToolStatus::Running => ToolCellStatus::Running,
            HistoryToolStatus::Success => ToolCellStatus::Success,
            HistoryToolStatus::Failed => ToolCellStatus::Failed,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PatchKind {
    Proposed,
    ApplyBegin,
    ApplySuccess,
    ApplyFailure,
}

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
        gutter_symbol_for_kind(self.kind())
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
