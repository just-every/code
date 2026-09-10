//! Best-effort loaded-thread snapshots during managed app-server shutdown.

use super::connection_handling_websocket::DEFAULT_READ_TIMEOUT;
use super::connection_handling_websocket::create_config_toml;
use anyhow::Context;
use anyhow::Result;
use app_test_support::DISABLE_PLUGIN_STARTUP_TASKS_ARG;
use app_test_support::create_final_assistant_message_sse_response;
use codex_app_server_protocol::InitializeCapabilities;
use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ThreadStartResponse;
use codex_app_server_transport::daemon_recovery;
use codex_app_server_transport::daemon_recovery_file_path;
use codex_uds::UnixStream;
use core_test_support::streaming_sse::StreamingSseChunk;
use core_test_support::streaming_sse::start_streaming_sse_server;
use futures::SinkExt;
use futures::StreamExt;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::path::Path;
#[cfg(unix)]
use std::process::Command as StdCommand;
use std::process::Stdio;
use tempfile::TempDir;
use tokio::process::Child;
use tokio::process::Command;
use tokio::sync::oneshot;
use tokio::time::Duration;
use tokio::time::sleep;
use tokio::time::timeout;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::client_async;
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn managed_restart_resumes_loaded_threads_and_goal_without_client() -> Result<()> {
    let home = TempDir::new()?;
    let (release_response, response_gate) = oneshot::channel();
    let (release_recovered, recovered_gate) = oneshot::channel();
    let (mock, _completions) = start_streaming_sse_server(vec![
        vec![stream_chunk(Some(response_gate), "Done")?],
        vec![stream_chunk(Some(recovered_gate), "Recovered")?],
    ])
    .await;
    create_config_toml(home.path(), mock.uri(), "never")?;
    let socket_path = home.path().join("control/server.sock");
    let recovery_file = daemon_recovery_file_path(home.path());
    std::fs::create_dir_all(recovery_file.parent().context("snapshot parent")?)?;
    std::fs::write(&recovery_file, "invalid JSON")?;
    let mut server = spawn_server(home.path(), &socket_path)?;
    let mut client = connect_daemon_client(
        &socket_path,
        InitializeCapabilities {
            experimental_api: true,
            extensions: Some(std::collections::HashMap::from([(
                "io.modelcontextprotocol/ui".into(),
                json!({"mimeTypes":["text/html"]}),
            )])),
            ..Default::default()
        },
    )
    .await?;
    assert!(!recovery_file.exists());
    let thread = start_thread(
        &mut client,
        /*id*/ 2,
        json!({
            "model":"gpt-5.2",
            "developerInstructions":"Session-only instructions",
            "dynamicTools":[{"name":"lookup_ticket","description":"Look up a ticket",
                             "inputSchema":{"type":"object","properties":{}}}]
        }),
    )
    .await?;
    for (id, method, params) in [
        (
            3,
            "skills/extraRoots/set",
            json!({"extraRoots":[home.path()]}),
        ),
        (
            4,
            "experimentalFeature/enablement/set",
            json!({"enablement":{"mcp_2026_07_28":true}}),
        ),
    ] {
        request(&mut client, id, method, params).await?;
    }
    let idle = start_thread(&mut client, /*id*/ 7, json!({})).await?;
    start_turn(&mut client, /*id*/ 5, &thread.thread.id).await?;
    wait_for_requests(&mock, /*count*/ 1).await?;
    request(
        &mut client,
        /*id*/ 6,
        "thread/goal/set",
        json!({
            "threadId":thread.thread.id,"objective":"continue after restart"
        }),
    )
    .await?;
    request_shutdown(&server, &socket_path).await?;
    assert_still_running(&mut server, "active turn must finish first").await;
    release_response.send(()).expect("response is waiting");
    wait_success(&mut server).await?;
    assert_eq!(
        daemon_recovery::read_candidates(&recovery_file)?,
        [thread.thread.id.clone(), idle.thread.id.clone()]
            .into_iter()
            .collect()
    );
    let mut saved = daemon_recovery::read_candidates(&recovery_file)?;
    saved.insert("!missing".into());
    daemon_recovery::write_candidates(&recovery_file, &saved)?;
    let mut successor = spawn_server(home.path(), &socket_path)?;
    // No client reconnects until the goal has made an inference request.
    wait_for_requests(&mock, /*count*/ 2).await?;
    let requests = mock.requests().await;
    let recovered: serde_json::Value = serde_json::from_slice(&requests[1])?;
    assert_eq!(recovered["model"], "gpt-5.2");
    assert!(recovered["input"]
        .as_array()
        .context("recovered conversation")?
        .iter()
        .any(|item| item["role"] == "assistant" && item["content"].to_string().contains("Done")));
    assert!(
        recovered["tools"].to_string().contains("lookup_ticket"),
        "{recovered}"
    );
    assert!(!recovery_file.exists());
    let mut reconnected = connect_daemon_client(
        &socket_path,
        InitializeCapabilities {
            experimental_api: true,
            extensions: Some(std::collections::HashMap::from([(
                "io.modelcontextprotocol/ui".into(),
                json!({"mimeTypes":["text/html"]}),
            )])),
            ..Default::default()
        },
    )
    .await?;
    let resumed = request(
        &mut reconnected,
        /*id*/ 2,
        "thread/resume",
        json!({
            "threadId":thread.thread.id,"excludeTurns":true
        }),
    )
    .await?;
    assert_eq!(resumed["thread"]["id"], thread.thread.id);
    timeout(DEFAULT_READ_TIMEOUT, async {
        loop {
            let loaded = request(
                &mut reconnected,
                /*id*/ 4,
                "thread/loaded/list",
                json!({}),
            )
            .await?;
            if loaded["data"]
                .as_array()
                .context("loaded IDs")?
                .contains(&json!(idle.thread.id))
            {
                return Ok::<(), anyhow::Error>(());
            }
            sleep(Duration::from_millis(25)).await;
        }
    })
    .await??;
    request(
        &mut reconnected,
        /*id*/ 3,
        "thread/goal/clear",
        json!({"threadId":thread.thread.id}),
    )
    .await?;
    release_recovered
        .send(())
        .expect("recovered goal is waiting");
    request_shutdown(&successor, &socket_path).await?;
    wait_success(&mut successor).await?;
    Ok(())
}

#[tokio::test]
async fn managed_force_shutdown_exits_without_snapshotting_active_work() -> Result<()> {
    let home = TempDir::new()?;
    let (_release_response, response_gate) = oneshot::channel();
    let (mock, _completions) =
        start_streaming_sse_server(vec![vec![stream_chunk(Some(response_gate), "Done")?]]).await;
    create_config_toml(home.path(), mock.uri(), "never")?;
    let socket_path = home.path().join("control/server.sock");
    let recovery_file = daemon_recovery_file_path(home.path());
    let mut server = spawn_server(home.path(), &socket_path)?;
    let mut client = connect_default_daemon_client(&socket_path).await?;
    let thread = start_thread(&mut client, /*id*/ 2, json!({})).await?;
    start_turn(&mut client, /*id*/ 3, &thread.thread.id).await?;
    wait_for_requests(&mock, /*count*/ 1).await?;
    request_shutdown(&server, &socket_path).await?;
    assert_still_running(&mut server, "graceful shutdown must wait").await;
    request_shutdown(&server, &socket_path).await?;
    wait_success(&mut server).await?;
    assert!(!recovery_file.exists());
    Ok(())
}

enum SnapshotScenario {
    WriteFailure,
    Ephemeral,
    Archived,
    Child,
    Parented,
}

#[test_case::test_case(SnapshotScenario::WriteFailure)]
#[test_case::test_case(SnapshotScenario::Ephemeral)]
#[test_case::test_case(SnapshotScenario::Archived)]
#[test_case::test_case(SnapshotScenario::Child)]
#[test_case::test_case(SnapshotScenario::Parented)]
#[tokio::test]
async fn managed_shutdown_skips_nonpersistent_threads_and_tolerates_save_failure(
    scenario: SnapshotScenario,
) -> Result<()> {
    let home = TempDir::new()?;
    create_config_toml(home.path(), "http://127.0.0.1:1", "never")?;
    let source = if matches!(scenario, SnapshotScenario::Child) {
        codex_protocol::protocol::SessionSource::SubAgent(
            codex_protocol::protocol::SubAgentSource::ThreadSpawn {
                parent_thread_id: codex_protocol::ThreadId::new(),
                depth: 1,
                agent_path: None,
                agent_nickname: None,
                agent_role: None,
            },
        )
    } else {
        codex_protocol::protocol::SessionSource::Cli
    };
    let stored = if matches!(scenario, SnapshotScenario::Parented) {
        app_test_support::create_fake_parented_rollout_with_source(
            home.path(),
            "2026-09-01T12-00-00",
            "2026-09-01T12:00:00Z",
            "Saved task",
            Some("mock_provider"),
            /*git_info*/ None,
            source,
            codex_protocol::SessionId::new(),
            codex_protocol::ThreadId::new(),
        )?
    } else {
        app_test_support::create_fake_rollout_with_source(
            home.path(),
            "2026-09-01T12-00-00",
            "2026-09-01T12:00:00Z",
            "Saved task",
            Some("mock_provider"),
            /*git_info*/ None,
            source,
        )?
    };
    let socket_path = home.path().join("control/server.sock");
    let mut server = spawn_server(home.path(), &socket_path)?;
    let mut client = connect_default_daemon_client(&socket_path).await?;
    if matches!(scenario, SnapshotScenario::Ephemeral) {
        request(
            &mut client,
            /*id*/ 2,
            "thread/start",
            json!({"ephemeral":true}),
        )
        .await?;
    } else {
        request(
            &mut client,
            /*id*/ 2,
            "thread/resume",
            json!({"threadId":stored}),
        )
        .await?;
    }
    let path = daemon_recovery_file_path(home.path());
    if matches!(scenario, SnapshotScenario::WriteFailure) {
        std::fs::create_dir_all(&path)?;
    } else if matches!(scenario, SnapshotScenario::Archived) {
        request(
            &mut client,
            /*id*/ 3,
            "thread/archive",
            json!({"threadId":stored}),
        )
        .await?;
    }
    request_shutdown(&server, &socket_path).await?;
    wait_success(&mut server).await?;
    if matches!(scenario, SnapshotScenario::WriteFailure) {
        assert!(path.is_dir());
    } else {
        assert_eq!(daemon_recovery::read_candidates(&path)?, Default::default());
    }
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn managed_force_shutdown_exits_with_blocked_rollout_writer() -> Result<()> {
    let home = TempDir::new()?;
    let mock = wiremock::MockServer::start().await;
    create_config_toml(home.path(), &mock.uri(), "never")?;
    let socket_path = home.path().join("control/server.sock");
    let mut server = spawn_server(home.path(), &socket_path)?;
    let mut client = connect_default_daemon_client(&socket_path).await?;
    let thread = start_thread(&mut client, /*id*/ 2, json!({})).await?;
    let rollout = thread
        .thread
        .path
        .context("persistent thread rollout path")?;
    assert!(!rollout.exists(), "idle rollout should still be deferred");
    let compressed = rollout.with_extension("jsonl.zst");
    std::fs::create_dir_all(compressed.parent().context("rollout directory")?)?;
    assert!(
        StdCommand::new("mkfifo")
            .arg(&compressed)
            .status()?
            .success()
    );

    request_shutdown(&server, &socket_path).await?;
    // Opening the write end succeeds only once the real rollout writer opens
    // the compressed input. Keep it empty and open to block that writer's read.
    let _blocked_input = timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(sender) = tokio::net::unix::pipe::OpenOptions::new().open_sender(&compressed)
            {
                return sender;
            }
            sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .context("rollout writer did not open the blocked input")?;
    assert_still_running(&mut server, "graceful shutdown must wait for the writer").await;
    request_shutdown(&server, &socket_path).await?;
    wait_success(&mut server).await?;
    Ok(())
}

async fn wait_success(server: &mut Child) -> Result<()> {
    anyhow::ensure!(
        timeout(Duration::from_secs(10), server.wait())
            .await??
            .success(),
        "server did not exit successfully"
    );
    Ok(())
}

async fn assert_still_running(server: &mut Child, message: &str) {
    assert!(
        timeout(Duration::from_millis(300), server.wait())
            .await
            .is_err(),
        "{message}"
    );
}

fn stream_chunk(gate: Option<oneshot::Receiver<()>>, body: &str) -> Result<StreamingSseChunk> {
    Ok(StreamingSseChunk {
        gate,
        body: create_final_assistant_message_sse_response(body)?,
    })
}

async fn wait_for_requests(
    mock: &core_test_support::streaming_sse::StreamingSseServer,
    count: usize,
) -> Result<()> {
    timeout(Duration::from_secs(5), mock.wait_for_request_count(count)).await?;
    Ok(())
}

async fn start_turn(
    websocket: &mut WebSocketStream<UnixStream>,
    id: i64,
    thread_id: impl serde::Serialize,
) -> Result<()> {
    request(
        websocket,
        id,
        "turn/start",
        json!({"threadId":thread_id,"input":[{"type":"text","text":"Hello"}]}),
    )
    .await?;
    Ok(())
}

async fn start_thread(
    websocket: &mut WebSocketStream<UnixStream>,
    id: i64,
    params: serde_json::Value,
) -> Result<ThreadStartResponse> {
    Ok(serde_json::from_value(
        request(websocket, id, "thread/start", params).await?,
    )?)
}

async fn connect_default_daemon_client(socket_path: &Path) -> Result<WebSocketStream<UnixStream>> {
    connect_daemon_client(socket_path, InitializeCapabilities::default()).await
}

async fn connect_daemon_client(
    socket_path: &Path,
    capabilities: InitializeCapabilities,
) -> Result<WebSocketStream<UnixStream>> {
    connect_initialized(socket_path, capabilities, "daemon_recovery_test", "0.1.0").await
}

fn spawn_server(home: &Path, socket_path: &Path) -> Result<Child> {
    let binary = codex_utils_cargo_bin::cargo_bin("codex-app-server")?;
    Ok(Command::new(binary)
        .args(["--listen", &format!("unix://{}", socket_path.display())])
        .arg(DISABLE_PLUGIN_STARTUP_TASKS_ARG)
        .env("CODEX_HOME", home)
        .arg("--managed-daemon")
        .env(
            codex_app_server_transport::DAEMON_SHUTDOWN_SOCKET_ENV,
            socket_path,
        )
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?)
}

#[cfg(unix)]
async fn request_shutdown(server: &Child, _socket_path: &Path) -> Result<()> {
    let pid = server.id().context("server pid")?;
    let status = StdCommand::new("kill")
        .arg("-TERM")
        .arg(pid.to_string())
        .status()?;
    anyhow::ensure!(status.success(), "failed to signal server {pid}: {status}");
    Ok(())
}

#[cfg(windows)]
async fn request_shutdown(server: &Child, socket_path: &Path) -> Result<()> {
    let pid = server.id().context("server pid")?.to_string();
    timeout(DEFAULT_READ_TIMEOUT, async {
        let stream = UnixStream::connect(socket_path).await?;
        let (mut websocket, _) = client_async("ws://localhost/daemon/shutdown", stream).await?;
        websocket.send(Message::Text(pid.clone().into())).await?;
        let reply = websocket
            .next()
            .await
            .context("missing shutdown acknowledgment")??;
        assert_eq!(reply, Message::Text(pid.into()));
        websocket.close(None).await?;
        Ok(())
    })
    .await?
}

async fn connect_initialized(
    socket_path: &Path,
    capabilities: InitializeCapabilities,
    client_name: &str,
    client_version: &str,
) -> Result<WebSocketStream<UnixStream>> {
    let mut websocket = timeout(DEFAULT_READ_TIMEOUT, async {
        loop {
            if let Ok(stream) = UnixStream::connect(socket_path).await
                && let Ok((websocket, _)) = client_async("ws://localhost/", stream).await
            {
                return websocket;
            }
            sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .context("server did not open its control socket")?;
    request(
        &mut websocket,
        /*id*/ 1,
        "initialize",
        json!({"clientInfo":{"name":client_name,"version":client_version},"capabilities":capabilities}),
    )
    .await?;
    websocket
        .send(Message::Text(
            json!({"method":"initialized"}).to_string().into(),
        ))
        .await?;
    Ok(websocket)
}

async fn request(
    websocket: &mut WebSocketStream<UnixStream>,
    id: i64,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value> {
    websocket
        .send(Message::Text(
            json!({"id":id,"method":method,"params":params})
                .to_string()
                .into(),
        ))
        .await?;
    timeout(DEFAULT_READ_TIMEOUT, async {
        loop {
            let frame = websocket.next().await.context("socket closed")??;
            let Message::Text(text) = frame else { continue };
            match serde_json::from_str::<JSONRPCMessage>(&text)? {
                JSONRPCMessage::Response(response) if response.id == RequestId::Integer(id) => {
                    return Ok(response.result);
                }
                JSONRPCMessage::Error(error) if error.id == RequestId::Integer(id) => {
                    anyhow::bail!("{}: {}", method, error.error.message);
                }
                _ => {}
            }
        }
    })
    .await
    .context("timed out waiting for app-server response")?
}
