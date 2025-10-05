use std::cmp::max;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;

use super::bottom_pane_view::{BottomPaneView, ConditionalUpdate};
use super::{BottomPane, CancellationEvent};

const MAX_VISIBLE_LIST_ROWS: usize = 12;

#[derive(Clone, Debug)]
pub(crate) enum UndoTimelineEntryKind {
    Snapshot { commit: String },
    Current,
}

#[derive(Clone, Debug)]
pub(crate) struct UndoTimelineEntry {
    pub label: String,
    pub summary: Option<String>,
    pub timestamp_line: Option<String>,
    pub relative_time: Option<String>,
    pub stats_line: Option<String>,
    pub commit_line: Option<String>,
    pub conversation_lines: Vec<Line<'static>>,
    pub file_lines: Vec<Line<'static>>,
    pub conversation_available: bool,
    pub files_available: bool,
    pub kind: UndoTimelineEntryKind,
}

impl UndoTimelineEntry {
    fn list_line_count(&self) -> usize {
        let mut rows = 1;
        if self.summary.is_some() {
            rows += 1;
        }
        if self.timestamp_line.is_some() || self.relative_time.is_some() {
            rows += 1;
        }
        if self.stats_line.is_some() {
            rows += 1;
        }
        if self.commit_line.is_some() {
            rows += 1;
        }
        rows + 1
    }
}

pub(crate) struct UndoTimelineView {
    entries: Vec<UndoTimelineEntry>,
    selected: usize,
    top_row: usize,
    restore_files: bool,
    restore_conversation: bool,
    restore_files_forced_off: bool,
    restore_conversation_forced_off: bool,
    app_event_tx: AppEventSender,
    is_complete: bool,
}

impl UndoTimelineView {
    pub fn new(entries: Vec<UndoTimelineEntry>, initial_selected: usize, app_event_tx: AppEventSender) -> Self {
        let selected = initial_selected.min(entries.len().saturating_sub(1));
        let mut view = Self {
            entries,
            selected,
            top_row: 0,
            restore_files: true,
            restore_conversation: true,
            restore_files_forced_off: false,
            restore_conversation_forced_off: false,
            app_event_tx,
            is_complete: false,
        };
        view.align_toggles_to_selection();
        view.ensure_visible();
        view
    }

    fn selected_entry(&self) -> Option<&UndoTimelineEntry> {
        self.entries.get(self.selected)
    }

    fn move_up(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        if self.selected == 0 {
            self.selected = self.entries.len().saturating_sub(1);
        } else {
            self.selected -= 1;
        }
        self.align_toggles_to_selection();
        self.ensure_visible();
    }

    fn move_down(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.entries.len();
        self.align_toggles_to_selection();
        self.ensure_visible();
    }

    fn page_up(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        let mut remaining = MAX_VISIBLE_LIST_ROWS;
        while remaining > 0 && self.selected > 0 {
            self.selected -= 1;
            remaining = remaining.saturating_sub(self.entries[self.selected].list_line_count());
        }
        self.align_toggles_to_selection();
        self.ensure_visible();
    }

    fn page_down(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        let mut remaining = MAX_VISIBLE_LIST_ROWS;
        while remaining > 0 && self.selected + 1 < self.entries.len() {
            self.selected += 1;
            remaining = remaining.saturating_sub(self.entries[self.selected].list_line_count());
        }
        self.align_toggles_to_selection();
        self.ensure_visible();
    }

    fn go_home(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.selected = 0;
        self.align_toggles_to_selection();
        self.ensure_visible();
    }

    fn go_end(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.selected = self.entries.len().saturating_sub(1);
        self.align_toggles_to_selection();
        self.ensure_visible();
    }

    fn align_toggles_to_selection(&mut self) {
        let Some(entry) = self.entries.get(self.selected).cloned() else {
            return;
        };
        if entry.files_available {
            if self.restore_files_forced_off {
                self.restore_files = true;
            }
            self.restore_files_forced_off = false;
        } else {
            self.restore_files = false;
            self.restore_files_forced_off = true;
        }

        if entry.conversation_available {
            if self.restore_conversation_forced_off {
                self.restore_conversation = true;
            }
            self.restore_conversation_forced_off = false;
        } else {
            self.restore_conversation = false;
            self.restore_conversation_forced_off = true;
        }
    }

    fn ensure_visible(&mut self) {
        if self.entries.is_empty() {
            self.top_row = 0;
            return;
        }

        let mut cumulative = 0usize;
        for (idx, entry) in self.entries.iter().enumerate() {
            if idx < self.selected {
                cumulative = cumulative.saturating_add(entry.list_line_count());
            }
        }
        let selected_height = self
            .entries
            .get(self.selected)
            .map(|entry| entry.list_line_count())
            .unwrap_or(1);

        if cumulative < self.top_row {
            self.top_row = cumulative;
        } else {
            let bottom = cumulative + selected_height;
            let window_bottom = self.top_row + MAX_VISIBLE_LIST_ROWS;
            if bottom > window_bottom {
                self.top_row = bottom.saturating_sub(MAX_VISIBLE_LIST_ROWS);
            }
        }
    }

    fn toggle_files(&mut self) {
        if let Some(entry) = self.selected_entry() {
            if entry.files_available {
                self.restore_files = !self.restore_files;
                self.restore_files_forced_off = false;
            }
        }
    }

    fn toggle_conversation(&mut self) {
        if let Some(entry) = self.selected_entry() {
            if entry.conversation_available {
                self.restore_conversation = !self.restore_conversation;
                self.restore_conversation_forced_off = false;
            }
        }
    }

    fn confirm(&mut self) {
        if let Some(entry) = self.selected_entry() {
            match entry.kind {
                UndoTimelineEntryKind::Snapshot { ref commit } => {
                    self.app_event_tx.send(AppEvent::PerformUndoRestore {
                        commit: Some(commit.clone()),
                        restore_files: self.restore_files && entry.files_available,
                        restore_conversation: self.restore_conversation && entry.conversation_available,
                    });
                    self.is_complete = true;
                }
                UndoTimelineEntryKind::Current => {
                    self.is_complete = true;
                }
            }
        }
    }

    fn total_list_height(&self) -> usize {
        self.entries.iter().map(|entry| entry.list_line_count()).sum()
    }

    fn visible_range(&self) -> (usize, usize) {
        let total = self.total_list_height();
        if total <= MAX_VISIBLE_LIST_ROWS {
            return (0, self.entries.len());
        }

        let mut start_entry = 0usize;
        let mut spent = 0usize;
        while start_entry < self.entries.len() && spent + self.entries[start_entry].list_line_count() <= self.top_row {
            spent = spent.saturating_add(self.entries[start_entry].list_line_count());
            start_entry += 1;
        }

        if start_entry > self.selected {
            start_entry = self.selected;
        }

        while start_entry < self.selected {
            let span: usize = self.entries[start_entry..=self.selected]
                .iter()
                .map(|entry| entry.list_line_count())
                .sum();
            if span <= MAX_VISIBLE_LIST_ROWS {
                break;
            }
            start_entry += 1;
        }

        let mut end_entry = start_entry;
        let mut used = 0usize;
        while end_entry < self.entries.len() {
            let lines = self.entries[end_entry].list_line_count();
            if used + lines > MAX_VISIBLE_LIST_ROWS && end_entry > self.selected {
                break;
            }
            used = used.saturating_add(lines);
            end_entry += 1;
            if used >= MAX_VISIBLE_LIST_ROWS && end_entry > self.selected {
                break;
            }
        }

        if end_entry <= self.selected && end_entry < self.entries.len() {
            while end_entry <= self.selected && end_entry < self.entries.len() {
                used = used.saturating_add(self.entries[end_entry].list_line_count());
                end_entry += 1;
            }
        }

        (start_entry, end_entry)
    }

    fn render_list(&self, area: Rect, buf: &mut Buffer) {
        let (start, end) = self.visible_range();
        let mut lines: Vec<Line<'static>> = Vec::new();
        for (idx, entry) in self.entries[start..end].iter().enumerate() {
            let absolute_idx = start + idx;
            let selected = absolute_idx == self.selected;

            let marker = if selected { "›" } else { " " };
            let mut title_spans = vec![
                Span::styled(format!("{marker} "), Style::default().fg(crate::colors::primary())),
                Span::styled(
                    entry.label.clone(),
                    if selected {
                        Style::default()
                            .fg(crate::colors::text())
                            .add_modifier(Modifier::BOLD)
                            .bg(crate::colors::selection())
                    } else {
                        Style::default().fg(crate::colors::text())
                    },
                ),
            ];
            if let Some(commit) = &entry.commit_line {
                title_spans.push(Span::raw(" "));
                title_spans.push(Span::styled(
                    commit.clone(),
                    if selected {
                        Style::default()
                            .fg(crate::colors::text_dim())
                            .bg(crate::colors::selection())
                    } else {
                        Style::default().fg(crate::colors::text_dim())
                    },
                ));
            }
            lines.push(Line::from(title_spans));

            if let Some(summary) = &entry.summary {
                let style = if selected {
                    Style::default()
                        .fg(crate::colors::text_dim())
                        .bg(crate::colors::selection())
                } else {
                    Style::default().fg(crate::colors::text_dim())
                };
                lines.push(Line::from(Span::styled(format!("  {summary}"), style)));
            }

            if entry.timestamp_line.is_some() || entry.relative_time.is_some() {
                let mut parts: Vec<String> = Vec::new();
                if let Some(ts) = &entry.timestamp_line {
                    parts.push(ts.clone());
                }
                if let Some(rel) = &entry.relative_time {
                    parts.push(rel.clone());
                }
                let style = if selected {
                    Style::default()
                        .fg(crate::colors::text_dim())
                        .bg(crate::colors::selection())
                } else {
                    Style::default().fg(crate::colors::text_dim())
                };
                lines.push(Line::from(Span::styled(format!("  {}", parts.join(" • ")), style)));
            }

            if let Some(stats) = &entry.stats_line {
                let style = if selected {
                    Style::default()
                        .fg(crate::colors::text_dim())
                        .bg(crate::colors::selection())
                } else {
                    Style::default().fg(crate::colors::text_dim())
                };
                lines.push(Line::from(Span::styled(format!("  {stats}"), style)));
            }

            if selected {
                lines.push(Line::from(Span::styled(
                    String::new(),
                    Style::default().bg(crate::colors::selection()),
                )));
            } else {
                lines.push(Line::from(String::new()));
            }
        }

        let paragraph = Paragraph::new(lines)
            .alignment(Alignment::Left)
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .wrap(ratatui::widgets::Wrap { trim: true });
        paragraph.render(area, buf);
    }

    fn render_preview(&self, area: Rect, buf: &mut Buffer) {
        let Some(entry) = self.selected_entry() else {
            return;
        };

        let [conversation_area, files_area, footer_area] = Layout::vertical([
            Constraint::Percentage(55),
            Constraint::Percentage(35),
            Constraint::Length(3),
        ])
        .areas(area);

        let conversation_block = Block::default()
            .borders(Borders::ALL)
            .title(" Conversation preview ")
            .border_style(Style::default().fg(crate::colors::border()))
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()));
        let conversation_inner = conversation_block.inner(conversation_area);
        conversation_block.render(conversation_area, buf);
        let conversation = Paragraph::new(entry.conversation_lines.clone())
            .wrap(ratatui::widgets::Wrap { trim: true })
            .style(Style::default().bg(crate::colors::background()))
            .alignment(Alignment::Left);
        conversation.render(conversation_inner, buf);

        let files_block = Block::default()
            .borders(Borders::ALL)
            .title(" File changes ")
            .border_style(Style::default().fg(crate::colors::border()))
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()));
        let files_inner = files_block.inner(files_area);
        files_block.render(files_area, buf);
        let file_lines = if entry.file_lines.is_empty() {
            vec![Line::from(Span::styled(
                "No file changes captured for this snapshot.",
                Style::default().fg(crate::colors::text_dim()),
            ))]
        } else {
            entry.file_lines.clone()
        };
        let file_summary = Paragraph::new(file_lines)
            .wrap(ratatui::widgets::Wrap { trim: true })
            .style(Style::default().bg(crate::colors::background()))
            .alignment(Alignment::Left);
        file_summary.render(files_inner, buf);

        let footer_lines = self.footer_lines(entry);
        Paragraph::new(footer_lines)
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .wrap(ratatui::widgets::Wrap { trim: true })
            .render(footer_area, buf);
    }

    fn footer_lines(&self, entry: &UndoTimelineEntry) -> Vec<Line<'static>> {
        let files_status = if entry.files_available {
            if self.restore_files {
                Span::styled("[x] Files", Style::default().fg(crate::colors::success()))
            } else {
                Span::styled("[ ] Files", Style::default().fg(crate::colors::text_dim()))
            }
        } else {
            Span::styled("[ ] Files", Style::default().fg(crate::colors::text_dim()))
        };

        let convo_status = if entry.conversation_available {
            if self.restore_conversation {
                Span::styled("[x] Conversation", Style::default().fg(crate::colors::success()))
            } else {
                Span::styled("[ ] Conversation", Style::default().fg(crate::colors::text_dim()))
            }
        } else {
            Span::styled("[ ] Conversation", Style::default().fg(crate::colors::text_dim()))
        };

        vec![
            Line::from(vec![files_status, Span::raw("  "), convo_status]),
            Line::from(vec![
                Span::styled("↑↓ PgUp PgDn", Style::default().fg(crate::colors::light_blue())),
                Span::raw(" Navigate  "),
                Span::styled("Space", Style::default().fg(crate::colors::success())),
                Span::raw(" Toggle files  "),
                Span::styled("C", Style::default().fg(crate::colors::success())),
                Span::raw(" Toggle conversation  "),
                Span::styled("Enter", Style::default().fg(crate::colors::success())),
                Span::raw(" Restore  "),
                Span::styled("Esc", Style::default().fg(crate::colors::error())),
                Span::raw(" Close"),
            ]),
        ]
    }
}

impl<'a> BottomPaneView<'a> for UndoTimelineView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Up => self.move_up(),
            KeyCode::Down => self.move_down(),
            KeyCode::PageUp => self.page_up(),
            KeyCode::PageDown => self.page_down(),
            KeyCode::Home => self.go_home(),
            KeyCode::End => self.go_end(),
            KeyCode::Enter => self.confirm(),
            KeyCode::Esc => self.is_complete = true,
            KeyCode::Char(' ') => self.toggle_files(),
            KeyCode::Char('c') | KeyCode::Char('C') => self.toggle_conversation(),
            KeyCode::Char('f') | KeyCode::Char('F') => self.toggle_files(),
            KeyCode::Tab => {
                if let Some(entry) = self.selected_entry() {
                    if entry.conversation_available && !entry.files_available {
                        self.toggle_conversation();
                    } else if entry.files_available && !entry.conversation_available {
                        self.toggle_files();
                    } else {
                        if self.restore_files {
                            self.toggle_conversation();
                        } else {
                            self.toggle_files();
                        }
                    }
                }
            }
            KeyCode::Right if key_event.modifiers.contains(KeyModifiers::CONTROL) => self.toggle_conversation(),
            KeyCode::Left if key_event.modifiers.contains(KeyModifiers::CONTROL) => self.toggle_files(),
            _ => {}
        }
    }

    fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn on_ctrl_c(&mut self, _pane: &mut BottomPane<'a>) -> CancellationEvent {
        self.is_complete = true;
        CancellationEvent::Handled
    }

    fn update_status_text(&mut self, _text: String) -> ConditionalUpdate {
        ConditionalUpdate::NeedsRedraw
    }

    fn desired_height(&self, _width: u16) -> u16 {
        max(MAX_VISIBLE_LIST_ROWS as u16 + 6, 24)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }

        Clear.render(area, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Restore workspace snapshot ")
            .border_style(Style::default().fg(crate::colors::border()))
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .title_alignment(Alignment::Center);
        let inner = block.inner(area);
        block.render(area, buf);
        let [list_area, preview_area] = Layout::horizontal([
            Constraint::Percentage(38),
            Constraint::Fill(1),
        ])
        .areas(inner);

        let list_block = Block::default()
            .borders(Borders::ALL)
            .title(" Snapshots ")
            .border_style(Style::default().fg(crate::colors::border()))
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()));
        let list_inner = list_block.inner(list_area);
        list_block.render(list_area, buf);
        self.render_list(list_inner, buf);

        self.render_preview(preview_area, buf);
    }
}
