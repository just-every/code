use code_core::protocol::{
    AgentMessageEvent,
    AgentMessageDeltaEvent,
    CustomToolCallBeginEvent,
    CustomToolCallEndEvent,
    Event,
    EventMsg,
    ErrorEvent,
    ExecCommandBeginEvent,
    ExecCommandEndEvent,
    TaskCompleteEvent,
    AgentReasoningDeltaEvent,
    McpInvocation,
    McpToolCallBeginEvent,
    OrderMeta,
    PatchApplyBeginEvent,
    PatchApplyEndEvent,
};
use code_core::parse_command::ParsedCommand as CoreParsedCommand;
use code_tui::test_helpers::{render_chat_widget_to_vt100, ChatWidgetHarness};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use std::time::Instant;

fn next_order_meta(request_ordinal: u64, seq: &mut u64) -> OrderMeta {
    let order = OrderMeta {
        request_ordinal,
        output_index: Some(0),
        sequence_number: Some(*seq),
    };
    *seq += 1;
    order
}

#[test]
fn exec_cell_clears_after_patch_flow() {
    let mut harness = ChatWidgetHarness::new();
    let mut seq = 0_u64;
    let call_id = "call_bug";
    let cwd = PathBuf::from("/tmp");

    harness.handle_event(Event {
        id: "exec-begin".to_string(),
        event_seq: 0,
        msg: EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
            call_id: call_id.to_string(),
            command: vec!["bash".into(), "-lc".into(), "apply_patch".into()],
            cwd: cwd.clone(),
            parsed_cmd: Vec::new(),
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });

    harness.handle_event(Event {
        id: "patch-begin".to_string(),
        event_seq: 0,
        msg: EventMsg::PatchApplyBegin(PatchApplyBeginEvent {
            call_id: call_id.to_string(),
            auto_approved: true,
            changes: HashMap::new(),
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });

    harness.handle_event(Event {
        id: "exec-end".to_string(),
        event_seq: 1,
        msg: EventMsg::ExecCommandEnd(ExecCommandEndEvent {
            call_id: call_id.to_string(),
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 0,
            duration: Duration::from_millis(50),
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });

    harness.handle_event(Event {
        id: "patch-end".to_string(),
        event_seq: 1,
        msg: EventMsg::PatchApplyEnd(PatchApplyEndEvent {
            call_id: call_id.to_string(),
            stdout: "Success".into(),
            stderr: String::new(),
            success: true,
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });

    let output = render_chat_widget_to_vt100(&mut harness, 80, 12);
    assert!(
        !output.contains("Running"),
        "exec cell should not remain running after patch apply:\n{}",
        output
    );
}

#[test]
fn exec_spinner_clears_after_final_answer() {
    let mut harness = ChatWidgetHarness::new();
    let mut seq = 0_u64;
    let call_id = "call_spinner".to_string();
    let cwd = PathBuf::from("/tmp");

    harness.handle_event(Event {
        id: "exec-begin-spinner".to_string(),
        event_seq: 0,
        msg: EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
            call_id: call_id.clone(),
            command: vec!["bash".into(), "-lc".into(), "echo running".into()],
            cwd: cwd.clone(),
            parsed_cmd: Vec::new(),
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });

    harness.handle_event(Event {
        id: "answer-final".to_string(),
        event_seq: 1,
        msg: EventMsg::AgentMessage(AgentMessageEvent {
            message: "All done.".into(),
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });

    let output = render_chat_widget_to_vt100(&mut harness, 80, 12);
    assert!(
        !output.contains("running command"),
        "spinner should clear after final answer, but output was:\n{}",
        output
    );
}

#[test]
fn exec_cell_clears_after_task_started_final_answer_without_task_complete() {
    let mut harness = ChatWidgetHarness::new();
    let mut seq = 0_u64;
    let call_id = "call_final".to_string();
    let cwd = PathBuf::from("/tmp");

    harness.handle_event(Event {
        id: "task-start".to_string(),
        event_seq: 0,
        msg: EventMsg::TaskStarted,
        order: Some(next_order_meta(1, &mut seq)),
    });

    harness.handle_event(Event {
        id: "exec-begin".to_string(),
        event_seq: 1,
        msg: EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
            call_id: call_id.clone(),
            command: vec!["bash".into(), "-lc".into(), "echo pending".into()],
            cwd: cwd.clone(),
            parsed_cmd: Vec::new(),
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });

    harness.handle_event(Event {
        id: "answer-final".to_string(),
        event_seq: 2,
        msg: EventMsg::AgentMessage(AgentMessageEvent {
            message: "done".into(),
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });

    let output = render_chat_widget_to_vt100(&mut harness, 80, 14);
    assert!(
        !output.contains("Running"),
        "exec should finalize after final answer even without TaskComplete:\n{}",
        output
    );
    assert!(
        output.contains("done"),
        "final answer should be visible in output:\n{}",
        output
    );
}

#[test]
fn synthetic_end_clears_cancelled_exec_spinner() {
    let mut harness = ChatWidgetHarness::new();
    let mut seq = 0_u64;
    let call_id = "call_cancel".to_string();
    let cwd = PathBuf::from("/tmp");
    let sub_id = "exec-cancel".to_string();

    harness.handle_event(Event {
        id: sub_id.clone(),
        event_seq: 0,
        msg: EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
            call_id: call_id.clone(),
            command: vec!["bash".into(), "-lc".into(), "sleep 5".into()],
            cwd: cwd.clone(),
            parsed_cmd: Vec::new(),
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });

    let before = render_chat_widget_to_vt100(&mut harness, 80, 12);
    assert!(
        before.contains("sleep 5"),
        "exec cell should include command before synthetic end, output:\n{}",
        before
    );
    assert!(
        !before.contains("Command cancelled by user."),
        "cancellation details should not appear before synthetic end, output:\n{}",
        before
    );

    harness.handle_event(Event {
        id: sub_id.clone(),
        event_seq: 1,
        msg: EventMsg::ExecCommandEnd(ExecCommandEndEvent {
            call_id: call_id,
            stdout: String::new(),
            stderr: "Command cancelled by user.".to_string(),
            exit_code: 130,
            duration: Duration::ZERO,
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });

    let after = render_chat_widget_to_vt100(&mut harness, 80, 12);
    assert!(
        after.contains("✖") || after.contains("exit code"),
        "synthetic end should mark the exec as finished:\n{}",
        after
    );
    assert!(
        after.contains("Command cancelled by user."),
        "expected cancellation context in output, got:\n{}",
        after
    );
}

#[test]
fn wait_tool_missing_background_job_clears_exec_wait() {
    let mut harness = ChatWidgetHarness::new();
    let mut seq = 0_u64;
    let exec_call_id = "call_wait_missing";
    let wait_call_id = "wait-1";
    let cwd = PathBuf::from("/tmp");

    harness.handle_event(Event {
        id: "exec-begin".to_string(),
        event_seq: 0,
        msg: EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
            call_id: exec_call_id.to_string(),
            command: vec!["bash".into(), "-lc".into(), "gh run watch".into()],
            cwd: cwd.clone(),
            parsed_cmd: Vec::new(),
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });

    harness.handle_event(Event {
        id: "wait-begin".to_string(),
        event_seq: 1,
        msg: EventMsg::CustomToolCallBegin(CustomToolCallBeginEvent {
            call_id: wait_call_id.to_string(),
            tool_name: "wait".to_string(),
            parameters: Some(serde_json::json!({"call_id": exec_call_id})),
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });

    harness.handle_event(Event {
        id: "wait-end".to_string(),
        event_seq: 2,
        msg: EventMsg::CustomToolCallEnd(CustomToolCallEndEvent {
            call_id: wait_call_id.to_string(),
            tool_name: "wait".to_string(),
            parameters: Some(serde_json::json!({"call_id": exec_call_id})),
            duration: Duration::from_secs(5),
            result: Err(format!(
                "No background job found for call_id={exec_call_id}"
            )),
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });

    let output = render_chat_widget_to_vt100(&mut harness, 80, 14);
    assert!(
        !output.contains("Waiting..."),
        "wait status should clear when background job is missing:\n{}",
        output
    );
    assert!(
        output.contains("No background job found for call_id=call_wait_missing"),
        "missing-job note should be visible:\n{}",
        output
    );
}

#[test]
fn wait_interrupt_after_exec_end_does_not_mutate_exec() {
    let mut harness = ChatWidgetHarness::new();
    let mut seq = 0_u64;
    let exec_call_id = "call_wait_interrupt";
    let wait_call_id = "wait-2";
    let cwd = PathBuf::from("/tmp");

    harness.handle_event(Event {
        id: "exec-begin".to_string(),
        event_seq: 0,
        msg: EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
            call_id: exec_call_id.to_string(),
            command: vec!["bash".into(), "-lc".into(), "echo done".into()],
            cwd: cwd.clone(),
            parsed_cmd: Vec::new(),
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });

    harness.handle_event(Event {
        id: "wait-begin".to_string(),
        event_seq: 1,
        msg: EventMsg::CustomToolCallBegin(CustomToolCallBeginEvent {
            call_id: wait_call_id.to_string(),
            tool_name: "wait".to_string(),
            parameters: Some(serde_json::json!({"call_id": exec_call_id})),
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });

    harness.handle_event(Event {
        id: "exec-end".to_string(),
        event_seq: 2,
        msg: EventMsg::ExecCommandEnd(ExecCommandEndEvent {
            call_id: exec_call_id.to_string(),
            stdout: "done".to_string(),
            stderr: String::new(),
            exit_code: 0,
            duration: Duration::from_millis(5),
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });

    harness.handle_event(Event {
        id: "wait-end".to_string(),
        event_seq: 3,
        msg: EventMsg::CustomToolCallEnd(CustomToolCallEndEvent {
            call_id: wait_call_id.to_string(),
            tool_name: "wait".to_string(),
            parameters: Some(serde_json::json!({"call_id": exec_call_id})),
            duration: Duration::from_secs(1),
            result: Err(format!(
                "wait ended due to new user message (background job {exec_call_id} still running)"
            )),
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });

    let output = render_chat_widget_to_vt100(&mut harness, 80, 14);
    assert!(
        output.contains("done"),
        "exec output should remain visible:\n{}",
        output
    );
    assert!(
        !output.contains("wait ended due to new user message"),
        "wait interruption text should not overwrite completed exec:\n{}",
        output
    );
}

#[test]
fn wait_missing_job_skips_merged_exec() {
    let mut harness = ChatWidgetHarness::new();
    let mut seq = 0_u64;
    let exec_call_id = "call_wait_merge_a";
    let exec_call_id_b = "call_wait_merge_b";
    let wait_call_id = "wait-merge";
    let cwd = PathBuf::from("/tmp");
    let parsed_search = vec![CoreParsedCommand::Search {
        cmd: "rg foo".to_string(),
        query: Some("foo".to_string()),
        path: Some(".".to_string()),
    }];

    harness.handle_event(Event {
        id: "exec-begin".to_string(),
        event_seq: 0,
        msg: EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
            call_id: exec_call_id.to_string(),
            command: vec!["rg".into(), "foo".into(), ".".into()],
            cwd: cwd.clone(),
            parsed_cmd: parsed_search.clone(),
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });

    harness.handle_event(Event {
        id: "wait-begin".to_string(),
        event_seq: 1,
        msg: EventMsg::CustomToolCallBegin(CustomToolCallBeginEvent {
            call_id: wait_call_id.to_string(),
            tool_name: "wait".to_string(),
            parameters: Some(serde_json::json!({"call_id": exec_call_id})),
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });

    harness.handle_event(Event {
        id: "exec-end".to_string(),
        event_seq: 2,
        msg: EventMsg::ExecCommandEnd(ExecCommandEndEvent {
            call_id: exec_call_id.to_string(),
            stdout: "match-1".to_string(),
            stderr: String::new(),
            exit_code: 0,
            duration: Duration::from_millis(5),
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });

    harness.handle_event(Event {
        id: "exec-begin-2".to_string(),
        event_seq: 3,
        msg: EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
            call_id: exec_call_id_b.to_string(),
            command: vec!["rg".into(), "foo".into(), ".".into()],
            cwd: cwd.clone(),
            parsed_cmd: parsed_search,
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });

    harness.handle_event(Event {
        id: "exec-end-2".to_string(),
        event_seq: 4,
        msg: EventMsg::ExecCommandEnd(ExecCommandEndEvent {
            call_id: exec_call_id_b.to_string(),
            stdout: "match-2".to_string(),
            stderr: String::new(),
            exit_code: 0,
            duration: Duration::from_millis(5),
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });

    harness.handle_event(Event {
        id: "wait-end".to_string(),
        event_seq: 5,
        msg: EventMsg::CustomToolCallEnd(CustomToolCallEndEvent {
            call_id: wait_call_id.to_string(),
            tool_name: "wait".to_string(),
            parameters: Some(serde_json::json!({"call_id": exec_call_id})),
            duration: Duration::from_secs(1),
            result: Err(format!(
                "No background job found for call_id={exec_call_id}"
            )),
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });

    let output = render_chat_widget_to_vt100(&mut harness, 80, 16);
    let search_count = output.matches("Search foo in /tmp/.").count();
    assert!(
        search_count >= 2,
        "merged exec should retain both search entries:\n{}",
        output
    );
    assert!(
        !output.contains("No background job found"),
        "wait error should not overwrite merged exec:\n{}",
        output
    );
}

#[test]
fn exec_begin_upgrades_running_tool_cell() {
    let mut harness = ChatWidgetHarness::new();
    let mut seq = 0_u64;
    let call_id = "call_coalesce".to_string();
    let cwd = PathBuf::from("/tmp");

    harness.handle_event(Event {
        id: "mcp-begin".to_string(),
        event_seq: 0,
        msg: EventMsg::McpToolCallBegin(McpToolCallBeginEvent {
            call_id: call_id.clone(),
            invocation: McpInvocation {
                server: "demo".to_string(),
                tool: "run_command".to_string(),
                arguments: None,
            },
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });

    harness.handle_event(Event {
        id: "exec-begin-coalesce".to_string(),
        event_seq: 1,
        msg: EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
            call_id: call_id.clone(),
            command: vec!["bash".into(), "-lc".into(), "echo upgraded".into()],
            cwd: cwd.clone(),
            parsed_cmd: Vec::new(),
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });

    harness.handle_event(Event {
        id: "exec-end-coalesce".to_string(),
        event_seq: 2,
        msg: EventMsg::ExecCommandEnd(ExecCommandEndEvent {
            call_id: call_id,
            stdout: "upgraded\n".into(),
            stderr: String::new(),
            exit_code: 0,
            duration: Duration::from_millis(5),
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });

    let output = render_chat_widget_to_vt100(&mut harness, 80, 12);
    assert!(
        !output.contains("Working..."),
        "running tool spinner should be upgraded to an exec cell:\n{output}",
    );
    let command_occurrences = output.matches("echo upgraded").count();
    assert_eq!(
        command_occurrences, 1,
        "expected exactly one exec command row after upgrade:\n{output}",
    );
    assert!(
        output.contains("upgraded"),
        "exec output should remain attached to the upgraded cell:\n{output}",
    );
}

#[test]
fn stale_exec_is_finalized_on_task_complete() {
    let mut harness = ChatWidgetHarness::new();
    let mut seq = 0_u64;
    let call_id = "call_stale".to_string();
    let cwd = PathBuf::from("/tmp");

    // Begin a command but never send ExecCommandEnd.
    harness.handle_event(Event {
        id: "exec-begin-stale".to_string(),
        event_seq: 0,
        msg: EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
            call_id: call_id.clone(),
            command: vec!["bash".into(), "-lc".into(), "git diff".into()],
            cwd: cwd.clone(),
            parsed_cmd: Vec::new(),
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });

    // Simulate the backend finishing the turn without ever emitting ExecEnd.
    harness.handle_event(Event {
        id: "task-complete".to_string(),
        event_seq: 1,
        msg: EventMsg::TaskComplete(TaskCompleteEvent {
            last_agent_message: None,
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });

    let output = render_chat_widget_to_vt100(&mut harness, 80, 14);
    assert!(
        output.contains("background") || output.contains("turn end"),
        "stale exec should surface a clear completion notice:\n{}",
        output
    );
}

#[test]
fn exec_interrupts_flush_when_stream_idles() {
    let mut harness = ChatWidgetHarness::new();
    let mut seq = 0_u64;
    let call_id = "call_idle".to_string();
    let cwd = PathBuf::from("/tmp");

    // Kick off a reasoning stream so write-cycle is active.
    harness.handle_event(Event {
        id: "reasoning-delta".to_string(),
        event_seq: 0,
        msg: EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent {
            delta: "Thinking through the next steps.\n".into(),
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });
    harness.flush_into_widget();
    // Drain any pending commits so the stream is idle but still marked active.
    for _ in 0..3 {
        harness.drive_commit_tick();
    }

    // Queue an Exec begin; it should not stay deferred once the stream is idle.
    harness.handle_event(Event {
        id: "exec-begin-idle".to_string(),
        event_seq: 1,
        msg: EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
            call_id: call_id.clone(),
            command: vec!["bash".into(), "-lc".into(), "echo idle".into()],
            cwd: cwd.clone(),
            parsed_cmd: Vec::new(),
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });

    // Allow the flush timer to run and deliver the queued interrupt.
    std::thread::sleep(Duration::from_millis(400));
    harness.flush_into_widget();
    // Give the idle-flush a second chance in case of scheduler jitter.
    std::thread::sleep(Duration::from_millis(200));
    harness.flush_into_widget();

    assert!(
        harness.running_exec_call_ids().contains(&call_id),
        "exec begin should flush once the stream is idle"
    );
}

#[test]
fn queued_exec_end_flushes_after_stream_clears() {
    let mut harness = ChatWidgetHarness::new();
    let mut seq = 0_u64;
    let call_id = "call_flush".to_string();
    let cwd = PathBuf::from("/tmp");

    harness.handle_event(Event {
        id: "exec-begin-flush".to_string(),
        event_seq: 0,
        msg: EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
            call_id: call_id.clone(),
            command: vec!["bash".into(), "-lc".into(), "echo queued".into()],
            cwd: cwd.clone(),
            parsed_cmd: Vec::new(),
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });

    // Start a stream so ExecEnd defers into the interrupt queue.
    harness.handle_event(Event {
        id: "answer-delta-active".to_string(),
        event_seq: 1,
        msg: EventMsg::AgentMessageDelta(AgentMessageDeltaEvent {
            delta: "streaming answer".into(),
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });

    // Exec end arrives while the stream is active; it is deferred behind the flush timer.
    harness.handle_event(Event {
        id: "exec-end-flush".to_string(),
        event_seq: 2,
        msg: EventMsg::ExecCommandEnd(ExecCommandEndEvent {
            call_id: call_id.clone(),
            stdout: "queued\n".into(),
            stderr: String::new(),
            exit_code: 0,
            duration: Duration::from_millis(5),
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });

    // Stream finishes before the idle-flush callback fires; pending ExecEnd should still flush.
    harness.force_stream_clear();

    // Give the flush timers enough headroom under nextest parallelism.
    let deadline = Instant::now() + Duration::from_secs(2);
    let output = loop {
        std::thread::sleep(Duration::from_millis(100));
        harness.flush_into_widget();
        let output = render_chat_widget_to_vt100(&mut harness, 80, 14);
        if output.contains("queued") && !output.contains("Running...") {
            break output;
        }
        if Instant::now() >= deadline {
            break output;
        }
    };
    assert!(
        output.contains("queued"),
        "exec output should render after the deferred end is delivered:\n{}",
        output
    );
    assert!(
        !output.contains("Running..."),
        "exec should not stay running after stream clears and the flush timer fires:\n{}",
        output
    );
}

#[test]
fn background_style_exec_end_with_zero_seq_does_not_get_stuck() {
    let mut harness = ChatWidgetHarness::new();
    let mut seq = 0_u64;
    let call_id = "call_zero_seq".to_string();
    let cwd = PathBuf::from("/tmp");

    // Simulate streaming so interrupts queue defers begin/end.
    harness.handle_event(Event {
        id: "reasoning-delta".to_string(),
        event_seq: 0,
        msg: EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent {
            delta: "thinking".into(),
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });

    // Exec begin uses a normal monotonic seq (matches live code path).
    harness.handle_event(Event {
        id: "exec-begin-zero".to_string(),
        event_seq: 1,
        msg: EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
            call_id: call_id.clone(),
            command: vec!["bash".into(), "-lc".into(), "echo bg".into()],
            cwd: cwd.clone(),
            parsed_cmd: Vec::new(),
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });

    // Exec end arrives while stream still active and (like background runner) reports seq 0.
    harness.handle_event(Event {
        id: "exec-end-zero".to_string(),
        event_seq: 0,
        msg: EventMsg::ExecCommandEnd(ExecCommandEndEvent {
            call_id: call_id.clone(),
            stdout: "bg\n".into(),
            stderr: String::new(),
            exit_code: 0,
            duration: Duration::from_millis(5),
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });

    // Stream finishes before the idle-flush callback fires; pending begin/end should still flush.
    harness.force_stream_clear();

    // Flush queued interrupts to deliver begin/end.
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut output;
    loop {
        std::thread::sleep(Duration::from_millis(100));
        harness.flush_into_widget();
        output = render_chat_widget_to_vt100(&mut harness, 80, 12);
        if output.contains("bg") && !output.contains("Running...") {
            break;
        }
        if Instant::now() >= deadline {
            panic!("exec with zero seq end should complete after flush:\n{}", output);
        }
    }
}

#[test]
fn running_exec_is_finalized_when_error_event_arrives() {
    let mut harness = ChatWidgetHarness::new();
    let mut seq = 0_u64;
    let call_id = "call_error".to_string();
    let cwd = PathBuf::from("/tmp");

    harness.handle_event(Event {
        id: "task-start".to_string(),
        event_seq: 0,
        msg: EventMsg::TaskStarted,
        order: Some(next_order_meta(1, &mut seq)),
    });

    harness.handle_event(Event {
        id: "exec-begin-error".to_string(),
        event_seq: 1,
        msg: EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
            call_id: call_id.clone(),
            command: vec!["bash".into(), "-lc".into(), "pgrep something".into()],
            cwd: cwd.clone(),
            parsed_cmd: Vec::new(),
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });

    harness.handle_event(Event {
        id: "fatal-error".to_string(),
        event_seq: 2,
        msg: EventMsg::Error(ErrorEvent {
            message: "fatal: provider crashed".into(),
        }),
        order: None,
    });

    let output = render_chat_widget_to_vt100(&mut harness, 80, 14);
    assert!(
        !output.contains("Running"),
        "exec cell should not linger after fatal error:\n{}",
        output
    );
    assert!(
        output.contains("fatal: provider crashed") || output.contains("Cancelled by user."),
        "error context should be visible after fatal error:\n{}",
        output
    );
}
