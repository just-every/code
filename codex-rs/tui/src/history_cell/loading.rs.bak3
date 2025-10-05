use super::*;
use crate::history::state::{HistoryId, LoadingState};

pub(crate) struct LoadingCell {
    state: LoadingState,
}

impl LoadingCell {
    pub(crate) fn new(message: String) -> Self {
        Self {
            state: LoadingState {
                id: HistoryId::ZERO,
                message,
            },
        }
    }

    #[allow(dead_code)]
    pub(crate) fn from_state(state: LoadingState) -> Self {
        Self { state }
    }

    pub(crate) fn state(&self) -> &LoadingState {
        &self.state
    }

    pub(crate) fn state_mut(&mut self) -> &mut LoadingState {
        &mut self.state
    }
}

impl HistoryCell for LoadingCell {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn kind(&self) -> HistoryCellType {
        HistoryCellType::Loading
    }

    fn display_lines(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("⟳ ", Style::default().fg(crate::colors::info())),
                Span::from(self.state.message.clone()),
            ]),
            Line::from(""),
        ]
    }

    fn desired_height(&self, _width: u16) -> u16 {
        3
    }

    fn has_custom_render(&self) -> bool {
        false
    }

    fn is_animating(&self) -> bool {
        false
    }

    fn is_loading_cell(&self) -> bool {
        true
    }
}
