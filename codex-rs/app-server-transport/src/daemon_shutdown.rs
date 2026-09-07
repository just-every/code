//! The detached Windows updater has no control socket, so its owner requests
//! termination through a file in its private state directory. Managed app-server
//! shutdown instead uses the protected local socket.

use std::io;
use std::io::Read;
use std::path::Path;
#[cfg(windows)]
use std::path::PathBuf;
#[cfg(windows)]
use std::time::Duration;

#[cfg(windows)]
pub const DAEMON_SHUTDOWN_FILE_ENV: &str = "CODEX_DAEMON_SHUTDOWN_FILE";

/// Waits for and consumes one updater shutdown request.
#[cfg(windows)]
pub async fn daemon_shutdown_signal() -> io::Result<()> {
    let Some(path) = std::env::var_os(DAEMON_SHUTDOWN_FILE_ENV).map(PathBuf::from) else {
        return std::future::pending().await;
    };
    loop {
        if take_shutdown_request(&path, std::process::id())? {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn take_shutdown_request(path: &Path, pid: u32) -> io::Result<bool> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(err),
    };
    // Descendant app-servers may inherit the control path. Only the intended
    // process can consume a request. Bound reads to a u32 PID plus one extra byte.
    let mut contents = Vec::new();
    file.take(/*limit*/ 11).read_to_end(&mut contents)?;
    if contents != pid.to_string().as_bytes() {
        return Ok(false);
    }
    // Synchronous consumption cannot lose a request to select cancellation.
    match std::fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err),
    }
}

#[cfg(test)]
#[path = "daemon_shutdown_tests.rs"]
mod tests;
