use super::ChatWidget;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};

pub(super) fn handle_settings_paste(chat: &mut ChatWidget<'_>, text: String) -> bool {
    if chat.settings.overlay.is_none() {
        return false;
    }

    let overlay = match chat.settings.overlay.as_mut() {
        Some(overlay) => overlay,
        None => return false,
    };

    if overlay.is_menu_active() || overlay.is_help_visible() {
        return false;
    }

    if let Some(content) = overlay.active_content_mut() {
        if content.handle_paste(text) {
            chat.request_redraw();
            return true;
        }
    }

    false
}

/// Handle key presses for the full-screen settings overlay. Returns true when the
/// key has been consumed (overlay stays modal while active).
pub(super) fn handle_settings_key(chat: &mut ChatWidget<'_>, key_event: KeyEvent) -> bool {
    if chat.settings.overlay.is_none() {
        return false;
    }

    if !matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
        return true;
    }

    let Some(ref mut overlay) = chat.settings.overlay else { return true };

    if overlay.is_help_visible() {
        match key_event.code {
            KeyCode::Esc => {
                overlay.hide_help();
                chat.request_redraw();
            }
            KeyCode::Char('?') => {
                overlay.hide_help();
                chat.request_redraw();
            }
            _ => {}
        }
        return true;
    }

    if matches!(key_event.code, KeyCode::Char('?')) {
        overlay.show_help(overlay.is_menu_active());
        chat.request_redraw();
        return true;
    }

    if overlay.is_menu_active() {
        let mut handled = true;
        let mut changed = false;

        match key_event.code {
            KeyCode::Enter => {
                let section = overlay.active_section();
                overlay.set_mode_section(section);
                chat.request_redraw();
                return true;
            }
            KeyCode::Esc => {
                chat.close_settings_overlay();
                return true;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                changed = overlay.select_previous();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                changed = overlay.select_next();
            }
            KeyCode::Home => {
                changed = overlay.set_section(crate::bottom_pane::SettingsSection::Model);
            }
            KeyCode::End => {
                let last = crate::bottom_pane::SettingsSection::ALL
                    .last()
                    .copied()
                    .unwrap_or(crate::bottom_pane::SettingsSection::Model);
                changed = overlay.set_section(last);
            }
            _ => {
                handled = false;
            }
        }

        if changed {
            chat.request_redraw();
        }

        return handled;
    }

    // Give the active content first chance to handle keys (including Esc)
    let mut handled_by_content = false;
    let mut should_close = false;

    if let Some(content) = overlay.active_content_mut() {
        if content.handle_key(key_event) {
            handled_by_content = true;
            if content.is_complete() {
                should_close = true;
            }
        }
    }

    if handled_by_content {
        chat.request_redraw();
        if should_close {
            chat.close_settings_overlay();
        }
        return true;
    }

    match key_event.code {
        KeyCode::Esc if key_event.modifiers.is_empty() => {
            overlay.set_mode_menu(None);
            chat.request_redraw();
            return true;
        }
        _ => {}
    }

    let mut handled = true;
    let mut changed = false;

    match key_event.code {
        KeyCode::Enter => {
            if chat.activate_current_settings_section() {
                return true;
            }
        }
        KeyCode::BackTab => {
            changed = overlay.select_previous();
        }
        KeyCode::Tab => {
            changed = overlay.select_next();
        }
        KeyCode::Up | KeyCode::Char('k') => {
            changed = overlay.select_previous();
        }
        KeyCode::Down | KeyCode::Char('j') => {
            changed = overlay.select_next();
        }
        KeyCode::Home => {
            changed = overlay.set_section(crate::bottom_pane::SettingsSection::Model);
        }
        KeyCode::End => {
            let last = crate::bottom_pane::SettingsSection::ALL
                .last()
                .copied()
                .unwrap_or(crate::bottom_pane::SettingsSection::Model);
            changed = overlay.set_section(last);
        }
        _ => {
            handled = false;
        }
    }

    if changed {
        chat.request_redraw();
    }

    handled
}
