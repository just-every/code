use super::ChatWidget;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub(super) fn handle_limits_key(chat: &mut ChatWidget<'_>, key_event: KeyEvent) -> bool {
    if chat.limits.overlay.is_none() {
        return false;
    }

    match key_event.code {
        KeyCode::Esc => {
            chat.limits.overlay = None;
            chat.request_redraw();
            true
        }
        KeyCode::Up => {
            if let Some(overlay) = chat.limits.overlay.as_ref() {
                let current = overlay.scroll();
                if current > 0 {
                    overlay.set_scroll(current.saturating_sub(1));
                    chat.request_redraw();
                }
            }
            true
        }
        KeyCode::Down => {
            if let Some(overlay) = chat.limits.overlay.as_ref() {
                let current = overlay.scroll();
                let max = overlay.max_scroll();
                let next = current.saturating_add(1).min(max);
                if next != current {
                    overlay.set_scroll(next);
                    chat.request_redraw();
                }
            }
            true
        }
        KeyCode::PageUp => {
            if let Some(overlay) = chat.limits.overlay.as_ref() {
                let step = overlay.visible_rows().max(1);
                let current = overlay.scroll();
                let next = current.saturating_sub(step);
                overlay.set_scroll(next);
                chat.request_redraw();
            }
            true
        }
        KeyCode::PageDown | KeyCode::Char(' ') => {
            if let Some(overlay) = chat.limits.overlay.as_ref() {
                let step = overlay.visible_rows().max(1);
                let current = overlay.scroll();
                let max = overlay.max_scroll();
                let next = current.saturating_add(step).min(max);
                overlay.set_scroll(next);
                chat.request_redraw();
            }
            true
        }
        KeyCode::Left | KeyCode::Char('[') => {
            if let Some(overlay) = chat.limits.overlay.as_ref() {
                if overlay.select_prev_tab() {
                    chat.request_redraw();
                }
            }
            true
        }
        KeyCode::Right | KeyCode::Char(']') => {
            if let Some(overlay) = chat.limits.overlay.as_ref() {
                if overlay.select_next_tab() {
                    chat.request_redraw();
                }
            }
            true
        }
        KeyCode::Tab => {
            if key_event.modifiers.contains(KeyModifiers::SHIFT) {
                if let Some(overlay) = chat.limits.overlay.as_ref() {
                    if overlay.select_prev_tab() {
                        chat.request_redraw();
                    }
                }
            } else if let Some(overlay) = chat.limits.overlay.as_ref() {
                if overlay.select_next_tab() {
                    chat.request_redraw();
                }
            }
            true
        }
        KeyCode::BackTab => {
            if let Some(overlay) = chat.limits.overlay.as_ref() {
                if overlay.select_prev_tab() {
                    chat.request_redraw();
                }
            }
            true
        }
        KeyCode::Home => {
            if let Some(overlay) = chat.limits.overlay.as_ref() {
                overlay.set_scroll(0);
                chat.request_redraw();
            }
            true
        }
        KeyCode::End => {
            if let Some(overlay) = chat.limits.overlay.as_ref() {
                overlay.set_scroll(overlay.max_scroll());
                chat.request_redraw();
            }
            true
        }
        _ => false,
    }
}
