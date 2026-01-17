use crate::spawn::CODEX_SANDBOX_ENV_VAR;
use reqwest::header::HeaderValue;
use std::sync::LazyLock;
use std::sync::Mutex;
use std::sync::RwLock;

pub const DEFAULT_ORIGINATOR: &str = "code_cli_rs";
pub const CODEX_INTERNAL_ORIGINATOR_OVERRIDE_ENV_VAR: &str = "CODEX_INTERNAL_ORIGINATOR_OVERRIDE";

/// Optional suffix for the Codex User-Agent string.
///
/// This is primarily used by the MCP server implementation to include
/// client-provided identity in the UA. Because there is a single MCP server
/// per process, a global is acceptable here. Other callers should prefer
/// passing an explicit originator.
pub static USER_AGENT_SUFFIX: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));

#[derive(Debug, Clone)]
pub struct Originator {
    pub value: String,
    pub header_value: HeaderValue,
}

static ORIGINATOR: LazyLock<RwLock<Option<Originator>>> = LazyLock::new(|| RwLock::new(None));

#[derive(Debug)]
pub enum SetOriginatorError {
    InvalidHeaderValue,
    AlreadyInitialized,
}

fn get_originator_value(provided: Option<String>) -> Originator {
    let value = std::env::var(CODEX_INTERNAL_ORIGINATOR_OVERRIDE_ENV_VAR)
        .ok()
        .or(provided)
        .unwrap_or(DEFAULT_ORIGINATOR.to_string());

    match HeaderValue::from_str(&value) {
        Ok(header_value) => Originator {
            value,
            header_value,
        },
        Err(e) => {
            tracing::error!("Unable to turn originator override {value} into header value: {e}");
            Originator {
                value: DEFAULT_ORIGINATOR.to_string(),
                header_value: HeaderValue::from_static(DEFAULT_ORIGINATOR),
            }
        }
    }
}

pub fn set_default_originator(value: String) -> Result<(), SetOriginatorError> {
    if HeaderValue::from_str(&value).is_err() {
        return Err(SetOriginatorError::InvalidHeaderValue);
    }
    let originator = get_originator_value(Some(value));
    let Ok(mut guard) = ORIGINATOR.write() else {
        return Err(SetOriginatorError::AlreadyInitialized);
    };
    if guard.is_some() {
        return Err(SetOriginatorError::AlreadyInitialized);
    }
    *guard = Some(originator);
    Ok(())
}

pub fn originator() -> Originator {
    if let Ok(guard) = ORIGINATOR.read()
        && let Some(originator) = guard.as_ref()
    {
        return originator.clone();
    }

    if std::env::var(CODEX_INTERNAL_ORIGINATOR_OVERRIDE_ENV_VAR).is_ok() {
        let originator = get_originator_value(None);
        if let Ok(mut guard) = ORIGINATOR.write() {
            match guard.as_ref() {
                Some(originator) => return originator.clone(),
                None => *guard = Some(originator.clone()),
            }
        }
        return originator;
    }

    get_originator_value(None)
}

pub fn get_code_user_agent(originator_override: Option<&str>) -> String {
    let build_version = code_version::version();
    let os_info = os_info::get();
    let originator_value = originator_override
        .map(str::to_string)
        .unwrap_or_else(|| originator().value);
    let prefix = format!(
        "{originator_value}/{build_version} ({} {}; {}) {}",
        os_info.os_type(),
        os_info.version(),
        os_info.architecture().unwrap_or("unknown"),
        crate::terminal::user_agent()
    );
    let suffix = USER_AGENT_SUFFIX
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map_or_else(String::new, |v| format!(" ({v})"));

    let candidate = format!("{prefix}{suffix}");
    sanitize_user_agent(candidate, &prefix)
}

/// Convenience wrapper using the default originator.
pub fn get_code_user_agent_default() -> String {
    get_code_user_agent(None)
}

/// Replace invalid header characters with '_' and ensure the UA is syntactically valid.
fn sanitize_user_agent(candidate: String, fallback: &str) -> String {
    if HeaderValue::from_str(candidate.as_str()).is_ok() {
        return candidate;
    }
    let sanitized: String = candidate
        .chars()
        .map(|ch| if matches!(ch, ' '..='~') { ch } else { '_' })
        .collect();
    if !sanitized.is_empty() && HeaderValue::from_str(sanitized.as_str()).is_ok() {
        tracing::warn!("Sanitized Codex user agent because provided suffix contained invalid header characters");
        sanitized
    } else if HeaderValue::from_str(fallback).is_ok() {
        tracing::warn!("Falling back to base Codex user agent because provided suffix could not be sanitized");
        fallback.to_string()
    } else {
        tracing::warn!("Falling back to default Codex originator because base user agent string is invalid");
        DEFAULT_ORIGINATOR.to_string()
    }
}

/// Create a reqwest client with default `originator` and `User-Agent` headers set.
pub fn create_client(originator: &str) -> reqwest::Client {
    use reqwest::header::HeaderMap;
    use reqwest::header::HeaderValue;

    let mut headers = HeaderMap::new();
    let originator = get_originator_value(Some(originator.to_string()));
    let originator_value = HeaderValue::from_str(&originator.value)
        .unwrap_or_else(|_| HeaderValue::from_static(DEFAULT_ORIGINATOR));
    headers.insert("originator", originator_value);
    let ua = get_code_user_agent(Some(originator.value.as_str()));

    let mut builder = reqwest::Client::builder()
        // Set UA via dedicated helper to avoid header validation pitfalls
        .user_agent(ua)
        .default_headers(headers);
    if is_sandboxed() {
        builder = builder.no_proxy();
    }

    builder.build().unwrap_or_else(|_| reqwest::Client::new())
}

fn is_sandboxed() -> bool {
    std::env::var(CODEX_SANDBOX_ENV_VAR).as_deref() == Ok("seatbelt")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_code_user_agent() {
        let user_agent = get_code_user_agent(None);
        assert!(user_agent.starts_with("code_cli_rs/"));
    }

    #[tokio::test]
    async fn test_create_client_sets_default_headers() {
        use wiremock::Mock;
        use wiremock::MockServer;
        use wiremock::ResponseTemplate;
        use wiremock::matchers::method;
        use wiremock::matchers::path;

        let originator = "test_originator";
        let client = create_client(originator);

        // Spin up a local mock server and capture a request.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let resp = client
            .get(server.uri())
            .send()
            .await
            .expect("failed to send request");
        assert!(resp.status().is_success());

        let requests = server
            .received_requests()
            .await
            .expect("failed to fetch received requests");
        assert!(!requests.is_empty());
        let headers = &requests[0].headers;

        // originator header is set to the provided value
        let originator_header = headers
            .get("originator")
            .expect("originator header missing");
        assert_eq!(originator_header.to_str().unwrap(), originator);

        // User-Agent matches the computed Codex UA for that originator
        let expected_ua = get_code_user_agent(Some(originator));
        let ua_header = headers
            .get("user-agent")
            .expect("user-agent header missing");
        assert_eq!(ua_header.to_str().unwrap(), expected_ua);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_macos() {
        use regex_lite::Regex;
        let user_agent = get_code_user_agent(None);
        let re = Regex::new(
            r"^code_cli_rs/\d+\.\d+\.\d+ \(Mac OS \d+\.\d+\.\d+; (x86_64|arm64)\) (\S+)$",
        )
        .unwrap();
        assert!(re.is_match(&user_agent));
    }
}
