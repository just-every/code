use base64::Engine as _;
use chrono::Utc;
use reqwest::header::HeaderMap;
use std::borrow::Cow;
use std::path::Path;
use std::path::PathBuf;

const CLOUD_TASKS_LOG_FILE: &str = "cloud-tasks.log";
const CLOUD_TASKS_LOG_MAX_BYTES: u64 = 5 * 1024 * 1024;
const CLOUD_TASKS_LOG_BACKUPS: usize = 2;
const CLOUD_TASKS_LOG_MAX_MESSAGE_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum CloudLogLevel {
    Off = 0,
    Error = 1,
    Info = 2,
    Debug = 3,
}

fn log_level_from_env() -> CloudLogLevel {
    if let Ok(raw) = std::env::var("CODEX_CLOUD_TASKS_LOG_LEVEL") {
        let value = raw.trim().to_ascii_lowercase();
        return match value.as_str() {
            "off" | "none" | "0" => CloudLogLevel::Off,
            "error" | "warn" | "1" => CloudLogLevel::Error,
            "info" | "2" => CloudLogLevel::Info,
            "debug" | "trace" | "3" => CloudLogLevel::Debug,
            _ => CloudLogLevel::Error,
        };
    }

    if env_truthy("CODE_SUBAGENT_DEBUG") || env_truthy("CODEX_CLOUD_TASKS_DEBUG") {
        return CloudLogLevel::Debug;
    }

    CloudLogLevel::Off
}

fn env_truthy(key: &str) -> bool {
    std::env::var(key)
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

fn should_log(level: CloudLogLevel) -> bool {
    let configured = log_level_from_env();
    configured != CloudLogLevel::Off && level <= configured
}

fn user_home_dir() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("HOME") {
        return Some(PathBuf::from(home));
    }
    if let Ok(home) = std::env::var("USERPROFILE") {
        return Some(PathBuf::from(home));
    }
    None
}

fn resolve_log_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("CODEX_CLOUD_TASKS_LOG_PATH") {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }

    let base = if let Ok(dir) = std::env::var("CODEX_CLOUD_TASKS_LOG_DIR") {
        PathBuf::from(dir)
    } else if let Ok(home) = std::env::var("CODE_HOME").or_else(|_| std::env::var("CODEX_HOME")) {
        PathBuf::from(home).join("debug_logs")
    } else if let Some(home) = user_home_dir() {
        home.join(".code").join("debug_logs")
    } else {
        return None;
    };

    Some(base.join(CLOUD_TASKS_LOG_FILE))
}

fn ensure_parent_dir(path: &Path) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
}

fn rotate_log_file(path: &Path, max_bytes: u64, backups: usize) {
    if backups == 0 {
        let _ = std::fs::remove_file(path);
        return;
    }

    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    if meta.len() <= max_bytes {
        return;
    }

    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return;
    };
    let Some(dir) = path.parent() else {
        return;
    };

    let lock_path = dir.join(format!("{file_name}.rotate.lock"));
    let lock_file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path);
    let Ok(_lock_file) = lock_file else {
        return;
    };

    let Ok(meta) = std::fs::metadata(path) else {
        let _ = std::fs::remove_file(&lock_path);
        return;
    };
    if meta.len() <= max_bytes {
        let _ = std::fs::remove_file(&lock_path);
        return;
    }

    let oldest = dir.join(format!("{file_name}.{backups}"));
    let _ = std::fs::remove_file(&oldest);

    if backups > 1 {
        for idx in (1..backups).rev() {
            let from = dir.join(format!("{file_name}.{idx}"));
            let to = dir.join(format!("{file_name}.{}", idx + 1));
            let _ = std::fs::rename(&from, &to);
        }
    }

    let rotated = dir.join(format!("{file_name}.1"));
    let _ = std::fs::copy(path, &rotated);
    if let Ok(file) = std::fs::OpenOptions::new().write(true).open(path) {
        let _ = file.set_len(0);
    }

    let _ = std::fs::remove_file(&lock_path);
}

fn truncate_message(message: &str, max_bytes: usize) -> Cow<'_, str> {
    if message.len() <= max_bytes {
        return Cow::Borrowed(message);
    }
    let bytes = message.as_bytes();
    let head = String::from_utf8_lossy(&bytes[..max_bytes]).to_string();
    Cow::Owned(format!("{head}\n...truncated..."))
}

pub fn log_path_hint() -> Option<String> {
    if !logging_enabled() {
        return None;
    }
    Some(
        resolve_log_path()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| CLOUD_TASKS_LOG_FILE.to_string()),
    )
}

pub fn logging_enabled() -> bool {
    log_level_from_env() != CloudLogLevel::Off
}

pub fn append_error_log(message: impl AsRef<str>) {
    append_cloud_log(CloudLogLevel::Error, message.as_ref());
}

pub fn append_info_log(message: impl AsRef<str>) {
    append_cloud_log(CloudLogLevel::Info, message.as_ref());
}

pub fn append_debug_log(message: impl AsRef<str>) {
    append_cloud_log(CloudLogLevel::Debug, message.as_ref());
}

fn append_cloud_log(level: CloudLogLevel, message: &str) {
    if !should_log(level) {
        return;
    }
    let Some(path) = resolve_log_path() else {
        return;
    };

    ensure_parent_dir(&path);
    rotate_log_file(&path, CLOUD_TASKS_LOG_MAX_BYTES, CLOUD_TASKS_LOG_BACKUPS);

    let ts = Utc::now().to_rfc3339();
    let level_label = match level {
        CloudLogLevel::Error => "ERROR",
        CloudLogLevel::Info => "INFO",
        CloudLogLevel::Debug => "DEBUG",
        CloudLogLevel::Off => return,
    };
    let message = truncate_message(message, CLOUD_TASKS_LOG_MAX_MESSAGE_BYTES);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        use std::io::Write as _;
        let _ = writeln!(f, "[{ts}] {level_label} {message}");
    }
}

pub fn set_user_agent_suffix(suffix: &str) {
    if let Ok(mut guard) = code_core::default_client::USER_AGENT_SUFFIX.lock() {
        guard.replace(suffix.to_string());
    }
}

/// Normalize the configured base URL to a canonical form used by the backend client.
/// - trims trailing '/'
/// - appends '/backend-api' for ChatGPT hosts when missing
pub fn normalize_base_url(input: &str) -> String {
    let mut base_url = input.to_string();
    while base_url.ends_with('/') {
        base_url.pop();
    }
    if (base_url.starts_with("https://chatgpt.com")
        || base_url.starts_with("https://chat.openai.com"))
        && !base_url.contains("/backend-api")
    {
        base_url = format!("{base_url}/backend-api");
    }
    base_url
}

/// Extract the ChatGPT account id from a JWT token, when present.
pub fn extract_chatgpt_account_id(token: &str) -> Option<String> {
    let mut parts = token.split('.');
    let (_h, payload_b64, _s) = match (parts.next(), parts.next(), parts.next()) {
        (Some(h), Some(p), Some(s)) if !h.is_empty() && !p.is_empty() && !s.is_empty() => (h, p, s),
        _ => return None,
    };
    let payload_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload_b64)
        .ok()?;
    let v: serde_json::Value = serde_json::from_slice(&payload_bytes).ok()?;
    v.get("https://api.openai.com/auth")
        .and_then(|auth| auth.get("chatgpt_account_id"))
        .and_then(|id| id.as_str())
        .map(str::to_string)
}

/// Build headers for ChatGPT-backed requests: `User-Agent`, optional `Authorization`,
/// and optional `ChatGPT-Account-Id`.
pub async fn build_chatgpt_headers() -> HeaderMap {
    use reqwest::header::AUTHORIZATION;
    use reqwest::header::HeaderName;
    use reqwest::header::HeaderValue;
    use reqwest::header::USER_AGENT;

    set_user_agent_suffix("code_cloud_tasks_tui");
    let ua = code_core::default_client::get_code_user_agent(None);
    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_str(&ua).unwrap_or(HeaderValue::from_static("codex-cli")),
    );
    if let Ok(home) = code_core::config::find_code_home() {
        let am = code_login::AuthManager::new(
            home,
            code_login::AuthMode::ChatGPT,
            code_core::default_client::DEFAULT_ORIGINATOR.to_string(),
        );
        if let Some(auth) = am.auth()
            && let Ok(tok) = auth.get_token().await
            && !tok.is_empty()
        {
            let v = format!("Bearer {tok}");
            if let Ok(hv) = HeaderValue::from_str(&v) {
                headers.insert(AUTHORIZATION, hv);
            }
            if let Some(acc) = auth
                .get_account_id()
                .or_else(|| extract_chatgpt_account_id(&tok))
                && let Ok(name) = HeaderName::from_bytes(b"ChatGPT-Account-Id")
                && let Ok(hv) = HeaderValue::from_str(&acc)
            {
                headers.insert(name, hv);
            }
        }
    }
    headers
}
