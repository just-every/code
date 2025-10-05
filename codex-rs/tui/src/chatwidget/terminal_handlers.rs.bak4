use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{ChatWidget, PendingCommandAction};
use crate::app_event::AppEvent;

pub(super) fn handle_terminal_key(chat: &mut ChatWidget<'_>, key_event: KeyEvent) -> bool {
    let Some(id) = chat.terminal_overlay_id() else {
        return false;
    };

    let running = chat.terminal_is_running();

    if running {
        if key_event
            .modifiers
            .contains(KeyModifiers::CONTROL)
            && !key_event
                .modifiers
                .intersects(KeyModifiers::ALT | KeyModifiers::SHIFT | KeyModifiers::SUPER)
        {
            match key_event.code {
                KeyCode::Up => {
                    chat.terminal_scroll_lines(-1);
                    return true;
                }
                KeyCode::Down => {
                    chat.terminal_scroll_lines(1);
                    return true;
                }
                KeyCode::PageUp => {
                    chat.terminal_scroll_page(-1);
                    return true;
                }
                KeyCode::PageDown => {
                    chat.terminal_scroll_page(1);
                    return true;
                }
                KeyCode::Home => {
                    chat.terminal_scroll_to_top();
                    return true;
                }
                KeyCode::End => {
                    chat.terminal_scroll_to_bottom();
                    return true;
                }
                _ => {}
            }
        }

        if let Some(bytes) = encode_key_for_pty(key_event) {
            chat.terminal_send_input(id, bytes);
            return true;
        }
    }

    if chat.terminal_handle_pending_key(key_event) {
        return true;
    }

    match key_event.code {
        KeyCode::Up => {
            chat.terminal_scroll_lines(-1);
            true
        }
        KeyCode::Down => {
            chat.terminal_scroll_lines(1);
            true
        }
        KeyCode::PageUp => {
            chat.terminal_scroll_page(-1);
            true
        }
        KeyCode::PageDown => {
            chat.terminal_scroll_page(1);
            true
        }
        KeyCode::Home => {
            chat.terminal_scroll_to_top();
            true
        }
        KeyCode::End => {
            chat.terminal_scroll_to_bottom();
            true
        }
        KeyCode::Esc => {
            if running {
                chat.request_terminal_cancel(id);
            } else {
                chat.close_terminal_overlay();
            }
            true
        }
        KeyCode::Char('c') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
            if running {
                false
            } else {
                chat.app_event_tx.send(AppEvent::TerminalCancel { id });
                true
            }
        }
        KeyCode::Char('r') | KeyCode::Char('R') => {
            if running {
                true
            } else if chat.terminal_prepare_rerun(id) {
                chat.app_event_tx.send(AppEvent::TerminalRerun { id });
                true
            } else {
                true
            }
        }
        KeyCode::Enter => {
            if let Some(action) = chat.terminal_accept_pending_command() {
                if let PendingCommandAction::Manual(command) = action {
                    chat.terminal_execute_manual_command(id, command);
                }
                true
            } else if chat.terminal_has_pending_command() {
                true
            } else if running {
                chat.terminal_send_input(id, vec![b'\r']);
                true
            } else {
                false
            }
        }
        _ => false,
    }
}

fn encode_key_for_pty(key_event: KeyEvent) -> Option<Vec<u8>> {
    let mods = key_event.modifiers;
    match key_event.code {
        KeyCode::Char(ch) => {
            if mods.contains(KeyModifiers::CONTROL) && !mods.contains(KeyModifiers::SUPER) {
                if let Some(ctrl) = control_byte(ch) {
                    let mut out = Vec::new();
                    if mods.contains(KeyModifiers::ALT) {
                        out.push(0x1b);
                    }
                    out.push(ctrl);
                    return Some(out);
                }
            }
            let mut out = Vec::new();
            if mods.contains(KeyModifiers::ALT) {
                out.push(0x1b);
            }
            let mut buf = [0u8; 4];
            let encoded = ch.encode_utf8(&mut buf);
            out.extend_from_slice(encoded.as_bytes());
            Some(out)
        }
        KeyCode::Enter => Some(vec![b'\r']),
        KeyCode::Tab => {
            if mods.contains(KeyModifiers::SHIFT) {
                Some(b"\x1b[Z".to_vec())
            } else {
                Some(vec![b'\t'])
            }
        }
        KeyCode::Backspace => Some(vec![0x7f]),
        KeyCode::Delete => Some(csi_with_modifier("3", b'~', mods)),
        KeyCode::Insert => Some(csi_with_modifier("2", b'~', mods)),
        KeyCode::Left => Some(arrow_sequence(b'D', mods)),
        KeyCode::Right => Some(arrow_sequence(b'C', mods)),
        KeyCode::Up => Some(arrow_sequence(b'A', mods)),
        KeyCode::Down => Some(arrow_sequence(b'B', mods)),
        KeyCode::Home => Some(arrow_sequence(b'H', mods)),
        KeyCode::End => Some(arrow_sequence(b'F', mods)),
        KeyCode::PageUp => Some(csi_with_modifier("5", b'~', mods)),
        KeyCode::PageDown => Some(csi_with_modifier("6", b'~', mods)),
        KeyCode::BackTab => Some(b"\x1b[Z".to_vec()),
        KeyCode::F(n) if (1..=12).contains(&n) => Some(function_key_sequence(n, mods)),
        KeyCode::Null => Some(vec![0]),
        KeyCode::Esc => None,
        _ => None,
    }
}

fn control_byte(ch: char) -> Option<u8> {
    match ch {
        '@' | ' ' => Some(0x00),
        'a'..='z' => Some((ch as u8 - b'a') + 1),
        'A'..='Z' => Some((ch as u8 - b'A') + 1),
        '[' => Some(0x1b),
        '\\' => Some(0x1c),
        ']' => Some(0x1d),
        '^' => Some(0x1e),
        '_' => Some(0x1f),
        '?' => Some(0x7f),
        _ => None,
    }
}

fn ansi_modifier_param(mods: KeyModifiers) -> Option<u8> {
    let mut bits = 0u8;
    if mods.contains(KeyModifiers::SHIFT) {
        bits |= 1;
    }
    if mods.contains(KeyModifiers::ALT) {
        bits |= 2;
    }
    if mods.contains(KeyModifiers::CONTROL) {
        bits |= 4;
    }
    if bits == 0 {
        None
    } else {
        Some(1 + bits)
    }
}

fn arrow_sequence(letter: u8, mods: KeyModifiers) -> Vec<u8> {
    if let Some(param) = ansi_modifier_param(mods) {
        format!("\x1b[1;{}{}", param, letter as char).into_bytes()
    } else {
        vec![0x1b, b'[', letter]
    }
}

fn csi_with_modifier(base: &str, suffix: u8, mods: KeyModifiers) -> Vec<u8> {
    if let Some(param) = ansi_modifier_param(mods) {
        format!("\x1b[{};{}{}", base, param, suffix as char).into_bytes()
    } else {
        format!("\x1b[{}{}", base, suffix as char).into_bytes()
    }
}

fn function_key_sequence(n: u8, mods: KeyModifiers) -> Vec<u8> {
    let (prefix, suffix, tilde) = match n {
        1 => ("1", 'P', false),
        2 => ("1", 'Q', false),
        3 => ("1", 'R', false),
        4 => ("1", 'S', false),
        5 => ("15", '~', true),
        6 => ("17", '~', true),
        7 => ("18", '~', true),
        8 => ("19", '~', true),
        9 => ("20", '~', true),
        10 => ("21", '~', true),
        11 => ("23", '~', true),
        12 => ("24", '~', true),
        _ => return Vec::new(),
    };

    if let Some(param) = ansi_modifier_param(mods) {
        if tilde {
            format!("\x1b[{};{}~", prefix, param).into_bytes()
        } else {
            format!("\x1b[1;{}{}", param, suffix).into_bytes()
        }
    } else if tilde {
        format!("\x1b[{}~", prefix).into_bytes()
    } else {
        format!("\x1bO{}", suffix).into_bytes()
    }
}
