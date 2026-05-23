//! Clipboard copy backend for the TUI's `/copy` command.
//!
//! The selection order mirrors upstream Codex: SSH uses OSC 52, local sessions
//! try the native clipboard first, WSL falls back to PowerShell, then OSC 52.

use base64::Engine;
use std::io::Write;

const OSC52_MAX_RAW_BYTES: usize = 100_000;

#[cfg(target_os = "macos")]
static STDERR_SUPPRESSION_MUTEX: std::sync::OnceLock<std::sync::Mutex<()>> =
    std::sync::OnceLock::new();

pub(crate) fn copy_to_clipboard(text: &str) -> Result<Option<ClipboardLease>, String> {
    copy_to_clipboard_with(
        text,
        is_ssh_session(),
        is_wsl_session(),
        osc52_copy,
        arboard_copy,
        wsl_clipboard_copy,
    )
}

pub(crate) struct ClipboardLease {
    #[cfg(target_os = "linux")]
    _clipboard: Option<arboard::Clipboard>,
}

impl ClipboardLease {
    #[cfg(target_os = "linux")]
    fn native_linux(clipboard: arboard::Clipboard) -> Self {
        Self {
            _clipboard: Some(clipboard),
        }
    }

    #[cfg(test)]
    pub(crate) fn test() -> Self {
        Self {
            #[cfg(target_os = "linux")]
            _clipboard: None,
        }
    }
}

fn copy_to_clipboard_with(
    text: &str,
    ssh_session: bool,
    wsl_session: bool,
    osc52_copy_fn: impl Fn(&str) -> Result<(), String>,
    arboard_copy_fn: impl Fn(&str) -> Result<Option<ClipboardLease>, String>,
    wsl_copy_fn: impl Fn(&str) -> Result<(), String>,
) -> Result<Option<ClipboardLease>, String> {
    if ssh_session {
        return osc52_copy_fn(text).map(|()| None).map_err(|osc_err| {
            tracing::warn!("OSC 52 clipboard copy failed over SSH: {osc_err}");
            format!("OSC 52 clipboard copy failed over SSH: {osc_err}")
        });
    }

    match arboard_copy_fn(text) {
        Ok(lease) => Ok(lease),
        Err(native_err) => {
            if wsl_session {
                tracing::warn!(
                    "native clipboard copy failed: {native_err}, falling back to WSL PowerShell"
                );
                match wsl_copy_fn(text) {
                    Ok(()) => return Ok(None),
                    Err(wsl_err) => {
                        tracing::warn!(
                            "WSL PowerShell clipboard copy failed: {wsl_err}, falling back to OSC 52"
                        );
                        return osc52_copy_fn(text).map(|()| None).map_err(|osc_err| {
                            format!(
                                "native clipboard: {native_err}; WSL fallback: {wsl_err}; OSC 52 fallback: {osc_err}"
                            )
                        });
                    }
                }
            }
            tracing::warn!("native clipboard copy failed: {native_err}, falling back to OSC 52");
            osc52_copy_fn(text).map(|()| None).map_err(|osc_err| {
                format!("native clipboard: {native_err}; OSC 52 fallback: {osc_err}")
            })
        }
    }
}

fn is_ssh_session() -> bool {
    std::env::var_os("SSH_TTY").is_some() || std::env::var_os("SSH_CONNECTION").is_some()
}

#[cfg(target_os = "linux")]
fn is_wsl_session() -> bool {
    if std::env::var_os("WSL_DISTRO_NAME").is_some()
        || std::env::var_os("WSL_INTEROP").is_some()
    {
        return true;
    }

    std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .map(|release| release.to_ascii_lowercase().contains("microsoft"))
        .unwrap_or(false)
}

#[cfg(not(target_os = "linux"))]
fn is_wsl_session() -> bool {
    false
}

#[cfg(all(not(target_os = "android"), not(target_os = "linux")))]
fn arboard_copy(text: &str) -> Result<Option<ClipboardLease>, String> {
    #[cfg(target_os = "macos")]
    let _stderr_lock = STDERR_SUPPRESSION_MUTEX
        .get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .map_err(|_| "stderr suppression lock poisoned".to_string())?;
    let _guard = SuppressStderr::new();
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("clipboard unavailable: {e}"))?;
    clipboard
        .set_text(text)
        .map_err(|e| format!("failed to set clipboard text: {e}"))?;
    Ok(None)
}

#[cfg(target_os = "linux")]
fn arboard_copy(text: &str) -> Result<Option<ClipboardLease>, String> {
    let _guard = SuppressStderr::new();
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("clipboard unavailable: {e}"))?;
    clipboard
        .set_text(text)
        .map_err(|e| format!("failed to set clipboard text: {e}"))?;
    Ok(Some(ClipboardLease::native_linux(clipboard)))
}

#[cfg(target_os = "android")]
fn arboard_copy(_text: &str) -> Result<Option<ClipboardLease>, String> {
    Err("native clipboard unavailable on Android".to_string())
}

#[cfg(target_os = "linux")]
fn wsl_clipboard_copy(text: &str) -> Result<(), String> {
    let mut child = std::process::Command::new("powershell.exe")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .args([
            "-NoProfile",
            "-Command",
            "[Console]::InputEncoding = [System.Text.Encoding]::UTF8; $ErrorActionPreference = 'Stop'; $text = [Console]::In.ReadToEnd(); Set-Clipboard -Value $text",
        ])
        .spawn()
        .map_err(|e| format!("failed to spawn powershell.exe: {e}"))?;

    let Some(mut stdin) = child.stdin.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err("failed to open powershell.exe stdin".to_string());
    };

    if let Err(err) = stdin.write_all(text.as_bytes()) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(format!("failed to write to powershell.exe: {err}"));
    }

    drop(stdin);

    let output = child
        .wait_with_output()
        .map_err(|e| format!("failed to wait for powershell.exe: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            let status = output.status;
            Err(format!("powershell.exe exited with status {status}"))
        } else {
            Err(format!("powershell.exe failed: {stderr}"))
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn wsl_clipboard_copy(_text: &str) -> Result<(), String> {
    Err("WSL clipboard fallback unavailable on this platform".to_string())
}

#[cfg(target_os = "macos")]
struct SuppressStderr {
    saved_fd: Option<libc::c_int>,
}

#[cfg(target_os = "macos")]
impl SuppressStderr {
    fn new() -> Self {
        unsafe {
            let saved = libc::dup(2);
            if saved < 0 {
                return Self { saved_fd: None };
            }
            let devnull = libc::open(c"/dev/null".as_ptr(), libc::O_WRONLY);
            if devnull < 0 {
                libc::close(saved);
                return Self { saved_fd: None };
            }
            if libc::dup2(devnull, 2) < 0 {
                libc::close(saved);
                libc::close(devnull);
                return Self { saved_fd: None };
            }
            libc::close(devnull);
            Self {
                saved_fd: Some(saved),
            }
        }
    }
}

#[cfg(target_os = "macos")]
impl Drop for SuppressStderr {
    fn drop(&mut self) {
        if let Some(saved) = self.saved_fd {
            unsafe {
                libc::dup2(saved, 2);
                libc::close(saved);
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
struct SuppressStderr;

#[cfg(not(target_os = "macos"))]
impl SuppressStderr {
    fn new() -> Self {
        Self
    }
}

fn osc52_copy(text: &str) -> Result<(), String> {
    let sequence = osc52_sequence(text, std::env::var_os("TMUX").is_some())?;
    #[cfg(unix)]
    {
        match std::fs::OpenOptions::new().write(true).open("/dev/tty") {
            Ok(tty) => match write_osc52_to_writer(tty, &sequence) {
                Ok(()) => return Ok(()),
                Err(err) => tracing::debug!(
                    "failed to write OSC 52 to /dev/tty: {err}; falling back to stdout"
                ),
            },
            Err(err) => {
                tracing::debug!("failed to open /dev/tty for OSC 52: {err}; falling back to stdout")
            }
        }
    }

    write_osc52_to_writer(std::io::stdout().lock(), &sequence)
}

fn write_osc52_to_writer(mut writer: impl Write, sequence: &str) -> Result<(), String> {
    writer
        .write_all(sequence.as_bytes())
        .map_err(|e| format!("failed to write OSC 52: {e}"))?;
    writer
        .flush()
        .map_err(|e| format!("failed to flush OSC 52: {e}"))
}

fn osc52_sequence(text: &str, tmux: bool) -> Result<String, String> {
    let raw_bytes = text.len();
    if raw_bytes > OSC52_MAX_RAW_BYTES {
        return Err(format!(
            "OSC 52 payload too large ({raw_bytes} bytes; max {OSC52_MAX_RAW_BYTES})"
        ));
    }

    let encoded = base64::engine::general_purpose::STANDARD.encode(text.as_bytes());
    if tmux {
        Ok(format!("\x1bPtmux;\x1b\x1b]52;c;{encoded}\x07\x1b\\"))
    } else {
        Ok(format!("\x1b]52;c;{encoded}\x07"))
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::OSC52_MAX_RAW_BYTES;
    use super::osc52_sequence;
    use super::write_osc52_to_writer;

    #[test]
    fn osc52_encoding_roundtrips() {
        use base64::Engine;
        let text = "# Hello\n\n```rust\nfn main() {}\n```\n";
        let sequence = osc52_sequence(text, false).expect("OSC 52 sequence");
        let encoded = sequence
            .trim_start_matches("\u{1b}]52;c;")
            .trim_end_matches('\u{7}');
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .unwrap();
        assert_eq!(decoded, text.as_bytes());
    }

    #[test]
    fn osc52_rejects_payload_larger_than_limit() {
        let text = "x".repeat(OSC52_MAX_RAW_BYTES + 1);
        assert_eq!(
            osc52_sequence(&text, false),
            Err(format!(
                "OSC 52 payload too large ({} bytes; max {OSC52_MAX_RAW_BYTES})",
                OSC52_MAX_RAW_BYTES + 1
            ))
        );
    }

    #[test]
    fn write_osc52_to_writer_emits_sequence_verbatim() {
        let sequence = "\u{1b}]52;c;aGVsbG8=\u{7}";
        let mut output = Vec::new();
        assert_eq!(write_osc52_to_writer(&mut output, sequence), Ok(()));
        assert_eq!(output, sequence.as_bytes());
    }
}
