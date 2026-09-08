//! Voice-keymap regressions for defaults, custom bindings, unbinding, and conflicts.
//! Voice shortcuts preserve ordinary chat bindings and unrelated chords.

use super::*;
use crate::key_hint::KeyBindingListExt;
use pretty_assertions::assert_eq;

#[test]
fn voice_mute_resolves_custom_bindings_unbinding_and_visible_hints() {
    for (config, expected) in [
        ("", Some("ctrl + x")),
        ("[chat]\ntoggle_voice_mute = 'f8'", Some("f8")),
        ("[chat]\ntoggle_voice_mute = []", None),
        ("[chat]\ntoggle_voice_mute = 'ctrl-x m'", Some("ctrl + x m")),
    ] {
        let config = toml::from_str::<TuiKeymap>(config).expect("valid voice keymap config");
        let runtime = RuntimeKeymap::from_config(&config).expect("valid voice bindings");
        assert_eq!(
            runtime
                .primary_hint(KeymapContext::Chat, "toggle_voice_mute")
                .map(ShortcutHint::display_label),
            expected.map(str::to_string),
        );
    }
}

#[test]
fn voice_mute_default_yields_to_existing_shortcuts_and_chord_prefixes() {
    for config in [
        "[editor]\nkill_line_end = 'ctrl-x'",
        "[global]\ncopy = 'ctrl-x'",
        "[global]\nopen_transcript = 'ctrl-x ctrl-t'",
    ] {
        let config = toml::from_str::<TuiKeymap>(config).unwrap();
        let runtime = RuntimeKeymap::from_config(&config).expect("existing binding stays valid");
        assert_eq!(
            runtime.primary_hint(KeymapContext::Chat, "toggle_voice_mute"),
            None
        );
    }
    for (binding, conflict) in [
        ("ctrl-k", "kill_line_end"),
        ("ctrl-c", "fixed.interrupt_or_quit"),
    ] {
        let config =
            toml::from_str::<TuiKeymap>(&format!("[chat]\ntoggle_voice_mute = '{binding}'"))
                .unwrap();
        assert!(
            RuntimeKeymap::from_config(&config)
                .unwrap_err()
                .contains(conflict)
        );
    }
}

#[test]
fn voice_mute_chord_dispatches_only_in_its_active_context() {
    let config = toml::from_str::<TuiKeymap>("[chat]\ntoggle_voice_mute = 'ctrl-x m'").unwrap();
    let runtime = RuntimeKeymap::from_config(&config).unwrap();
    let action = keymap_action_id("chat", "toggle_voice_mute").unwrap();
    let mut matcher = KeyChordMatcher::default();
    let now = tokio::time::Instant::now();
    let prefix = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL);
    let completion = KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE);
    assert_eq!(
        matcher.advance(
            prefix,
            &runtime.chords,
            KeymapContextSet::new(KeymapContext::Pager),
            now
        ),
        KeyChordMatch::PassThrough
    );
    let contexts = KeymapContextSet::new(action.context);
    assert!(matches!(
        matcher.advance(prefix, &runtime.chords, contexts, now),
        KeyChordMatch::Pending(_)
    ));
    let KeyChordMatch::Completed(dispatch) =
        matcher.advance(completion, &runtime.chords, contexts, now)
    else {
        panic!("mute chord completes")
    };
    assert!(runtime.chat.toggle_voice_mute.is_pressed(dispatch));
}
