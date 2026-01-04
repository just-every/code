use crossterm::event::{KeyCode, KeyEvent, MouseEvent};

use crate::app_event::AppEvent;
use crate::onboarding::onboarding_screen::KeyboardHandler;
use code_core::protocol::Event;

use super::state::{App, AppState};

impl App<'_> {
    /// Dispatch a KeyEvent to the current view and let it decide what to do
    /// with it.
    pub(super) fn dispatch_key_event(&mut self, key_event: KeyEvent) {
        match &mut self.app_state {
            AppState::Chat { widget } => {
                widget.handle_key_event(key_event);
            }
            AppState::Onboarding { screen } => match key_event.code {
                KeyCode::Char('q') => {
                    self.app_event_tx.send(AppEvent::ExitRequest);
                }
                _ => screen.handle_key_event(key_event),
            },
        }
    }

    pub(super) fn dispatch_paste_event(&mut self, pasted: String) {
        match &mut self.app_state {
            AppState::Chat { widget } => widget.handle_paste(pasted),
            AppState::Onboarding { .. } => {}
        }
    }

    pub(super) fn dispatch_mouse_event(&mut self, mouse_event: MouseEvent) {
        match &mut self.app_state {
            AppState::Chat { widget } => {
                widget.handle_mouse_event(mouse_event);
            }
            AppState::Onboarding { .. } => {}
        }
    }

    pub(super) fn dispatch_code_event(&mut self, event: Event) {
        match &mut self.app_state {
            AppState::Chat { widget } => widget.handle_code_event(event),
            AppState::Onboarding { .. } => {}
        }
    }

    pub(super) fn normalize_non_enhanced_release_code(code: KeyCode) -> KeyCode {
        match code {
            KeyCode::Char('\r') | KeyCode::Char('\n') => KeyCode::Enter,
            KeyCode::Char('\t') => KeyCode::Tab,
            KeyCode::Char('\u{1b}') => KeyCode::Esc,
            other => other,
        }
    }
}
