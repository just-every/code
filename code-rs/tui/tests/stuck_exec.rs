use code_core::protocol::{
    Event,
    EventMsg,
    ExecCommandBeginEvent,
    ExecCommandEndEvent,
    TaskCompleteEvent,
    McpInvocation,
    McpToolCallBeginEvent,
    OrderMeta,
    PatchApplyBeginEvent,
    PatchApplyEndEvent,
};
use code_tui::test_helpers::{render_chat_widget_to_vt100, ChatWidgetHarness};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

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
    use code_core::protocol::AgentMessageEvent;

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
