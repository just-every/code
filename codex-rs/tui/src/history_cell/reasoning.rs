use super::semantic::{lines_from_ratatui, lines_to_ratatui, SemanticLine};
use super::semantic;
use super::*;
use std::cell::{Cell, RefCell};
use unicode_width::UnicodeWidthStr as _;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CollapsibleReasoningState {
    pub lines: Vec<SemanticLine>,
    pub in_progress: bool,
    pub hide_when_collapsed: bool,
    pub id: Option<String>,
}

impl CollapsibleReasoningState {
    pub(crate) fn new(lines: Vec<Line<'static>>, id: Option<String>) -> Self {
        Self {
            lines: lines_from_ratatui(lines),
            in_progress: false,
            hide_when_collapsed: false,
            id,
        }
    }
}

pub(crate) struct CollapsibleReasoningCell {
    state: RefCell<CollapsibleReasoningState>,
    collapsed: Cell<bool>,
}

impl CollapsibleReasoningCell {
    pub(crate) fn new_with_id(lines: Vec<Line<'static>>, id: Option<String>) -> Self {
        Self {
            state: RefCell::new(CollapsibleReasoningState::new(lines, id)),
            collapsed: Cell::new(true),
        }
    }

    pub(crate) fn matches_id(&self, candidate: &str) -> bool {
        self.state
            .borrow()
            .id
            .as_ref()
            .map(|id| id == candidate)
            .unwrap_or(false)
    }

    pub(crate) fn set_in_progress(&self, in_progress: bool) {
        self.state.borrow_mut().in_progress = in_progress;
    }

    pub(crate) fn toggle_collapsed(&self) {
        let current = self.collapsed.get();
        self.collapsed.set(!current);
    }

    pub(crate) fn set_collapsed(&self, collapsed: bool) {
        self.collapsed.set(collapsed);
    }

    pub(crate) fn is_collapsed(&self) -> bool {
        self.collapsed.get()
    }

    pub(crate) fn set_hide_when_collapsed(&self, hide: bool) {
        self.state.borrow_mut().hide_when_collapsed = hide;
    }

    pub(crate) fn append_lines_dedup(&self, new_lines: Vec<Line<'static>>) {
        if new_lines.is_empty() {
            return;
        }
        let mut state = self.state.borrow_mut();
        let mut incoming = semantic::lines_from_ratatui(new_lines);
        dedup_append_semantic(&mut state.lines, &mut incoming);
        state.lines.extend(incoming);
    }

    pub(crate) fn retint(&self, _old: &crate::theme::Theme, _new: &crate::theme::Theme) {}

    pub(crate) fn debug_title_overlay(&self) -> String {
        let state = self.state.borrow();
        let theme = crate::theme::current_theme();
        debug_title_overlay(&lines_to_ratatui(&state.lines, &theme))
    }
}

impl HistoryCell for CollapsibleReasoningCell {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn kind(&self) -> HistoryCellType {
        HistoryCellType::Reasoning
    }

    fn display_lines(&self) -> Vec<Line<'static>> {
        let state = self.state.borrow();
        if state.lines.is_empty() {
            return Vec::new();
        }

        let theme = crate::theme::current_theme();
        let stored_lines = lines_to_ratatui(&state.lines, &theme);
        let normalized = normalized_lines(&stored_lines);

        if self.collapsed.get() {
            if state.hide_when_collapsed {
                return Vec::new();
            }
            let mut titles = extract_section_titles_locked(&state, &normalized);
            if state.in_progress {
                if let Some(mut last) = titles.pop() {
                    last.spans.push(Span::styled(
                        "…",
                        Style::default().fg(crate::colors::text_dim()),
                    ));
                    return vec![last];
                }
                return vec![Line::from("…".dim())];
            }
            titles.pop().into_iter().collect()
        } else {
            let mut out = normalized;
            if state.in_progress {
                out.push(Line::from("…".dim()));
            }
            out
        }
    }

    fn gutter_symbol(&self) -> Option<&'static str> {
        None
    }

    fn has_custom_render(&self) -> bool {
        true
    }

    fn custom_render_with_skip(&self, area: Rect, buf: &mut Buffer, skip_rows: u16) {
        let state = self.state.borrow();
        if self.collapsed.get() {
            if state.hide_when_collapsed {
                return;
            }
            let bg_style = Style::default()
                .bg(crate::colors::background())
                .fg(crate::colors::text());
            fill_rect(buf, area, Some(' '), bg_style);
            let lines = self.display_lines_trimmed();
            Paragraph::new(Text::from(lines))
                .block(Block::default().style(Style::default().bg(crate::colors::background())))
                .wrap(Wrap { trim: false })
                .scroll((skip_rows, 0))
                .style(Style::default().bg(crate::colors::background()))
                .render(area, buf);
            return;
        }

        let dim = crate::colors::text_dim();
        let stored_lines = lines_to_ratatui(&state.lines, &crate::theme::current_theme());
        let mut lines = normalized_lines(&stored_lines)
            .into_iter()
            .map(|mut line| {
                line.spans = line
                    .spans
                    .into_iter()
                    .map(|s| s.clone().style(Style::default().fg(dim)))
                    .collect();
                line.style = line.style.patch(Style::default().fg(dim));
                line
            })
            .collect::<Vec<_>>();
        if state.in_progress {
            lines.push(Line::from("…".dim()));
        }

        let text = Text::from(trim_empty_lines(lines));
        let bg = crate::colors::background();
        let bg_style = Style::default().bg(bg).fg(dim);
        fill_rect(buf, area, Some(' '), bg_style);

        let block = Block::default()
            .borders(Borders::LEFT)
            .border_style(Style::default().fg(crate::colors::border_dim()).bg(bg))
            .style(Style::default().bg(bg))
            .padding(Padding {
                left: 1,
                right: 0,
                top: 0,
                bottom: 0,
            });

        Paragraph::new(text)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((skip_rows, 0))
            .style(Style::default().bg(bg).fg(dim))
            .render(area, buf);
    }
}

fn dedup_append_semantic(
    existing: &mut Vec<semantic::SemanticLine>,
    incoming: &mut Vec<semantic::SemanticLine>,
) {
    if incoming.is_empty() {
        return;
    }

    let to_plain = |line: &semantic::SemanticLine| -> String {
        line.spans
            .iter()
            .map(|span| span.text.as_str())
            .collect::<String>()
    };
    let is_marker = |line: &semantic::SemanticLine| -> bool {
        let t = to_plain(line).trim().to_string();
        t.starts_with('[') && t.ends_with(']')
    };

    let mut existing_plain: Vec<String> = existing
        .iter()
        .rev()
        .filter(|l| !is_marker(l))
        .take(64)
        .map(|l| to_plain(l))
        .collect();
    existing_plain.reverse();

    let incoming_plain: Vec<String> = incoming
        .iter()
        .filter(|l| !is_marker(l))
        .map(|l| to_plain(l))
        .collect();

    let max_overlap = existing_plain.len().min(incoming_plain.len());
    let mut overlap = 0usize;
    for k in (1..=max_overlap).rev() {
        if existing_plain[existing_plain.len() - k..] == incoming_plain[..k] {
            overlap = k;
            break;
        }
    }

    if overlap > 0 {
        let mut to_drop = overlap;
        let mut trimmed: Vec<semantic::SemanticLine> = Vec::with_capacity(incoming.len());
        for l in incoming.drain(..) {
            if to_drop > 0 && !is_marker(&l) {
                to_drop -= 1;
                continue;
            }
            trimmed.push(l);
        }
        *incoming = trimmed;
    }

    for nl in incoming.drain(..) {
        let dup = existing
            .last()
            .map(|last| to_plain(last) == to_plain(&nl))
            .unwrap_or(false);
        if !dup {
            existing.push(nl);
        }
    }
}

fn normalized_lines(lines: &[Line<'static>]) -> Vec<Line<'static>> {
    let mut out: Vec<Line<'static>> = Vec::new();
    for line in lines {
        if line.spans.len() <= 1 {
            out.push(line.clone());
            continue;
        }

        let mut idx = 0usize;
        while idx < line.spans.len() {
            let s = &line.spans[idx];
            let is_bold = s.style.add_modifier.contains(Modifier::BOLD);
            if idx == 0 && s.content.trim().is_empty() {
                idx += 1;
                continue;
            }
            if is_bold {
                idx += 1;
                continue;
            }
            break;
        }

        if idx == 0 || idx >= line.spans.len() {
            out.push(line.clone());
            continue;
        }

        let mut title_spans = Vec::new();
        let mut rest_spans = Vec::new();
        for (i, s) in line.spans.iter().enumerate() {
            if i < idx {
                title_spans.push(s.clone());
            } else {
                rest_spans.push(s.clone());
            }
        }

        out.push(Line::from(title_spans));
        let rest_is_blank = rest_spans.iter().all(|s| s.content.trim().is_empty());
        if !rest_is_blank {
            out.push(Line::from(rest_spans));
        }
    }
    out
}

fn extract_section_titles_locked(
    state: &CollapsibleReasoningState,
    normalized: &[Line<'static>],
) -> Vec<Line<'static>> {
    let mut titles: Vec<Line<'static>> = Vec::new();
    for l in normalized {
        let text: String = l.spans.iter().map(|s| s.content.as_ref()).collect();
        if text.trim().is_empty() {
            continue;
        }
        let all_bold = !l.spans.is_empty()
            && l
                .spans
                .iter()
                .all(|s| s.style.add_modifier.contains(Modifier::BOLD) || s.content.trim().is_empty());
        if all_bold {
            titles.push(l.clone());
        }
    }

    if titles.is_empty() && !state.in_progress {
        for l in normalized {
            let text: String = l.spans.iter().map(|s| s.content.as_ref()).collect();
            if text.trim().is_empty() {
                continue;
            }
            titles.push(l.clone());
            break;
        }
    }

    let color = crate::colors::text_dim();
    titles
        .into_iter()
        .map(|line| {
            let spans: Vec<Span<'static>> = line
                .spans
                .into_iter()
                .map(|s| s.style(Style::default().fg(color)))
                .collect();
            Line::from(spans)
        })
        .collect()
}

fn debug_title_overlay(lines: &[Line<'static>]) -> String {
    let mut title_idxs: Vec<usize> = Vec::new();
    let mut title_previews: Vec<String> = Vec::new();
    for (i, l) in normalized_lines(lines).iter().enumerate() {
        let is_title = !l.spans.is_empty()
            && l.spans.iter().all(|s| {
                s.style.add_modifier.contains(Modifier::BOLD) || s.content.trim().is_empty()
            });
        if is_title {
            title_idxs.push(i);
            let mut text: String = l.spans.iter().map(|s| s.content.as_ref()).collect();
            let maxw = 60usize;
            if text.width() > maxw {
                let (prefix, _suffix, _w) =
                    crate::live_wrap::take_prefix_by_width(&text, maxw.saturating_sub(1));
                text = format!("{}…", prefix);
            }
            title_previews.push(text);
        }
    }

    let total = lines.len();
    let titles = title_previews.len();
    let lastw = lines
        .last()
        .map(|l| {
            l.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>()
                .width()
        })
        .unwrap_or(0);
    format!(
        "rtitles={} idx={:?} total_lines={} lastw={} prevs={:?}",
        titles, title_idxs, total, lastw, title_previews
    )
}
