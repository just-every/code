#![allow(dead_code)]

use std::path::Path;
use std::sync::{Arc, Mutex};

use code_core::config::{Config, ConfigOverrides, ConfigToml};
use code_core::protocol::EventMsg;
use code_core::spawn::CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR;
use code_core::CodexConversation;
use serde_json::Value;
use tempfile::TempDir;
use tokio::time::{timeout, Duration};
use wiremock::matchers::{method, path_regex};
use wiremock::{Match, Mock, MockServer, Request, ResponseTemplate};

/// Returns a default `Config` whose on-disk state is confined to the provided
/// temporary directory. Using a per-test directory keeps tests hermetic and
/// avoids clobbering a developer's real `~/.code` directory.
pub fn load_default_config_for_test(code_home: &TempDir) -> Config {
    Config::load_from_base_config_with_overrides(
        ConfigToml::default(),
        default_test_overrides(),
        code_home.path().to_path_buf(),
    )
    .expect("defaults for test should always succeed")
}

#[cfg(target_os = "linux")]
fn default_test_overrides() -> ConfigOverrides {
    use std::path::PathBuf;

    let infer_sandbox_path = || {
        let mut target_dir = std::env::current_exe().ok()?;
        target_dir.pop();
        if target_dir.ends_with("deps") {
            target_dir.pop();
        }
        let exe_suffix = std::env::consts::EXE_SUFFIX;
        let candidate = target_dir.join(format!("code-linux-sandbox{exe_suffix}"));
        candidate.exists().then_some(candidate)
    };

    let sandbox_path = std::env::var_os("CARGO_BIN_EXE_code-linux-sandbox")
        .map(PathBuf::from)
        .or_else(infer_sandbox_path);

    match sandbox_path {
        Some(sandbox_path) => ConfigOverrides {
            code_linux_sandbox_exe: Some(sandbox_path),
            ..ConfigOverrides::default()
        },
        None => {
            eprintln!(
                "code-linux-sandbox binary missing; running tests without linux sandbox overrides"
            );
            ConfigOverrides::default()
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn default_test_overrides() -> ConfigOverrides {
    ConfigOverrides::default()
}

/// Builds an SSE stream body from a JSON fixture template, replacing `__ID__`
/// before parsing so a single template can be reused across tests.
pub fn load_sse_fixture_with_id(path: impl AsRef<Path>, id: &str) -> String {
    let raw = std::fs::read_to_string(path).expect("read fixture template");
    let replaced = raw.replace("__ID__", id);
    let events: Vec<Value> = serde_json::from_str(&replaced).expect("parse JSON fixture");
    events
        .into_iter()
        .map(|event| {
            let kind = event
                .get("type")
                .and_then(|v| v.as_str())
                .expect("fixture event missing type");
            if event
                .as_object()
                .map(|obj| obj.len() == 1)
                .unwrap_or(false)
            {
                format!("event: {kind}\n\n")
            } else {
                format!("event: {kind}\ndata: {event}\n\n")
            }
        })
        .collect()
}

/// Waits for the next event that matches `predicate`, timing out to surface
/// hung conversations quickly during tests.
pub async fn wait_for_event<F>(conversation: &CodexConversation, mut predicate: F) -> EventMsg
where
    F: FnMut(&EventMsg) -> bool,
{
    loop {
        let event = timeout(Duration::from_secs(5), conversation.next_event())
            .await
            .expect("timeout waiting for event")
            .expect("event stream ended unexpectedly");
        if predicate(&event.msg) {
            return event.msg;
        }
    }
}

/// Returns true when network-dependent tests should be skipped.
pub fn skip_if_no_network() -> bool {
    if std::env::var(CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR).is_ok() {
        println!(
            "Skipping test because network access is disabled inside the sandbox."
        );
        true
    } else {
        false
    }
}

#[derive(Debug, Clone)]
pub struct ResponseMock {
    requests: Arc<Mutex<Vec<Request>>>,
}

impl ResponseMock {
    pub fn new() -> Self {
        Self {
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn record(&self, request: Request) {
        self.requests.lock().unwrap().push(request);
    }

    /// Returns the JSON body for the only recorded request, panicking if the
    /// mock saw zero or multiple requests.
    pub fn single_body_json(&self) -> Value {
        let requests = self.requests.lock().unwrap();
        if requests.len() != 1 {
            panic!("expected 1 request, got {}", requests.len());
        }
        requests.first().unwrap().body_json().unwrap()
    }
}

struct RequestCapture {
    recorder: ResponseMock,
}

impl Match for RequestCapture {
    fn matches(&self, request: &Request) -> bool {
        self.recorder.record(request.clone());
        true
    }
}

fn sse_response(body: String) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("content-type", "text/event-stream")
        .set_body_string(body)
}

/// Mounts a single-use SSE response handler that also captures request bodies
/// so tests can assert against the payload that was sent to the model.
pub async fn mount_sse_once(server: &MockServer, body: String) -> ResponseMock {
    let recorder = ResponseMock::new();
    let capture = RequestCapture {
        recorder: recorder.clone(),
    };

    Mock::given(method("POST"))
        .and(path_regex(".*/responses$"))
        .and(capture)
        .respond_with(sse_response(body))
        .up_to_n_times(1)
        .mount(server)
        .await;

    recorder
}
