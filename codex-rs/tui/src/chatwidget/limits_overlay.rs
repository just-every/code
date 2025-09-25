use std::cell::Cell;

use ratatui::style::{Style, Stylize};
use ratatui::text::{Line as RtLine, Span};

use crate::colors;
use crate::rate_limits_view::LimitsView;

#[derive(Clone)]
pub(crate) enum LimitsOverlayContent {
    Loading,
    Placeholder,
    Error(String),
    Tabs(Vec<LimitsTab>),
}

#[derive(Clone)]
pub(crate) struct LimitsTab {
    pub title: String,
    pub header: Vec<RtLine<'static>>,
    pub body: LimitsTabBody,
    pub extra: Vec<RtLine<'static>>,
}

#[derive(Clone)]
pub(crate) enum LimitsTabBody {
    View(LimitsView),
    Lines(Vec<RtLine<'static>>),
}

pub(crate) struct LimitsOverlay {
    content: LimitsOverlayContent,
    scroll: Cell<u16>,
    max_scroll: Cell<u16>,
    visible_rows: Cell<u16>,
    selected_tab: Cell<usize>,
}

impl LimitsOverlay {
    pub(crate) fn new(content: LimitsOverlayContent) -> Self {
        Self {
            content,
            scroll: Cell::new(0),
            max_scroll: Cell::new(0),
            visible_rows: Cell::new(0),
            selected_tab: Cell::new(0),
        }
    }

    pub(crate) fn set_content(&mut self, content: LimitsOverlayContent) {
        self.content = content;
        self.scroll.set(0);
        self.max_scroll.set(0);
        self.selected_tab.set(0);
    }

    pub(crate) fn scroll(&self) -> u16 {
        self.scroll.get()
    }

    pub(crate) fn set_scroll(&self, value: u16) {
        let max = self.max_scroll.get();
        self.scroll.set(value.min(max));
    }

    pub(crate) fn max_scroll(&self) -> u16 {
        self.max_scroll.get()
    }

    pub(crate) fn set_max_scroll(&self, max: u16) {
        self.max_scroll.set(max);
        if self.scroll.get() > max {
            self.scroll.set(max);
        }
    }

    pub(crate) fn visible_rows(&self) -> u16 {
        self.visible_rows.get()
    }

    pub(crate) fn set_visible_rows(&self, rows: u16) {
        self.visible_rows.set(rows);
    }

    pub(crate) fn tab_count(&self) -> usize {
        match &self.content {
            LimitsOverlayContent::Tabs(tabs) => tabs.len(),
            _ => 0,
        }
    }

    pub(crate) fn selected_tab(&self) -> usize {
        self.selected_tab.get().min(self.tab_count().saturating_sub(1))
    }

    pub(crate) fn select_next_tab(&self) -> bool {
        let count = self.tab_count();
        if count <= 1 {
            return false;
        }
        let current = self.selected_tab();
        let next = (current + 1) % count;
        if next != current {
            self.selected_tab.set(next);
            self.scroll.set(0);
            true
        } else {
            false
        }
    }

    pub(crate) fn select_prev_tab(&self) -> bool {
        let count = self.tab_count();
        if count <= 1 {
            return false;
        }
        let current = self.selected_tab();
        let prev = if current == 0 { count - 1 } else { current - 1 };
        if prev != current {
            self.selected_tab.set(prev);
            self.scroll.set(0);
            true
        } else {
            false
        }
    }

    pub(crate) fn tabs(&self) -> Option<&[LimitsTab]> {
        match &self.content {
            LimitsOverlayContent::Tabs(tabs) => Some(tabs.as_slice()),
            _ => None,
        }
    }

    pub(crate) fn lines_for_width(&self, width: u16) -> Vec<RtLine<'static>> {
        let mut lines = match &self.content {
            LimitsOverlayContent::Loading => loading_lines(),
            LimitsOverlayContent::Placeholder => placeholder_lines(),
            LimitsOverlayContent::Error(message) => error_lines(message),
            LimitsOverlayContent::Tabs(tabs) => {
                let idx = self.selected_tab();
                match tabs.get(idx) {
                    Some(tab) => tab.lines_for_width(width),
                    None => Vec::new(),
                }
            }
        };

        strip_header(&mut lines);
        strip_status_line(&mut lines);
        lines
    }
}

impl LimitsTab {
    pub fn view(
        title: impl Into<String>,
        header: Vec<RtLine<'static>>,
        view: LimitsView,
        extra: Vec<RtLine<'static>>,
    ) -> Self {
        Self {
            title: title.into(),
            header,
            body: LimitsTabBody::View(view),
            extra,
        }
    }

    pub fn message(
        title: impl Into<String>,
        header: Vec<RtLine<'static>>,
        lines: Vec<RtLine<'static>>,
    ) -> Self {
        Self {
            title: title.into(),
            header,
            body: LimitsTabBody::Lines(lines),
            extra: Vec::new(),
        }
    }

    fn lines_for_width(&self, width: u16) -> Vec<RtLine<'static>> {
        let mut body_lines = match &self.body {
            LimitsTabBody::View(view) => view.lines_for_width(width),
            LimitsTabBody::Lines(lines) => lines.clone(),
        };
        strip_header(&mut body_lines);
        strip_status_line(&mut body_lines);

        let mut lines = Vec::new();
        if !self.header.is_empty() {
            lines.extend(self.header.clone());
            if !body_lines.is_empty() {
                lines.push(RtLine::from(String::new()));
            }
        }
        lines.extend(body_lines);
        if !self.extra.is_empty() {
            if !lines.is_empty()
                && !lines
                    .last()
                    .map(|line| line.spans.is_empty())
                    .unwrap_or(false)
            {
                lines.push(RtLine::from(String::new()));
            }
            lines.extend(self.extra.clone());
        }
        lines
    }
}

fn placeholder_lines() -> Vec<RtLine<'static>> {
    vec![
        RtLine::from("Usage Limits".bold()),
        RtLine::from("  Real usage data is not available yet."),
        RtLine::from("  Send a message to Code, then run /limits again.".dim()),
    ]
}

fn loading_lines() -> Vec<RtLine<'static>> {
    vec![
        RtLine::from(Span::styled(
            "Loading...",
            Style::default().fg(colors::text_dim()),
        )),
    ]
}

fn error_lines(message: &str) -> Vec<RtLine<'static>> {
    vec![
        RtLine::from(Span::styled(
            message.to_string(),
            Style::default().fg(colors::error()),
        )),
    ]
}

fn strip_header(lines: &mut Vec<RtLine<'static>>) {
    if let Some(first) = lines.first() {
        if line_text(first).trim() == "/limits" {
            lines.remove(0);
            while lines.first().map_or(false, |line| line_text(line).trim().is_empty()) {
                lines.remove(0);
            }
        }
    }
}

fn strip_status_line(lines: &mut Vec<RtLine<'static>>) {
    while lines
        .last()
        .map_or(false, |line| line_text(line).trim().is_empty())
    {
        lines.pop();
    }
    if let Some(last) = lines.last() {
        let text = line_text(last);
        if is_status_line(&text) {
            lines.pop();
            while lines
                .last()
                .map_or(false, |line| line_text(line).trim().is_empty())
            {
                lines.pop();
            }
        }
    }
}

fn is_status_line(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed.starts_with('✓') || trimmed.starts_with('✕')
}

fn line_text(line: &RtLine<'static>) -> String {
    line
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>()
}
