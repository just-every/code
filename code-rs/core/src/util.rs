use std::path::Path;
use std::time::Duration;

use std::sync::Arc;

use rand::Rng;
use reqwest;
use shlex::try_join;
use tokio::sync::Notify;
use tracing::debug;

use crate::config::Config;

const INITIAL_DELAY_MS: u64 = 200;
const BACKOFF_FACTOR: f64 = 2.0;

pub(crate) fn backoff(attempt: u64) -> Duration {
    let exp = BACKOFF_FACTOR.powi(attempt.saturating_sub(1) as i32);
    let base = (INITIAL_DELAY_MS as f64 * exp) as u64;
    let jitter = rand::rng().random_range(0.9..1.1);
    Duration::from_millis((base as f64 * jitter) as u64)
}

/// Blocks until the given endpoint responds, pausing between attempts with
/// exponential backoff (capped). Used to pause retries while the user is
/// offline so we resume immediately once connectivity returns.
pub(crate) async fn wait_for_connectivity(probe_url: &str) {
    // Cap individual waits to avoid very long sleeps while still backing off.
    const MAX_DELAY: Duration = Duration::from_secs(30);
    let client = reqwest::Client::new();
    let mut attempt: u64 = 1;
    loop {
        // Treat any HTTP response as proof that DNS + TLS + routing are back.
        // Servers like api.openai.com respond 4xx/421 to bare HEADs, so do
        // not gate on status here.
        if client.head(probe_url).send().await.is_ok() {
            return;
        }

        let delay = backoff(attempt).min(MAX_DELAY);
        attempt = attempt.saturating_add(1);
        tokio::time::sleep(delay).await;
    }
}

pub fn escape_command(command: &[String]) -> String {
    try_join(command.iter().map(|s| s.as_str())).unwrap_or_else(|_| command.join(" "))
}

pub fn extract_shell_script(command: &[String]) -> Option<(usize, &str)> {
    match command {
        [script] => Some((0, script.as_str())),
        [first, second, third]
            if is_shell_like_executable(first)
                && matches!(second.as_str(), "-lc" | "-c") =>
        {
            Some((2, third.as_str()))
        }
        _ => None,
    }
}

pub fn strip_bash_lc_and_escape(command: &[String]) -> String {
    extract_shell_script(command)
        .map(|(_, script)| strip_shell_wrapper_for_display(script))
        .unwrap_or_else(|| escape_command(command))
}

fn strip_shell_wrapper_for_display(script: &str) -> String {
    unwrap_profile_wrapper(script).unwrap_or(script).to_string()
}

fn unwrap_profile_wrapper(script: &str) -> Option<&str> {
    let script = script.strip_prefix("set +m; ").unwrap_or(script);
    let body = script.strip_prefix("source ")?;
    let (rc_path, wrapped) = body.split_once(" && ")?;
    if !looks_like_shell_rc_path(rc_path) {
        return None;
    }

    wrapped
        .strip_prefix('(')
        .and_then(|inner| inner.strip_suffix(')'))
        .or_else(|| {
            wrapped
                .strip_prefix("{\n")
                .and_then(|inner| inner.strip_suffix("\n}"))
        })
}

fn looks_like_shell_rc_path(path: &str) -> bool {
    let Some(home) = std::env::var_os("HOME") else {
        return false;
    };
    let home = Path::new(&home);
    path == home.join(".bashrc").to_string_lossy() || path == home.join(".zshrc").to_string_lossy()
}

pub(crate) fn is_shell_like_executable(token: &str) -> bool {
    let trimmed = token.trim_matches('"').trim_matches('\'');
    let name = Path::new(trimmed)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(trimmed)
        .to_ascii_lowercase();
    matches!(
        name.as_str(),
        "bash"
            | "bash.exe"
            | "sh"
            | "sh.exe"
            | "zsh"
            | "zsh.exe"
            | "dash"
            | "dash.exe"
            | "ksh"
            | "ksh.exe"
            | "busybox"
    )
}

#[allow(dead_code)]
pub fn notify_on_sigint() -> Arc<Notify> {
    let notify = Arc::new(Notify::new());

    tokio::spawn({
        let notify = Arc::clone(&notify);
        async move {
            loop {
                tokio::signal::ctrl_c().await.ok();
                debug!("Keyboard interrupt");
                notify.notify_waiters();
            }
        }
    });

    notify
}

#[allow(dead_code)]
pub fn is_inside_git_repo(config: &Config) -> bool {
    let mut dir = config.cwd.to_path_buf();

    loop {
        if dir.join(".git").exists() {
            return true;
        }

        if !dir.pop() {
            break;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::strip_bash_lc_and_escape;
    use std::path::PathBuf;

    fn home_rc_path(name: &str) -> String {
        let home = std::env::var_os("HOME").expect("HOME should be set for util tests");
        PathBuf::from(home).join(name).to_string_lossy().to_string()
    }

    #[test]
    fn strip_bash_lc_and_escape_hides_profile_wrapper() {
        let bashrc = home_rc_path(".bashrc");
        let command = vec![
            "/bin/bash".to_string(),
            "-lc".to_string(),
            format!("source {bashrc} && (sed -n '1,5p' file.txt)"),
        ];

        assert_eq!(strip_bash_lc_and_escape(&command), "sed -n '1,5p' file.txt");
    }

    #[test]
    fn strip_bash_lc_and_escape_shows_raw_shell_script_without_quotes() {
        let command = vec!["git status --short".to_string()];

        assert_eq!(strip_bash_lc_and_escape(&command), "git status --short");
    }

    #[test]
    fn strip_bash_lc_and_escape_shows_raw_multiline_shell_script_without_quotes() {
        let command = vec!["cat <<'EOF'\nhello\nEOF".to_string()];

        assert_eq!(strip_bash_lc_and_escape(&command), "cat <<'EOF'\nhello\nEOF");
    }

    #[test]
    fn strip_bash_lc_and_escape_hides_multiline_profile_wrapper() {
        let bashrc = home_rc_path(".bashrc");
        let command = vec![
            "/bin/bash".to_string(),
            "-lc".to_string(),
            format!(
                "set +m; source {bashrc} && {{\napply_patch <<'PATCH'\n*** Begin Patch\n*** End Patch\nPATCH\n}}"
            ),
        ];

        assert_eq!(
            strip_bash_lc_and_escape(&command),
            "apply_patch <<'PATCH'\n*** Begin Patch\n*** End Patch\nPATCH"
        );
    }

    #[test]
    fn strip_bash_lc_and_escape_preserves_user_set_plus_m_command() {
        let command = vec![
            "/bin/bash".to_string(),
            "-lc".to_string(),
            "set +m; echo done".to_string(),
        ];

        assert_eq!(strip_bash_lc_and_escape(&command), "set +m; echo done");
    }

    #[test]
    fn strip_bash_lc_and_escape_preserves_user_source_command() {
        let command = vec![
            "/bin/bash".to_string(),
            "-lc".to_string(),
            "source script.sh && echo done".to_string(),
        ];

        assert_eq!(
            strip_bash_lc_and_escape(&command),
            "source script.sh && echo done"
        );
    }

    #[test]
    fn strip_bash_lc_and_escape_preserves_other_bashrc_paths() {
        let command = vec![
            "/bin/bash".to_string(),
            "-lc".to_string(),
            "source /tmp/project/.bashrc && echo done".to_string(),
        ];

        assert_eq!(
            strip_bash_lc_and_escape(&command),
            "source /tmp/project/.bashrc && echo done"
        );
    }
}
