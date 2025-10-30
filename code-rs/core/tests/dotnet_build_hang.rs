#![cfg(unix)]

use std::sync::Arc;
use std::time::Duration;

use code_core::exec_command::{ExecCommandParams, ExecSessionManager};
use serde_json::json;
use tokio::time::timeout;

fn make_params(script: &str) -> ExecCommandParams {
    serde_json::from_value(json!({
        "cmd": script,
        "yield_time_ms": 1_000u64,
        "max_output_tokens": 1_000u64,
        "shell": "/bin/bash",
        "login": true,
    }))
    .expect("deserialize ExecCommandParams")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn kill_all_unblocks_hanging_exec() {
    let manager = Arc::new(ExecSessionManager::default());
    let script = r#"python3 -u - <<'PY'
import sys
import time
while True:
    print("tick", flush=True)
    time.sleep(0.05)
PY"#;

    let params = make_params(script);
    let manager_clone = manager.clone();
    let exec_task = tokio::spawn(async move {
        manager_clone.handle_exec_command_request(params).await
    });

    tokio::time::sleep(Duration::from_millis(200)).await;
    manager.kill_all().await;

    let result = timeout(Duration::from_secs(2), exec_task)
        .await
        .expect("exec task should finish after kill_all");

    let output = result.expect("exec task join");
    assert!(output.is_ok(), "exec request should return Ok even after kill");
}
