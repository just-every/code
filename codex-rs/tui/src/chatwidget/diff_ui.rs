//! Diff overlay types used by the chat widget.
//!
//! Separated to keep `chatwidget.rs` smaller and focused on behavior.

use ratatui::text::Line;

pub struct DiffOverlay {
    pub tabs: Vec<(String, Vec<DiffBlock>)>,
    pub selected: usize,
    pub scroll_offsets: Vec<u16>,
}

impl DiffOverlay {
    pub fn new(tabs: Vec<(String, Vec<DiffBlock>)>) -> Self {
        let n = tabs.len();
        Self {
            tabs,
            selected: 0,
            scroll_offsets: vec![0; n],
        }
    }
}

#[derive(Clone)]
pub struct DiffBlock {
    pub lines: Vec<Line<'static>>,
}

pub struct DiffConfirm {
    pub text_to_submit: String,
}
