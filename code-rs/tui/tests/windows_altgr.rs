#![cfg(target_os = "windows")]

use code_tui::public_widgets::ComposerInput;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[test]
fn composer_input_altgr_characters_insert_text() {
    let mut composer = ComposerInput::new();

    let cases = [
        ('/', KeyModifiers::CONTROL | KeyModifiers::ALT),
        ('@', KeyModifiers::CONTROL | KeyModifiers::ALT),
        ('{', KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT),
    ];

    for (ch, modifiers) in cases {
        composer.clear();
        let _ = composer.input(KeyEvent::new(KeyCode::Char(ch), modifiers));
        assert_eq!(composer.text(), ch.to_string(), "AltGr input should insert printable character");
    }
}

#[test]
fn composer_input_ctrl_alt_letter_shortcut_still_deletes_word() {
    let mut composer = ComposerInput::new();
    composer.handle_paste("word".to_string());

    let _ = composer.input(KeyEvent::new(
        KeyCode::Char('h'),
        KeyModifiers::CONTROL | KeyModifiers::ALT,
    ));

    assert!(composer.text().is_empty(), "Ctrl+Alt+H should delete the previous word");
}
