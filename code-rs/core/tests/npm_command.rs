#![cfg(unix)]

use std::fs;
use std::process::Command;
use std::time::Duration;

use code_core::{result_into_payload, ExecCommandParams, ExecSessionManager};
use serde_json::json;
use tempfile::tempdir;
use tokio::time::timeout;

fn make_params(cmd: &str, cwd: Option<&std::path::Path>) -> ExecCommandParams {
    let mut value = json!({
        "cmd": cmd,
        "yield_time_ms": 10_000u64,
        "max_output_tokens": 10_000u64,
        "shell": "/bin/bash",
        "login": true
    });

    if let Some(dir) = cwd {
        value["cmd"] = json!(format!("cd {} && {cmd}", dir.display()));
    }

    serde_json::from_value(value).expect("deserialize ExecCommandParams")
}

fn npm_available() -> bool {
    match Command::new("npm").arg("--version").output() {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn npm_version_executes() {
    if !npm_available() {
        eprintln!("skipping npm_version_executes: npm not available");
        return;
    }

    let manager = ExecSessionManager::default();
    let params = make_params("npm --version", None);

    let summary = manager
        .handle_exec_command_request(params)
        .await
        .map(|output| result_into_payload(Ok(output)))
        .expect("exec request should succeed");

    assert_eq!(summary.success, Some(true));
    assert!(
        !summary.content.to_lowercase().contains("not found"),
        "npm --version output should not indicate a missing binary"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn npm_init_creates_package_json() {
    if !npm_available() {
        eprintln!("skipping npm_init_creates_package_json: npm not available");
        return;
    }

    let temp = tempdir().expect("create temp dir");
    let workspace_root = temp.path();
    let workspace = workspace_root.join("npm-workspace");
    fs::create_dir(&workspace).expect("create npm workspace");

    let manager = ExecSessionManager::default();
    let params = make_params("npm init -y", Some(workspace.as_path()));

    let exec_future = manager.handle_exec_command_request(params);
    let summary = timeout(Duration::from_secs(30), exec_future)
        .await
        .expect("npm init should complete within timeout")
        .map(|output| result_into_payload(Ok(output)))
        .expect("exec request should succeed");

    assert_eq!(summary.success, Some(true));
    let package_json = workspace.join("package.json");
    assert!(package_json.exists(), "npm init should create package.json");
}
