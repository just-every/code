use std::net::SocketAddr;
use std::time::Duration;

use code_app_server::AppServerTransport;
use code_app_server::run_main_with_transport;
use code_common::CliConfigOverrides;
use futures::SinkExt;
use futures::StreamExt;
use serde_json::Value;
use serde_json::json;
use tokio::net::TcpStream;
use tokio::time::sleep;
use tokio_tungstenite::MaybeTlsStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

async fn connect_with_retry(
    url: &str,
) -> WebSocketStream<MaybeTlsStream<TcpStream>> {
    let mut attempts = 0;
    loop {
        match connect_async(url).await {
            Ok((stream, _)) => return stream,
            Err(err) => {
                attempts += 1;
                assert!(attempts < 40, "failed to connect to {url}: {err}");
                sleep(Duration::from_millis(25)).await;
            }
        }
    }
}

async fn send_request(
    ws: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
    request: Value,
) {
    ws.send(Message::Text(request.to_string().into()))
        .await
        .expect("request should send");
}

async fn recv_response_for_id(
    ws: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
    id: i64,
) -> Value {
    loop {
        let message = ws
            .next()
            .await
            .expect("websocket should stay open")
            .expect("websocket frame should decode");
        let Message::Text(text) = message else {
            continue;
        };
        let json: Value = serde_json::from_str(text.as_ref()).expect("response must be JSON");
        let json_id = json.get("id").and_then(Value::as_i64);
        if json_id == Some(id) {
            return json;
        }
    }
}

async fn recv_error_for_id(
    ws: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
    id: i64,
) -> Value {
    loop {
        let message = ws
            .next()
            .await
            .expect("websocket should stay open")
            .expect("websocket frame should decode");
        let Message::Text(text) = message else {
            continue;
        };
        let json: Value = serde_json::from_str(text.as_ref()).expect("response must be JSON");
        let json_id = json.get("id").and_then(Value::as_i64);
        if json_id == Some(id) && json.get("error").is_some() {
            return json;
        }
    }
}

async fn assert_no_message(
    ws: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
    wait_for: Duration,
) {
    match tokio::time::timeout(wait_for, ws.next()).await {
        Ok(Some(Ok(frame))) => {
            panic!("unexpected frame while waiting for silence: {frame:?}");
        }
        Ok(Some(Err(err))) => {
            panic!("unexpected websocket read error while waiting for silence: {err}");
        }
        Ok(None) => {
            panic!("websocket closed unexpectedly while waiting for silence");
        }
        Err(_) => {}
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn websocket_user_agent_is_connection_scoped() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let addr: SocketAddr = listener.local_addr().expect("resolve bound address");
    drop(listener);

    let server_handle = tokio::spawn(async move {
        run_main_with_transport(
            None,
            CliConfigOverrides::default(),
            AppServerTransport::WebSocket { bind_address: addr },
        )
        .await
    });

    let url = format!("ws://{addr}");
    let mut client_a = connect_with_retry(&url).await;
    let mut client_b = connect_with_retry(&url).await;

    send_request(
        &mut client_a,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "clientInfo": {
                    "name": "client-a",
                    "version": "1.0.0"
                }
            }
        }),
    )
    .await;
    let _ = recv_response_for_id(&mut client_a, 1).await;

    send_request(
        &mut client_b,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "clientInfo": {
                    "name": "client-b",
                    "version": "2.0.0"
                }
            }
        }),
    )
    .await;
    let _ = recv_response_for_id(&mut client_b, 1).await;

    send_request(
        &mut client_a,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "getUserAgent"
        }),
    )
    .await;
    let response_a = recv_response_for_id(&mut client_a, 2).await;

    send_request(
        &mut client_b,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "getUserAgent"
        }),
    )
    .await;
    let response_b = recv_response_for_id(&mut client_b, 2).await;

    let user_agent_a = response_a
        .get("result")
        .and_then(|result| result.get("userAgent"))
        .and_then(Value::as_str)
        .expect("client a should receive user agent");
    let user_agent_b = response_b
        .get("result")
        .and_then(|result| result.get("userAgent"))
        .and_then(Value::as_str)
        .expect("client b should receive user agent");

    assert!(
        user_agent_a.contains("(client-a; 1.0.0)"),
        "client a user-agent should include its own suffix: {user_agent_a}"
    );
    assert!(
        user_agent_b.contains("(client-b; 2.0.0)"),
        "client b user-agent should include its own suffix: {user_agent_b}"
    );
    assert!(
        !user_agent_a.contains("client-b; 2.0.0"),
        "client a user-agent should not include client b suffix: {user_agent_a}"
    );
    assert!(
        !user_agent_b.contains("client-a; 1.0.0"),
        "client b user-agent should not include client a suffix: {user_agent_b}"
    );

    client_a.close(None).await.expect("client a should close");
    client_b.close(None).await.expect("client b should close");
    server_handle.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn websocket_routes_handshake_and_same_id_requests_per_connection() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let addr: SocketAddr = listener.local_addr().expect("resolve bound address");
    drop(listener);

    let server_handle = tokio::spawn(async move {
        run_main_with_transport(
            None,
            CliConfigOverrides::default(),
            AppServerTransport::WebSocket { bind_address: addr },
        )
        .await
    });

    let url = format!("ws://{addr}");
    let mut client_a = connect_with_retry(&url).await;
    let mut client_b = connect_with_retry(&url).await;

    send_request(
        &mut client_a,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "clientInfo": {
                    "name": "client-a",
                    "version": "1.0.0"
                }
            }
        }),
    )
    .await;
    let _ = recv_response_for_id(&mut client_a, 1).await;

    // Initialize responses are request-scoped and should not leak to other clients.
    assert_no_message(&mut client_b, Duration::from_millis(200)).await;

    send_request(
        &mut client_b,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "getUserAgent"
        }),
    )
    .await;
    let pre_init_error = recv_error_for_id(&mut client_b, 2).await;
    let pre_init_message = pre_init_error
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .expect("error message should exist");
    assert!(
        pre_init_message.contains("Not initialized"),
        "unexpected pre-init error: {pre_init_message}"
    );

    send_request(
        &mut client_b,
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "initialize",
            "params": {
                "clientInfo": {
                    "name": "client-b",
                    "version": "2.0.0"
                }
            }
        }),
    )
    .await;
    let _ = recv_response_for_id(&mut client_b, 3).await;

    // Same request id on different connections should route independently.
    send_request(
        &mut client_a,
        json!({
            "jsonrpc": "2.0",
            "id": 77,
            "method": "getUserAgent"
        }),
    )
    .await;
    send_request(
        &mut client_b,
        json!({
            "jsonrpc": "2.0",
            "id": 77,
            "method": "getUserAgent"
        }),
    )
    .await;

    let response_a = recv_response_for_id(&mut client_a, 77).await;
    let response_b = recv_response_for_id(&mut client_b, 77).await;

    assert!(response_a.get("result").is_some(), "client a should get response");
    assert!(response_b.get("result").is_some(), "client b should get response");

    client_a.close(None).await.expect("client a should close");
    client_b.close(None).await.expect("client b should close");
    server_handle.abort();
}
