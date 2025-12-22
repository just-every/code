//! Regression test for #333: MCP stdio servers should tear down before a new session starts.
//!
//! Ensures the `/new` flow tears down the previous `McpConnectionManager`
//! before spawning the next session so stdio servers exit cleanly.

use std::collections::{HashMap, HashSet};
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use code_core::config::{Config, ConfigOverrides, ConfigToml};
use code_core::config_types::{McpServerConfig, McpServerTransportConfig};
use code_core::mcp_connection_manager::McpConnectionManager;

const WRAPPER_SOURCE: &str = r#"
use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::process::{Command, Stdio};

fn main() {
    let log_path = env::var("MCP_STUB_LOG").expect("MCP_STUB_LOG");
    let target = env::var("MCP_STUB_BINARY").expect("MCP_STUB_BINARY");

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .expect("open log");
    writeln!(file, "spawn:{}", std::process::id()).expect("write spawn");

    let status = Command::new(target)
        .args(env::args().skip(1))
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .expect("spawn target");

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .expect("reopen log");
    writeln!(file, "exit:{}", std::process::id()).expect("write exit");

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
}
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mcp_stdio_server_exits_before_next_session() {
    let temp = tempfile::tempdir().expect("tempdir");
    let temp_path = temp.path();

    let wrapper_src = temp_path.join("wrapper.rs");
    std::fs::write(&wrapper_src, WRAPPER_SOURCE).expect("write wrapper source");

    let mut wrapper_bin = temp_path.join("mcp-wrapper");
    if cfg!(windows) {
        wrapper_bin = wrapper_bin.with_extension("exe");
    }

    let rustc = env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let status = Command::new(rustc)
        .arg("--edition=2021")
        .arg(&wrapper_src)
        .arg("-o")
        .arg(&wrapper_bin)
        .status()
        .expect("compile wrapper");
    assert!(status.success(), "failed to compile wrapper");

    let log_path = temp_path.join("stub.log");
    let mcp_binary = std::env::var("CARGO_BIN_EXE_mcp-test-server")
        .or_else(|_| std::env::var("MCP_TEST_SERVER"));
    let mcp_binary = match mcp_binary {
        Ok(path) => path,
        Err(_) => {
            println!(
                "skipping mcp_session_cleanup: CARGO_BIN_EXE_mcp-test-server (or MCP_TEST_SERVER) not set"
            );
            return;
        }
    };

    let mut env_map = HashMap::new();
    env_map.insert(
        "MCP_STUB_BINARY".to_string(),
        PathBuf::from(mcp_binary).to_string_lossy().into_owned(),
    );
    env_map.insert(
        "MCP_STUB_LOG".to_string(),
        log_path.to_string_lossy().into_owned(),
    );

    let server_cfg = McpServerConfig {
        transport: McpServerTransportConfig::Stdio {
            command: wrapper_bin.to_string_lossy().into_owned(),
            args: Vec::new(),
            env: Some(env_map.clone()),
        },
        startup_timeout_sec: Some(Duration::from_secs(5)),
        tool_timeout_sec: Some(Duration::from_secs(5)),
    };

    let mut mcp_servers = HashMap::new();
    mcp_servers.insert("stub".to_string(), server_cfg.clone());

    let mut overrides = ConfigOverrides::default();
    overrides.mcp_servers = Some(mcp_servers);

    let config = Config::load_from_base_config_with_overrides(
        ConfigToml::default(),
        overrides,
        temp_path.to_path_buf(),
    )
    .expect("load config");

    let servers_map = config.mcp_servers.clone();

    let (manager1, _) = McpConnectionManager::new(servers_map.clone(), HashSet::new())
    .await
    .expect("start first MCP manager");

    wait_for_spawn_count(&log_path, 1).await;

    manager1.shutdown_all().await;
    drop(manager1);

    let first_pid = latest_spawn_pid(&log_path).expect("first pid");

    assert!(
        wait_for_exit(&log_path, first_pid, Duration::from_secs(2)).await,
        "expected first MCP server to exit before spawning a new session. log contents:\n{}",
        std::fs::read_to_string(&log_path).unwrap_or_default()
    );

    let (_manager2, _) = McpConnectionManager::new(servers_map, HashSet::new())
        .await
        .expect("start second MCP manager");
}

fn parse_log(log: &str, prefix: &str) -> Vec<u32> {
    log.lines()
        .filter_map(|line| line.strip_prefix(prefix))
        .filter_map(|rest| rest.trim().parse::<u32>().ok())
        .collect()
}

async fn wait_for_spawn_count(path: &Path, expected: usize) {
    for _ in 0..40 {
        if parse_log(&read_log(path), "spawn:").len() >= expected {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("timed out waiting for {expected} spawn entries; log: {}", read_log(path));
}

async fn wait_for_exit(path: &Path, pid: u32, timeout: Duration) -> bool {
    let mut elapsed = Duration::from_millis(0);
    while elapsed <= timeout {
        if parse_log(&read_log(path), "exit:").contains(&pid) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        elapsed += Duration::from_millis(50);
    }
    false
}

fn latest_spawn_pid(path: &Path) -> Option<u32> {
    parse_log(&read_log(path), "spawn:").last().copied()
}

fn read_log(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}
