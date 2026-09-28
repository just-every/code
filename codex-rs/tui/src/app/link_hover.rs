//! Match transcript hover to the links Codex opens while terminal mouse input is captured.
//! Recheck after painting so scrolling and live output cannot leave a hand over stale content.

use crossterm::event::KeyModifiers;

use super::App;
use crate::tui::Tui;

impl App {
    pub(super) fn refresh_link_hover(&self, tui: &mut Tui) -> std::io::Result<()> {
        let over_link = tui.is_owned_screen()
            && self.overlay.is_none()
            && self.chat_widget.no_modal_or_popup_active()
            && tui.link_hover.mouse.is_some_and(|mouse| {
                (mouse.modifiers.is_empty()
                    || mouse
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::SUPER))
                    && self
                        .transcript_view
                        .link_at(mouse.column, mouse.row)
                        .is_some()
            });
        tui.set_link_pointer(over_link)
    }
}
