use std::collections::HashMap;
use std::path::PathBuf;

use codex_core::config_types::{McpServerConfig, McpServerTransportConfig};
use codex_core::mcp_connection_manager::McpConnectionManager;
use std::time::Duration;

fn server_bin_path() -> PathBuf {
    // Compute workspace root from this crate dir
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let mut root = PathBuf::from(manifest_dir);
    // codex-rs/core -> codex-rs
    root.pop();
    // prefer debug profile location for tests
    let mut p = root;
    p.push("target");
    p.push("debug");
    #[cfg(windows)]
    p.push("codex-mcp-test-server.exe");
    #[cfg(not(windows))]
    p.push("codex-mcp-test-server");
    p
}

#[tokio::test]
async fn mcp_manager_skips_slow_server_on_timeout() {
    // Ensure test server binary exists
    let server = server_bin_path();
    assert!(
        server.exists(),
        "expected test server at {}",
        server.display()
    );

    // Slow server exceeds timeout (init/list 200ms vs 100ms timeout)
    let slow_cfg = McpServerConfig {
        transport: McpServerTransportConfig::Stdio {
            command: "bash".to_string(),
            args: vec![
                "-lc".to_string(),
                format!("SLOW_INIT_MS=200 SLOW_LIST_MS=200 {}", server.display()),
            ],
            env: None,
        },
        startup_timeout_sec: Some(Duration::from_millis(100)),
        tool_timeout_sec: None,
    };
    // Fast server responds immediately
    let fast_cfg = McpServerConfig {
        transport: McpServerTransportConfig::Stdio {
            command: server.to_string_lossy().to_string(),
            args: vec![],
            env: None,
        },
        startup_timeout_sec: Some(Duration::from_millis(500)),
        tool_timeout_sec: None,
    };

    let mut servers = HashMap::new();
    servers.insert("slow".to_string(), slow_cfg);
    servers.insert("fast".to_string(), fast_cfg);

    let (mgr, errs) = McpConnectionManager::new(servers, false, std::collections::HashSet::new())
        .await
        .expect("manager creation should not fail entirely");

    // Slow should be reported as error; fast should be available.
    assert!(errs.contains_key("slow"));
    assert!(!errs.contains_key("fast"));

    let tools = mgr.list_all_tools();
    // Expect tool echo from fast server only: qualified name fast__echo
    assert!(tools.keys().any(|k| k.starts_with("fast__")));
    assert!(!tools.keys().any(|k| k.starts_with("slow__")));
}

#[tokio::test]
async fn mcp_manager_respects_extended_startup_timeout() {
    // Ensure test server binary exists
    let server = server_bin_path();
    assert!(
        server.exists(),
        "expected test server at {}",
        server.display()
    );

    // Slow server within extended timeout (init/list 200ms vs 500ms)
    let slow_ok = McpServerConfig {
        transport: McpServerTransportConfig::Stdio {
            command: "bash".to_string(),
            args: vec![
                "-lc".to_string(),
                format!("SLOW_INIT_MS=200 SLOW_LIST_MS=200 {}", server.display()),
            ],
            env: None,
        },
        startup_timeout_sec: Some(Duration::from_millis(500)),
        tool_timeout_sec: None,
    };
    let mut servers = HashMap::new();
    servers.insert("slow_ok".to_string(), slow_ok);

    let (mgr, errs) = McpConnectionManager::new(servers, false, std::collections::HashSet::new())
        .await
        .expect("manager creation should not fail");

    assert!(errs.is_empty(), "no errors expected, got: {errs:?}");
    let tools = mgr.list_all_tools();
    assert!(tools.keys().any(|k| k.starts_with("slow_ok__")));
}
