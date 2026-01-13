#![cfg(test)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use code_core::protocol::{
    AgentMessageDeltaEvent,
    AgentMessageEvent,
    AgentReasoningDeltaEvent,
    AgentReasoningEvent,
    BrowserSnapshotEvent,
    Event,
    EventMsg,
    ExecCommandBeginEvent,
    ExecCommandEndEvent,
    ExecCommandOutputDeltaEvent,
    ExecOutputStream,
    OrderMeta,
};
use code_tui::test_helpers::{layout_metrics, render_chat_widget_to_vt100, ChatWidgetHarness};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_bytes::ByteBuf;
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;

const PLAIN_SENTINEL: &str = "SENTINEL-PLAIN";
const STREAM_SENTINEL: &str = "SENTINEL-STREAM";
const EXEC_SENTINEL: &str = "SENTINEL-EXEC";
const BROWSER_SENTINEL: &str = "SENTINEL-BROWSER";
const COLLAPSED_EXEC_SENTINEL: &str = "SENTINEL-COLLAPSED-EXEC";

fn seed_plain_transcript(harness: &mut ChatWidgetHarness) {
    for idx in 0..8 {
        harness.push_user_prompt(format!("User message #{idx}"));
        harness.push_assistant_markdown(format!("Assistant reply #{idx} {PLAIN_SENTINEL}"));
    }
}

fn seed_reasoning_transcript(harness: &mut ChatWidgetHarness, suffix: &str) {
    harness.push_user_prompt("Share your reasoning and final summary.");

    let reasoning_chunks = [
        "Evaluating viewport overscan heuristics.",
        "Tracking spacer toggles near viewport multiples.",
        "Checking collapsed spacing adjustments.",
    ];

    for (idx, chunk) in reasoning_chunks.iter().enumerate() {
        harness.handle_event(Event {
            id: "reasoning-stream".into(),
            event_seq: (idx + 1) as u64,
            msg: EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent {
                delta: format!("{chunk}\n"),
            }),
            order: Some(OrderMeta {
                request_ordinal: 1,
                output_index: Some(0),
                sequence_number: Some(idx as u64),
            }),
        });
    }

    let final_reasoning = reasoning_chunks.join("\n");
    harness.handle_event(Event {
        id: "reasoning-stream".into(),
        event_seq: (reasoning_chunks.len() + 1) as u64,
        msg: EventMsg::AgentReasoning(AgentReasoningEvent {
            text: final_reasoning,
        }),
        order: Some(OrderMeta {
            request_ordinal: 1,
            output_index: Some(0),
            sequence_number: Some((reasoning_chunks.len() + 1) as u64),
        }),
    });

    harness.push_assistant_markdown(format!(
        "Final answer summarising anti-cutoff behaviour. Sentinel => {suffix}"
    ));
}

fn seed_reasoning_stream_only(harness: &mut ChatWidgetHarness) {
    harness.push_user_prompt("Share your reasoning.");

    let reasoning_chunks = [
        "Evaluating viewport overscan heuristics.",
        "Tracking spacer toggles near viewport multiples.",
        "Checking collapsed spacing adjustments.",
    ];

    for (idx, chunk) in reasoning_chunks.iter().enumerate() {
        harness.handle_event(Event {
            id: "reasoning-stream".into(),
            event_seq: (idx + 1) as u64,
            msg: EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent {
                delta: format!("{chunk}\n"),
            }),
            order: Some(OrderMeta {
                request_ordinal: 1,
                output_index: Some(0),
                sequence_number: Some(idx as u64),
            }),
        });
    }

    let final_reasoning = reasoning_chunks.join("\n");
    harness.handle_event(Event {
        id: "reasoning-stream".into(),
        event_seq: (reasoning_chunks.len() + 1) as u64,
        msg: EventMsg::AgentReasoning(AgentReasoningEvent {
            text: final_reasoning,
        }),
        order: Some(OrderMeta {
            request_ordinal: 1,
            output_index: Some(0),
            sequence_number: Some((reasoning_chunks.len() + 1) as u64),
        }),
    });
}

#[test]
fn collapsed_reasoning_before_exec_keeps_last_line() {
    let mut harness = ChatWidgetHarness::new();

    seed_plain_transcript(&mut harness);
    seed_reasoning_stream_only(&mut harness);
    harness.flush_into_widget();

    // Hide reasoning so it collapses into a compact title cell.
    harness.send_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL));

    let mut seq = 0_u64;
    let order = |seq: &mut u64| {
        let order = OrderMeta {
            request_ordinal: 42,
            output_index: Some(0),
            sequence_number: Some(*seq),
        };
        *seq += 1;
        order
    };
    let call_id = "exec-cutoff-collapsed".to_string();
    let cwd = PathBuf::from("/workspace");

    harness.handle_event(Event {
        id: "exec-begin-cutoff-collapsed".into(),
        event_seq: 0,
        msg: EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
            call_id: call_id.clone(),
            command: vec!["bash".into(), "-lc".into(), "printf".into()],
            cwd: cwd.clone(),
            parsed_cmd: Vec::new(),
        }),
        order: Some(order(&mut seq)),
    });

    harness.handle_event(Event {
        id: "exec-stdout-cutoff-collapsed".into(),
        event_seq: 1,
        msg: EventMsg::ExecCommandOutputDelta(ExecCommandOutputDeltaEvent {
            call_id: call_id.clone(),
            stream: ExecOutputStream::Stdout,
            chunk: ByteBuf::from(
                format!("line 1\nline 2\nline 3\n").into_bytes(),
            ),
        }),
        order: Some(order(&mut seq)),
    });

    harness.handle_event(Event {
        id: "exec-end-cutoff-collapsed".into(),
        event_seq: 2,
        msg: EventMsg::ExecCommandEnd(ExecCommandEndEvent {
            call_id,
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 0,
            duration: Duration::from_millis(120),
        }),
        order: Some(order(&mut seq)),
    });

    harness.handle_event(Event {
        id: "tail-answer".into(),
        event_seq: 3,
        msg: EventMsg::AgentMessage(AgentMessageEvent {
            message: format!(
                "Final tail marker to validate scroll-to-bottom. {COLLAPSED_EXEC_SENTINEL}"
            ),
        }),
        order: Some(OrderMeta {
            request_ordinal: 43,
            output_index: Some(0),
            sequence_number: Some(0),
        }),
    });
    harness.flush_into_widget();

    let output = render_chat_widget_to_vt100(&mut harness, 80, 10);
    let metrics = layout_metrics(&harness);
    let viewport_h = harness.history_viewport_height();
    assert!(
        output.contains(COLLAPSED_EXEC_SENTINEL),
        "collapsed reasoning -> exec transcript should keep the final sentinel visible (viewport_h={viewport_h}, scroll_offset={}, max_scroll={}):\n{}",
        metrics.scroll_offset,
        metrics.last_max_scroll,
        output
    );
    assert_eq!(
        metrics.scroll_offset, 0,
        "transcript should stay pinned to bottom"
    );
}

#[derive(Clone, Copy, Debug)]
enum Scenario {
    Plain,
    ReasoningExpanded,
    ReasoningCollapsed,
}

#[ignore]
#[test]
fn scan_history_cutoff_regressions() {
    let scenarios = [Scenario::Plain, Scenario::ReasoningExpanded, Scenario::ReasoningCollapsed];

    let viewports = [
        (60, 2u16),
        (60, 3),
        (60, 4),
        (60, 5),
        (60, 6),
        (60, 7),
        (60, 8),
        (70, 9),
        (70, 10),
        (70, 11),
        (80, 12),
        (80, 13),
        (90, 17),
        (90, 23),
        (100, 29),
        (100, 37),
        (120, 60),
        (120, 80),
    ];

    for scenario in scenarios {
        println!("=== scenario={scenario:?} ===");
        let mut harness = ChatWidgetHarness::new();
        seed_plain_transcript(&mut harness);
        let mut target_sentinel = PLAIN_SENTINEL;

        match scenario {
            Scenario::Plain => {}
            Scenario::ReasoningExpanded => {
                seed_reasoning_transcript(&mut harness, "SENTINEL-R");
                target_sentinel = "SENTINEL-R";
            }
            Scenario::ReasoningCollapsed => {
                seed_reasoning_transcript(&mut harness, "SENTINEL-R");
                harness.send_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL));
                target_sentinel = "SENTINEL-R";
            }
        }

        for &(width, height) in &viewports {
            let output = render_chat_widget_to_vt100(&mut harness, width, height);
            let metrics = layout_metrics(&harness);
            let plain_visible = output.contains(PLAIN_SENTINEL);
            let reasoning_visible = output.contains("SENTINEL-R");
            let last_line = output.lines().last().unwrap_or("");

            println!(
                "viewport {width}x{height}: plain={plain_visible}, reasoning={reasoning_visible}, last_line={:?}, rows={}, scroll_offset={}, max_scroll={}, viewport_h={}",
                last_line.trim_end(),
                output.lines().count(),
                metrics.scroll_offset,
                metrics.last_max_scroll,
                metrics.last_viewport_height
            );

            if !output.contains(target_sentinel) {
                println!("!! sentinel missing");
            }
        }

        println!();
    }
}

#[test]
fn plain_history_last_line_visible_80x12() {
    let mut harness = ChatWidgetHarness::new();
    seed_plain_transcript(&mut harness);

    let width = 80u16;
    let height = 12u16;
    let output = render_chat_widget_to_vt100(&mut harness, width, height);
    let metrics = layout_metrics(&harness);
    assert!(
        output.contains(PLAIN_SENTINEL),
        "plain transcript should keep the final sentinel visible"
    );
    assert_eq!(metrics.scroll_offset, 0, "plain transcript should rest at bottom");
}

#[test]
fn streaming_history_growth_keeps_last_line() {
    let mut harness = ChatWidgetHarness::new();
    harness.push_user_prompt("Stream a detailed answer.");

    let mut seq = 0_u64;
    let call_id = "stream-msg";

    let mut next_order = || {
        let order = OrderMeta {
            request_ordinal: 2,
            output_index: Some(0),
            sequence_number: Some(seq),
        };
        seq += 1;
        order
    };

    harness.handle_event(Event {
        id: call_id.into(),
        event_seq: 0,
        msg: EventMsg::AgentMessageDelta(AgentMessageDeltaEvent {
            delta: "Partial reasoning...\n".into(),
        }),
        order: Some(next_order()),
    });
    harness.handle_event(Event {
        id: "stream-msg".into(),
        event_seq: 1,
        msg: EventMsg::AgentMessageDelta(AgentMessageDeltaEvent {
            delta: format!("Continuing with more details. {STREAM_SENTINEL}\n"),
        }),
        order: Some(next_order()),
    });
    harness.handle_event(Event {
        id: "stream-msg".into(),
        event_seq: 2,
        msg: EventMsg::AgentMessage(AgentMessageEvent {
            message: format!(
                "Partial reasoning...\nContinuing with more details. {STREAM_SENTINEL}"
            ),
        }),
        order: Some(next_order()),
    });
    let output = render_chat_widget_to_vt100(&mut harness, 80, 12);
    let metrics = layout_metrics(&harness);
    assert!(
        output.contains(STREAM_SENTINEL),
        "streaming transcript should keep the last sentinel visible"
    );
    assert_eq!(metrics.scroll_offset, 0, "streaming transcript should stay pinned to bottom");
}

#[test]
fn exec_history_small_viewport_keeps_last_line() {
    let mut harness = ChatWidgetHarness::new();
    harness.push_user_prompt("Run an exec command with verbose output.");

    let mut seq = 0_u64;
    let order = |seq: &mut u64| {
        let order = OrderMeta {
            request_ordinal: 3,
            output_index: Some(0),
            sequence_number: Some(*seq),
        };
        *seq += 1;
        order
    };
    let call_id = "exec-cutoff".to_string();
    let cwd = PathBuf::from("/workspace");

    harness.handle_event(Event {
        id: "exec-begin-cutoff".into(),
        event_seq: 0,
        msg: EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
            call_id: call_id.clone(),
            command: vec!["bash".into(), "-lc".into(), "printf".into()],
            cwd: cwd.clone(),
            parsed_cmd: Vec::new(),
        }),
        order: Some(order(&mut seq)),
    });

    harness.handle_event(Event {
        id: "exec-stdout".into(),
        event_seq: 1,
        msg: EventMsg::ExecCommandOutputDelta(ExecCommandOutputDeltaEvent {
            call_id: call_id.clone(),
            stream: ExecOutputStream::Stdout,
            chunk: ByteBuf::from(format!("line 1\nline 2 {EXEC_SENTINEL}\n").into_bytes()),
        }),
        order: Some(order(&mut seq)),
    });

    harness.handle_event(Event {
        id: "exec-end-cutoff".into(),
        event_seq: 2,
        msg: EventMsg::ExecCommandEnd(ExecCommandEndEvent {
            call_id,
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 0,
            duration: Duration::from_millis(120),
        }),
        order: Some(order(&mut seq)),
    });

    let output = render_chat_widget_to_vt100(&mut harness, 80, 12);
    let metrics = layout_metrics(&harness);
    assert!(
        output.contains(EXEC_SENTINEL),
        "exec sentinel unexpectedly missing; clipping reproduced"
    );
    assert_eq!(metrics.scroll_offset, 0);
}

#[test]
fn browser_history_small_viewport_keeps_last_line() {
    let mut harness = ChatWidgetHarness::new();
    harness.enable_context_ui();
    harness.push_user_prompt("Summarise browsing results.");

    let snapshot = json!({
        "url": "https://example.com",
        "title": "Example Domain",
        "captured_at": "2025-11-05T12:00:00Z",
        "viewport": {"width": 1280, "height": 720},
        "metadata": {"browser_type": "chromium"}
    });

    harness.handle_event(Event {
        id: "browser-1".into(),
        event_seq: 0,
        msg: EventMsg::BrowserSnapshot(BrowserSnapshotEvent {
            snapshot,
            url: Some("https://example.com".into()),
            captured_at: Some("2025-11-05T12:00:00Z".into()),
        }),
        order: Some(OrderMeta {
            request_ordinal: 4,
            output_index: Some(0),
            sequence_number: Some(0),
        }),
    });

    harness.push_assistant_markdown(format!("Browser summary complete. {BROWSER_SENTINEL}"));

    let output = render_chat_widget_to_vt100(&mut harness, 80, 12);
    let metrics = layout_metrics(&harness);
    assert!(
        output.contains(BROWSER_SENTINEL),
        "browser sentinel should remain visible in a compact viewport"
    );
    assert_eq!(metrics.scroll_offset, 0);
}

#[test]
fn tiny_viewport_history_stays_visible() {
    let mut harness = ChatWidgetHarness::new();
    seed_plain_transcript(&mut harness);

    let output = render_chat_widget_to_vt100(&mut harness, 50, 10);
    assert!(
        output.contains(PLAIN_SENTINEL),
        "tiny viewport should still show the final history line"
    );
}
