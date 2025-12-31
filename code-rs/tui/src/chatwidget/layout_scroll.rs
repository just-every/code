//! Layout computation and scrolling/HUD helpers for ChatWidget.

use super::ChatWidget;
use crate::height_manager::HeightEvent;
use ratatui::layout::Rect;

pub(super) fn autoscroll_if_near_bottom(chat: &mut ChatWidget<'_>) {
    if chat.layout.scroll_offset <= 3 {
        let before = chat.layout.scroll_offset;
        chat.layout.scroll_offset = 0;
        chat.bottom_pane.set_compact_compose(false);
        chat.height_manager
            .borrow_mut()
            .record_event(HeightEvent::ComposerModeChange);
        chat.perf_track_scroll_delta(before, chat.layout.scroll_offset);
        chat.sync_history_virtualization();
    }
}

pub(super) fn page_up(chat: &mut ChatWidget<'_>) {
    let before = chat.layout.scroll_offset;
    let step = chat.layout.last_history_viewport_height.get().max(1);
    let new_offset = chat
        .layout.scroll_offset
        .saturating_add(step)
        .min(chat.layout.last_max_scroll.get());
    chat.layout.scroll_offset = new_offset;
    chat.bottom_pane.set_compact_compose(true);
    flash_scrollbar(chat);
    chat.sync_history_virtualization();
    chat.app_event_tx.send(crate::app_event::AppEvent::RequestRedraw);
    chat.height_manager.borrow_mut().record_event(crate::height_manager::HeightEvent::UserScroll);
    chat.maybe_show_history_nav_hint_on_first_scroll();
    chat.perf_track_scroll_delta(before, chat.layout.scroll_offset);
}

pub(super) fn line_up(chat: &mut ChatWidget<'_>) {
    let before = chat.layout.scroll_offset;
    let max_scroll = chat.layout.last_max_scroll.get();
    let new_offset = chat
        .layout
        .scroll_offset
        .saturating_add(1)
        .min(max_scroll);
    if new_offset == chat.layout.scroll_offset {
        return;
    }
    chat.layout.scroll_offset = new_offset;
    chat.bottom_pane.set_compact_compose(true);
    flash_scrollbar(chat);
    chat.sync_history_virtualization();
    chat.app_event_tx.send(crate::app_event::AppEvent::RequestRedraw);
    chat.height_manager
        .borrow_mut()
        .record_event(HeightEvent::UserScroll);
    chat.maybe_show_history_nav_hint_on_first_scroll();
    chat.perf_track_scroll_delta(before, chat.layout.scroll_offset);
}

pub(super) fn line_down(chat: &mut ChatWidget<'_>) {
    let before = chat.layout.scroll_offset;
    if chat.layout.scroll_offset == 0 {
        chat.perf_track_scroll_delta(before, chat.layout.scroll_offset);
        return;
    }
    let new_offset = chat.layout.scroll_offset.saturating_sub(1);
    chat.layout.scroll_offset = new_offset;
    if chat.layout.scroll_offset == 0 {
        chat.bottom_pane.set_compact_compose(false);
    }
    flash_scrollbar(chat);
    chat.sync_history_virtualization();
    chat.app_event_tx.send(crate::app_event::AppEvent::RequestRedraw);
    chat.height_manager
        .borrow_mut()
        .record_event(HeightEvent::UserScroll);
    chat.maybe_show_history_nav_hint_on_first_scroll();
    chat.perf_track_scroll_delta(before, chat.layout.scroll_offset);
}

pub(super) fn page_down(chat: &mut ChatWidget<'_>) {
    let before = chat.layout.scroll_offset;
    let step = chat.layout.last_history_viewport_height.get().max(1);
    if chat.layout.scroll_offset > step {
        chat.layout.scroll_offset = chat.layout.scroll_offset.saturating_sub(step);
    } else {
        chat.layout.scroll_offset = 0;
        chat.bottom_pane.set_compact_compose(false);
    }
    flash_scrollbar(chat);
    chat.sync_history_virtualization();
    chat.app_event_tx.send(crate::app_event::AppEvent::RequestRedraw);
    chat.height_manager.borrow_mut().record_event(crate::height_manager::HeightEvent::UserScroll);
    chat.maybe_show_history_nav_hint_on_first_scroll();
    chat.perf_track_scroll_delta(before, chat.layout.scroll_offset);
}

pub(super) fn mouse_scroll(chat: &mut ChatWidget<'_>, up: bool) {
    let before = chat.layout.scroll_offset;
    if up {
        let new_offset = chat
            .layout.scroll_offset
            .saturating_add(3)
            .min(chat.layout.last_max_scroll.get());
        chat.layout.scroll_offset = new_offset;
        flash_scrollbar(chat);
        if chat.layout.scroll_offset > 0 {
            chat.bottom_pane.set_compact_compose(true);
        }
        chat.sync_history_virtualization();
        chat.app_event_tx.send(crate::app_event::AppEvent::RequestRedraw);
        chat.maybe_show_history_nav_hint_on_first_scroll();
    } else {
        if chat.layout.scroll_offset >= 3 {
            chat.layout.scroll_offset = chat.layout.scroll_offset.saturating_sub(3);
            chat.sync_history_virtualization();
            chat.app_event_tx.send(crate::app_event::AppEvent::RequestRedraw);
            chat.maybe_show_history_nav_hint_on_first_scroll();
        } else if chat.layout.scroll_offset > 0 {
            chat.layout.scroll_offset = 0;
            chat.sync_history_virtualization();
            chat.app_event_tx.send(crate::app_event::AppEvent::RequestRedraw);
            chat.maybe_show_history_nav_hint_on_first_scroll();
        }
        flash_scrollbar(chat);
        if chat.layout.scroll_offset == 0 {
            chat.bottom_pane.set_compact_compose(false);
        }
    }
    chat.perf_track_scroll_delta(before, chat.layout.scroll_offset);
}

pub(super) fn flash_scrollbar(chat: &ChatWidget<'_>) {
    use std::time::{Duration, Instant};
    let until = Instant::now() + Duration::from_millis(1200);
    chat.layout.scrollbar_visible_until.set(Some(until));
    let tx = chat.app_event_tx.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(1300)).await;
        tx.send(crate::app_event::AppEvent::RequestRedraw);
    });
}

/// Jump to the very top of the history (oldest content).
pub(super) fn to_top(chat: &mut ChatWidget<'_>) {
    let before = chat.layout.scroll_offset;
    chat.layout.scroll_offset = chat.layout.last_max_scroll.get();
    chat.bottom_pane.set_compact_compose(true);
    flash_scrollbar(chat);
    chat.sync_history_virtualization();
    chat.app_event_tx
        .send(crate::app_event::AppEvent::RequestRedraw);
    chat.height_manager
        .borrow_mut()
        .record_event(HeightEvent::UserScroll);
    chat.maybe_show_history_nav_hint_on_first_scroll();
    chat.perf_track_scroll_delta(before, chat.layout.scroll_offset);
}

/// Jump to the very bottom of the history (latest content).
pub(super) fn to_bottom(chat: &mut ChatWidget<'_>) {
    let before = chat.layout.scroll_offset;
    chat.layout.scroll_offset = 0;
    chat.bottom_pane.set_compact_compose(false);
    flash_scrollbar(chat);
    chat.sync_history_virtualization();
    chat.app_event_tx
        .send(crate::app_event::AppEvent::RequestRedraw);
    chat.height_manager
        .borrow_mut()
        .record_event(HeightEvent::UserScroll);
    // No hint necessary when landing at bottom, but keep behavior consistent
    chat.maybe_show_history_nav_hint_on_first_scroll();
    chat.perf_track_scroll_delta(before, chat.layout.scroll_offset);
}

pub(super) fn layout_areas(chat: &ChatWidget<'_>, area: Rect) -> Vec<Rect> {
    let bottom_desired = chat.bottom_pane.desired_height(area.width);
    let font_cell = chat.measured_font_size();
    let mut hm = chat.height_manager.borrow_mut();
    hm.begin_frame(
        area,
        false,
        bottom_desired,
        font_cell,
        None,
        // Disable status bar when in standard terminal mode
        !chat.standard_terminal_mode,
    )
}
