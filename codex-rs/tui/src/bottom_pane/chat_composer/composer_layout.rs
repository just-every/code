//! Reserves popup and voice-control space around the editable draft.

use super::ChatComposer;
use crate::bottom_pane::voice_strip::VoiceStrip;
use crate::bottom_pane::voice_strip::VoiceStripState;
use crate::render::renderable::Renderable;
use crate::tui::FrameRequester;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

impl ChatComposer {
    pub(crate) fn set_voice_strip(
        &mut self,
        state: Option<VoiceStripState>,
        frame_requester: FrameRequester,
    ) {
        match (self.voice_strip.as_mut(), state) {
            (Some(strip), Some(state)) => strip.update(state),
            (_, Some(state)) => self.voice_strip = Some(VoiceStrip::new(state, frame_requester)),
            (_, None) => self.voice_strip = None,
        }
    }

    pub(super) fn render_voice_strip(&self, composer_rect: Rect, buf: &mut Buffer) {
        if let Some(strip) = &self.voice_strip
            && composer_rect.height >= 6
        {
            let voice_rect = Rect {
                y: composer_rect.y.saturating_add(/*rhs*/ 1),
                height: 2,
                ..composer_rect
            };
            strip.render(voice_rect, buf);
        }
    }
}
