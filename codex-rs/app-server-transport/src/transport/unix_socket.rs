//! Control socket startup, guarded rendezvous paths, and WebSocket acceptance.

use std::fs::OpenOptions;
use std::io::ErrorKind;
use std::io::Result as IoResult;
use std::path::Path;

use super::TransportEvent;
use crate::transport::websocket::run_websocket_connection;
use codex_uds::UnixListener;
use codex_uds::UnixStream;
use codex_utils_absolute_path::AbsolutePathBuf;
use futures::SinkExt;
use futures::StreamExt;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::Duration;
use tokio_tungstenite::accept_hdr_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::http::Response;
use tokio_tungstenite::tungstenite::http::StatusCode;
use tokio_util::sync::CancellationToken;
use tracing::error;
use tracing::info;
use tracing::warn;

#[cfg(unix)]
const CONTROL_SOCKET_MODE: u32 = 0o600;

#[derive(Clone, Copy)]
pub enum DaemonShutdownAccess {
    Disabled,
    Managed,
}

pub async fn start_control_socket_acceptor(
    socket_path: AbsolutePathBuf,
    transport_event_tx: mpsc::Sender<TransportEvent>,
    shutdown_token: CancellationToken,
    daemon_shutdown_access: DaemonShutdownAccess,
) -> IoResult<JoinHandle<()>> {
    #[cfg(windows)]
    let (socket_path, directory_guard) = {
        if let Some(parent) = socket_path.as_path().parent() {
            codex_uds::prepare_private_socket_directory(parent).await?;
        }
        let (path, guard) = codex_uds::validate_private_socket_path(socket_path.as_path())?;
        (AbsolutePathBuf::from_absolute_path_checked(path)?, guard)
    };
    prepare_control_socket_path(socket_path.as_path()).await?;
    let listener = UnixListener::bind(socket_path.as_path()).await?;
    let socket_guard = ControlSocketFileGuard {
        socket_path,
        #[cfg(windows)]
        _directory_guard: directory_guard,
    };
    set_control_socket_permissions(socket_guard.socket_path.as_path()).await?;
    info!(
        socket_path = %socket_guard.socket_path.display(),
        "app-server control socket listening"
    );

    Ok(tokio::spawn(run_control_socket_acceptor(
        listener,
        transport_event_tx,
        shutdown_token,
        socket_guard,
        daemon_shutdown_access,
    )))
}

async fn run_control_socket_acceptor(
    mut listener: UnixListener,
    transport_event_tx: mpsc::Sender<TransportEvent>,
    shutdown_token: CancellationToken,
    socket_guard: ControlSocketFileGuard,
    daemon_shutdown_access: DaemonShutdownAccess,
) {
    let _socket_guard = socket_guard;
    loop {
        let stream = tokio::select! {
            _ = shutdown_token.cancelled() => {
                break;
            }
            result = listener.accept() => {
                match result {
                    Ok(stream) => stream,
                    Err(err) => {
                        if matches!(
                            err.kind(),
                            ErrorKind::ConnectionAborted | ErrorKind::ConnectionReset | ErrorKind::Interrupted
                        ) {
                            warn!("recoverable control socket accept error: {err}");
                            continue;
                        }
                        error!("control socket accept error: {err}");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                }
            }
        };

        let transport_event_tx = transport_event_tx.clone();
        tokio::spawn(async move {
            let mut shutdown_request = false;
            let websocket_stream = match accept_hdr_async(
                stream,
                |request: &tokio_tungstenite::tungstenite::handshake::server::Request, response| {
                    if request.uri().path() == "/daemon/shutdown" {
                        if !matches!(daemon_shutdown_access, DaemonShutdownAccess::Managed) {
                            let mut rejection = Response::new(Some("unmanaged server".to_string()));
                            *rejection.status_mut() = StatusCode::FORBIDDEN;
                            return Err(rejection);
                        }
                        shutdown_request = true;
                    }
                    Ok(response)
                },
            )
            .await
            {
                Ok(websocket_stream) => websocket_stream,
                Err(err) => {
                    warn!("failed to upgrade control socket websocket connection: {err}");
                    return;
                }
            };
            if shutdown_request {
                run_daemon_shutdown(websocket_stream, transport_event_tx).await;
                return;
            }
            let (websocket_writer, websocket_reader) = websocket_stream.split();
            run_websocket_connection(websocket_writer, websocket_reader, transport_event_tx).await;
        });
    }
    info!("control socket acceptor shutting down");
}

async fn run_daemon_shutdown(
    mut websocket: tokio_tungstenite::WebSocketStream<UnixStream>,
    transport_event_tx: mpsc::Sender<TransportEvent>,
) {
    let pid = std::process::id().to_string();
    if !matches!(websocket.next().await, Some(Ok(Message::Text(request))) if request == pid) {
        return;
    }
    if websocket.send(Message::Text(pid.into())).await.is_err() {
        return;
    }
    // Let the manager receive the acknowledgment before the main loop closes connections.
    let _ = tokio::time::timeout(Duration::from_secs(2), websocket.next()).await;
    let _ = transport_event_tx
        .send(TransportEvent::DaemonShutdown)
        .await;
}

pub async fn prepare_control_socket_path(socket_path: &Path) -> IoResult<()> {
    if let Some(parent) = socket_path.parent() {
        codex_uds::prepare_private_socket_directory(parent).await?;
    }

    #[cfg(windows)]
    let (socket_path, _directory_guard) = codex_uds::validate_private_socket_path(socket_path)?;
    #[cfg(windows)]
    let socket_path = AbsolutePathBuf::from_absolute_path_checked(socket_path)?;
    #[cfg(windows)]
    let socket_path = socket_path.as_path();

    match UnixStream::connect(socket_path).await {
        Ok(_stream) => {
            return Err(std::io::Error::new(
                ErrorKind::AddrInUse,
                format!(
                    "app-server control socket is already in use at {}",
                    socket_path.display()
                ),
            ));
        }
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(()),
        Err(err) if err.kind() == ErrorKind::ConnectionRefused => {}
        Err(err) => {
            if !socket_path.exists() {
                return Ok(());
            }
            return Err(err);
        }
    }

    if !socket_path.try_exists()? {
        return Ok(());
    }

    if !codex_uds::is_stale_socket_path(socket_path).await? {
        return Err(std::io::Error::new(
            ErrorKind::AlreadyExists,
            format!(
                "app-server control socket path exists and is not a socket: {}",
                socket_path.display()
            ),
        ));
    }
    tokio::fs::remove_file(socket_path).await
}

pub struct AppServerStartupLock {
    _file: std::fs::File,
}

pub async fn acquire_app_server_startup_lock(
    startup_lock_path: AbsolutePathBuf,
) -> IoResult<AppServerStartupLock> {
    if let Some(parent) = startup_lock_path.as_path().parent() {
        codex_uds::prepare_private_socket_directory(parent).await?;
    }
    tokio::task::spawn_blocking(move || {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(startup_lock_path.as_path())?;
        file.lock()?;
        Ok(AppServerStartupLock { _file: file })
    })
    .await
    .map_err(|err| std::io::Error::other(format!("startup lock task failed: {err}")))?
}

#[cfg(unix)]
async fn set_control_socket_permissions(socket_path: &Path) -> IoResult<()> {
    use std::os::unix::fs::PermissionsExt;

    tokio::fs::set_permissions(
        socket_path,
        std::fs::Permissions::from_mode(CONTROL_SOCKET_MODE),
    )
    .await
}

#[cfg(not(unix))]
async fn set_control_socket_permissions(_socket_path: &Path) -> IoResult<()> {
    Ok(())
}

struct ControlSocketFileGuard {
    socket_path: AbsolutePathBuf,
    // Keep the directory pinned until after the socket file is removed in Drop.
    #[cfg(windows)]
    _directory_guard: std::os::windows::io::OwnedHandle,
}

impl Drop for ControlSocketFileGuard {
    fn drop(&mut self) {
        if let Err(err) = std::fs::remove_file(self.socket_path.as_path())
            && err.kind() != ErrorKind::NotFound
        {
            warn!(
                socket_path = %self.socket_path.display(),
                %err,
                "failed to remove app-server control socket file"
            );
        }
    }
}
