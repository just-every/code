use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use codex_core::config_types::{KeyChord, KeyCodeName};

pub fn matches_event(ev: &KeyEvent, chord: &KeyChord) -> bool {
    let ctrl = ev.modifiers.contains(KeyModifiers::CONTROL);
    let alt = ev.modifiers.contains(KeyModifiers::ALT);
    let shift = ev.modifiers.contains(KeyModifiers::SHIFT);
    if ctrl != chord.ctrl || alt != chord.alt || shift != chord.shift {
        return false;
    }
    match (&ev.code, &chord.code) {
        (KeyCode::Char(c1), KeyCodeName::Char(c2)) => c1.to_ascii_lowercase() == *c2,
        (KeyCode::F(n1), KeyCodeName::F(n2)) => *n1 == *n2,
        (KeyCode::Enter, KeyCodeName::Enter) => true,
        (KeyCode::Esc, KeyCodeName::Esc) => true,
        (KeyCode::Tab, KeyCodeName::Tab) => true,
        (KeyCode::BackTab, KeyCodeName::BackTab) => true,
        (KeyCode::Insert, KeyCodeName::Insert) => true,
        (KeyCode::Char(' '), KeyCodeName::Space) => true,
        (KeyCode::Left, KeyCodeName::Left) => true,
        (KeyCode::Right, KeyCodeName::Right) => true,
        (KeyCode::Up, KeyCodeName::Up) => true,
        (KeyCode::Down, KeyCodeName::Down) => true,
        (KeyCode::PageUp, KeyCodeName::PageUp) => true,
        (KeyCode::PageDown, KeyCodeName::PageDown) => true,
        (KeyCode::Home, KeyCodeName::Home) => true,
        (KeyCode::End, KeyCodeName::End) => true,
        _ => false,
    }
}

pub fn label_for_chord(ch: &KeyChord) -> String {
    let mut parts = Vec::new();
    if ch.ctrl { parts.push("Ctrl".to_string()); }
    if ch.alt { parts.push("Alt".to_string()); }
    if ch.shift { parts.push("Shift".to_string()); }
    let code = match ch.code {
        KeyCodeName::Char(c) => c.to_ascii_uppercase().to_string(),
        KeyCodeName::F(n) => format!("F{n}"),
        KeyCodeName::Enter => "Enter".to_string(),
        KeyCodeName::Esc => "Esc".to_string(),
        KeyCodeName::Tab => "Tab".to_string(),
        KeyCodeName::BackTab => "Shift+Tab".to_string(),
        KeyCodeName::Insert => "Insert".to_string(),
        KeyCodeName::Space => "Space".to_string(),
        KeyCodeName::Left => "Left".to_string(),
        KeyCodeName::Right => "Right".to_string(),
        KeyCodeName::Up => "Up".to_string(),
        KeyCodeName::Down => "Down".to_string(),
        KeyCodeName::PageUp => "PageUp".to_string(),
        KeyCodeName::PageDown => "PageDown".to_string(),
        KeyCodeName::Home => "Home".to_string(),
        KeyCodeName::End => "End".to_string(),
    };
    if code == "Shift+Tab" {
        if parts.is_empty() { return code; }
        return format!("{}+{}", parts.join("+"), code);
    }
    parts.push(code);
    parts.join("+")
}

