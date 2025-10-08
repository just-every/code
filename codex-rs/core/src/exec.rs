#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;

use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::process::ExitStatus;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use async_channel::Sender;
use tokio::io::AsyncRead;
use tokio::io::AsyncReadExt;
use tokio::io::BufReader;
use tokio::process::Child;

use crate::codex::Session;
use crate::error::CodexErr;
use crate::error::Result;
use crate::error::SandboxErr;
use crate::landlock::spawn_command_under_linux_sandbox;
use crate::protocol::Event;
use crate::protocol::EventMsg;
use crate::protocol::ExecCommandOutputDeltaEvent;
use crate::protocol::ExecOutputStream;
use crate::protocol::OrderMeta;
use crate::protocol::SandboxPolicy;
use crate::seatbelt::spawn_command_under_seatbelt;
use crate::spawn::StdioPolicy;
use crate::spawn::spawn_child_async;
use serde_bytes::ByteBuf;

// Note: legacy stream caps were removed in favor of streaming all bytes and
// truncating at the consumer where appropriate. (CI cache test touch)

// Shell calls now default to NO hard timeout; long-running commands are
// backgrounded by higher-level orchestration.

// Hardcode these since it does not seem worth including the libc crate just
// for these.
const SIGKILL_CODE: i32 = 9;
const TIMEOUT_CODE: i32 = 64;
const EXIT_CODE_SIGNAL_BASE: i32 = 128; // conventional shell: 128 + signal
const EXEC_TIMEOUT_EXIT_CODE: i32 = 124; // conventional timeout exit code

// I/O buffer sizing
const READ_CHUNK_SIZE: usize = 8192; // bytes per read
const AGGREGATE_BUFFER_INITIAL_CAPACITY: usize = 8 * 1024; // 8 KiB

/// Limit the number of ExecCommandOutputDelta events emitted per exec call.
/// Aggregation still collects full output; only the live event stream is capped.
pub(crate) const MAX_EXEC_OUTPUT_DELTAS_PER_CALL: usize = 10_000;

#[derive(Clone, Debug)]
pub struct ExecParams {
    pub command: Vec<String>,
    pub cwd: PathBuf,
    pub timeout_ms: Option<u64>,
    pub env: HashMap<String, String>,
    pub with_escalated_permissions: Option<bool>,
    pub justification: Option<String>,
}

impl ExecParams {
    /// Optional timeout for the exec. When `None`, no timeout is enforced and
    /// the child runs until completion or interruption.
    pub fn maybe_timeout_duration(&self) -> Option<Duration> {
        self.timeout_ms.map(Duration::from_millis)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SandboxType {
    None,

    /// Only available on macOS.
    MacosSeatbelt,

    /// Only available on Linux.
    LinuxSeccomp,
}

#[derive(Clone)]
pub struct StdoutStream {
    pub sub_id: String,
    pub call_id: String,
    pub tx_event: Sender<Event>,
    pub(crate) session: Option<Arc<Session>>,
    /// Optional tail buffer for capturing a small window of the live stream.
    /// Used by callers that may return early and want to include "output so far".
    pub(crate) tail_buf: Option<std::sync::Arc<std::sync::Mutex<Vec<u8>>>>,
    /// Optional ordering metadata so UIs can associate deltas with the correct
    /// provider attempt/output index even when `session` is not available.
    pub(crate) order: Option<OrderMeta>,
}

pub async fn process_exec_tool_call(
    params: ExecParams,
    sandbox_type: SandboxType,
    sandbox_policy: &SandboxPolicy,
    sandbox_cwd: &Path,
    codex_linux_sandbox_exe: &Option<PathBuf>,
    stdout_stream: Option<StdoutStream>,
) -> Result<ExecToolCallOutput> {
    let start = Instant::now();

    let timeout_duration = params.maybe_timeout_duration();

    let raw_output_result: std::result::Result<RawExecToolCallOutput, CodexErr> = match sandbox_type
    {
        SandboxType::None => exec(params, sandbox_policy, stdout_stream.clone()).await,
        SandboxType::MacosSeatbelt => {
            let ExecParams {
                command,
                cwd: command_cwd,
                env,
                ..
            } = params;
            let child = spawn_command_under_seatbelt(
                command,
                command_cwd,
                sandbox_policy,
                sandbox_cwd,
                StdioPolicy::RedirectForShellTool,
                env,
            )
            .await?;
            consume_truncated_output(child, timeout_duration, stdout_stream.clone()).await
        }
        SandboxType::LinuxSeccomp => {
            let ExecParams {
                command,
                cwd: command_cwd,
                env,
                ..
            } = params;

            let codex_linux_sandbox_exe = codex_linux_sandbox_exe
                .as_ref()
                .ok_or(CodexErr::LandlockSandboxExecutableNotProvided)?;
            let child = spawn_command_under_linux_sandbox(
                codex_linux_sandbox_exe,
                command,
                command_cwd,
                sandbox_policy,
                sandbox_cwd,
                StdioPolicy::RedirectForShellTool,
                env,
            )
            .await?;

            consume_truncated_output(child, timeout_duration, stdout_stream).await
        }
    };
    let duration = start.elapsed();
    match raw_output_result {
        Ok(raw_output) => {
            #[allow(unused_mut)]
            let mut timed_out = raw_output.timed_out;

            #[cfg(target_family = "unix")]
            {
                if let Some(signal) = raw_output.exit_status.signal() {
                    if signal == TIMEOUT_CODE {
                        timed_out = true;
                    } else {
                        return Err(CodexErr::Sandbox(SandboxErr::Signal(signal)));
                    }
                }
            }

            let mut exit_code = raw_output.exit_status.code().unwrap_or(-1);
            if timed_out {
                exit_code = EXEC_TIMEOUT_EXIT_CODE;
            }

            let stdout = raw_output.stdout.from_utf8_lossy();
            let stderr = raw_output.stderr.from_utf8_lossy();
            let aggregated_output = raw_output.aggregated_output.from_utf8_lossy();
            let exec_output = ExecToolCallOutput {
                exit_code,
                stdout,
                stderr,
                aggregated_output,
                duration,
                timed_out,
            };

            if timed_out {
                return Err(CodexErr::Sandbox(SandboxErr::Timeout {
                    output: Box::new(exec_output),
                }));
            }

            if exit_code != 0 && is_likely_sandbox_denied(sandbox_type, exit_code) {
                return Err(CodexErr::Sandbox(SandboxErr::Denied {
                    output: Box::new(exec_output),
                }));
            }

            Ok(exec_output)
        }
        Err(err) => {
            tracing::error!("exec error: {err}");
            Err(err)
        }
    }
}

/// We don't have a fully deterministic way to tell if our command failed
/// because of the sandbox - a command in the user's zshrc file might hit an
/// error, but the command itself might fail or succeed for other reasons.
/// For now, we conservatively check for 'command not found' (exit code 127),
/// and can add additional cases as necessary.
fn is_likely_sandbox_denied(sandbox_type: SandboxType, exit_code: i32) -> bool {
    if sandbox_type == SandboxType::None {
        return false;
    }

    match exit_code {
        126 => true,          // found but not executable (likely permission denial)
        1 | 2 | 127 => false, // common non-sandbox failures
        _ => false,
    }
}

#[derive(Debug, Clone)]
pub struct StreamOutput<T> {
    pub text: T,
    pub truncated_after_lines: Option<u32>,
}
#[derive(Debug)]
struct RawExecToolCallOutput {
    pub exit_status: ExitStatus,
    pub stdout: StreamOutput<Vec<u8>>,
    pub stderr: StreamOutput<Vec<u8>>,
    pub aggregated_output: StreamOutput<Vec<u8>>,
    pub timed_out: bool,
}

impl StreamOutput<String> {
    pub fn new(text: String) -> Self {
        Self {
            text,
            truncated_after_lines: None,
        }
    }
}

impl StreamOutput<Vec<u8>> {
    pub fn from_utf8_lossy(&self) -> StreamOutput<String> {
        StreamOutput {
            text: String::from_utf8_lossy(&self.text).to_string(),
            truncated_after_lines: self.truncated_after_lines,
        }
    }
}

#[inline]
fn append_all(dst: &mut Vec<u8>, src: &[u8]) {
    dst.extend_from_slice(src);
}

#[derive(Debug, Clone)]
pub struct ExecToolCallOutput {
    pub exit_code: i32,
    pub stdout: StreamOutput<String>,
    pub stderr: StreamOutput<String>,
    pub aggregated_output: StreamOutput<String>,
    pub duration: Duration,
    pub timed_out: bool,
}

async fn exec(
    params: ExecParams,
    sandbox_policy: &SandboxPolicy,
    stdout_stream: Option<StdoutStream>,
) -> Result<RawExecToolCallOutput> {
    let timeout = params.maybe_timeout_duration();
    let ExecParams {
        command, cwd, env, ..
    } = params;

    let (program, args) = command.split_first().ok_or_else(|| {
        CodexErr::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "command args are empty",
        ))
    })?;
    let arg0 = None;
    let child = spawn_child_async(
        PathBuf::from(program),
        args.into(),
        arg0,
        cwd,
        sandbox_policy,
        StdioPolicy::RedirectForShellTool,
        env,
    )
    .await?;
    consume_truncated_output(child, timeout, stdout_stream).await
}

/// Consumes the output of a child process, truncating it so it is suitable for
/// use as the output of a `shell` tool call. Also enforces specified timeout.
async fn consume_truncated_output(
    child: Child,
    timeout: Option<Duration>,
    stdout_stream: Option<StdoutStream>,
) -> Result<RawExecToolCallOutput> {
    // Both stdout and stderr were configured with `Stdio::piped()`
    // above, therefore `take()` should normally return `Some`.  If it doesn't
    // we treat it as an exceptional I/O error

    let mut killer = KillOnDrop::new(child);

    let stdout_reader = killer.as_mut().stdout.take().ok_or_else(|| {
        CodexErr::Io(io::Error::other(
            "stdout pipe was unexpectedly not available",
        ))
    })?;
    let stderr_reader = killer.as_mut().stderr.take().ok_or_else(|| {
        CodexErr::Io(io::Error::other(
            "stderr pipe was unexpectedly not available",
        ))
    })?;

    let (agg_tx, agg_rx) = async_channel::unbounded::<Vec<u8>>();

    let stdout_handle = tokio::spawn(read_capped(
        BufReader::new(stdout_reader),
        stdout_stream.clone(),
        false,
        Some(agg_tx.clone()),
    ));
    let stderr_handle = tokio::spawn(read_capped(
        BufReader::new(stderr_reader),
        stdout_stream.clone(),
        true,
        Some(agg_tx.clone()),
    ));

    let (exit_status, timed_out) = match timeout {
        Some(timeout) => {
            tokio::select! {
                result = tokio::time::timeout(timeout, killer.as_mut().wait()) => {
                    match result {
                        Ok(status_result) => {
                            let exit_status = status_result?;
                            (exit_status, false)
                        }
                        Err(_) => {
                            // timeout
                            #[cfg(unix)]
                            {
                                if let Some(pid) = killer.as_mut().id() {
                                    // Best-effort kill entire process group
                                    unsafe { libc::kill(-(pid as i32), libc::SIGKILL); }
                                }
                            }
                            killer.as_mut().start_kill()?;
                            // Debatable whether `child.wait().await` should be called here.
                            (synthetic_exit_status(EXIT_CODE_SIGNAL_BASE + TIMEOUT_CODE), true)
                        }
                    }
                }
                _ = tokio::signal::ctrl_c() => {
                    killer.as_mut().start_kill()?;
                    (synthetic_exit_status(EXIT_CODE_SIGNAL_BASE + SIGKILL_CODE), false)
                }
            }
        }
        None => {
            // No timeout: wait until process completes or user interrupts.
            tokio::select! {
                status_result = killer.as_mut().wait() => {
                    let exit_status = status_result?;
                    (exit_status, false)
                }
                _ = tokio::signal::ctrl_c() => {
                    killer.as_mut().start_kill()?;
                    (synthetic_exit_status(EXIT_CODE_SIGNAL_BASE + SIGKILL_CODE), false)
                }
            }
        }
    };

    // Disarm killer now that we've observed process termination status to
    // avoid re-sending a kill signal during Drop.
    killer.disarm();

    // If we timed out, abort the readers after a short grace to prevent hanging when pipes
    // remain open due to orphaned grandchildren.
    let (stdout, stderr) = if timed_out {
        // Abort reader tasks to avoid hanging if pipes remain open.
        stdout_handle.abort();
        stderr_handle.abort();
        (
            StreamOutput {
                text: Vec::new(),
                truncated_after_lines: None,
            },
            StreamOutput {
                text: Vec::new(),
                truncated_after_lines: None,
            },
        )
    } else {
        (stdout_handle.await??, stderr_handle.await??)
    };

    drop(agg_tx);

    let mut combined_buf = Vec::with_capacity(AGGREGATE_BUFFER_INITIAL_CAPACITY);
    while let Ok(chunk) = agg_rx.recv().await {
        append_all(&mut combined_buf, &chunk);
    }
    let aggregated_output = StreamOutput {
        text: combined_buf,
        truncated_after_lines: None,
    };

    Ok(RawExecToolCallOutput {
        exit_status,
        stdout,
        stderr,
        aggregated_output,
        timed_out,
    })
}

async fn read_capped<R: AsyncRead + Unpin + Send + 'static>(
    mut reader: R,
    stream: Option<StdoutStream>,
    is_stderr: bool,
    aggregate_tx: Option<Sender<Vec<u8>>>,
) -> io::Result<StreamOutput<Vec<u8>>> {
    let mut buf = Vec::with_capacity(AGGREGATE_BUFFER_INITIAL_CAPACITY);
    let mut tmp = [0u8; READ_CHUNK_SIZE];
    let mut emitted_deltas: usize = 0;

    // No caps: append all bytes

    loop {
        let n = reader.read(&mut tmp).await?;
        if n == 0 {
            break;
        }

        if let Some(stream) = &stream {
            if emitted_deltas < MAX_EXEC_OUTPUT_DELTAS_PER_CALL {
                let chunk = tmp[..n].to_vec();
                let msg = EventMsg::ExecCommandOutputDelta(ExecCommandOutputDeltaEvent {
                    call_id: stream.call_id.clone(),
                    stream: if is_stderr {
                        ExecOutputStream::Stderr
                    } else {
                        ExecOutputStream::Stdout
                    },
                    chunk: ByteBuf::from(chunk),
                });
                let event = if let Some(sess) = &stream.session {
                    sess.make_event(&stream.sub_id, msg)
                } else {
                    Event {
                        id: stream.sub_id.clone(),
                        event_seq: 0,
                        msg,
                        order: stream.order.clone(),
                    }
                };
                #[allow(clippy::let_unit_value)]
                let _ = stream.tx_event.send(event).await;
                emitted_deltas += 1;

                // Update tail buffer if present (keep last ~8 KiB)
                if let Some(buf_arc) = &stream.tail_buf {
                    let mut b = buf_arc.lock().unwrap();
                    const MAX_TAIL: usize = 8 * 1024;
                    b.extend_from_slice(&tmp[..n]);
                    if b.len() > MAX_TAIL {
                        let drop_len = b.len() - MAX_TAIL;
                        b.drain(..drop_len);
                    }
                }
            }
        }

        if let Some(tx) = &aggregate_tx {
            let _ = tx.send(tmp[..n].to_vec()).await;
        }

        append_all(&mut buf, &tmp[..n]);
        // Continue reading to EOF to avoid back-pressure
    }

    Ok(StreamOutput {
        text: buf,
        truncated_after_lines: None,
    })
}

#[cfg(unix)]
fn synthetic_exit_status(code: i32) -> ExitStatus {
    use std::os::unix::process::ExitStatusExt;
    std::process::ExitStatus::from_raw(code)
}

/// Guard that ensures a spawned child process is terminated if the owning
/// future is dropped before the child has exited. This prevents orphaned
/// processes when a running turn is interrupted (e.g., user presses Esc or
/// Ctrl+C) and the task executing the command is aborted.
struct KillOnDrop {
    child: Option<Child>,
}

impl KillOnDrop {
    fn new(child: Child) -> Self {
        Self { child: Some(child) }
    }
    fn as_mut(&mut self) -> &mut Child {
        self.child.as_mut().expect("child present")
    }
    fn disarm(&mut self) {
        self.child = None;
    }
}

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.start_kill();
        }
    }
}
#[cfg(windows)]
fn synthetic_exit_status(code: i32) -> ExitStatus {
    use std::os::windows::process::ExitStatusExt;
    #[expect(clippy::unwrap_used)]
    std::process::ExitStatus::from_raw(code.try_into().unwrap())
}
