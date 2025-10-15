use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;

use super::BottomPane;
use super::bottom_pane_view::BottomPaneView;

pub(crate) struct UndoRestoreView {
    snapshot_index: usize,
    short_id: String,
    title_line: String,
    summary: Option<String>,
    timestamp_line: String,
    user_delta: usize,
    assistant_delta: usize,
    restore_files: bool,
    restore_conversation: bool,
    conversation_available: bool,
    selection: usize,
    is_complete: bool,
    app_event_tx: AppEventSender,
}

impl UndoRestoreView {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        snapshot_index: usize,
        short_id: String,
        title_line: String,
        summary: Option<String>,
        timestamp_line: String,
        user_delta: usize,
        assistant_delta: usize,
        restore_conversation_default: bool,
        conversation_available: bool,
        app_event_tx: AppEventSender,
    ) -> Self {
        Self {
            snapshot_index,
            short_id,
            title_line,
            summary,
            timestamp_line,
            user_delta,
            assistant_delta,
            restore_files: true,
            restore_conversation: restore_conversation_default && conversation_available,
            conversation_available,
            selection: 0,
            is_complete: false,
            app_event_tx,
        }
    }

    fn toggle_files(&mut self) {
        self.restore_files = !self.restore_files;
    }

    fn toggle_conversation(&mut self) {
        if self.conversation_available {
            self.restore_conversation = !self.restore_conversation;
        }
    }

    fn send_restore_request(&mut self) {
        self.app_event_tx.send(AppEvent::PerformUndoRestore {
            index: self.snapshot_index,
            restore_files: self.restore_files,
            restore_conversation: self.restore_conversation,
        });
        self.is_complete = true;
    }

    fn checkbox(label: &str, enabled: bool, disabled: bool) -> Line<'static> {
        let box_text = if enabled { "[x]" } else { "[ ]" };
        let mut spans = Vec::new();
        let mut style = Style::default().fg(crate::colors::text());
        if disabled {
            style = Style::default().fg(crate::colors::text_dim());
        }
        spans.push(Span::styled(box_text, style.add_modifier(Modifier::BOLD)));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(label.to_string(), style));
        Line::from(spans)
    }

    fn footer_line(&self) -> Line<'static> {
        Line::from(vec![
            Span::styled("↑↓", Style::default().fg(crate::colors::light_blue())),
            Span::raw(" Navigate  "),
            Span::styled("Space", Style::default().fg(crate::colors::success())),
            Span::raw(" Toggle  "),
            Span::styled("Enter", Style::default().fg(crate::colors::success())),
            Span::raw(" Confirm  "),
            Span::styled("Esc", Style::default().fg(crate::colors::error())),
            Span::raw(" Cancel"),
        ])
    }

    fn highlight_if_selected(line: Line<'static>, selected: bool) -> Line<'static> {
        if !selected {
            return line;
        }
        let mut line = line;
        let patch = Style::default().bg(crate::colors::selection());
        for span in &mut line.spans {
            span.style = span.style.patch(patch);
        }
        line
    }

    fn conversation_detail_line(&self) -> Option<Line<'static>> {
        if !self.conversation_available {
            return None;
        }
        let mut parts = Vec::new();
        if self.user_delta > 0 {
            parts.push(if self.user_delta == 1 {
                "1 user turn".to_string()
            } else {
                format!("{} user turns", self.user_delta)
            });
        }
        if self.assistant_delta > 0 {
            parts.push(if self.assistant_delta == 1 {
                "1 assistant reply".to_string()
            } else {
                format!("{} assistant replies", self.assistant_delta)
            });
        }
        if parts.is_empty() {
            return None;
        }
        Some(Line::from(vec![Span::styled(
            format!("    → {}", parts.join(", ")),
            Style::default().fg(crate::colors::text_dim()),
        )]))
    }
}

impl<'a> BottomPaneView<'a> for UndoRestoreView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        match key_event {
            KeyEvent {
                code: KeyCode::Up, ..
            } => {
                if self.selection > 0 {
                    self.selection -= 1;
                }
            }
            KeyEvent {
                code: KeyCode::Down,
                ..
            } => {
                if self.selection < 3 {
                    self.selection += 1;
                }
            }
            KeyEvent {
                code: KeyCode::Esc, ..
            } => {
                self.is_complete = true;
            }
            KeyEvent {
                code: KeyCode::Char(' '),
                ..
            } => match self.selection {
                0 => self.toggle_files(),
                1 => self.toggle_conversation(),
                _ => {}
            },
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => match self.selection {
                0 => self.toggle_files(),
                1 => self.toggle_conversation(),
                2 => self.send_restore_request(),
                3 => self.is_complete = true,
                _ => {}
            },
            KeyEvent {
                code: KeyCode::Left | KeyCode::Right,
                ..
            } => match self.selection {
                0 => self.toggle_files(),
                1 => self.toggle_conversation(),
                _ => {}
            },
            _ => {}
        }
    }

    fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn desired_height(&self, _width: u16) -> u16 {
        15
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(crate::colors::border()))
            .style(
                Style::default()
                    .bg(crate::colors::background())
                    .fg(crate::colors::text()),
            )
            .title(" Restore snapshot ")
            .title_alignment(Alignment::Center);
        let inner = block.inner(area);
        block.render(area, buf);

        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.push(Line::from(vec![Span::styled(
            self.title_line.clone(),
            Style::default().fg(crate::colors::text_dim()),
        )]));
        lines.push(Line::from(vec![Span::styled(
            format!("Commit {}", self.short_id),
            Style::default()
                .fg(crate::colors::text())
                .add_modifier(Modifier::BOLD),
        )]));
        lines.push(Line::from(vec![Span::styled(
            self.timestamp_line.clone(),
            Style::default().fg(crate::colors::text_dim()),
        )]));
        if let Some(summary) = &self.summary {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![Span::styled(
                summary.clone(),
                Style::default().fg(crate::colors::text()),
            )]));
        }

        lines.push(Line::from(""));

        let files_line = Self::highlight_if_selected(
            Self::checkbox("Restore workspace files", self.restore_files, false),
            self.selection == 0,
        );
        lines.push(files_line);

        let conversation_disabled = !self.conversation_available;
        let conversation_line = Self::checkbox(
            if conversation_disabled {
                "Restore conversation (no newer turns)"
            } else {
                "Restore conversation"
            },
            self.restore_conversation,
            conversation_disabled,
        );
        let conversation_line = Self::highlight_if_selected(conversation_line, self.selection == 1);
        lines.push(conversation_line);
        if let Some(detail) = self.conversation_detail_line() {
            lines.push(detail);
        }

        lines.push(Line::from(""));

        let confirm_style = if self.selection == 2 {
            Style::default()
                .bg(crate::colors::selection())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        lines.push(Line::from(vec![Span::styled(
            "Confirm restore",
            confirm_style,
        )]));

        let cancel_style = if self.selection == 3 {
            Style::default()
                .bg(crate::colors::selection())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        lines.push(Line::from(vec![Span::styled("Cancel", cancel_style)]));

        lines.push(Line::from(""));
        lines.push(self.footer_line());

        let paragraph = Paragraph::new(lines).alignment(Alignment::Left).style(
            Style::default()
                .bg(crate::colors::background())
                .fg(crate::colors::text()),
        );
        paragraph.render(
            Rect {
                x: inner.x.saturating_add(1),
                y: inner.y,
                width: inner.width.saturating_sub(2),
                height: inner.height,
            },
            buf,
        );
    }
}
