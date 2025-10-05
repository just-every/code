#![allow(dead_code)]
use std::io::Result;

use crate::insert_history;
use crate::util::buffer::fill_rect;
use crate::tui;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::style::Styled;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Paragraph;
use ratatui::widgets::WidgetRef;

pub(crate) struct TranscriptApp {
    pub(crate) transcript_lines: Vec<Line<'static>>,
    pub(crate) scroll_offset: usize,
    pub(crate) is_done: bool,
}

impl TranscriptApp {
    pub(crate) fn new(transcript_lines: Vec<Line<'static>>) -> Self {
        Self {
            transcript_lines,
            scroll_offset: 0,
            is_done: false,
        }
    }

    pub(crate) fn handle_event(&mut self, tui: &mut tui::Tui, event: TuiEvent) -> Result<()> {
        match event {
            TuiEvent::Key(key_event) => self.handle_key_event(tui, key_event),
            TuiEvent::Draw => {
                tui.draw(|frame| {
                    let area = frame.area();
                    let buf = frame.buffer_mut();
                    self.render(area, buf);
                })?;
            }
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn insert_lines(&mut self, lines: Vec<Line<'static>>) {
        self.transcript_lines.extend(lines);
    }

    fn handle_key_event(&mut self, _tui: &mut tui::Tui, key_event: KeyEvent) {
        match key_event {
            KeyEvent {
                code: KeyCode::Esc,
                kind: KeyEventKind::Press | KeyEventKind::Repeat,
                ..
            } => {
                self.is_done = true;
            }
            KeyEvent {
                code: KeyCode::Char('q'),
                kind: KeyEventKind::Press,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('t'),
                modifiers: crossterm::event::KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('c'),
                modifiers: crossterm::event::KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                ..
            } => {
                self.is_done = true;
            }
            KeyEvent {
                code: KeyCode::Up,
                kind: KeyEventKind::Press | KeyEventKind::Repeat,
                ..
            } => {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
            }
            KeyEvent {
                code: KeyCode::Down,
                kind: KeyEventKind::Press | KeyEventKind::Repeat,
                ..
            } => {
                self.scroll_offset = self.scroll_offset.saturating_add(1);
            }
            KeyEvent {
                code: KeyCode::PageUp,
                kind: KeyEventKind::Press | KeyEventKind::Repeat,
                ..
            } => {
                let (w, h) = crossterm::terminal::size().unwrap_or((80, 24));
                let area = self.scroll_area(Rect::new(0, 0, w, h));
                self.scroll_offset = self.scroll_offset.saturating_sub(area.height as usize);
            }
            KeyEvent {
                code: KeyCode::PageDown,
                kind: KeyEventKind::Press | KeyEventKind::Repeat,
                ..
            } => {
                let (w, h) = crossterm::terminal::size().unwrap_or((80, 24));
                let area = self.scroll_area(Rect::new(0, 0, w, h));
                self.scroll_offset = self.scroll_offset.saturating_add(area.height as usize);
            }
            KeyEvent {
                code: KeyCode::Home,
                kind: KeyEventKind::Press | KeyEventKind::Repeat,
                ..
            } => {
                self.scroll_offset = 0;
            }
            KeyEvent {
                code: KeyCode::End,
                kind: KeyEventKind::Press | KeyEventKind::Repeat,
                ..
            } => {
                self.scroll_offset = usize::MAX;
            }
            _ => {}
        }
    }

    fn scroll_area(&self, area: Rect) -> Rect {
        let mut area = area;
        // Reserve 1 line for the header and 4 lines for the bottom status section. This matches the chat composer.
        area.y = area.y.saturating_add(1);
        area.height = area.height.saturating_sub(5);
        area
    }

    fn render(&mut self, area: Rect, buf: &mut Buffer) {
        Span::from("/ ".repeat(area.width as usize / 2))
            .dim()
            .render_ref(area, buf);
        Span::from("/ T R A N S C R I P T")
            .dim()
            .render_ref(area, buf);

        // Main content area (excludes header and bottom status section)
        let content_area = self.scroll_area(area);
        let wrapped = insert_history::word_wrap_lines(&self.transcript_lines, content_area.width);

        // Proactively clear the content area to avoid any stale glyphs when
        // scrolling reveals shorter lines than in previous frames.
        let clear_style = Style::default()
            .bg(crate::colors::background())
            .fg(crate::colors::text());
        fill_rect(buf, content_area, Some(' '), clear_style);

        // Clamp scroll offset to valid range
        self.scroll_offset = self
            .scroll_offset
            .min(wrapped.len().saturating_sub(content_area.height as usize));
        let start = self.scroll_offset;
        let end = (start + content_area.height as usize).min(wrapped.len());
        let page = &wrapped[start..end];
        Paragraph::new(page.to_vec())
            .style(clear_style)
            .render_ref(content_area, buf);

        // Fill remaining visible lines (if any) with a leading '~' in the first column.
        let visible = (end - start) as u16;
        if content_area.height > visible {
            let extra = content_area.height - visible;
            for i in 0..extra {
                let y = content_area.y.saturating_add(visible + i);
                Span::from("~")
                    .dim()
                    .render_ref(Rect::new(content_area.x, y, 1, 1), buf);
            }
        }

        // Bottom status section (4 lines): separator with % scrolled, then key hints (styled like chat composer)
        const RESERVED_BOTTOM_ROWS: u16 = 4;
        let area_bottom = area.y.saturating_add(area.height);
        if area.width == 0 || area.height == 0 || area_bottom <= area.y {
            return;
        }

        let desired_sep_y = content_area.bottom();
        let min_sep_y = area
            .y
            .max(area_bottom.saturating_sub(RESERVED_BOTTOM_ROWS));
        let max_sep_y = area_bottom.saturating_sub(1);
        if max_sep_y < area.y {
            return;
        }
        let mut sep_y = desired_sep_y;
        if sep_y < min_sep_y {
            sep_y = min_sep_y;
        }
        if sep_y > max_sep_y {
            sep_y = max_sep_y;
        }

        let sep_height = area_bottom.saturating_sub(sep_y).min(1);
        if sep_height == 0 {
            return;
        }
        let sep_rect = Rect::new(area.x, sep_y, area.width, sep_height);
        if sep_rect.width > 0 {
            // Separator line (dim)
            Span::from("─".repeat(sep_rect.width as usize))
                .dim()
                .render_ref(sep_rect, buf);

            // Scroll percentage (0-100%) aligned near the right edge
            let max_scroll = wrapped.len().saturating_sub(content_area.height as usize);
            let percent: u8 = if max_scroll == 0 {
                100
            } else {
                (((self.scroll_offset.min(max_scroll)) as f32 / max_scroll as f32) * 100.0).round()
                    as u8
            };
            let pct_text = format!(" {percent}% ");
            let pct_w = pct_text.chars().count() as u16;
            if pct_w < sep_rect.width {
                let padding = sep_rect
                    .width
                    .saturating_sub(pct_w.saturating_add(1));
                let pct_x = sep_rect.x.saturating_add(padding);
                Span::from(pct_text)
                    .dim()
                    .render_ref(Rect::new(pct_x, sep_rect.y, pct_w, 1), buf);
            }
        }

        let key_hint_style = Style::default().fg(Color::Cyan);
        let hints_start_y = sep_rect.y.saturating_add(sep_rect.height);
        let hints_avail = area_bottom.saturating_sub(hints_start_y);
        if hints_avail == 0 || area.width == 0 {
            return;
        }
        let hints_height = hints_avail.min(2);
        let hints_rect = Rect::new(area.x, hints_start_y, area.width, hints_height);

        let hints1 = vec![
            " ".into(),
            "↑".set_style(key_hint_style),
            "/".into(),
            "↓".set_style(key_hint_style),
            " scroll   ".into(),
            "PgUp".set_style(key_hint_style),
            "/".into(),
            "PgDn".set_style(key_hint_style),
            " page   ".into(),
            "Home".set_style(key_hint_style),
            "/".into(),
            "End".set_style(key_hint_style),
            " jump".into(),
        ];

        let hints2 = vec![
            " ".into(),
            "Esc".set_style(key_hint_style),
            "/".into(),
            "q".set_style(key_hint_style),
            " cancel".into(),
        ];

        let mut hint_lines = vec![Line::from(hints1).dim()];
        if hints_height >= 2 {
            hint_lines.push(Line::from(hints2).dim());
        }
        Paragraph::new(hint_lines).render_ref(hints_rect, buf);
    }
}

/// Minimal local event enum for transcript overlay rendering
pub(crate) enum TuiEvent {
    Key(KeyEvent),
    Draw,
    Other,
}
