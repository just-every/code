use super::semantic::{SemanticLine, lines_from_ratatui, lines_to_ratatui};
use super::*;
use crate::theme::current_theme;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PlanUpdateState {
    pub lines: Vec<SemanticLine>,
    pub icon: &'static str,
    pub is_complete: bool,
}

impl PlanUpdateState {
    pub(crate) fn new(lines: Vec<Line<'static>>, icon: &'static str, is_complete: bool) -> Self {
        Self {
            lines: lines_from_ratatui(lines),
            icon,
            is_complete,
        }
    }
}

pub(crate) struct PlanUpdateCell {
    state: PlanUpdateState,
}

impl PlanUpdateCell {
    pub(crate) fn new(lines: Vec<Line<'static>>, icon: &'static str, is_complete: bool) -> Self {
        Self {
            state: PlanUpdateState::new(lines, icon, is_complete),
        }
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.state.is_complete
    }
}

impl HistoryCell for PlanUpdateCell {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn kind(&self) -> HistoryCellType {
        HistoryCellType::PlanUpdate
    }

    fn display_lines(&self) -> Vec<Line<'static>> {
        let theme = current_theme();
        lines_to_ratatui(&self.state.lines, &theme)
    }

    fn gutter_symbol(&self) -> Option<&'static str> {
        Some(self.state.icon)
    }
}
