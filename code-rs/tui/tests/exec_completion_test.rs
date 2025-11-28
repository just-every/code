use code_core::protocol::{
    Event,
    EventMsg,
    ExecCommandBeginEvent,
    ExecCommandEndEvent,
    OrderMeta,
};
use code_tui::test_helpers::{render_chat_widget_to_vt100, ChatWidgetHarness};
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
fn exec_command_completes_properly() {
    let mut harness = ChatWidgetHarness::new();
    let mut seq = 0_u64;
    let call_id = "call_test";
    let cwd = PathBuf::from("/tmp");

    harness.handle_event(Event {
        id: "exec-begin".to_string(),
        event_seq: 0,
        msg: EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
            call_id: call_id.to_string(),
            command: vec!["ls".into(), "-la".into()],
            cwd: cwd.clone(),
            parsed_cmd: Vec::new(),
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });

    let before = render_chat_widget_to_vt100(&mut harness, 80, 12);
    assert!(
        before.contains("Running") || before.contains("ls"),
        "exec cell should show running state before end, output:\n{}",
        before
    );

    harness.handle_event(Event {
        id: "exec-end".to_string(),
        event_seq: 1,
        msg: EventMsg::ExecCommandEnd(ExecCommandEndEvent {
            call_id: call_id.to_string(),
            stdout: "total 0\ndrwxr-xr-x  2 user user 4096 Nov 28 00:00 .\n".to_string(),
            stderr: String::new(),
            exit_code: 0,
            duration: Duration::from_millis(50),
        }),
        order: Some(next_order_meta(1, &mut seq)),
    });

    let after = render_chat_widget_to_vt100(&mut harness, 80, 14);
    assert!(
        !after.contains("Running..."),
        "exec cell should NOT show 'Running...' after completion, output:\n{}",
        after
    );
    assert!(
        after.contains("total") || after.contains("drwx"),
        "exec cell should show output after completion, output:\n{}",
        after
    );
}
