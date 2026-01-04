use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Child;
use tokio::process::Command;
use tokio::time::sleep;
use tracing::trace;

use crate::protocol::SandboxPolicy;

/// Experimental environment variable that will be set to some non-empty value
/// if both of the following are true:
///
/// 1. The process was spawned by Codex as part of a shell tool call.
/// 2. SandboxPolicy.has_full_network_access() was false for the tool call.
///
/// We may try to have just one environment variable for all sandboxing
/// attributes, so this may change in the future.
pub const CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR: &str = "CODEX_SANDBOX_NETWORK_DISABLED";

/// Should be set when the process is spawned under a sandbox. Currently, the
/// value is "seatbelt" for macOS, but it may change in the future to
/// accommodate sandboxing configuration and other sandboxing mechanisms.
pub const CODEX_SANDBOX_ENV_VAR: &str = "CODEX_SANDBOX";

const SPAWN_RETRY_DELAYS_MS: [u64; 3] = [0, 10, 50];

fn is_temporary_resource_error(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::WouldBlock
        || matches!(err.raw_os_error(), Some(35) | Some(libc::ENOMEM))
}

fn spawn_with_retry_blocking<F, T>(mut spawn: F) -> io::Result<T>
where
    F: FnMut() -> io::Result<T>,
{
    let mut last_err: Option<io::Error> = None;
    for delay_ms in SPAWN_RETRY_DELAYS_MS {
        match spawn() {
            Ok(child) => return Ok(child),
            Err(err) if is_temporary_resource_error(&err) => {
                last_err = Some(err);
                if delay_ms > 0 {
                    std::thread::sleep(Duration::from_millis(delay_ms));
                }
            }
            Err(err) => return Err(err),
        }
    }

    Err(last_err.unwrap_or_else(|| io::Error::other("spawn failed")))
}

async fn spawn_with_retry_async<F, T>(mut spawn: F) -> io::Result<T>
where
    F: FnMut() -> io::Result<T>,
{
    let mut last_err: Option<io::Error> = None;
    for delay_ms in SPAWN_RETRY_DELAYS_MS {
        match spawn() {
            Ok(child) => return Ok(child),
            Err(err) if is_temporary_resource_error(&err) => {
                last_err = Some(err);
                if delay_ms > 0 {
                    sleep(Duration::from_millis(delay_ms)).await;
                }
            }
            Err(err) => return Err(err),
        }
    }

    Err(last_err.unwrap_or_else(|| io::Error::other("spawn failed")))
}

pub fn spawn_std_command_with_retry(
    cmd: &mut std::process::Command,
) -> io::Result<std::process::Child> {
    spawn_with_retry_blocking(|| cmd.spawn())
}

pub async fn spawn_tokio_command_with_retry(cmd: &mut Command) -> io::Result<Child> {
    spawn_with_retry_async(|| cmd.spawn()).await
}

#[derive(Debug, Clone, Copy)]
pub enum StdioPolicy {
    RedirectForShellTool,
    Inherit,
}

/// Spawns the appropriate child process for the ExecParams and SandboxPolicy,
/// ensuring the args and environment variables used to create the `Command`
/// (and `Child`) honor the configuration.
///
/// For now, we take `SandboxPolicy` as a parameter to spawn_child() because
/// we need to determine whether to set the
/// `CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR` environment variable.
pub(crate) async fn spawn_child_async(
    program: PathBuf,
    args: Vec<String>,
    #[cfg_attr(not(unix), allow(unused_variables))] arg0: Option<&str>,
    cwd: PathBuf,
    sandbox_policy: &SandboxPolicy,
    stdio_policy: StdioPolicy,
    env: HashMap<String, String>,
) -> std::io::Result<Child> {
    trace!(
        "spawn_child_async: {program:?} {args:?} {arg0:?} {cwd:?} {sandbox_policy:?} {stdio_policy:?} {env:?}"
    );

    let mut cmd = Command::new(&program);
    #[cfg(unix)]
    cmd.arg0(arg0.map_or_else(|| program.to_string_lossy().to_string(), String::from));
    cmd.args(args);
    cmd.current_dir(cwd);
    cmd.env_clear();
    cmd.envs(env);

    if !sandbox_policy.has_full_network_access() {
        cmd.env(CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR, "1");
    }

    // If this Codex process dies (including being killed via SIGKILL), we want
    // any child processes that were spawned as part of a `"shell"` tool call
    // to also be terminated.

    // Ensure children form their own process group; on timeout we can kill the group.
    // Also, on Linux, set PDEATHSIG so children die if parent dies.
    #[cfg(unix)]
    unsafe {
        #[cfg(target_os = "linux")]
        let exec_memory_max_bytes = match stdio_policy {
            StdioPolicy::RedirectForShellTool => crate::cgroup::default_exec_memory_max_bytes(),
            StdioPolicy::Inherit => None,
        };
        cmd.pre_exec(move || {
            // Start a new process group
            let _ = libc::setpgid(0, 0);
            #[cfg(target_os = "linux")]
            {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::getppid() == 1 {
                    libc::raise(libc::SIGTERM);
                }

                if let Some(memory_max_bytes) = exec_memory_max_bytes {
                    crate::cgroup::best_effort_attach_self_to_exec_cgroup(
                        libc::getpid() as u32,
                        memory_max_bytes,
                    );
                }
            }
            Ok(())
        });
    }

    match stdio_policy {
        StdioPolicy::RedirectForShellTool => {
            // Do not create a file descriptor for stdin because otherwise some
            // commands may hang forever waiting for input. For example, ripgrep has
            // a heuristic where it may try to read from stdin as explained here:
            // https://github.com/BurntSushi/ripgrep/blob/e2362d4d5185d02fa857bf381e7bd52e66fafc73/crates/core/flags/hiargs.rs#L1101-L1103
            cmd.stdin(Stdio::null());

            cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        }
        StdioPolicy::Inherit => {
            // Inherit stdin, stdout, and stderr from the parent process.
            cmd.stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit());
        }
    }

    cmd.kill_on_drop(true);

    spawn_tokio_command_with_retry(&mut cmd).await
}
