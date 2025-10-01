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
        chat.app_event_tx.send(crate::app_event::AppEvent::RequestRedraw);
        chat.maybe_show_history_nav_hint_on_first_scroll();
    } else {
        if chat.layout.scroll_offset >= 3 {
            chat.layout.scroll_offset = chat.layout.scroll_offset.saturating_sub(3);
            chat.app_event_tx.send(crate::app_event::AppEvent::RequestRedraw);
            chat.maybe_show_history_nav_hint_on_first_scroll();
        } else if chat.layout.scroll_offset > 0 {
            chat.layout.scroll_offset = 0;
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
    chat.app_event_tx
        .send(crate::app_event::AppEvent::RequestRedraw);
    chat.height_manager
        .borrow_mut()
        .record_event(HeightEvent::UserScroll);
    // No hint necessary when landing at bottom, but keep behavior consistent
    chat.maybe_show_history_nav_hint_on_first_scroll();
    chat.perf_track_scroll_delta(before, chat.layout.scroll_offset);
}

pub(super) fn toggle_browser_hud(chat: &mut ChatWidget<'_>) {
    let new_state = !chat.layout.browser_hud_expanded;
    chat.layout.browser_hud_expanded = new_state;
    if new_state {
        chat.layout.agents_hud_expanded = false;
    }
    chat.height_manager.borrow_mut().record_event(HeightEvent::HudToggle(true));
    chat.request_redraw();
}

pub(super) fn layout_areas(chat: &ChatWidget<'_>, area: Rect) -> Vec<Rect> {
    let has_browser_screenshot = chat
        .latest_browser_screenshot
        .lock()
        .map(|lock| lock.is_some())
        .unwrap_or(false);
    let has_active_agents = !chat.active_agents.is_empty() || chat.agents_ready_to_start;
    // In standard terminal mode, suppress HUD entirely.
    let hud_present = if chat.standard_terminal_mode {
        false
    } else {
        has_browser_screenshot || has_active_agents
    };

    let bottom_desired = chat.bottom_pane.desired_height(area.width);
    let font_cell = chat.measured_font_size();
    let mut hm = chat.height_manager.borrow_mut();

    let last = chat.layout.last_hud_present.get();
    if last != hud_present {
        hm.record_event(HeightEvent::HudToggle(hud_present));
        chat.layout.last_hud_present.set(hud_present);
    }

    let collapsed_unit: u16 = 3;
    let present_count: u16 = (has_active_agents as u16)
        + (has_browser_screenshot as u16);
    let hud_target: Option<u16> = if !hud_present || present_count == 0 {
        None
    } else {
        let base_collapsed = collapsed_unit * present_count.max(1);
        let term_h = chat.layout.last_frame_height.get().max(1);
        let thirty = ((term_h as u32) * 30 / 100) as u16;
        let sixty = ((term_h as u32) * 60 / 100) as u16;
        let mut expanded = if thirty < 25 { 25.min(sixty) } else { thirty };
        expanded = expanded.max(collapsed_unit.saturating_add(2));
        let any_expanded = (chat.layout.browser_hud_expanded && has_browser_screenshot)
            || (chat.layout.agents_hud_expanded && has_active_agents);
        let target = if any_expanded {
            base_collapsed.saturating_add(expanded)
        } else {
            base_collapsed
        };
        Some(target)
    };

    hm.begin_frame(
        area,
        hud_present,
        bottom_desired,
        font_cell,
        hud_target,
        // Disable status bar when in standard terminal mode
        !chat.standard_terminal_mode,
    )
}
