mod common;

use anyhow::Context;
use codex_build_info::BuildInfo;
use codex_build_info::build_id;
use codex_exec_server::EnvironmentInfo;
use codex_exec_server::InitializeParams;
use codex_exec_server::InitializeResponse;
use codex_exec_server_protocol::JSONRPCError;
use codex_exec_server_protocol::JSONRPCErrorError;
use codex_exec_server_protocol::JSONRPCMessage;
use codex_exec_server_protocol::JSONRPCResponse;
use common::TEST_BUILD_COMMIT;
use common::exec_server::ExecServerHarness;
use common::exec_server::exec_server_with_env;
use pretty_assertions::assert_eq;
use tempfile::TempDir;
use tokio::process::Command;
use uuid::Uuid;

#[test_case::test_case(Some("1.2.3-alpha.4"); "packaged")]
#[test_case::test_case(None; "without_manifest")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exec_server_accepts_initialize(version: Option<&str>) -> anyhow::Result<()> {
    let package = TempDir::new()?;
    let bin_dir = package.path().join("bin");
    std::fs::create_dir(&bin_dir)?;
    let executable = bin_dir.join(format!("codex{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(std::env::current_exe()?, &executable)?;
    let manifest = package.path().join("codex-package.json");
    if let Some(version) = version {
        std::fs::write(
            &manifest,
            serde_json::to_vec(&serde_json::json!({ "version": version }))?,
        )?;
    }

    let mut command = Command::new(&executable);
    command.args(["exec-server", "--listen", "ws://127.0.0.1:0"]);
    // Runtime environment variables cannot replace the executable's build stamp.
    command.envs([
        (
            "STABLE_GIT_COMMIT",
            "ffffffffffffffffffffffffffffffffffffffff",
        ),
        ("GITHUB_SHA", "ffffffffffffffffffffffffffffffffffffffff"),
        ("CODEX_BUILD_TARGET", "runtime-override"),
    ]);
    let mut server = ExecServerHarness::start(command).await?;

    // Updates after startup cannot change the advertised release version.
    std::fs::write(&manifest, r#"{"version":"9.9.9"}"#)?;
    let initialize_id = server
        .send_request(
            "initialize",
            serde_json::to_value(InitializeParams {
                client_name: "exec-server-test".to_string(),
                resume_session_id: None,
            })?,
        )
        .await?;

    let response = server.next_event().await?;
    let JSONRPCMessage::Response(JSONRPCResponse { id, result }) = response else {
        panic!("expected initialize response");
    };
    assert_eq!(id, initialize_id);
    let initialize_response: InitializeResponse = serde_json::from_value(result)?;
    Uuid::parse_str(&initialize_response.session_id)?;
    let mut expected_environment = EnvironmentInfo::local();
    expected_environment.executor_version = version.unwrap_or("0.0.0").to_string();
    let build_info = BuildInfo::get();
    let target = build_info
        .target()
        .context("the test binary has a compiled target")?;
    expected_environment.provider_id = build_id(TEST_BUILD_COMMIT, target);
    assert!(expected_environment.provider_id.is_some());
    assert_eq!(
        initialize_response.environment_info,
        Some(expected_environment.clone())
    );

    server
        .send_notification("initialized", serde_json::json!({}))
        .await?;
    std::fs::remove_file(&manifest)?;
    let environment_id = server
        .send_request("environment/info", serde_json::json!({}))
        .await?;
    let JSONRPCMessage::Response(JSONRPCResponse { id, result }) = server.next_event().await?
    else {
        panic!("expected environment info response");
    };
    assert_eq!(id, environment_id);
    assert_eq!(
        serde_json::from_value::<EnvironmentInfo>(result)?,
        expected_environment
    );

    server.shutdown().await?;
    Ok(())
}

/// Requests retain their wire-order initialization errors even when later handshake messages are pipelined.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exec_server_rejects_pipelined_requests_before_initialized() -> anyhow::Result<()> {
    let mut server = exec_server_with_env(
        std::iter::empty::<(&str, &str)>(),
        &["--concurrent-requests", "32"],
    )
    .await?;
    let before_initialize_id = server
        .send_request("environment/info", serde_json::json!({}))
        .await?;
    let initialize_id = server
        .send_request(
            "initialize",
            serde_json::to_value(InitializeParams {
                client_name: "exec-server-test".to_string(),
                resume_session_id: None,
            })?,
        )
        .await?;

    assert_eq!(
        server.next_event().await?,
        JSONRPCMessage::Error(JSONRPCError {
            id: before_initialize_id,
            error: JSONRPCErrorError {
                code: -32600,
                data: None,
                message: "client must call initialize before using environment info methods"
                    .to_string(),
            },
        })
    );
    let JSONRPCMessage::Response(JSONRPCResponse { id, .. }) = server.next_event().await? else {
        panic!("expected initialize response");
    };
    assert_eq!(id, initialize_id);

    let before_initialized_id = server
        .send_request("environment/info", serde_json::json!({}))
        .await?;
    server
        .send_notification("initialized", serde_json::json!({}))
        .await?;
    assert_eq!(
        server.next_event().await?,
        JSONRPCMessage::Error(JSONRPCError {
            id: before_initialized_id,
            error: JSONRPCErrorError {
                code: -32600,
                data: None,
                message: "client must send initialized before using environment info methods"
                    .to_string(),
            },
        })
    );

    server.shutdown().await?;
    Ok(())
}
