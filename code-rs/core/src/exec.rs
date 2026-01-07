#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;

use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::pin::Pin;
use std::process::ExitStatus;
use std::time::Duration;
use std::time::Instant;
use std::sync::Arc;

use async_channel::Sender;
use tokio::io::AsyncRead;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::process::Child;
use tokio::time::Sleep;

use crate::codex::Session;
use crate::error::CodexErr;
use crate::error::Result;
use crate::error::SandboxErr;
use crate::landlock::spawn_command_under_linux_sandbox;
use crate::text_encoding::bytes_to_string_smart;
use crate::protocol::Event;
use crate::protocol::EventMsg;
use crate::protocol::OrderMeta;
use crate::protocol::ExecCommandOutputDeltaEvent;
use crate::protocol::ExecOutputStream;
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
const EXEC_DELTA_FLUSH_BYTES: usize = 256 * 1024; // aggregate stdout/stderr deltas before emitting
const EXEC_DELTA_FLUSH_INTERVAL: Duration = Duration::from_millis(200); // max interval between live deltas
const AGGREGATE_BUFFER_INITIAL_CAPACITY: usize = 8 * 1024; // 8 KiB
pub(crate) const EXEC_CAPTURE_MAX_BYTES: usize = 32 * 1024 * 1024; // 32 MiB cap per stream

fn append_with_cap(
    buf: &mut Vec<u8>,
    chunk: &[u8],
    truncated: &mut bool,
    truncated_lines: &mut u32,
    truncated_bytes: &mut usize,
) {
    if chunk.is_empty() {
        return;
    }

    buf.extend_from_slice(chunk);
    if buf.len() > EXEC_CAPTURE_MAX_BYTES {
        let drop_len = buf.len() - EXEC_CAPTURE_MAX_BYTES;
        let mut lines_dropped = 0u32;
        for byte in buf.drain(0..drop_len) {
            if byte == b'\n' {
                lines_dropped = lines_dropped.saturating_add(1);
            }
        }
        *truncated = true;
        *truncated_lines = (*truncated_lines).saturating_add(lines_dropped);
        *truncated_bytes = (*truncated_bytes).saturating_add(drop_len);
    }
}

/// Limit the number of ExecCommandOutputDelta events emitted per exec call.
/// Aggregation still collects full output; only the live event stream is capped.
pub(crate) const MAX_EXEC_OUTPUT_DELTAS_PER_CALL: usize = 2_048;

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

    /// Optional directory to spool full stdout/stderr output for this exec.
    ///
    /// When set, Code writes raw stream bytes to disk while still keeping only
    /// a bounded tail in memory.
    pub(crate) spool_dir: Option<PathBuf>,
}

pub async fn process_exec_tool_call(
    params: ExecParams,
    sandbox_type: SandboxType,
    sandbox_policy: &SandboxPolicy,
    sandbox_cwd: &Path,
    code_linux_sandbox_exe: &Option<PathBuf>,
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

            let code_linux_sandbox_exe = code_linux_sandbox_exe
                .as_ref()
                .ok_or(CodexErr::LandlockSandboxExecutableNotProvided)?;
            let child = spawn_command_under_linux_sandbox(
                code_linux_sandbox_exe,
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

            #[allow(unused_variables)]
            let mut exit_signal: Option<i32> = None;

            #[cfg(target_family = "unix")]
            {
                if let Some(sig) = raw_output.exit_status.signal() {
                    if sig == TIMEOUT_CODE {
                        timed_out = true;
                    } else {
                        exit_signal = Some(sig);
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

            if let Some(signal) = exit_signal {
                if raw_output.oom_killed {
                    return Err(CodexErr::Sandbox(SandboxErr::OutOfMemory {
                        output: Box::new(exec_output),
                        memory_max_bytes: raw_output.cgroup_memory_max_bytes,
                    }));
                }
                return Err(CodexErr::Sandbox(SandboxErr::Signal(signal)));
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
    pub truncated_before_bytes: Option<usize>,
}
#[derive(Debug)]
struct RawExecToolCallOutput {
    pub exit_status: ExitStatus,
    pub stdout: StreamOutput<Vec<u8>>,
    pub stderr: StreamOutput<Vec<u8>>,
    pub aggregated_output: StreamOutput<Vec<u8>>,
    pub timed_out: bool,
    pub oom_killed: bool,
    pub cgroup_memory_max_bytes: Option<u64>,
}

impl StreamOutput<String> {
    pub fn new(text: String) -> Self {
        Self {
            text,
            truncated_after_lines: None,
            truncated_before_bytes: None,
        }
    }
}

impl StreamOutput<Vec<u8>> {
    pub fn from_utf8_lossy(&self) -> StreamOutput<String> {
        StreamOutput {
            text: bytes_to_string_smart(&self.text),
            truncated_after_lines: self.truncated_after_lines,
            truncated_before_bytes: self.truncated_before_bytes,
        }
    }
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

    #[allow(unused_variables)]
    let pid = killer.as_mut().id();

    let (spool_stdout, spool_stderr, spool_combined) = if let Some(stream) = stdout_stream.as_ref()
        && let Some(root) = stream.spool_dir.as_ref()
    {
        let safe_sub_id = crate::fs_sanitize::safe_path_component(&stream.sub_id, "sub");
        let safe_call_id = crate::fs_sanitize::safe_path_component(&stream.call_id, "call");
        let base_dir = root.join(safe_sub_id).join(safe_call_id);
        let _ = tokio::fs::create_dir_all(&base_dir).await;

        let stdout_path = base_dir.join("stdout.log");
        let stderr_path = base_dir.join("stderr.log");
        let combined_path = base_dir.join("combined.log");
        let stdout = tokio::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(stdout_path)
            .await
            .ok();
        let stderr = tokio::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(stderr_path)
            .await
            .ok();
        let combined = tokio::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(combined_path)
            .await
            .ok();
        (stdout, stderr, combined)
    } else {
        (None, None, None)
    };

    let (agg_tx, agg_rx) = async_channel::bounded::<Vec<u8>>(256);

    let combined_handle = tokio::spawn(async move {
        let mut combined_buf = Vec::with_capacity(AGGREGATE_BUFFER_INITIAL_CAPACITY);
        let mut combined_truncated = false;
        let mut combined_truncated_lines = 0u32;
        let mut combined_truncated_bytes = 0usize;
        let mut combined_file = spool_combined;
        while let Ok(chunk) = agg_rx.recv().await {
            if let Some(file) = combined_file.as_mut() {
                let _ = file.write_all(&chunk).await;
            }
            append_with_cap(
                &mut combined_buf,
                &chunk,
                &mut combined_truncated,
                &mut combined_truncated_lines,
                &mut combined_truncated_bytes,
            );
        }
        StreamOutput {
            text: combined_buf,
            truncated_after_lines: combined_truncated
                .then_some(combined_truncated_lines.max(1)),
            truncated_before_bytes: (combined_truncated_bytes > 0)
                .then_some(combined_truncated_bytes),
        }
    });

    let stdout_handle = tokio::spawn(read_capped(
        BufReader::new(stdout_reader),
        stdout_stream.clone(),
        false,
        Some(agg_tx.clone()),
        spool_stdout,
    ));
    let stderr_handle = tokio::spawn(read_capped(
        BufReader::new(stderr_reader),
        stdout_stream.clone(),
        true,
        Some(agg_tx.clone()),
        spool_stderr,
    ));

    let mut reap_after_kill = false;
    let mut child_exited = false;

    let (exit_status, timed_out) = match timeout {
        Some(timeout) => {
            tokio::select! {
                result = tokio::time::timeout(timeout, killer.as_mut().wait()) => {
                    match result {
                        Ok(status_result) => {
                            let exit_status = status_result?;
                            child_exited = true;
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
                            reap_after_kill = true;
                            // Debatable whether `child.wait().await` should be called here.
                            (synthetic_exit_status(EXIT_CODE_SIGNAL_BASE + TIMEOUT_CODE), true)
                        }
                    }
                }
                _ = tokio::signal::ctrl_c() => {
                    killer.as_mut().start_kill()?;
                    reap_after_kill = true;
                    (synthetic_exit_status(EXIT_CODE_SIGNAL_BASE + SIGKILL_CODE), false)
                }
            }
        }
        None => {
            // No timeout: wait until process completes or user interrupts.
            tokio::select! {
                status_result = killer.as_mut().wait() => {
                    let exit_status = status_result?;
                    child_exited = true;
                    (exit_status, false)
                }
                _ = tokio::signal::ctrl_c() => {
                    killer.as_mut().start_kill()?;
                    reap_after_kill = true;
                    (synthetic_exit_status(EXIT_CODE_SIGNAL_BASE + SIGKILL_CODE), false)
                }
            }
        }
    };

    if reap_after_kill {
        let reap_timeout = Duration::from_secs(2);
        if let Ok(Ok(_)) = tokio::time::timeout(reap_timeout, killer.as_mut().wait()).await {
            child_exited = true;
        }
    }

    // Disarm killer now that we've observed process termination status to
    // avoid re-sending a kill signal during Drop.
    if child_exited {
        killer.disarm();
    }

    // If we timed out, abort the readers after a short grace to prevent hanging when pipes
    // remain open due to orphaned grandchildren.
    let (stdout, stderr) = if timed_out {
        // Abort reader tasks to avoid hanging if pipes remain open.
        stdout_handle.abort();
        stderr_handle.abort();
        combined_handle.abort();
        (
            StreamOutput {
                text: Vec::new(),
                truncated_after_lines: None,
                truncated_before_bytes: None,
            },
            StreamOutput {
                text: Vec::new(),
                truncated_after_lines: None,
                truncated_before_bytes: None,
            },
        )
    } else {
        (stdout_handle.await??, stderr_handle.await??)
    };

    drop(agg_tx);

    let aggregated_output = if timed_out {
        StreamOutput {
            text: Vec::new(),
            truncated_after_lines: None,
            truncated_before_bytes: None,
        }
    } else {
        combined_handle.await.map_err(CodexErr::from)?
    };

    let (oom_killed, cgroup_memory_max_bytes) = {
        #[cfg(target_os = "linux")]
        {
            let mut oom_killed = false;
            let mut cgroup_memory_max_bytes: Option<u64> = None;
            if !timed_out {
                if let Some(pid) = pid {
                    if matches!(exit_status.signal(), Some(SIGKILL_CODE))
                        && crate::cgroup::exec_cgroup_oom_killed(pid).unwrap_or(false)
                    {
                        oom_killed = true;
                        cgroup_memory_max_bytes = crate::cgroup::exec_cgroup_memory_max_bytes(pid);
                    }
                }
            }
            if let Some(pid) = pid {
                crate::cgroup::best_effort_cleanup_exec_cgroup(pid);
            }
            (oom_killed, cgroup_memory_max_bytes)
        }
        #[cfg(not(target_os = "linux"))]
        {
            (false, None)
        }
    };

    Ok(RawExecToolCallOutput {
        exit_status,
        stdout,
        stderr,
        aggregated_output,
        timed_out,
        oom_killed,
        cgroup_memory_max_bytes,
    })
}

async fn emit_pending_delta(
    stream: &StdoutStream,
    is_stderr: bool,
    pending_delta: &mut Vec<u8>,
    emitted_deltas: &mut usize,
) {
    if pending_delta.is_empty() || *emitted_deltas >= MAX_EXEC_OUTPUT_DELTAS_PER_CALL {
        if *emitted_deltas >= MAX_EXEC_OUTPUT_DELTAS_PER_CALL {
            pending_delta.clear();
        }
        return;
    }

    let chunk = std::mem::take(pending_delta);
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
        Event { id: stream.sub_id.clone(), event_seq: 0, msg, order: stream.order.clone() }
    };
    #[allow(clippy::let_unit_value)]
    let _ = stream.tx_event.send(event).await;
    *emitted_deltas = emitted_deltas.saturating_add(1);
}

async fn read_capped<R: AsyncRead + Unpin + Send + 'static>(
    mut reader: R,
    stream: Option<StdoutStream>,
    is_stderr: bool,
    aggregate_tx: Option<Sender<Vec<u8>>>,
    mut spool: Option<tokio::fs::File>,
) -> io::Result<StreamOutput<Vec<u8>>> {
    let mut buf = Vec::with_capacity(AGGREGATE_BUFFER_INITIAL_CAPACITY);
    let mut truncated = false;
    let mut truncated_lines = 0u32;
    let mut truncated_bytes = 0usize;
    let mut tmp = [0u8; READ_CHUNK_SIZE];
    let mut emitted_deltas: usize = 0;
    let mut pending_delta: Vec<u8> = Vec::with_capacity(EXEC_DELTA_FLUSH_BYTES);
    let mut flush_deadline: Option<Pin<Box<Sleep>>> = None;

    loop {
        tokio::select! {
            read_result = reader.read(&mut tmp) => {
                let n = read_result?;
                if n == 0 {
                    break;
                }

                if let Some(stream) = &stream {
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

                    // Accumulate deltas and emit frequently enough to keep the UI live.
                    pending_delta.extend_from_slice(&tmp[..n]);

                    if emitted_deltas >= MAX_EXEC_OUTPUT_DELTAS_PER_CALL {
                        // Drop buffered payload once we've hit the live-stream cap to avoid unbounded growth.
                        pending_delta.clear();
                        flush_deadline = None;
                    } else if pending_delta.len() >= EXEC_DELTA_FLUSH_BYTES {
                        emit_pending_delta(stream, is_stderr, &mut pending_delta, &mut emitted_deltas).await;
                        flush_deadline = None;
                    } else if flush_deadline.is_none() {
                        flush_deadline = Some(Box::pin(tokio::time::sleep(EXEC_DELTA_FLUSH_INTERVAL)));
                    }
                }

                if let Some(file) = spool.as_mut() {
                    let _ = file.write_all(&tmp[..n]).await;
                }

                if let Some(tx) = &aggregate_tx {
                    let _ = tx.send(tmp[..n].to_vec()).await;
                }

                append_with_cap(
                    &mut buf,
                    &tmp[..n],
                    &mut truncated,
                    &mut truncated_lines,
                    &mut truncated_bytes,
                );
                // Continue reading to EOF to avoid back-pressure
            }
            _ = async {
                if let Some(deadline) = &mut flush_deadline {
                    deadline.as_mut().await;
                }
            }, if flush_deadline.is_some() => {
                if let Some(stream) = &stream {
                    emit_pending_delta(stream, is_stderr, &mut pending_delta, &mut emitted_deltas).await;
                } else {
                    pending_delta.clear();
                }
                flush_deadline = None;
            }
        }
    }

    // Emit any remaining buffered delta
    if let Some(stream) = &stream {
        emit_pending_delta(stream, is_stderr, &mut pending_delta, &mut emitted_deltas).await;
    }

    Ok(StreamOutput {
        text: buf,
        truncated_after_lines: truncated.then_some(truncated_lines.max(1)),
        truncated_before_bytes: (truncated_bytes > 0).then_some(truncated_bytes),
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
    fn new(child: Child) -> Self { Self { child: Some(child) } }
    fn as_mut(&mut self) -> &mut Child { self.child.as_mut().expect("child present") }
    fn disarm(&mut self) { self.child = None; }
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
