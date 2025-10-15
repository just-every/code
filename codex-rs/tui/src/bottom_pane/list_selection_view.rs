#![allow(dead_code)]
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyModifiers;
use ratatui::buffer::Buffer;
use ratatui::layout::Alignment;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Widget;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app_event_sender::AppEventSender;

use super::BottomPane;
use super::CancellationEvent;
use super::bottom_pane_view::BottomPaneView;
use super::scroll_state::ScrollState;
use super::selection_popup_common::GenericDisplayRow;
use super::selection_popup_common::render_rows;

/// One selectable item in the generic selection list.
pub(crate) type SelectionAction = Box<dyn Fn(&AppEventSender) + Send + Sync>;

pub(crate) struct SelectionItem {
    pub name: String,
    pub description: Option<String>,
    pub is_current: bool,
    pub actions: Vec<SelectionAction>,
}

pub(crate) struct ListSelectionView {
    title: String,
    subtitle: Option<String>,
    footer_hint: Option<String>,
    items: Vec<SelectionItem>,
    state: ScrollState,
    complete: bool,
    app_event_tx: AppEventSender,
    max_rows: usize,
}

impl ListSelectionView {
    fn dim_prefix_span() -> Span<'static> {
        Span::styled("▌ ", Style::default().add_modifier(Modifier::DIM))
    }

    fn render_dim_prefix_line(area: Rect, buf: &mut Buffer) {
        // Render a simple blank spacer line (no glyphs like '▌')
        let para = Paragraph::new(Line::from(""));
        para.render(area, buf);
    }

    fn wrapped_lines_for(text: &str, width: u16) -> u16 {
        if text.is_empty() || width == 0 {
            return 0;
        }
        let w = width as usize;
        let mut lines: u16 = 0;
        for part in text.split('\n') {
            let len = part.chars().count();
            if len == 0 {
                lines = lines.saturating_add(1);
                continue;
            }
            let mut l = (len / w) as u16;
            if len % w != 0 {
                l = l.saturating_add(1);
            }
            if l == 0 {
                l = 1;
            }
            lines = lines.saturating_add(l);
        }
        lines
    }

    // Compute a consistent layout for both height calculation and rendering.
    // Returns (content_width, subtitle_rows, spacer_top_rows, bottom_spacer_rows, footer_rows, rows_visible, total_height)
    fn compute_layout(&self, total_width: u16) -> (u16, u16, u16, u16, u16, u16, u16) {
        // Borders consume 2 cols; we also left-pad content by 1 col inside inner.
        let inner_width = total_width.saturating_sub(2);
        let content_width = inner_width.saturating_sub(1);

        // How many list rows we want to show
        let target_rows = (self.items.len()).clamp(1, self.max_rows) as u16;

        // Subtitle wraps on content width
        let subtitle_rows = self
            .subtitle
            .as_ref()
            .map(|s| Self::wrapped_lines_for(s, content_width))
            .unwrap_or(0);

        // Always include one spacer row between subtitle/title and the list
        let spacer_top_rows: u16 = 1;

        // Footer: single line of hints when present
        let footer_rows: u16 = if self.footer_hint.is_some() { 1 } else { 0 };

        // A visual spacer between the last list row and the footer
        let bottom_spacer_rows: u16 = if footer_rows > 0 { 1 } else { 0 };

        // Content rows budget equals the rows we want to show
        let rows_visible = target_rows;

        // Total height = borders (2) + subtitle + spacer + rows + bottom spacer + footer
        let total_height =
            2 + subtitle_rows + spacer_top_rows + rows_visible + bottom_spacer_rows + footer_rows;

        (
            content_width,
            subtitle_rows,
            spacer_top_rows,
            bottom_spacer_rows,
            footer_rows,
            rows_visible,
            total_height,
        )
    }
    pub fn new(
        title: String,
        subtitle: Option<String>,
        footer_hint: Option<String>,
        items: Vec<SelectionItem>,
        app_event_tx: AppEventSender,
        max_rows: usize,
    ) -> Self {
        let mut s = Self {
            title,
            subtitle,
            footer_hint,
            items,
            state: ScrollState::new(),
            complete: false,
            app_event_tx,
            max_rows,
        };
        let len = s.items.len();
        if let Some(idx) = s.items.iter().position(|it| it.is_current) {
            s.state.selected_idx = Some(idx);
        }
        s.state.clamp_selection(len);
        s.state.ensure_visible(len, s.max_rows.min(len));
        s
    }

    fn move_up(&mut self) {
        let len = self.items.len();
        self.state.move_up_wrap(len);
        self.state.ensure_visible(len, self.max_rows.min(len));
    }

    fn move_down(&mut self) {
        let len = self.items.len();
        self.state.move_down_wrap(len);
        self.state.ensure_visible(len, self.max_rows.min(len));
    }

    fn accept(&mut self) {
        if let Some(idx) = self.state.selected_idx {
            if let Some(item) = self.items.get(idx) {
                for act in &item.actions {
                    act(&self.app_event_tx);
                }
                self.complete = true;
            }
        } else {
            self.complete = true;
        }
    }

    fn cancel(&mut self) {
        // Close the popup without performing any actions.
        self.complete = true;
    }
}

impl BottomPaneView<'_> for ListSelectionView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'_>, key_event: KeyEvent) {
        match key_event {
            KeyEvent {
                code: KeyCode::Up, ..
            } => self.move_up(),
            KeyEvent {
                code: KeyCode::Down,
                ..
            } => self.move_down(),
            KeyEvent {
                code: KeyCode::Esc, ..
            } => self.cancel(),
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => self.accept(),
            _ => {}
        }
    }

    fn is_complete(&self) -> bool {
        self.complete
    }

    fn on_ctrl_c(&mut self, _pane: &mut BottomPane<'_>) -> CancellationEvent {
        self.complete = true;
        CancellationEvent::Handled
    }

    fn desired_height(&self, width: u16) -> u16 {
        let (_cw, _sub, _sp, _bsp, _foot, _rows, total) = self.compute_layout(width);
        total
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        // Clear and draw a bordered block matching other slash popups
        Clear.render(area, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(crate::colors::border()))
            .style(
                Style::default()
                    .bg(crate::colors::background())
                    .fg(crate::colors::text()),
            )
            .title(self.title.clone())
            .title_alignment(Alignment::Center);
        let inner = block.inner(area);
        block.render(area, buf);

        // Layout inside the block: optional subtitle header, spacer, rows, footer
        let (
            content_width,
            subtitle_rows,
            spacer_top,
            bottom_spacer_rows,
            footer_rows,
            rows_visible,
            _total,
        ) = self.compute_layout(area.width);
        let mut next_y = inner.y;
        if let Some(sub) = &self.subtitle {
            let sub_h = subtitle_rows;
            if sub_h > 0 {
                let subtitle_area = Rect {
                    x: inner.x.saturating_add(1),
                    y: next_y,
                    width: content_width,
                    height: sub_h,
                };
                Paragraph::new(sub.clone())
                    .style(Style::default().fg(crate::colors::text_dim()))
                    .render(subtitle_area, buf);
                next_y = next_y.saturating_add(sub_h);
            }
        }
        if spacer_top > 0 && next_y < inner.y.saturating_add(inner.height) {
            let spacer_area = Rect {
                x: inner.x.saturating_add(1),
                y: next_y,
                width: content_width,
                height: 1,
            };
            Self::render_dim_prefix_line(spacer_area, buf);
            next_y = next_y.saturating_add(1);
        }

        // Compute rows area height from inner
        let reserved = bottom_spacer_rows.saturating_add(footer_rows); // exactly as measured
        let rows_area = Rect {
            // Left pad by one column
            x: inner.x.saturating_add(1),
            y: next_y,
            width: content_width,
            height: inner
                .height
                .saturating_sub(next_y.saturating_sub(inner.y))
                .saturating_sub(reserved),
        };

        let rows: Vec<GenericDisplayRow> = self
            .items
            .iter()
            .enumerate()
            .map(|(i, it)| {
                let is_selected = self.state.selected_idx == Some(i);
                // Use a nicer selector: '›' when selected, otherwise space
                let prefix = if is_selected { '›' } else { ' ' };
                let name_with_marker = if it.is_current {
                    format!("{} (current)", it.name)
                } else {
                    it.name.clone()
                };
                let display_name = format!("{} {}. {}", prefix, i + 1, name_with_marker);
                GenericDisplayRow {
                    name: display_name,
                    match_indices: None,
                    is_current: it.is_current,
                    description: if is_selected {
                        it.description.clone()
                    } else {
                        None
                    },
                    name_color: None,
                }
            })
            .collect();
        if rows_area.height > 0 {
            let max_rows_to_render = rows_visible.min(rows_area.height);
            render_rows(
                rows_area,
                buf,
                &rows,
                &self.state,
                max_rows_to_render as usize,
                true,
            );
        }

        if self.footer_hint.is_some() {
            // Bottom spacer above footer, if reserved
            if bottom_spacer_rows > 0 && rows_area.height > 0 {
                let spacer_y = inner.y + inner.height - footer_rows - bottom_spacer_rows;
                let spacer_area = Rect {
                    x: inner.x.saturating_add(1),
                    y: spacer_y,
                    width: content_width,
                    height: bottom_spacer_rows,
                };
                Paragraph::new(Line::from(""))
                    .style(
                        Style::default()
                            .bg(crate::colors::background())
                            .fg(crate::colors::text()),
                    )
                    .render(spacer_area, buf);
            }
            // Render footer on the last inner line
            let footer_area = Rect {
                x: inner.x.saturating_add(1),
                y: inner.y + inner.height - 1,
                width: content_width,
                height: 1,
            };
            let line = Line::from(vec![
                Span::styled("↑↓", Style::default().fg(crate::colors::function())),
                Span::styled(
                    " Navigate  ",
                    Style::default().fg(crate::colors::text_dim()),
                ),
                Span::styled("Enter", Style::default().fg(crate::colors::success())),
                Span::styled(" Select  ", Style::default().fg(crate::colors::text_dim())),
                Span::styled("Esc", Style::default().fg(crate::colors::error())),
                Span::styled(" Cancel", Style::default().fg(crate::colors::text_dim())),
            ]);
            Paragraph::new(line)
                .style(
                    Style::default()
                        .bg(crate::colors::background())
                        .fg(crate::colors::text()),
                )
                .render(footer_area, buf);
        }
    }
}
