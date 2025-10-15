//! Diff overlay key handling extracted from ChatWidget::handle_key_event.

use super::ChatWidget;
use crossterm::event::{KeyCode, KeyEvent};

// Returns true if the key was handled by the diff overlay.
pub(super) fn handle_diff_key(chat: &mut ChatWidget<'_>, key_event: KeyEvent) -> bool {
    // Only operate when an overlay is present
    let Some(ref mut overlay) = chat.diffs.overlay else {
        return false;
    };

    // If a confirmation banner is active, only Enter/Esc apply to it.
    if let Some(confirm) = chat.diffs.confirm.take() {
        match key_event.code {
            KeyCode::Enter => {
                chat.submit_user_message(confirm.text_to_submit.into());
                chat.request_redraw();
                return true;
            }
            KeyCode::Esc => {
                chat.diffs.confirm = None;
                chat.request_redraw();
                return true;
            }
            _ => {
                // Put it back for other keys
                chat.diffs.confirm = Some(confirm);
            }
        }
    }

    match key_event.code {
        KeyCode::Left => {
            if overlay.selected > 0 {
                overlay.selected -= 1;
            }
            if let Some(off) = overlay.scroll_offsets.get_mut(overlay.selected) {
                *off = 0;
            }
            chat.request_redraw();
            true
        }
        KeyCode::Right => {
            if overlay.selected + 1 < overlay.tabs.len() {
                overlay.selected += 1;
            }
            if let Some(off) = overlay.scroll_offsets.get_mut(overlay.selected) {
                *off = 0;
            }
            chat.request_redraw();
            true
        }
        KeyCode::Up => {
            if let Some(off) = overlay.scroll_offsets.get_mut(overlay.selected) {
                let visible_rows = chat.diffs.body_visible_rows.get() as usize;
                let total_lines: usize = overlay
                    .tabs
                    .get(overlay.selected)
                    .map(|(_, blocks)| blocks.iter().map(|b| b.lines.len()).sum())
                    .unwrap_or(0);
                let max_off = total_lines.saturating_sub(visible_rows.max(1));
                let cur = (*off).min(max_off as u16);
                *off = cur.saturating_sub(1);
            }
            chat.request_redraw();
            true
        }
        KeyCode::Down => {
            if let Some(off) = overlay.scroll_offsets.get_mut(overlay.selected) {
                let visible_rows = chat.diffs.body_visible_rows.get() as usize;
                let total_lines: usize = overlay
                    .tabs
                    .get(overlay.selected)
                    .map(|(_, blocks)| blocks.iter().map(|b| b.lines.len()).sum())
                    .unwrap_or(0);
                let max_off = total_lines.saturating_sub(visible_rows.max(1));
                let next = (*off as usize).saturating_add(1).min(max_off);
                *off = next as u16;
            }
            chat.request_redraw();
            true
        }
        KeyCode::Char('u') => {
            if let Some((_, blocks)) = overlay.tabs.get(overlay.selected) {
                let visible_rows = chat.diffs.body_visible_rows.get() as usize;
                let total_lines: usize = blocks.iter().map(|b| b.lines.len()).sum();
                let max_off = total_lines.saturating_sub(visible_rows.max(1));
                let skip_raw = overlay
                    .scroll_offsets
                    .get(overlay.selected)
                    .copied()
                    .unwrap_or(0) as usize;
                let skip = skip_raw.min(max_off);
                let mut start = 0usize;
                let mut chosen: Option<&super::diff_ui::DiffBlock> = None;
                for b in blocks {
                    let len = b.lines.len();
                    if start <= skip && skip < start + len {
                        chosen = Some(b);
                    }
                    start += len;
                }
                if let Some(block) = chosen {
                    let mut diff_text = String::new();
                    for l in &block.lines {
                        let s: String = l.spans.iter().map(|sp| sp.content.clone()).collect();
                        diff_text.push_str(&s);
                        diff_text.push('\n');
                    }
                    let submit_text = format!("Please undo this:\n{}", diff_text);
                    chat.diffs.confirm = Some(super::diff_ui::DiffConfirm {
                        text_to_submit: submit_text,
                    });
                    chat.request_redraw();
                }
            }
            true
        }
        KeyCode::Char('e') => {
            if let Some((_, blocks)) = overlay.tabs.get(overlay.selected) {
                let visible_rows = chat.diffs.body_visible_rows.get() as usize;
                let total_lines: usize = blocks.iter().map(|b| b.lines.len()).sum();
                let max_off = total_lines.saturating_sub(visible_rows.max(1));
                let skip_raw = overlay
                    .scroll_offsets
                    .get(overlay.selected)
                    .copied()
                    .unwrap_or(0) as usize;
                let skip = skip_raw.min(max_off);
                let mut start = 0usize;
                let mut chosen: Option<&super::diff_ui::DiffBlock> = None;
                for b in blocks {
                    let len = b.lines.len();
                    if start <= skip && skip < start + len {
                        chosen = Some(b);
                    }
                    start += len;
                }
                if let Some(block) = chosen {
                    let mut diff_text = String::new();
                    for l in &block.lines {
                        let s: String = l.spans.iter().map(|sp| sp.content.clone()).collect();
                        diff_text.push_str(&s);
                        diff_text.push('\n');
                    }
                    let prompt = format!(
                        "Can you please explain what this diff does and the reason behind it?\n\n{}",
                        diff_text
                    );
                    chat.submit_user_message(prompt.into());
                    chat.request_redraw();
                }
            }
            true
        }
        KeyCode::Esc => {
            chat.diffs.overlay = None;
            chat.diffs.confirm = None;
            chat.request_redraw();
            true
        }
        _ => false,
    }
}
