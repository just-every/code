use crate::exec::ExecToolCallOutput;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use reqwest::StatusCode;
use serde_json;
use std::io;
use std::time::Duration;
use thiserror::Error;
use tokio::task::JoinError;
use uuid::Uuid;

pub type Result<T> = std::result::Result<T, CodexErr>;

#[derive(Error, Debug)]
pub enum SandboxErr {
    /// Error from sandbox execution
    #[error(
        "sandbox denied exec error, exit code: {}, stdout: {}, stderr: {}",
        .output.exit_code, .output.stdout.text, .output.stderr.text
    )]
    Denied { output: Box<ExecToolCallOutput> },

    /// Error from linux seccomp filter setup
    #[cfg(target_os = "linux")]
    #[error("seccomp setup error")]
    SeccompInstall(#[from] seccompiler::Error),

    /// Error from linux seccomp backend
    #[cfg(target_os = "linux")]
    #[error("seccomp backend error")]
    SeccompBackend(#[from] seccompiler::BackendError),

    /// Command timed out
    #[error("command timed out")]
    Timeout { output: Box<ExecToolCallOutput> },

    /// Command was killed by a signal
    #[error("command was killed by a signal")]
    Signal(i32),

    /// Command exceeded its memory limit and was killed.
    #[error(
        "command exceeded memory limit{}",
        memory_max_bytes
            .map(|bytes| format!(": {} bytes", bytes))
            .unwrap_or_default()
    )]
    OutOfMemory {
        output: Box<ExecToolCallOutput>,
        memory_max_bytes: Option<u64>,
    },

    /// Error from linux landlock
    #[error("Landlock was not able to fully enforce all sandbox rules")]
    LandlockRestrict,
}

#[derive(Debug, Clone)]
pub struct RetryAfter {
    pub delay: Duration,
    pub resume_at: DateTime<Utc>,
}

impl RetryAfter {
    pub fn from_duration(delay: Duration, now: DateTime<Utc>) -> Self {
        let resume_at = now
            + ChronoDuration::from_std(delay)
                .unwrap_or_else(|_| ChronoDuration::zero());
        Self { delay, resume_at }
    }

    pub fn from_resume_at(resume_at: DateTime<Utc>, now: DateTime<Utc>) -> Self {
        let clamped = if resume_at < now { now } else { resume_at };
        let delay = clamped
            .signed_duration_since(now)
            .to_std()
            .unwrap_or_else(|_| Duration::ZERO);
        Self {
            delay,
            resume_at: clamped,
        }
    }
}

#[derive(Error, Debug)]
pub enum CodexErr {
    /// Returned by ResponsesClient when the SSE stream disconnects or errors out **after** the HTTP
    /// handshake has succeeded but **before** it finished emitting `response.completed`.
    ///
    /// The Session loop treats this as a transient error and will automatically retry the turn.
    ///
    /// Optionally includes the requested delay before retrying the turn.
    #[error("stream disconnected before completion: {0}")]
    Stream(String, Option<RetryAfter>, Option<String>),

    #[error("no conversation with id: {0}")]
    ConversationNotFound(Uuid),

    #[error("session configured event was not the first event in the stream")]
    SessionConfiguredNotFirstEvent,

    /// Returned by run_command_stream when the spawned child process timed out (default 120s).
    #[error("timeout waiting for child process to exit")]
    Timeout,

    /// Returned by run_command_stream when the child could not be spawned (its stdout/stderr pipes
    /// could not be captured). Analogous to the previous `CodexError::Spawn` variant.
    #[error("spawn failed: child stdout/stderr not captured")]
    Spawn,

    /// Returned by run_command_stream when the user pressed Ctrl‑C (SIGINT). Session uses this to
    /// surface a polite FunctionCallOutput back to the model instead of crashing the CLI.
    #[error("interrupted (Ctrl-C)")]
    Interrupted,

    /// Unexpected HTTP status code.
    #[error("{0}")]
    UnexpectedStatus(UnexpectedResponseError),

    #[error("{0}")]
    UsageLimitReached(UsageLimitReachedError),

    #[error("Quota exceeded. Check your plan and billing details.")]
    QuotaExceeded,

    #[error("Authentication expired. {0}")]
    AuthRefreshPermanent(String),

    #[error(
        "To use Codex with your ChatGPT plan, upgrade to Plus: https://openai.com/chatgpt/pricing."
    )]
    UsageNotIncluded,

    /// Final (non‑retryable by our policy) HTTP 5xx from the model provider.
    ///
    /// We intentionally embed a preformatted message string so the UI and
    /// logs surface actionable context (status, request-id, body excerpt)
    /// without having to plumb additional fields everywhere.
    #[error("{0}")]
    ServerError(String),

    /// Retry limit exceeded.
    #[error("{0}")]
    RetryLimit(RetryLimitReachedError),

    /// Agent loop died unexpectedly
    #[error("internal error; agent loop died unexpectedly")]
    InternalAgentDied,

    /// Sandbox error
    #[error("sandbox error: {0}")]
    Sandbox(#[from] SandboxErr),

    #[error("codex-linux-sandbox was required but not provided")]
    LandlockSandboxExecutableNotProvided,

    #[error("unsupported operation: {0}")]
    UnsupportedOperation(String),

    // -----------------------------------------------------------------
    // Automatic conversions for common external error types
    // -----------------------------------------------------------------
    #[error(transparent)]
    Io(#[from] io::Error),

    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[cfg(target_os = "linux")]
    #[error(transparent)]
    LandlockRuleset(#[from] landlock::RulesetError),

    #[cfg(target_os = "linux")]
    #[error(transparent)]
    LandlockPathFd(#[from] landlock::PathFdError),

    #[error(transparent)]
    TokioJoin(#[from] JoinError),

    #[error("{0}")]
    EnvVar(EnvVarError),
}

#[derive(Debug)]
pub struct UnexpectedResponseError {
    pub status: StatusCode,
    pub body: String,
    pub request_id: Option<String>,
}

impl std::fmt::Display for UnexpectedResponseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "unexpected status {}: {}{}",
            self.status,
            self.body,
            self.request_id
                .as_ref()
                .map(|id| format!(", request id: {id}"))
                .unwrap_or_default()
        )
    }
}

impl std::error::Error for UnexpectedResponseError {}
#[derive(Debug)]
pub struct RetryLimitReachedError {
    pub status: StatusCode,
    pub request_id: Option<String>,
    /// Whether the underlying failure looked transient (network/5xx/429/etc.).
    /// Callers can use this to decide if longer-horizon retries are safe.
    pub retryable: bool,
}

impl std::fmt::Display for RetryLimitReachedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "exceeded retry limit, last status: {}{}",
            self.status,
            self.request_id
                .as_ref()
                .map(|id| format!(", request id: {id}"))
                .unwrap_or_default()
        )
    }
}

#[derive(Debug)]
pub struct UsageLimitReachedError {
    pub plan_type: Option<String>,
    pub resets_in_seconds: Option<u64>,
}

impl UsageLimitReachedError {
    pub fn retry_after(&self, now: DateTime<Utc>) -> Option<RetryAfter> {
        let seconds = self.resets_in_seconds?;
        let delay = Duration::from_secs(seconds).saturating_add(Duration::from_secs(5));
        Some(RetryAfter::from_duration(delay, now))
    }
}

impl std::fmt::Display for UsageLimitReachedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Base message differs slightly for legacy ChatGPT Plus plan users.
        let is_plus = matches!(&self.plan_type, Some(p) if p == "plus");
        if is_plus {
            write!(
                f,
                "You've hit your usage limit. Upgrade to Pro (https://openai.com/chatgpt/pricing) or try again"
            )?;
            if let Some(secs) = self.resets_in_seconds {
                let reset_duration = format_reset_duration(secs);
                write!(f, " in {reset_duration}.")?;
            } else {
                write!(f, " later.")?;
            }
        } else {
            write!(f, "You've hit your usage limit.")?;

            if let Some(secs) = self.resets_in_seconds {
                let reset_duration = format_reset_duration(secs);
                write!(f, " Try again in {reset_duration}.")?;
            } else {
                write!(f, " Try again later.")?;
            }
        }

        Ok(())
    }
}

fn format_reset_duration(total_secs: u64) -> String {
    let days = total_secs / 86_400;
    let hours = (total_secs % 86_400) / 3_600;
    let minutes = (total_secs % 3_600) / 60;

    let mut parts: Vec<String> = Vec::new();
    if days > 0 {
        let unit = if days == 1 { "day" } else { "days" };
        parts.push(format!("{days} {unit}"));
    }
    if hours > 0 {
        let unit = if hours == 1 { "hour" } else { "hours" };
        parts.push(format!("{hours} {unit}"));
    }
    if minutes > 0 {
        let unit = if minutes == 1 { "minute" } else { "minutes" };
        parts.push(format!("{minutes} {unit}"));
    }

    if parts.is_empty() {
        return "less than a minute".to_string();
    }

    match parts.len() {
        1 => parts[0].clone(),
        2 => format!("{} {}", parts[0], parts[1]),
        _ => format!("{} {} {}", parts[0], parts[1], parts[2]),
    }
}

#[derive(Debug)]
pub struct EnvVarError {
    /// Name of the environment variable that is missing.
    pub var: String,

    /// Optional instructions to help the user get a valid value for the
    /// variable and set it.
    pub instructions: Option<String>,
}

impl std::fmt::Display for EnvVarError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Missing environment variable: `{}`.", self.var)?;
        if let Some(instructions) = &self.instructions {
            write!(f, " {instructions}")?;
        }
        Ok(())
    }
}

impl CodexErr {
    /// Minimal shim so that existing `e.downcast_ref::<CodexErr>()` checks continue to compile
    /// after replacing `anyhow::Error` in the return signature. This mirrors the behavior of
    /// `anyhow::Error::downcast_ref` but works directly on our concrete enum.
    pub fn downcast_ref<T: std::any::Any>(&self) -> Option<&T> {
        (self as &dyn std::any::Any).downcast_ref::<T>()
    }
}

pub fn get_error_message_ui(e: &CodexErr) -> String {
    match e {
        CodexErr::Sandbox(SandboxErr::Denied { output }) => output.stderr.text.clone(),
        CodexErr::Sandbox(SandboxErr::OutOfMemory {
            output,
            memory_max_bytes,
        }) => {
            let limit_note = memory_max_bytes
                .map(|bytes| format!(" (memory.max={bytes} bytes)"))
                .unwrap_or_default();
            format!("error: command exceeded memory limit{limit_note}\n{}", output.stderr.text)
        }
        // Timeouts are not sandbox errors from a UX perspective; present them plainly
        CodexErr::Sandbox(SandboxErr::Timeout { output }) => format!(
            "error: command timed out after {} ms",
            output.duration.as_millis()
        ),
        _ => e.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn usage_limit_reached_error_formats_plus_plan() {
        let err = UsageLimitReachedError {
            plan_type: Some("plus".to_string()),
            resets_in_seconds: None,
        };
        assert_eq!(
            err.to_string(),
            "You've hit your usage limit. Upgrade to Pro (https://openai.com/chatgpt/pricing) or try again later."
        );
    }

    #[test]
    fn usage_limit_reached_error_formats_default_when_none() {
        let err = UsageLimitReachedError {
            plan_type: None,
            resets_in_seconds: None,
        };
        assert_eq!(
            err.to_string(),
            "You've hit your usage limit. Try again later."
        );
    }

    #[test]
    fn usage_limit_reached_error_formats_default_for_other_plans() {
        let err = UsageLimitReachedError {
            plan_type: Some("pro".to_string()),
            resets_in_seconds: None,
        };
        assert_eq!(
            err.to_string(),
            "You've hit your usage limit. Try again later."
        );
    }

    #[test]
    fn usage_limit_reached_includes_minutes_when_available() {
        let err = UsageLimitReachedError {
            plan_type: None,
            resets_in_seconds: Some(5 * 60),
        };
        assert_eq!(
            err.to_string(),
            "You've hit your usage limit. Try again in 5 minutes."
        );
    }

    #[test]
    fn usage_limit_reached_includes_hours_and_minutes() {
        let err = UsageLimitReachedError {
            plan_type: Some("plus".to_string()),
            resets_in_seconds: Some(3 * 3600 + 32 * 60),
        };
        assert_eq!(
            err.to_string(),
            "You've hit your usage limit. Upgrade to Pro (https://openai.com/chatgpt/pricing) or try again in 3 hours 32 minutes."
        );
    }

    #[test]
    fn usage_limit_reached_includes_days_hours_minutes() {
        let err = UsageLimitReachedError {
            plan_type: None,
            resets_in_seconds: Some(2 * 86_400 + 3 * 3600 + 5 * 60),
        };
        assert_eq!(
            err.to_string(),
            "You've hit your usage limit. Try again in 2 days 3 hours 5 minutes."
        );
    }

    #[test]
    fn usage_limit_reached_less_than_minute() {
        let err = UsageLimitReachedError {
            plan_type: None,
            resets_in_seconds: Some(30),
        };
        assert_eq!(
            err.to_string(),
            "You've hit your usage limit. Try again in less than a minute."
        );
    }
}
