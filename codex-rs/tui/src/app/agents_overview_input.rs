//! Focus and input routing for the agent dashboard. Refreshes retain the shared
//! composer; dashboard actions never consume keys while its editor owns focus.

use super::*;
use crate::bottom_pane::InputResult;

impl AgentsOverviewView {
    pub(super) fn handle_composer_key(&mut self, key: KeyEvent) {
        let mut state = self.state();
        let offline = state.connection_notice.is_some();
        let status_grouping = state.status_grouping;
        if key.code == KeyCode::Esc && !state.composer_owns_escape() {
            state.focus = AgentsOverviewFocus::List;
            return;
        }
        let Some(composer) = state.composer.as_mut() else {
            return;
        };
        if offline
            && !composer.popup_active()
            && (self.composer_keymap.submit.is_pressed(key)
                || self.composer_keymap.queue.is_pressed(key))
        {
            if key.code == KeyCode::Enter {
                composer.handle_paste_enter(std::time::Instant::now());
            }
            return;
        }
        let (result, _) = composer.handle_key_event(key);
        drop(state);
        if let InputResult::Submitted { text, .. } = result {
            self.app_event_tx
                .send(AppEvent::DispatchAgentsOverviewTask {
                    prompt: text,
                    cwd: (!status_grouping)
                        .then(|| self.selected_row().map(|row| row.thread.cwd.clone()))
                        .flatten(),
                });
        }
    }
    pub(super) fn layout_areas(&self, area: Rect) -> [Rect; 7] {
        let header_height = self
            .state()
            .server_version_notice
            .as_deref()
            .map(|notice| {
                textwrap::wrap(notice, usize::from(area.width.saturating_sub(4).max(1))).len()
                    as u16
            })
            .unwrap_or(1)
            .min(area.height.saturating_sub(7).max(1));
        let footer_height = if self.state().composing() {
            0
        } else {
            (self.footer_lines(area.width.saturating_sub(4)).len() as u16)
                .min(area.height.saturating_sub(7))
        };
        let mut state = self.state();
        let composing = state.composing();
        let mut hints = if state.connection_notice.is_some() && composing {
            vec![("esc".to_string(), "tasks · dispatch paused".to_string())]
        } else if composing {
            self.composer_hints.clone()
        } else {
            Vec::new()
        };
        if let Some((key, _)) = hints.last_mut()
            && state.composer_owns_escape()
        {
            *key = "esc esc".to_string();
        }
        if area.width < 60 && hints.len() > 2 {
            hints.remove(/*index*/ 1);
        }
        if composing && let Some(override_hints) = &state.key_chord_hint {
            hints = override_hints.clone();
        }
        let metadata = state.editing_metadata();
        let input_height = if metadata {
            1
        } else if let Some(composer) = state.composer.as_mut() {
            composer.set_footer_hint_override(Some(hints));
            composer
                .desired_height(area.width)
                .min((area.height / 3).max(/*other*/ 5))
                .min(area.height.saturating_sub(/*rhs*/ 7))
                .max(/*other*/ 3)
        } else {
            3
        };
        Layout::vertical([
            Constraint::Length(header_height),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(u16::from(!metadata)),
            Constraint::Length(input_height),
            Constraint::Length(footer_height),
        ])
        .areas(area)
    }
}
