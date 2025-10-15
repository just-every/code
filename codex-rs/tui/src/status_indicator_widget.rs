//! A live status indicator that shows the *latest* log line emitted by the
//! application while the agent is processing a long‑running task.

use std::cell::Cell;
use std::time::Duration;
use std::time::Instant;

use codex_core::protocol::Op;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::widgets::WidgetRef;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use textwrap::Options as TwOptions;
use textwrap::WordSplitter;

#[allow(dead_code)]
pub(crate) struct StatusIndicatorWidget {
    /// Animated header text (defaults to "Working").
    header: String,
    /// Queued user messages to display under the status line.
    queued_messages: Vec<String>,

    start_time: Instant,
    /// Last time we scheduled a follow-up frame; used to throttle redraws.
    last_schedule: Cell<Instant>,
    last_rendered_second: Cell<u64>,
    app_event_tx: AppEventSender,
    // We schedule frames via AppEventSender; no direct frame requester.
}

#[allow(dead_code)]
impl StatusIndicatorWidget {
    pub(crate) fn new(app_event_tx: AppEventSender) -> Self {
        Self {
            header: String::from("Working"),
            queued_messages: Vec::new(),
            start_time: Instant::now(),
            last_schedule: Cell::new(Instant::now()),
            last_rendered_second: Cell::new(u64::MAX),

            app_event_tx,
        }
    }

    pub fn desired_height(&self, width: u16) -> u16 {
        // Status line + optional blank line + wrapped queued messages (up to 3 lines per message)
        // + optional ellipsis line per truncated message + 1 spacer line
        let inner_width = width.max(1) as usize;
        let mut total: u16 = 1; // status line
        if !self.queued_messages.is_empty() {
            total = total.saturating_add(1); // blank line between status and queued messages
        }
        let text_width = inner_width.saturating_sub(3); // account for " ↳ " prefix
        if text_width > 0 {
            let opts = TwOptions::new(text_width)
                .break_words(false)
                .word_splitter(WordSplitter::NoHyphenation);
            for q in &self.queued_messages {
                let wrapped = textwrap::wrap(q, &opts);
                let lines = wrapped.len().min(3) as u16;
                total = total.saturating_add(lines);
                if wrapped.len() > 3 {
                    total = total.saturating_add(1); // ellipsis line
                }
            }
            if !self.queued_messages.is_empty() {
                total = total.saturating_add(1); // keybind hint line
            }
        } else {
            // At least one line per message if width is extremely narrow
            total = total.saturating_add(self.queued_messages.len() as u16);
        }
        total.saturating_add(1) // spacer line
    }

    pub(crate) fn interrupt(&self) {
        self.app_event_tx.send(AppEvent::CodexOp(Op::Interrupt));
    }

    /// Update the animated header label (left of the brackets).
    pub(crate) fn update_header(&mut self, header: String) {
        if self.header != header {
            self.header = header;
        }
    }

    /// Replace the queued messages displayed beneath the header.
    pub(crate) fn set_queued_messages(&mut self, queued: Vec<String>) {
        self.queued_messages = queued;
        // Ensure a redraw so changes are visible.
        // Use the app's debounced redraw path; no need to arm a fast timer here.
        self.app_event_tx.send(AppEvent::RequestRedraw);
    }
}

impl WidgetRef for StatusIndicatorWidget {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }

        // Schedule periodic refreshes so the elapsed timer stays up to date without
        // forcing a high-FPS redraw of the entire UI.
        let now = Instant::now();
        let elapsed_since_start = self.start_time.elapsed();
        let elapsed = elapsed_since_start.as_secs();
        if elapsed != self.last_rendered_second.get()
            || now.duration_since(self.last_schedule.get()) >= Duration::from_secs(1)
        {
            self.last_schedule.set(now);
            self.last_rendered_second.set(elapsed);
            self.app_event_tx
                .send(AppEvent::ScheduleFrameIn(Duration::from_secs(1)));
        }

        // Plain rendering: no borders or padding so the live cell is visually indistinguishable from terminal scrollback.
        // Theme-aware base styles
        let bg = crate::colors::background();
        let text = crate::colors::text();
        let text_dim = crate::colors::text_dim();
        let accent = crate::colors::info();

        // Build header spans using theme colors (no terminal-default cyan/dim)
        let mut spans = vec![
            ratatui::text::Span::raw(" "),
            ratatui::text::Span::styled(
                self.header.clone(),
                Style::default()
                    .fg(accent)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            ),
            ratatui::text::Span::raw(" "),
        ];
        spans.extend(vec![
            ratatui::text::Span::raw(" "),
            // (12s • Esc to interrupt)
            ratatui::text::Span::raw(format!("({elapsed}s • "))
                .style(Style::default().fg(text_dim)),
            ratatui::text::Span::raw("Esc").style(
                Style::default()
                    .fg(accent)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            ),
            ratatui::text::Span::raw(")").style(Style::default().fg(text_dim)),
        ]);

        // Build lines: status, then queued messages, then spacer.
        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.push(Line::from(spans));
        if !self.queued_messages.is_empty() {
            lines.push(Line::from(""));
        }
        // Wrap queued messages using textwrap and show up to the first 3 lines per message.
        let text_width = area.width.saturating_sub(3); // " ↳ " prefix
        let opts = TwOptions::new(text_width as usize)
            .break_words(false)
            .word_splitter(WordSplitter::NoHyphenation);
        for q in &self.queued_messages {
            let wrapped = textwrap::wrap(q, &opts);
            for (i, piece) in wrapped.iter().take(3).enumerate() {
                let prefix = if i == 0 { " ↳ " } else { "   " };
                let content = format!("{prefix}{piece}");
                lines.push(Line::from(content).style(Style::default().fg(text_dim).italic()));
            }
            if wrapped.len() > 3 {
                lines.push(Line::from("   …").style(Style::default().fg(text_dim).italic()));
            }
        }
        if !self.queued_messages.is_empty() {
            lines.push(
                Line::from(vec![
                    ratatui::text::Span::raw("   "),
                    // Key hint in accent, label in dim text
                    ratatui::text::Span::raw("Alt+↑").style(Style::default().fg(accent)),
                    ratatui::text::Span::raw(" edit").style(Style::default().fg(text_dim)),
                ])
                .style(Style::default()),
            );
        }

        // Ensure background/foreground reflect theme
        let paragraph = Paragraph::new(lines).style(Style::default().bg(bg).fg(text));
        paragraph.render_ref(area, buf);
    }
}

#[cfg(all(test, feature = "legacy_tests"))]
mod tests {
    use super::*;
    use crate::app_event::AppEvent;
    use crate::app_event_sender::AppEventSender;
    use insta::assert_snapshot;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use tokio::sync::mpsc::unbounded_channel;

    // no extra tests added from upstream for elapsed formatting; our widget uses simple seconds
    #[test]
    fn renders_with_working_header() {
        let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let w = StatusIndicatorWidget::new(tx);

        // Render into a fixed-size test terminal and snapshot the backend.
        let mut terminal = Terminal::new(TestBackend::new(80, 2)).expect("terminal");
        terminal
            .draw(|f| w.render_ref(f.area(), f.buffer_mut()))
            .expect("draw");
        assert_snapshot!(terminal.backend());
    }

    #[test]
    fn renders_truncated() {
        let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let w = StatusIndicatorWidget::new(tx);

        // Render into a fixed-size test terminal and snapshot the backend.
        let mut terminal = Terminal::new(TestBackend::new(20, 2)).expect("terminal");
        terminal
            .draw(|f| w.render_ref(f.area(), f.buffer_mut()))
            .expect("draw");
        assert_snapshot!(terminal.backend());
    }

    #[test]
    fn renders_with_queued_messages() {
        let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let mut w = StatusIndicatorWidget::new(tx);
        w.set_queued_messages(vec!["first".to_string(), "second".to_string()]);

        // Render into a fixed-size test terminal and snapshot the backend.
        let mut terminal = Terminal::new(TestBackend::new(80, 8)).expect("terminal");
        terminal
            .draw(|f| w.render_ref(f.area(), f.buffer_mut()))
            .expect("draw");
        assert_snapshot!(terminal.backend());
    }
}
