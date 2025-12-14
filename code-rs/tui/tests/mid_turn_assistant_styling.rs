//! Regression test: mid-turn Answer outputs should not render assistant gutter/bold styling.

#![cfg(test)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use code_core::protocol::{AgentMessageEvent, Event, EventMsg, OrderMeta};
use code_tui::test_helpers::{render_chat_widget_to_vt100, ChatWidgetHarness};

#[test]
fn mid_turn_answer_suppresses_bullet_gutter() {
    let mut harness = ChatWidgetHarness::new();

    // Start a turn.
    harness.handle_event(Event {
        id: "task-1".into(),
        event_seq: 0,
        msg: EventMsg::TaskStarted,
        order: None,
    });

    // Simulate two Answer output items in the same turn. The first is a mid-turn
    // progress update; the second is the real final answer.
    harness.handle_event(Event {
        id: "msg-1".into(),
        event_seq: 1,
        msg: EventMsg::AgentMessage(AgentMessageEvent {
            message: "Progress update".into(),
        }),
        order: Some(OrderMeta {
            request_ordinal: 1,
            output_index: Some(0),
            sequence_number: None,
        }),
    });

    harness.handle_event(Event {
        id: "msg-2".into(),
        event_seq: 2,
        msg: EventMsg::AgentMessage(AgentMessageEvent {
            message: "Final answer".into(),
        }),
        order: Some(OrderMeta {
            request_ordinal: 1,
            output_index: Some(1),
            sequence_number: None,
        }),
    });

    // Flush pending InsertFinalAnswer events so both assistant cells exist.
    let _ = render_chat_widget_to_vt100(&mut harness, 80, 24);

    // End the turn so the last Answer is treated as final.
    harness.handle_event(Event {
        id: "task-1".into(),
        event_seq: 3,
        msg: EventMsg::TaskComplete(code_core::protocol::TaskCompleteEvent {
            last_agent_message: None,
        }),
        order: None,
    });

    let output = render_chat_widget_to_vt100(&mut harness, 80, 24);

    assert!(output.contains("Progress update"));
    assert!(
        !output.contains(" • Progress update"),
        "mid-turn assistant messages should not show bullet gutter"
    );
    assert!(
        output.contains(" • Final answer"),
        "final assistant message should retain bullet gutter"
    );
}

#[test]
fn missing_task_complete_does_not_stick_mid_turn_across_turns() {
    let mut harness = ChatWidgetHarness::new();

    // Turn 1 starts and emits an assistant message, but TaskComplete is never received.
    harness.handle_event(Event {
        id: "task-1".into(),
        event_seq: 0,
        msg: EventMsg::TaskStarted,
        order: None,
    });

    harness.handle_event(Event {
        id: "msg-1".into(),
        event_seq: 1,
        msg: EventMsg::AgentMessage(AgentMessageEvent {
            message: "First answer".into(),
        }),
        order: Some(OrderMeta {
            request_ordinal: 1,
            output_index: Some(0),
            sequence_number: None,
        }),
    });

    // Flush pending InsertFinalAnswer.
    let _ = render_chat_widget_to_vt100(&mut harness, 80, 24);

    // Turn 2 begins. The new TaskStarted should defensively clear the stale task state
    // from turn 1 so the assistant output from subsequent turns is not treated as mid-turn.
    harness.handle_event(Event {
        id: "task-2".into(),
        event_seq: 0,
        msg: EventMsg::TaskStarted,
        order: None,
    });

    harness.handle_event(Event {
        id: "msg-2".into(),
        event_seq: 1,
        msg: EventMsg::AgentMessage(AgentMessageEvent {
            message: "Second answer".into(),
        }),
        order: Some(OrderMeta {
            request_ordinal: 2,
            output_index: Some(0),
            sequence_number: None,
        }),
    });

    let _ = render_chat_widget_to_vt100(&mut harness, 80, 24);

    harness.handle_event(Event {
        id: "task-2".into(),
        event_seq: 2,
        msg: EventMsg::TaskComplete(code_core::protocol::TaskCompleteEvent {
            last_agent_message: None,
        }),
        order: None,
    });

    let output = render_chat_widget_to_vt100(&mut harness, 80, 24);

    assert!(output.contains(" • First answer"));
    assert!(output.contains(" • Second answer"));
}
