#![cfg(test)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use code_tui::test_helpers::{
    render_chat_widget_to_vt100, AutoContinueModeFixture, ChatWidgetHarness,
};

#[test]
fn auto_drive_countdown_keeps_render_requests_local() {
    let mut harness = ChatWidgetHarness::new();
    harness.enable_perf(true);

    for idx in 0..300 {
        harness.push_user_prompt(format!(
            "User {idx}: keep extending the long AutoDrive session transcript."
        ));
        harness.push_assistant_markdown(format!(
            "Assistant {idx}: adding more content so the history grows large enough to exercise render performance."
        ));
    }

    harness.auto_drive_activate(
        "Diagnose long-running session performance",
        false,
        false,
        AutoContinueModeFixture::TenSeconds,
    );
    harness.auto_drive_set_awaiting_submission(
        "Continue",
        "Waiting for coordinator",
        None,
    );

    let _ = render_chat_widget_to_vt100(&mut harness, 120, 40);
    let before = harness.perf_stats_snapshot();

    harness.auto_drive_advance_countdown(9);
    let _ = render_chat_widget_to_vt100(&mut harness, 120, 40);
    let after = harness.perf_stats_snapshot();

    let full_delta = after
        .render_requests_full
        .saturating_sub(before.render_requests_full);
    let visible_delta = after
        .render_requests_visible
        .saturating_sub(before.render_requests_visible);
    let history_len = harness.history_len();

    assert!(
        history_len > 100,
        "history should be large enough to stress render requests"
    );
    assert_eq!(
        full_delta, 0,
        "stable history should not rebuild full render requests on countdown ticks"
    );
    assert!(
        visible_delta > 0 && (visible_delta as usize) < history_len,
        "visible render requests should stay below the full history size"
    );
}
