use super::*;
use super::text::{message_lines_from_ratatui, message_lines_to_ratatui};
use crate::history::state::{
    HistoryId,
    InlineSpan,
    MessageHeader,
    MessageLine,
    MessageLineKind,
    NoticeRecord,
    PlainMessageKind,
    PlainMessageRole,
    PlainMessageState,
    TextEmphasis,
    TextTone,
};
use crate::theme::{current_theme, Theme};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Padding, Paragraph, Wrap};

struct PlainLayoutCache {
    requested_width: u16,
    effective_width: u16,
    height: u16,
    buffer: Option<Buffer>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PlainCellState {
    pub message: PlainMessageState,
    pub kind: HistoryCellType,
}

impl PlainCellState {
    fn role(&self) -> PlainMessageRole {
        self.message.role
    }

    fn header(&self) -> Option<&MessageHeader> {
        self.message.header.as_ref()
    }

    fn body(&self) -> &[MessageLine] {
        &self.message.lines
    }
}

pub(crate) struct PlainHistoryCell {
    state: PlainCellState,
    cached_layout: std::cell::RefCell<Option<PlainLayoutCache>>,
}

impl PlainHistoryCell {
    pub(crate) fn from_state(state: PlainMessageState) -> Self {
        let kind = history_cell_kind_from_plain(state.kind);
        Self {
            state: PlainCellState {
                message: state,
                kind,
            },
            cached_layout: std::cell::RefCell::new(None),
        }
    }

    pub(crate) fn from_notice_record(record: NoticeRecord) -> Self {
        let header = record
            .title
            .filter(|title| !title.trim().is_empty())
            .map(|label| MessageHeader { label, badge: None });
        let state = PlainMessageState {
            id: record.id,
            role: PlainMessageRole::System,
            kind: PlainMessageKind::Notice,
            header,
            lines: record.body,
            metadata: None,
        };
        Self::from_state(state)
    }

    pub(crate) fn state(&self) -> &PlainMessageState {
        &self.state.message
    }

    pub(crate) fn state_mut(&mut self) -> &mut PlainMessageState {
        self.invalidate_layout_cache();
        &mut self.state.message
    }

    pub(crate) fn invalidate_layout_cache(&self) {
        self.cached_layout.borrow_mut().take();
    }

    fn ensure_layout(&self, requested_width: u16, effective_width: u16) {
        let mut cache = self.cached_layout.borrow_mut();
        let needs_rebuild = cache
            .as_ref()
            .map_or(true, |cached| {
                cached.requested_width != requested_width
                    || cached.effective_width != effective_width
            });
        if needs_rebuild {
            *cache = Some(self.build_layout(requested_width, effective_width));
        }
    }

    fn build_layout(&self, requested_width: u16, effective_width: u16) -> PlainLayoutCache {
        if requested_width == 0 || effective_width == 0 {
            return PlainLayoutCache {
                requested_width,
                effective_width,
                height: 0,
                buffer: None,
            };
        }

        let cell_bg = match self.state.kind {
            HistoryCellType::Assistant => crate::colors::assistant_bg(),
            _ => crate::colors::background(),
        };
        let bg_style = Style::default().bg(cell_bg).fg(crate::colors::text());

        let trimmed_lines = self.display_lines_trimmed();
        let text = Text::from(trimmed_lines.clone());
        let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });
        let height: u16 = paragraph
            .line_count(effective_width)
            .try_into()
            .unwrap_or(0);

        if height == 0 {
            return PlainLayoutCache {
                requested_width,
                effective_width,
                height,
                buffer: None,
            };
        }

        let render_height = height.max(1);
        let render_area = Rect::new(0, 0, requested_width, render_height);
        let mut buffer = Buffer::empty(render_area);
        fill_rect(&mut buffer, render_area, Some(' '), bg_style);

        let paragraph_lines = Text::from(trimmed_lines);
        if matches!(self.state.kind, HistoryCellType::User) {
            let block = Block::default()
                .style(bg_style)
                .padding(Padding {
                    left: 0,
                    right: crate::layout_consts::USER_HISTORY_RIGHT_PAD.into(),
                    top: 0,
                    bottom: 0,
                });
            Paragraph::new(paragraph_lines)
                .block(block)
                .wrap(Wrap { trim: false })
                .style(bg_style)
                .render(render_area, &mut buffer);
        } else {
            let block = Block::default().style(Style::default().bg(cell_bg));
            Paragraph::new(paragraph_lines)
                .block(block)
                .wrap(Wrap { trim: false })
                .style(Style::default().bg(cell_bg))
                .render(render_area, &mut buffer);
        }

        PlainLayoutCache {
            requested_width,
            effective_width,
            height,
            buffer: Some(buffer),
        }
    }

    fn hide_header(&self) -> bool {
        should_hide_header(self.state.kind)
    }

    fn header_line(&self, theme: &Theme) -> Option<Line<'static>> {
        let header = self.state.header()?;
        let mut spans: Vec<Span<'static>> = Vec::new();
        let style = header_style(self.state.role(), theme);
        spans.push(Span::styled(header.label.clone(), style));
        if let Some(badge) = &header.badge {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(badge.clone(), header_badge_style(theme)));
        }
        Some(Line::from(spans))
    }
}

impl HistoryCell for PlainHistoryCell {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn kind(&self) -> HistoryCellType {
        self.state.kind
    }

    fn display_lines(&self) -> Vec<Line<'static>> {
        let theme = current_theme();
        let mut lines: Vec<Line<'static>> = Vec::new();

        if !self.hide_header() {
            if let Some(header) = self.header_line(&theme) {
                lines.push(header);
            }
        }

        lines.extend(message_lines_to_ratatui(self.state.body(), &theme));
        lines
    }

    fn has_custom_render(&self) -> bool {
        matches!(self.state.kind, HistoryCellType::User)
    }

    fn desired_height(&self, width: u16) -> u16 {
        let effective_width = if matches!(self.state.kind, HistoryCellType::User) {
            width.saturating_sub(crate::layout_consts::USER_HISTORY_RIGHT_PAD.into())
        } else {
            width
        };

        self.ensure_layout(width, effective_width);
        self.cached_layout
            .borrow()
            .as_ref()
            .map(|cache| cache.height)
            .unwrap_or(0)
    }

    fn render_with_skip(&self, area: Rect, buf: &mut Buffer, skip_rows: u16) {
        let requested_width = area.width;
        let effective_width = if matches!(self.state.kind, HistoryCellType::User) {
            requested_width
                .saturating_sub(crate::layout_consts::USER_HISTORY_RIGHT_PAD.into())
        } else {
            requested_width
        };

        let cell_bg = match self.state.kind {
            HistoryCellType::Assistant => crate::colors::assistant_bg(),
            _ => crate::colors::background(),
        };
        if matches!(self.state.kind, HistoryCellType::Assistant) {
            let bg_style = Style::default().bg(cell_bg).fg(crate::colors::text());
            fill_rect(buf, area, Some(' '), bg_style);
        }

        if requested_width == 0 || effective_width == 0 {
            return;
        }

        self.ensure_layout(requested_width, effective_width);
        let cache_ref = self.cached_layout.borrow();
        let Some(cache) = cache_ref.as_ref() else {
            return;
        };
        let Some(src_buffer) = cache.buffer.as_ref() else {
            return;
        };

        let content_height = cache.height as usize;
        if content_height == 0 || skip_rows as usize >= content_height {
            return;
        }

        let src_area = src_buffer.area();
        let copy_width = usize::from(src_area.width.min(area.width));
        let max_rows = usize::from(area.height);

        for row_offset in 0..max_rows {
            let src_y = skip_rows as usize + row_offset;
            if src_y >= content_height || src_y >= usize::from(src_area.height) {
                break;
            }
            let dest_y = area.y + row_offset as u16;
            for col_offset in 0..copy_width {
                let dest_x = area.x + col_offset as u16;
                let src_cell = &src_buffer[(col_offset as u16, src_y as u16)];
                buf[(dest_x, dest_y)] = src_cell.clone();
            }
        }
    }

    fn display_lines_trimmed(&self) -> Vec<Line<'static>> {
        trim_empty_lines(self.display_lines())
    }
}

struct PlainMessageStateBuilder;

impl PlainMessageStateBuilder {
    fn from_lines(lines: Vec<Line<'static>>, kind: HistoryCellType) -> PlainMessageState {
        let role = plain_role_from_kind(kind);
        let mut iter = lines.into_iter();
        let header_line = if should_hide_header(kind) {
            iter.next()
        } else {
            None
        };

        let header = header_line.map(|line| MessageHeader {
            label: line_plain_text(&line),
            badge: None,
        });

        let body_lines: Vec<Line<'static>> = iter.collect();
        let message_lines = message_lines_from_ratatui(body_lines);

        PlainMessageState {
            id: HistoryId::ZERO,
            role,
            kind: plain_message_kind_from_cell_kind(kind),
            header,
            lines: message_lines,
            metadata: None,
        }
    }
}

pub(crate) fn plain_message_state_from_lines(
    lines: Vec<Line<'static>>,
    kind: HistoryCellType,
) -> PlainMessageState {
    PlainMessageStateBuilder::from_lines(lines, kind)
}

pub(crate) fn plain_message_state_from_paragraphs<I, S>(
    kind: PlainMessageKind,
    role: PlainMessageRole,
    lines: I,
) -> PlainMessageState
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let message_lines = lines
        .into_iter()
        .map(|text| MessageLine {
            kind: MessageLineKind::Paragraph,
            spans: vec![InlineSpan {
                text: text.into(),
                tone: TextTone::Default,
                emphasis: TextEmphasis::default(),
                entity: None,
            }],
        })
        .collect();

    PlainMessageState {
        id: HistoryId::ZERO,
        role,
        kind,
        header: None,
        lines: message_lines,
        metadata: None,
    }
}

pub(crate) fn plain_role_for_kind(kind: PlainMessageKind) -> PlainMessageRole {
    match kind {
        PlainMessageKind::User => PlainMessageRole::User,
        PlainMessageKind::Assistant => PlainMessageRole::Assistant,
        PlainMessageKind::Tool => PlainMessageRole::Tool,
        PlainMessageKind::Error => PlainMessageRole::Error,
        PlainMessageKind::Background => PlainMessageRole::BackgroundEvent,
        PlainMessageKind::Notice | PlainMessageKind::Plain => PlainMessageRole::System,
    }
}

fn plain_role_from_kind(kind: HistoryCellType) -> PlainMessageRole {
    match kind {
        HistoryCellType::User => PlainMessageRole::User,
        HistoryCellType::Assistant => PlainMessageRole::Assistant,
        HistoryCellType::Tool { .. } => PlainMessageRole::Tool,
        HistoryCellType::Error => PlainMessageRole::Error,
        HistoryCellType::BackgroundEvent => PlainMessageRole::BackgroundEvent,
        HistoryCellType::Notice => PlainMessageRole::System,
        _ => PlainMessageRole::System,
    }
}

fn plain_message_kind_from_cell_kind(kind: HistoryCellType) -> PlainMessageKind {
    match kind {
        HistoryCellType::User => PlainMessageKind::User,
        HistoryCellType::Assistant => PlainMessageKind::Assistant,
        HistoryCellType::Tool { .. } => PlainMessageKind::Tool,
        HistoryCellType::Error => PlainMessageKind::Error,
        HistoryCellType::BackgroundEvent => PlainMessageKind::Background,
        HistoryCellType::Notice => PlainMessageKind::Notice,
        _ => PlainMessageKind::Plain,
    }
}

fn history_cell_kind_from_plain(kind: PlainMessageKind) -> HistoryCellType {
    match kind {
        PlainMessageKind::User => HistoryCellType::User,
        PlainMessageKind::Assistant => HistoryCellType::Assistant,
        PlainMessageKind::Tool => HistoryCellType::Tool {
            status: super::ToolCellStatus::Success,
        },
        PlainMessageKind::Error => HistoryCellType::Error,
        PlainMessageKind::Background => HistoryCellType::BackgroundEvent,
        PlainMessageKind::Notice => HistoryCellType::Notice,
        PlainMessageKind::Plain => HistoryCellType::Plain,
    }
}

fn should_hide_header(kind: HistoryCellType) -> bool {
    matches!(
        kind,
        HistoryCellType::User
            | HistoryCellType::Assistant
            | HistoryCellType::Tool { .. }
            | HistoryCellType::Error
            | HistoryCellType::BackgroundEvent
            | HistoryCellType::Notice
    )
}

fn header_style(role: PlainMessageRole, theme: &Theme) -> Style {
    match role {
        PlainMessageRole::User => Style::default().fg(theme.text),
        PlainMessageRole::Assistant => Style::default()
            .fg(theme.primary)
            .add_modifier(Modifier::BOLD),
        PlainMessageRole::Tool => Style::default()
            .fg(theme.info)
            .add_modifier(Modifier::BOLD),
        PlainMessageRole::Error => Style::default()
            .fg(theme.error)
            .add_modifier(Modifier::BOLD),
        PlainMessageRole::BackgroundEvent => Style::default().fg(theme.text_dim),
        PlainMessageRole::System => Style::default().fg(theme.text_dim),
    }
}

fn header_badge_style(theme: &Theme) -> Style {
    Style::default().fg(theme.text_dim).add_modifier(Modifier::ITALIC)
}

fn line_plain_text(line: &Line<'_>) -> String {
    if line.spans.is_empty() {
        String::new()
    } else {
        line.spans
            .iter()
            .map(|span| span.content.to_string())
            .collect::<Vec<String>>()
            .join("")
    }
}
