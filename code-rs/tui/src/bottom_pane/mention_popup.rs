use ratatui::buffer::Buffer;
use ratatui::layout::Margin;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::WidgetRef;

use code_common::fuzzy_match::fuzzy_match;

use super::popup_consts::MAX_POPUP_ROWS;
use super::scroll_state::ScrollState;
use super::selection_popup_common::GenericDisplayRow;
use super::selection_popup_common::render_rows;

pub(crate) struct MentionPopup {
    query: String,
    candidates: Vec<String>,
    state: ScrollState,
}

impl MentionPopup {
    pub(crate) fn new(candidates: Vec<String>) -> Self {
        let mut s = Self {
            query: String::new(),
            candidates,
            state: ScrollState::new(),
        };
        s.state.clamp_selection(s.filtered_items().len());
        s
    }

    pub(crate) fn set_candidates(&mut self, mut candidates: Vec<String>) {
        candidates.sort();
        candidates.dedup();
        self.candidates = candidates;
        let len = self.filtered_items().len();
        self.state.clamp_selection(len);
        self.state.ensure_visible(len, MAX_POPUP_ROWS.min(len));
    }

    pub(crate) fn on_query_change(&mut self, query: String) {
        let query = query.trim().to_string();
        if self.query == query {
            return;
        }
        self.query = query;
        let len = self.filtered_items().len();
        self.state.clamp_selection(len);
        self.state.ensure_visible(len, MAX_POPUP_ROWS.min(len));
    }

    pub(crate) fn match_count(&self) -> usize {
        self.filtered_items().len()
    }

    pub(crate) fn move_up(&mut self) {
        let len = self.filtered_items().len();
        self.state.move_up_wrap(len);
        self.state.ensure_visible(len, MAX_POPUP_ROWS.min(len));
    }

    pub(crate) fn move_down(&mut self) {
        let len = self.filtered_items().len();
        self.state.move_down_wrap(len);
        self.state.ensure_visible(len, MAX_POPUP_ROWS.min(len));
    }

    pub(crate) fn selected_mention(&self) -> Option<String> {
        let matches = self.filtered_items();
        self.state
            .selected_idx
            .and_then(|idx| matches.get(idx).cloned())
    }

    pub(crate) fn calculate_required_height(&self) -> u16 {
        self.filtered_items().len().clamp(1, MAX_POPUP_ROWS) as u16
    }

    fn filtered(&self) -> Vec<(String, Option<Vec<usize>>, i32)> {
        let filter = self.query.trim();
        if filter.is_empty() {
            return self
                .candidates
                .iter()
                .cloned()
                .map(|name| (name, None, 0))
                .collect();
        }

        let mut out: Vec<(String, Option<Vec<usize>>, i32)> = Vec::new();
        for candidate in &self.candidates {
            if let Some((indices, score)) = fuzzy_match(candidate, filter) {
                out.push((candidate.clone(), Some(indices), score));
            }
        }
        out.sort_by(|a, b| a.2.cmp(&b.2).then_with(|| a.0.cmp(&b.0)));
        out
    }

    fn filtered_items(&self) -> Vec<String> {
        self.filtered().into_iter().map(|(s, _, _)| s).collect()
    }
}

impl WidgetRef for &MentionPopup {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let indented_area = area.inner(Margin::new(2, 0));

        let matches = self.filtered();
        if matches.is_empty() {
            let msg = if self.candidates.is_empty() {
                "no weave agents"
            } else {
                "no matches"
            };
            let x = indented_area.x;
            let y = indented_area.y;
            let w = indented_area.width;
            let start = x.saturating_add(w.saturating_sub(msg.len() as u16) / 2);
            for xi in x..x + w {
                buf[(xi, y)].set_char(' ');
            }
            buf.set_string(start, y, msg, Style::default().fg(crate::colors::text_dim()));
            return;
        }

        let rows_all: Vec<GenericDisplayRow> = matches
            .into_iter()
            .map(|(name, indices, _)| {
                let rendered = format!("#{name}");
                GenericDisplayRow {
                    name: rendered,
                    match_indices: indices.map(|v| v.into_iter().map(|i| i + 1).collect()),
                    is_current: false,
                    description: None,
                    name_color: Some(crate::colors::info()),
                }
            })
            .collect();

        render_rows(
            indented_area,
            buf,
            &rows_all,
            &self.state,
            MAX_POPUP_ROWS,
            false,
        );
    }
}
