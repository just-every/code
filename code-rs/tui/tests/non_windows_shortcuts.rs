#![cfg(not(target_os = "windows"))]

use code_tui::ComposerInput;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[test]
fn composer_ctrl_alt_letter_shortcut_deletes_word() {
    let mut composer = ComposerInput::new();
    composer.handle_paste("word".to_string());

    let _ = composer.input(KeyEvent::new(
        KeyCode::Char('h'),
        KeyModifiers::CONTROL | KeyModifiers::ALT,
    ));

    assert!(composer.text().is_empty(), "Ctrl+Alt+H should delete the previous word");
}

#[test]
fn composer_ctrl_alt_symbol_does_not_insert_text() {
    let mut composer = ComposerInput::new();

    let _ = composer.input(KeyEvent::new(
        KeyCode::Char('@'),
        KeyModifiers::CONTROL | KeyModifiers::ALT,
    ));

    assert!(
        composer.text().is_empty(),
        "Ctrl+Alt symbol should be treated as a shortcut, not inserted text"
    );
}
