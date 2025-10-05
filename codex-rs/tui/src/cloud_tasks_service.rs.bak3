use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use codex_cloud_tasks_client::ApplyOutcome;
use codex_cloud_tasks_client::CloudBackend;
use codex_cloud_tasks_client::CreatedTask;
use codex_cloud_tasks_client::HttpClient;
use codex_cloud_tasks_client::MockClient;
use codex_cloud_tasks_client::TaskId;
use codex_cloud_tasks_client::TaskSummary;
use codex_login::AuthManager;
use codex_login::AuthMode;
use reqwest::header::HeaderMap;
use reqwest::header::HeaderName;
use reqwest::header::HeaderValue;
use reqwest::header::AUTHORIZATION;
use reqwest::header::USER_AGENT;
use serde::Deserialize;

#[derive(Clone, Debug)]
pub struct CloudEnvironment {
    pub id: String,
    pub label: Option<String>,
    pub repo_hints: Option<String>,
    pub is_pinned: bool,
}

struct CloudTasksConfig {
    base_url: String,
    token: Option<String>,
    account_id: Option<String>,
    use_mock: bool,
}

pub async fn fetch_tasks(environment: Option<String>) -> Result<Vec<TaskSummary>> {
    let config = load_config().await?;
    let backend = build_backend(&config)?;
    backend
        .list_tasks(environment.as_deref())
        .await
        .map_err(|err| anyhow!("list cloud tasks failed: {err}"))
}

pub async fn fetch_task_diff(task_id: TaskId) -> Result<Option<String>> {
    let config = load_config().await?;
    let backend = build_backend(&config)?;
    backend
        .get_task_diff(task_id.clone())
        .await
        .map_err(|err| anyhow!("fetch diff for {} failed: {err}", task_id.0))
}

pub async fn fetch_task_messages(task_id: TaskId) -> Result<Vec<String>> {
    let config = load_config().await?;
    let backend = build_backend(&config)?;
    backend
        .get_task_messages(task_id.clone())
        .await
        .map_err(|err| anyhow!("fetch messages for {} failed: {err}", task_id.0))
}

pub async fn apply_task(task_id: TaskId, preflight: bool) -> Result<ApplyOutcome> {
    let config = load_config().await?;
    let backend = build_backend(&config)?;
    let fut = if preflight {
        backend.apply_task_preflight(task_id.clone(), None)
    } else {
        backend.apply_task(task_id.clone(), None)
    };
    fut.await.map_err(|err| anyhow!("apply task {} failed: {err}", task_id.0))
}

pub async fn create_task(env_id: String, prompt: String, best_of_n: usize) -> Result<CreatedTask> {
    let config = load_config().await?;
    if config.use_mock {
        let backend = build_backend(&config)?;
        return backend
            .create_task(&env_id, &prompt, "main", false, best_of_n)
            .await
            .map_err(|err| anyhow!("create mock task failed: {err}"));
    }

    let backend = build_backend(&config)?;
    let git_ref = detect_git_ref().await.unwrap_or_else(|| "main".to_string());
    backend
        .create_task(&env_id, &prompt, &git_ref, false, best_of_n)
        .await
        .map_err(|err| anyhow!("create task failed: {err}"))
}

pub async fn fetch_environments() -> Result<Vec<CloudEnvironment>> {
    let config = load_config().await?;
    if config.use_mock {
        return Ok(vec![CloudEnvironment {
            id: "mock".to_string(),
            label: Some("Mock environment".to_string()),
            repo_hints: None,
            is_pinned: true,
        }]);
    }

    let client = reqwest::Client::builder()
        .build()
        .context("build reqwest client")?;
    let mut headers = HeaderMap::new();
    let ua = codex_core::default_client::get_codex_user_agent(None);
    headers.insert(
        USER_AGENT,
        HeaderValue::from_str(&ua).unwrap_or(HeaderValue::from_static("codex-cli")),
    );
    if let Some(token) = &config.token {
        if let Ok(value) = HeaderValue::from_str(&format!("Bearer {token}")) {
            headers.insert(AUTHORIZATION, value);
        }
    }
    if let Some(account) = &config.account_id {
        if let Ok(name) = HeaderName::from_bytes(b"ChatGPT-Account-Id")
            && let Ok(value) = HeaderValue::from_str(account)
        {
            headers.insert(name, value);
        }
    }

    let url = environments_url(&config.base_url);
    let response = client
        .get(url.clone())
        .headers(headers)
        .send()
        .await
        .map_err(|err| anyhow!("GET {url} failed: {err}"))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow!("GET {url} failed: {status}; body={body}"));
    }

    let entries: Vec<EnvironmentEntry> = response
        .json()
        .await
        .map_err(|err| anyhow!("decode environments response failed: {err}"))?;

    let mut map: HashMap<String, CloudEnvironment> = HashMap::new();
    for entry in entries {
        let e = map.entry(entry.id.clone()).or_insert_with(|| CloudEnvironment {
            id: entry.id,
            label: entry.label.clone(),
            repo_hints: entry.repo_hints.clone(),
            is_pinned: entry.is_pinned.unwrap_or(false),
        });
        if e.label.is_none() {
            e.label = entry.label;
        }
        if e.repo_hints.is_none() {
            e.repo_hints = entry.repo_hints;
        }
        e.is_pinned = e.is_pinned || entry.is_pinned.unwrap_or(false);
    }

    let mut environments: Vec<CloudEnvironment> = map.into_values().collect();
    environments.sort_by(|a, b| {
        b.is_pinned
            .cmp(&a.is_pinned)
            .then_with(|| a.label.as_deref().unwrap_or("").cmp(b.label.as_deref().unwrap_or("")))
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(environments)
}

fn environments_url(base_url: &str) -> String {
    if base_url.contains("/backend-api") {
        format!("{base_url}/wham/environments")
    } else {
        format!("{base_url}/api/codex/environments")
    }
}

async fn detect_git_ref() -> Option<String> {
    let cwd = std::env::current_dir().ok()?;
    codex_core::git_info::current_branch_name(&cwd).await
}

fn build_backend(config: &CloudTasksConfig) -> Result<Arc<dyn CloudBackend>> {
    if config.use_mock {
        return Ok(Arc::new(MockClient));
    }

    let ua = codex_core::default_client::get_codex_user_agent(None);
    let mut client = HttpClient::new(config.base_url.clone()).context("create cloud http client")?;
    client = client.with_user_agent(ua);
    if let Some(token) = &config.token {
        client = client.with_bearer_token(token.clone());
    }
    if let Some(account) = &config.account_id {
        client = client.with_chatgpt_account_id(account.clone());
    }
    Ok(Arc::new(client))
}

async fn load_config() -> Result<CloudTasksConfig> {
    let base_url_env = std::env::var("CODEX_CLOUD_TASKS_BASE_URL")
        .unwrap_or_else(|_| "https://chatgpt.com/backend-api".to_string());
    let base_url = normalize_base_url(&base_url_env);
    let use_mock = std::env::var("CODEX_CLOUD_TASKS_MODE")
        .ok()
        .map(|mode| mode.eq_ignore_ascii_case("mock"))
        .unwrap_or(false);
    if use_mock {
        return Ok(CloudTasksConfig {
            base_url,
            token: None,
            account_id: None,
            use_mock,
        });
    }

    let codex_home = codex_core::config::find_codex_home()
        .context("determine codex home directory")?;
    let auth_manager = AuthManager::new(
        codex_home,
        AuthMode::ChatGPT,
        codex_core::default_client::DEFAULT_ORIGINATOR.to_string(),
    );
    let auth = auth_manager
        .auth()
        .ok_or_else(|| anyhow!("Not signed in. Run `codex login` to authenticate with ChatGPT."))?;
    let token = auth
        .get_token()
        .await
        .context("retrieve ChatGPT access token")?;
    if token.is_empty() {
        return Err(anyhow!("ChatGPT access token is empty"));
    }
    let account_id = auth
        .get_account_id()
        .or_else(|| extract_chatgpt_account_id(&token));
    Ok(CloudTasksConfig {
        base_url,
        token: Some(token),
        account_id,
        use_mock: false,
    })
}

fn normalize_base_url(input: &str) -> String {
    let mut url = input.trim().to_string();
    while url.ends_with('/') {
        url.pop();
    }
    if (url.starts_with("https://chatgpt.com") || url.starts_with("https://chat.openai.com"))
        && !url.contains("/backend-api")
    {
        url.push_str("/backend-api");
    }
    url
}

fn extract_chatgpt_account_id(token: &str) -> Option<String> {
    let mut parts = token.split('.');
    let (_header, payload, _sig) = (parts.next()?, parts.next()?, parts.next()?);
    let data = URL_SAFE_NO_PAD.decode(payload).ok()?;
    let json: serde_json::Value = serde_json::from_slice(&data).ok()?;
    json.get("https://api.openai.com/auth")
        .and_then(|auth| auth.get("chatgpt_account_id"))
        .and_then(|id| id.as_str())
        .map(|s| s.to_string())
}

#[derive(Debug, Deserialize)]
struct EnvironmentEntry {
    id: String,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    repo_hints: Option<String>,
    #[serde(default)]
    is_pinned: Option<bool>,
}
