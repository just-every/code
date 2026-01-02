use chrono::DateTime;
use chrono::Utc;
use reqwest::StatusCode;
use serde::Deserialize;
use serde::Serialize;
use std::env;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::Read;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use code_app_server_protocol::AuthMode;

use crate::token_data::TokenData;
use crate::token_data::{parse_id_token, PlanType};
use crate::token_data::KnownPlan;
use crate::config::resolve_code_path_for_read;
use crate::util::backoff;

#[derive(Debug, Clone)]
pub struct CodexAuth {
    pub mode: AuthMode,

    pub(crate) api_key: Option<String>,
    pub(crate) auth_dot_json: Arc<Mutex<Option<AuthDotJson>>>,
    pub(crate) auth_file: PathBuf,
    pub(crate) client: reqwest::Client,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshTokenErrorKind {
    Permanent,
    Transient,
}

#[derive(Debug, Clone)]
pub struct RefreshTokenError {
    pub kind: RefreshTokenErrorKind,
    pub message: String,
}

impl RefreshTokenError {
    pub fn permanent(message: impl Into<String>) -> Self {
        Self {
            kind: RefreshTokenErrorKind::Permanent,
            message: message.into(),
        }
    }

    pub fn transient(message: impl Into<String>) -> Self {
        Self {
            kind: RefreshTokenErrorKind::Transient,
            message: message.into(),
        }
    }

    pub fn is_permanent(&self) -> bool {
        matches!(self.kind, RefreshTokenErrorKind::Permanent)
    }

    pub fn is_refresh_token_reused(&self) -> bool {
        self.message.contains("refresh_token_reused")
    }
}

impl std::fmt::Display for RefreshTokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for RefreshTokenError {}

impl PartialEq for CodexAuth {
    fn eq(&self, other: &Self) -> bool {
        self.mode == other.mode
    }
}

impl CodexAuth {
    pub async fn refresh_token(&self) -> Result<String, RefreshTokenError> {
        let token_data = self
            .get_current_token_data()
            .ok_or_else(|| RefreshTokenError::permanent("Token data is not available."))?;
        let refresh_token = token_data.refresh_token.clone();

        let mut attempt: u32 = 0;
        loop {
            attempt = attempt.saturating_add(1);
            match try_refresh_token(refresh_token.clone(), &self.client).await {
                Ok(refresh_response) => {
                    return self.persist_refresh_response(refresh_response).await
                }
                Err(err) => {
                    if err.is_refresh_token_reused() {
                        if let Some(access) =
                            self.adopt_rotated_refresh_token_from_disk(&refresh_token)?
                        {
                            return Ok(access);
                        }
                    }
                    if err.kind == RefreshTokenErrorKind::Transient && attempt < 4 {
                        let delay = backoff(attempt as u64);
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                    return Err(err);
                }
            }
        }
    }

    fn adopt_rotated_refresh_token_from_disk(
        &self,
        stale_refresh_token: &str,
    ) -> Result<Option<String>, RefreshTokenError> {
        let auth_dot_json = try_read_auth_json(&self.auth_file)
            .map_err(|err| RefreshTokenError::permanent(err.to_string()))?;

        let Some(tokens) = auth_dot_json.tokens.clone() else {
            return Ok(None);
        };

        if tokens.refresh_token == stale_refresh_token {
            return Ok(None);
        }

        if let Ok(mut auth_lock) = self.auth_dot_json.lock() {
            *auth_lock = Some(auth_dot_json);
        }

        Ok(Some(tokens.access_token))
    }

    async fn persist_refresh_response(
        &self,
        refresh_response: RefreshResponse,
    ) -> Result<String, RefreshTokenError> {
        let updated = update_tokens(
            &self.auth_file,
            refresh_response.id_token,
            refresh_response.access_token,
            refresh_response.refresh_token,
        )
        .await
        .map_err(|err| RefreshTokenError::permanent(err.to_string()))?;

        if let Ok(mut auth_lock) = self.auth_dot_json.lock() {
            *auth_lock = Some(updated.clone());
        }

        let access = match updated.tokens {
            Some(t) => t.access_token,
            None => {
                return Err(RefreshTokenError::permanent(
                    "Token data is not available after refresh.",
                ));
            }
        };
        Ok(access)
    }

    /// Loads the available auth information from the auth.json or
    /// OPENAI_API_KEY environment variable.
    pub fn from_code_home(
        code_home: &Path,
        preferred_auth_method: AuthMode,
        originator: &str,
    ) -> std::io::Result<Option<CodexAuth>> {
        load_auth(code_home, true, preferred_auth_method, originator)
    }

    pub async fn get_token_data(&self) -> Result<TokenData, std::io::Error> {
        let auth_dot_json: Option<AuthDotJson> = self.get_current_auth_json();
        match auth_dot_json {
            Some(AuthDotJson {
                tokens: Some(mut tokens),
                last_refresh: Some(last_refresh),
                ..
            }) => {
                if last_refresh < Utc::now() - chrono::Duration::days(28) {
                    let refresh_response = tokio::time::timeout(
                        Duration::from_secs(60),
                        try_refresh_token(tokens.refresh_token.clone(), &self.client),
                    )
                    .await
        .map_err(|_| {
            std::io::Error::other("timed out while refreshing OpenAI API key")
        })?
        .map_err(|err| std::io::Error::other(err))?;

                    let updated_auth_dot_json = update_tokens(
                        &self.auth_file,
                        refresh_response.id_token,
                        refresh_response.access_token,
                        refresh_response.refresh_token,
                    )
                    .await?;

                    tokens = updated_auth_dot_json
                        .tokens
                        .clone()
                        .ok_or(std::io::Error::other(
                            "Token data is not available after refresh.",
                        ))?;

                    #[expect(clippy::unwrap_used)]
                    let mut auth_lock = self.auth_dot_json.lock().unwrap();
                    *auth_lock = Some(updated_auth_dot_json);
                }

                Ok(tokens)
            }
            _ => Err(std::io::Error::other("Token data is not available.")),
        }
    }

    pub async fn get_token(&self) -> Result<String, std::io::Error> {
        match self.mode {
            AuthMode::ApiKey => Ok(self.api_key.clone().unwrap_or_default()),
            AuthMode::ChatGPT => {
                let id_token = self.get_token_data().await?.access_token;
                Ok(id_token)
            }
        }
    }

    pub fn get_account_id(&self) -> Option<String> {
        self.get_current_token_data()
            .and_then(|t| t.account_id.clone())
    }

    pub fn get_plan_type(&self) -> Option<String> {
        self.get_current_token_data()
            .and_then(|t| t.id_token.chatgpt_plan_type.as_ref().map(|p| p.as_string()))
    }

    fn get_current_auth_json(&self) -> Option<AuthDotJson> {
        #[expect(clippy::unwrap_used)]
        self.auth_dot_json.lock().unwrap().clone()
    }

    fn get_current_token_data(&self) -> Option<TokenData> {
        self.get_current_auth_json().and_then(|t| t.tokens.clone())
    }

    /// Consider this private to integration tests.
    pub fn create_dummy_chatgpt_auth_for_testing() -> Self {
        let auth_dot_json = AuthDotJson {
            openai_api_key: None,
            tokens: Some(TokenData {
                id_token: Default::default(),
                access_token: "Access Token".to_string(),
                refresh_token: "test".to_string(),
                account_id: Some("account_id".to_string()),
            }),
            last_refresh: Some(Utc::now()),
        };

        let auth_dot_json = Arc::new(Mutex::new(Some(auth_dot_json)));
        Self {
            api_key: None,
            mode: AuthMode::ChatGPT,
            auth_file: PathBuf::new(),
            auth_dot_json,
            client: crate::default_client::create_client("code_cli_rs"),
        }
    }

    fn from_api_key_with_client(api_key: &str, client: reqwest::Client) -> Self {
        Self {
            api_key: Some(api_key.to_owned()),
            mode: AuthMode::ApiKey,
            auth_file: PathBuf::new(),
            auth_dot_json: Arc::new(Mutex::new(None)),
            client,
        }
    }

    pub fn from_api_key(api_key: &str) -> Self {
        Self::from_api_key_with_client(
            api_key,
            crate::default_client::create_client(crate::default_client::DEFAULT_ORIGINATOR),
        )
    }

    pub fn from_tokens_with_originator(
        tokens: TokenData,
        last_refresh: Option<DateTime<Utc>>,
        originator: &str,
    ) -> Self {
        let auth_dot_json = AuthDotJson {
            openai_api_key: None,
            tokens: Some(tokens),
            last_refresh,
        };

        Self {
            api_key: None,
            mode: AuthMode::ChatGPT,
            auth_file: PathBuf::new(),
            auth_dot_json: Arc::new(Mutex::new(Some(auth_dot_json))),
            client: crate::default_client::create_client(originator),
        }
    }
}

pub const OPENAI_API_KEY_ENV_VAR: &str = "OPENAI_API_KEY";
pub const CODEX_API_KEY_ENV_VAR: &str = "CODEX_API_KEY";

fn read_openai_api_key_from_env() -> Option<String> {
    env::var(OPENAI_API_KEY_ENV_VAR)
        .ok()
        .filter(|s| !s.is_empty())
}

pub fn read_code_api_key_from_env() -> Option<String> {
    env::var(CODEX_API_KEY_ENV_VAR)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn get_auth_file(code_home: &Path) -> PathBuf {
    code_home.join("auth.json")
}

/// Delete the auth.json file inside `code_home` if it exists. Returns `Ok(true)`
/// if a file was removed, `Ok(false)` if no auth file was present.
pub fn logout(code_home: &Path) -> std::io::Result<bool> {
    let auth_file = get_auth_file(code_home);
    let removed = match std::fs::remove_file(&auth_file) {
        Ok(_) => true,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => false,
        Err(err) => return Err(err),
    };

    let _ = crate::auth_accounts::set_active_account_id(code_home, None)?;
    Ok(removed)
}

/// Writes an `auth.json` that contains only the API key. Intended for CLI use.
pub fn login_with_api_key(code_home: &Path, api_key: &str) -> std::io::Result<()> {
    let auth_dot_json = AuthDotJson {
        openai_api_key: Some(api_key.to_string()),
        tokens: None,
        last_refresh: None,
    };
    write_auth_json(&get_auth_file(code_home), &auth_dot_json)?;
    let _ = crate::auth_accounts::upsert_api_key_account(
        code_home,
        api_key.to_string(),
        None,
        true,
    )?;
    Ok(())
}

pub async fn auth_for_stored_account(
    code_home: &Path,
    account: &crate::auth_accounts::StoredAccount,
    originator: &str,
) -> std::io::Result<CodexAuth> {
    match account.mode {
        AuthMode::ApiKey => {
            let api_key = account.openai_api_key.clone().ok_or_else(|| {
                std::io::Error::other("stored API key account is missing the key value")
            })?;
            Ok(CodexAuth::from_api_key_with_client(
                &api_key,
                crate::default_client::create_client(originator),
            ))
        }
        AuthMode::ChatGPT => {
            let mut tokens = account.tokens.clone().ok_or_else(|| {
                std::io::Error::other("stored ChatGPT account is missing token data")
            })?;
            let mut last_refresh = account.last_refresh;
            let now = Utc::now();
            let refresh_needed = last_refresh
                .map(|last| last < now - chrono::Duration::days(28))
                .unwrap_or(true);

            if refresh_needed {
                let client = crate::default_client::create_client(originator);
                let refresh_response = tokio::time::timeout(
                    Duration::from_secs(60),
                    try_refresh_token(tokens.refresh_token.clone(), &client),
                )
                .await
                .map_err(|_| {
                    std::io::Error::other("timed out while refreshing OpenAI API key")
                })?;

                let refresh_response = match refresh_response {
                    Ok(response) => response,
                    Err(err) => {
                        if err.is_refresh_token_reused() {
                            if let Ok(Some(updated)) =
                                crate::auth_accounts::find_account(code_home, &account.id)
                            {
                                if let Some(updated_tokens) = updated.tokens {
                                    return Ok(CodexAuth::from_tokens_with_originator(
                                        updated_tokens,
                                        updated.last_refresh,
                                        originator,
                                    ));
                                }
                            }
                        }
                        return Err(std::io::Error::other(err));
                    }
                };
                tokens.id_token =
                    parse_id_token(&refresh_response.id_token).map_err(std::io::Error::other)?;
                if let Some(access_token) = refresh_response.access_token {
                    tokens.access_token = access_token;
                }
                if let Some(refresh_token) = refresh_response.refresh_token {
                    tokens.refresh_token = refresh_token;
                }
                last_refresh = Some(now);
                let _ = crate::auth_accounts::upsert_chatgpt_account(
                    code_home,
                    tokens.clone(),
                    now,
                    account.label.clone(),
                    false,
                )?;
            }

            Ok(CodexAuth::from_tokens_with_originator(
                tokens,
                last_refresh,
                originator,
            ))
        }
    }
}

/// Activate a stored account by writing its credentials to auth.json and
/// marking it active in the account store.
pub fn activate_account(code_home: &Path, account_id: &str) -> std::io::Result<()> {
    let Some(account) = crate::auth_accounts::find_account(code_home, account_id)? else {
        return Err(std::io::Error::other(format!(
            "account with id {account_id} was not found"
        )));
    };

    let auth_file = get_auth_file(code_home);
    let account_id_owned = account.id.clone();
    match account.mode {
        AuthMode::ApiKey => {
            let api_key = account.openai_api_key.clone().ok_or_else(|| {
                std::io::Error::other("stored API key account is missing the key value")
            })?;
            let auth = AuthDotJson {
                openai_api_key: Some(api_key),
                tokens: None,
                last_refresh: None,
            };
            write_auth_json(&auth_file, &auth)?;
        }
        AuthMode::ChatGPT => {
            let tokens = account.tokens.clone().ok_or_else(|| {
                std::io::Error::other("stored ChatGPT account is missing token data")
            })?;
            let auth = AuthDotJson {
                openai_api_key: None,
                tokens: Some(tokens),
                last_refresh: account.last_refresh,
            };
            write_auth_json(&auth_file, &auth)?;
        }
    }

    let _ = crate::auth_accounts::set_active_account_id(code_home, Some(account_id_owned))?;
    Ok(())
}

fn load_auth(
    code_home: &Path,
    include_env_var: bool,
    preferred_auth_method: AuthMode,
    originator: &str,
) -> std::io::Result<Option<CodexAuth>> {
    // First, check to see if there is a valid auth.json file. If not, we fall
    // back to AuthMode::ApiKey using the OPENAI_API_KEY environment variable
    // (if it is set).
    let auth_file = get_auth_file(code_home);
    let auth_read_path = resolve_code_path_for_read(code_home, Path::new("auth.json"));
    let client = crate::default_client::create_client(originator);
    let auth_dot_json = match try_read_auth_json(&auth_read_path) {
        Ok(auth) => auth,
        // If auth.json does not exist, try to read the OPENAI_API_KEY from the
        // environment variable.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && include_env_var => {
            return match read_openai_api_key_from_env() {
                Some(api_key) => Ok(Some(CodexAuth::from_api_key_with_client(&api_key, client))),
                None => Ok(None),
            };
        }
        // Though if auth.json exists but is malformed, do not fall back to the
        // env var because the user may be expecting to use AuthMode::ChatGPT.
        Err(e) => {
            return Err(e);
        }
    };

    let AuthDotJson {
        openai_api_key: auth_json_api_key,
        tokens,
        last_refresh,
    } = auth_dot_json;

    // If the auth.json has an API key, decide whether to use it.
    if let Some(api_key) = &auth_json_api_key {
        let plan_requires_api_key = tokens
            .as_ref()
            .and_then(|t| t.id_token.chatgpt_plan_type.as_ref())
            .is_some_and(|plan| matches!(plan, PlanType::Known(KnownPlan::Enterprise)));

        if plan_requires_api_key {
            return Ok(Some(CodexAuth::from_api_key_with_client(api_key, client)));
        }

        // Should any of these be AuthMode::ChatGPT with the api_key set?
        // Does AuthMode::ChatGPT indicate that there is an auth.json that is
        // "refreshable" even if we are using the API key for auth?
        match &tokens {
            Some(_tokens) => {
                // When tokens are present, honor the caller's preference strictly:
                // - If the caller prefers API key, use it.
                // - Otherwise, prefer ChatGPT and ignore the API key.
                if preferred_auth_method == AuthMode::ApiKey {
                    return Ok(Some(CodexAuth::from_api_key_with_client(api_key, client)));
                }
                // else: fall through to ChatGPT auth
            }
            None => {
                // We have an API key but no tokens in the auth.json file.
                // Perhaps the user ran `codex login --api-key <KEY>` or updated
                // auth.json by hand. Either way, let's assume they are trying
                // to use their API key.
                return Ok(Some(CodexAuth::from_api_key_with_client(api_key, client)));
            }
        }
    }

    // For the AuthMode::ChatGPT variant, perhaps neither api_key nor
    // openai_api_key should exist?
    Ok(Some(CodexAuth {
        api_key: None,
        mode: AuthMode::ChatGPT,
        auth_file,
        auth_dot_json: Arc::new(Mutex::new(Some(AuthDotJson {
            openai_api_key: None,
            tokens,
            last_refresh,
        }))),
        client,
    }))
}

/// Attempt to read and refresh the `auth.json` file in the given `CODEX_HOME` directory.
/// Returns the full AuthDotJson structure after refreshing if necessary.
pub fn try_read_auth_json(auth_file: &Path) -> std::io::Result<AuthDotJson> {
    let mut file = File::open(auth_file)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    let auth_dot_json: AuthDotJson = serde_json::from_str(&contents)?;

    Ok(auth_dot_json)
}

pub fn write_auth_json(auth_file: &Path, auth_dot_json: &AuthDotJson) -> std::io::Result<()> {
    let json_data = serde_json::to_string_pretty(auth_dot_json)?;
    let mut options = OpenOptions::new();
    options.truncate(true).write(true).create(true);
    #[cfg(unix)]
    {
        options.mode(0o600);
    }
    let mut file = options.open(auth_file)?;
    file.write_all(json_data.as_bytes())?;
    file.flush()?;
    Ok(())
}

async fn update_tokens(
    auth_file: &Path,
    id_token: String,
    access_token: Option<String>,
    refresh_token: Option<String>,
) -> std::io::Result<AuthDotJson> {
    let mut auth_dot_json = try_read_auth_json(auth_file)?;

    let tokens = auth_dot_json.tokens.get_or_insert_with(TokenData::default);
    tokens.id_token = parse_id_token(&id_token).map_err(std::io::Error::other)?;
    if let Some(access_token) = access_token {
        tokens.access_token = access_token.to_string();
    }
    if let Some(refresh_token) = refresh_token {
        tokens.refresh_token = refresh_token.to_string();
    }
    auth_dot_json.last_refresh = Some(Utc::now());
    write_auth_json(auth_file, &auth_dot_json)?;

    if let Some(code_home) = auth_file.parent() {
        if let Some(tokens) = auth_dot_json.tokens.clone() {
            let last_refresh = auth_dot_json
                .last_refresh
                .unwrap_or_else(Utc::now);
            let email = tokens.id_token.email.clone();
            let _ = crate::auth_accounts::upsert_chatgpt_account(
                code_home,
                tokens,
                last_refresh,
                email,
                true,
            )?;
        }
    }
    Ok(auth_dot_json)
}

async fn try_refresh_token(
    refresh_token: String,
    client: &reqwest::Client,
) -> Result<RefreshResponse, RefreshTokenError> {
    let refresh_request = RefreshRequest {
        client_id: CLIENT_ID,
        grant_type: "refresh_token",
        refresh_token,
        scope: "openid profile email",
    };

    // Use shared client factory to include standard headers
    let response = client
        .post("https://auth.openai.com/oauth/token")
        .header("Content-Type", "application/json")
        .json(&refresh_request)
        .send()
        .await
        .map_err(|err| RefreshTokenError::transient(format!("network error: {err}")))?;

    if response.status().is_success() {
        let refresh_response = response
            .json::<RefreshResponse>()
            .await
            .map_err(|err| RefreshTokenError::transient(format!("invalid response: {err}")))?;
        return Ok(refresh_response);
    }

    let status = response.status();
    let body = response
        .text()
        .await
        .unwrap_or_else(|_| "<body unavailable>".to_string());
    Err(classify_refresh_failure(status, &body))
}

#[derive(Serialize)]
struct RefreshRequest {
    client_id: &'static str,
    grant_type: &'static str,
    refresh_token: String,
    scope: &'static str,
}

#[derive(Deserialize, Clone)]
struct RefreshResponse {
    id_token: String,
    access_token: Option<String>,
    refresh_token: Option<String>,
}

#[derive(Deserialize)]
struct OAuthErrorBody {
    error: Option<String>,
    error_description: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiErrorWrapper {
    error: Option<OpenAiErrorData>,
}

#[derive(Deserialize)]
struct OpenAiErrorData {
    code: Option<String>,
    message: Option<String>,
}

fn classify_refresh_failure(status: StatusCode, body: &str) -> RefreshTokenError {
    if let Ok(parsed) = serde_json::from_str::<OpenAiErrorWrapper>(body) {
        if let Some(error) = parsed.error {
            if error.code.as_deref() == Some("refresh_token_reused") {
                let message = error
                    .message
                    .unwrap_or_else(|| "refresh token already rotated".to_string());
                return RefreshTokenError::transient(format!(
                    "refresh_token_reused: {message}"
                ));
            }
        }
    }

    if let Ok(parsed) = serde_json::from_str::<OAuthErrorBody>(body) {
        if let Some(code) = parsed.error.as_deref() {
            let description = parsed
                .error_description
                .as_deref()
                .unwrap_or(code)
                .trim();
            let formatted = format!("OAuth error ({code}): {description}");
            match code {
                "invalid_grant" | "invalid_client" | "invalid_scope" => {
                    return RefreshTokenError::permanent(formatted)
                }
                "access_denied" => {
                    return RefreshTokenError::permanent(formatted);
                }
                "temporarily_unavailable" => {
                    return RefreshTokenError::transient(formatted);
                }
                _ => {
                    if status.is_server_error() {
                        return RefreshTokenError::transient(formatted);
                    }
                    if status.is_client_error() {
                        return RefreshTokenError::permanent(formatted);
                    }
                }
            }
        }
    }

    if status == StatusCode::FORBIDDEN || status == StatusCode::UNAUTHORIZED {
        return RefreshTokenError::permanent(format!(
            "OAuth refresh rejected ({status}): {}",
            summarize_body(body)
        ));
    }

    if status.is_client_error() {
        return RefreshTokenError::permanent(format!(
            "OAuth refresh failed ({status}): {}",
            summarize_body(body)
        ));
    }

    if status.is_server_error() {
        return RefreshTokenError::transient(format!(
            "OAuth refresh temporarily unavailable ({status}): {}",
            summarize_body(body)
        ));
    }

    RefreshTokenError::transient(format!(
        "OAuth refresh failed with unexpected response ({status})"
    ))
}

fn summarize_body(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return "<empty response>".to_string();
    }
    const MAX_LEN: usize = 240;
    if trimmed.len() > MAX_LEN {
        format!("{}…", &trimmed[..MAX_LEN])
    } else {
        trimmed.to_string()
    }
}

/// Expected structure for $CODEX_HOME/auth.json.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct AuthDotJson {
    #[serde(rename = "OPENAI_API_KEY")]
    pub openai_api_key: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<TokenData>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_refresh: Option<DateTime<Utc>>,
}

// Shared constant for token refresh (client id used for oauth token refresh flow)
pub const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";

use std::sync::RwLock;

/// Internal cached auth state.
#[derive(Clone, Debug)]
struct CachedAuth {
    preferred_auth_mode: AuthMode,
    auth: Option<CodexAuth>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token_data::IdTokenInfo;
    use crate::token_data::KnownPlan;
    use crate::token_data::PlanType;
    use base64::Engine;
    use reqwest::StatusCode;
    use pretty_assertions::assert_eq;
    use serde::Serialize;
    use serde_json::json;
    use tempfile::tempdir;

    const LAST_REFRESH: &str = "2025-08-06T20:41:36.232376Z";

    #[tokio::test]
    async fn roundtrip_auth_dot_json() {
        let code_home = tempdir().unwrap();
        let _ = write_auth_file(
            AuthFileParams {
                openai_api_key: None,
                chatgpt_plan_type: "pro".to_string(),
            },
            code_home.path(),
        )
        .expect("failed to write auth file");

        let file = get_auth_file(code_home.path());
        let auth_dot_json = try_read_auth_json(&file).unwrap();
        write_auth_json(&file, &auth_dot_json).unwrap();

        let same_auth_dot_json = try_read_auth_json(&file).unwrap();
        assert_eq!(auth_dot_json, same_auth_dot_json);
    }

    #[test]
    fn login_with_api_key_overwrites_existing_auth_json() {
        let dir = tempdir().unwrap();
        let auth_path = dir.path().join("auth.json");
        let stale_auth = json!({
            "OPENAI_API_KEY": "sk-old",
            "tokens": {
                "id_token": "stale.header.payload",
                "access_token": "stale-access",
                "refresh_token": "stale-refresh",
                "account_id": "stale-acc"
            }
        });
        std::fs::write(
            &auth_path,
            serde_json::to_string_pretty(&stale_auth).unwrap(),
        )
        .unwrap();

        super::login_with_api_key(dir.path(), "sk-new").expect("login_with_api_key should succeed");

        let auth = super::try_read_auth_json(&auth_path).expect("auth.json should parse");
        assert_eq!(auth.openai_api_key.as_deref(), Some("sk-new"));
        assert!(auth.tokens.is_none(), "tokens should be cleared");
    }

    #[tokio::test]
    async fn pro_account_with_no_api_key_uses_chatgpt_auth() {
        let code_home = tempdir().unwrap();
        let fake_jwt = write_auth_file(
            AuthFileParams {
                openai_api_key: None,
                chatgpt_plan_type: "pro".to_string(),
            },
            code_home.path(),
        )
        .expect("failed to write auth file");

        let CodexAuth {
            api_key,
            mode,
            auth_dot_json,
            auth_file: _,
            ..
        } = super::load_auth(code_home.path(), false, AuthMode::ChatGPT, "code_cli_rs")
            .unwrap()
            .unwrap();
        assert_eq!(None, api_key);
        assert_eq!(AuthMode::ChatGPT, mode);

        let guard = auth_dot_json.lock().unwrap();
        let auth_dot_json = guard.as_ref().expect("AuthDotJson should exist");
        assert_eq!(
            &AuthDotJson {
                openai_api_key: None,
                tokens: Some(TokenData {
                    id_token: IdTokenInfo {
                        email: Some("user@example.com".to_string()),
                        chatgpt_plan_type: Some(PlanType::Known(KnownPlan::Pro)),
                        raw_jwt: fake_jwt,
                    },
                    access_token: "test-access-token".to_string(),
                    refresh_token: "test-refresh-token".to_string(),
                    account_id: None,
                }),
                last_refresh: Some(
                    DateTime::parse_from_rfc3339(LAST_REFRESH)
                        .unwrap()
                        .with_timezone(&Utc)
                ),
            },
            auth_dot_json
        )
    }

    /// Even if the OPENAI_API_KEY is set in auth.json, if the plan is not in
    /// [`TokenData::is_plan_that_should_use_api_key`], it should use
    /// [`AuthMode::ChatGPT`].
    #[tokio::test]
    async fn pro_account_with_api_key_still_uses_chatgpt_auth() {
        let code_home = tempdir().unwrap();
        let fake_jwt = write_auth_file(
            AuthFileParams {
                openai_api_key: Some("sk-test-key".to_string()),
                chatgpt_plan_type: "pro".to_string(),
            },
            code_home.path(),
        )
        .expect("failed to write auth file");

        let CodexAuth {
            api_key,
            mode,
            auth_dot_json,
            auth_file: _,
            ..
        } = super::load_auth(code_home.path(), false, AuthMode::ChatGPT, "code_cli_rs")
            .unwrap()
            .unwrap();
        assert_eq!(None, api_key);
        assert_eq!(AuthMode::ChatGPT, mode);

        let guard = auth_dot_json.lock().unwrap();
        let auth_dot_json = guard.as_ref().expect("AuthDotJson should exist");
        assert_eq!(
            &AuthDotJson {
                openai_api_key: None,
                tokens: Some(TokenData {
                    id_token: IdTokenInfo {
                        email: Some("user@example.com".to_string()),
                        chatgpt_plan_type: Some(PlanType::Known(KnownPlan::Pro)),
                        raw_jwt: fake_jwt,
                    },
                    access_token: "test-access-token".to_string(),
                    refresh_token: "test-refresh-token".to_string(),
                    account_id: None,
                }),
                last_refresh: Some(
                    DateTime::parse_from_rfc3339(LAST_REFRESH)
                        .unwrap()
                        .with_timezone(&Utc)
                ),
            },
            auth_dot_json
        )
    }

    /// If the OPENAI_API_KEY is set in auth.json and it is an enterprise
    /// account, then it should use [`AuthMode::ApiKey`].
    #[tokio::test]
    async fn enterprise_account_with_api_key_uses_apikey_auth() {
        let code_home = tempdir().unwrap();
        write_auth_file(
            AuthFileParams {
                openai_api_key: Some("sk-test-key".to_string()),
                chatgpt_plan_type: "enterprise".to_string(),
            },
            code_home.path(),
        )
        .expect("failed to write auth file");

        let CodexAuth {
            api_key,
            mode,
            auth_dot_json,
            auth_file: _,
            ..
        } = super::load_auth(code_home.path(), false, AuthMode::ChatGPT, "code_cli_rs")
            .unwrap()
            .unwrap();
        assert_eq!(Some("sk-test-key".to_string()), api_key);
        assert_eq!(AuthMode::ApiKey, mode);

        let guard = auth_dot_json.lock().expect("should unwrap");
        assert!(guard.is_none(), "auth_dot_json should be None");
    }

    #[tokio::test]
    async fn loads_api_key_from_auth_json() {
        let dir = tempdir().unwrap();
        let auth_file = dir.path().join("auth.json");
        std::fs::write(
            auth_file,
            r#"{"OPENAI_API_KEY":"sk-test-key","tokens":null,"last_refresh":null}"#,
        )
        .unwrap();

        let auth = super::load_auth(dir.path(), false, AuthMode::ChatGPT, "code_cli_rs")
            .unwrap()
            .unwrap();
        assert_eq!(auth.mode, AuthMode::ApiKey);
        assert_eq!(auth.api_key, Some("sk-test-key".to_string()));

        assert!(auth.get_token_data().await.is_err());
    }

    #[test]
    fn logout_removes_auth_file() -> Result<(), std::io::Error> {
        let dir = tempdir()?;
        let auth_dot_json = AuthDotJson {
            openai_api_key: Some("sk-test-key".to_string()),
            tokens: None,
            last_refresh: None,
        };
        write_auth_json(&get_auth_file(dir.path()), &auth_dot_json)?;
        assert!(dir.path().join("auth.json").exists());
        let removed = logout(dir.path())?;
        assert!(removed);
        assert!(!dir.path().join("auth.json").exists());
        Ok(())
    }

    fn assert_permanent(body: &str, status: StatusCode) {
        let err = classify_refresh_failure(status, body);
        assert!(err.is_permanent(), "expected permanent error, got {:?}", err.kind);
    }

    fn assert_transient(body: &str, status: StatusCode) {
        let err = classify_refresh_failure(status, body);
        assert!(
            matches!(err.kind, RefreshTokenErrorKind::Transient),
            "expected transient error, got {:?}",
            err.kind
        );
    }

    #[test]
    fn invalid_grant_is_permanent() {
        assert_permanent(
            r#"{"error":"invalid_grant","error_description":"refresh token revoked"}"#,
            StatusCode::BAD_REQUEST,
        );
    }

    #[test]
    fn invalid_client_is_permanent() {
        assert_permanent(
            r#"{"error":"invalid_client","error_description":"client mismatch"}"#,
            StatusCode::UNAUTHORIZED,
        );
    }

    #[test]
    fn temporarily_unavailable_is_transient() {
        assert_transient(
            r#"{"error":"temporarily_unavailable","error_description":"please retry"}"#,
            StatusCode::SERVICE_UNAVAILABLE,
        );
    }

    #[test]
    fn refresh_token_reused_is_transient_and_detected() {
        let body = r#"{
  "error": {
    "message": "Your refresh token has already been used to generate a new access token. Please try signing in again.",
    "type": "invalid_request_error",
    "code": "refresh_token_reused"
  }
}"#;

        let err = classify_refresh_failure(StatusCode::UNAUTHORIZED, body);
        assert!(matches!(err.kind, RefreshTokenErrorKind::Transient));
        assert!(err.is_refresh_token_reused());
    }

    #[test]
    fn five_hundred_without_body_is_transient() {
        assert_transient("", StatusCode::BAD_GATEWAY);
    }

    #[test]
    fn forbidden_without_body_is_permanent() {
        assert_permanent("", StatusCode::FORBIDDEN);
    }

    #[test]
    fn adopts_rotated_refresh_token_from_disk() {
        let dir = tempdir().unwrap();
        let auth_file = get_auth_file(dir.path());
        let fake_jwt = write_auth_file(
            AuthFileParams {
                openai_api_key: None,
                chatgpt_plan_type: "pro".to_string(),
            },
            dir.path(),
        )
        .expect("failed to write auth file");

        let cached_tokens = TokenData {
            id_token: parse_id_token(&fake_jwt).expect("failed to parse id token"),
            access_token: "cached-access".to_string(),
            refresh_token: "stale-refresh".to_string(),
            account_id: None,
        };

        let cached_auth = AuthDotJson {
            openai_api_key: None,
            tokens: Some(cached_tokens.clone()),
            last_refresh: None,
        };

        let rotated_tokens = TokenData {
            id_token: parse_id_token(&fake_jwt).expect("failed to parse id token"),
            access_token: "rotated-access".to_string(),
            refresh_token: "rotated-refresh".to_string(),
            account_id: None,
        };

        let rotated_auth = AuthDotJson {
            openai_api_key: None,
            tokens: Some(rotated_tokens.clone()),
            last_refresh: Some(Utc::now()),
        };

        write_auth_json(&auth_file, &rotated_auth).expect("failed to write rotated auth");

        let auth = CodexAuth {
            mode: AuthMode::ChatGPT,
            api_key: None,
            auth_dot_json: Arc::new(Mutex::new(Some(cached_auth))),
            auth_file,
            client: reqwest::Client::new(),
        };

        let rotated_access = rotated_tokens.access_token.clone();

        let adopted = auth
            .adopt_rotated_refresh_token_from_disk(&cached_tokens.refresh_token)
            .expect("adoption should succeed");

        assert_eq!(adopted, Some(rotated_access.clone()));

        let guard = auth.auth_dot_json.lock().expect("mutex poisoned");
        let updated = guard
            .as_ref()
            .and_then(|auth| auth.tokens.as_ref())
            .expect("tokens should exist after adoption");

        assert_eq!(updated.refresh_token, rotated_tokens.refresh_token);
        assert_eq!(updated.access_token, rotated_access);
    }

    struct AuthFileParams {
        openai_api_key: Option<String>,
        chatgpt_plan_type: String,
    }

    fn write_auth_file(params: AuthFileParams, code_home: &Path) -> std::io::Result<String> {
        let auth_file = get_auth_file(code_home);
        // Create a minimal valid JWT for the id_token field.
        #[derive(Serialize)]
        struct Header {
            alg: &'static str,
            typ: &'static str,
        }
        let header = Header {
            alg: "none",
            typ: "JWT",
        };
        let payload = serde_json::json!({
            "email": "user@example.com",
            "email_verified": true,
            "https://api.openai.com/auth": {
                "chatgpt_account_id": "bc3618e3-489d-4d49-9362-1561dc53ba53",
                "chatgpt_plan_type": params.chatgpt_plan_type,
                "chatgpt_user_id": "user-12345",
                "user_id": "user-12345",
            }
        });
        let b64 = |b: &[u8]| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b);
        let header_b64 = b64(&serde_json::to_vec(&header)?);
        let payload_b64 = b64(&serde_json::to_vec(&payload)?);
        let signature_b64 = b64(b"sig");
        let fake_jwt = format!("{header_b64}.{payload_b64}.{signature_b64}");

        let auth_json_data = json!({
            "OPENAI_API_KEY": params.openai_api_key,
            "tokens": {
                "id_token": fake_jwt,
                "access_token": "test-access-token",
                "refresh_token": "test-refresh-token"
            },
            "last_refresh": LAST_REFRESH,
        });
        let auth_json = serde_json::to_string_pretty(&auth_json_data)?;
        std::fs::write(auth_file, auth_json)?;
        Ok(fake_jwt)
    }
}

/// Central manager providing a single source of truth for auth.json derived
/// authentication data. It loads once (or on preference change) and then
/// hands out cloned `CodexAuth` values so the rest of the program has a
/// consistent snapshot.
///
/// External modifications to `auth.json` will NOT be observed until
/// `reload()` is called explicitly. This matches the design goal of avoiding
/// different parts of the program seeing inconsistent auth data mid‑run.
#[derive(Debug)]
pub struct AuthManager {
    code_home: PathBuf,
    originator: String,
    inner: RwLock<CachedAuth>,
    enable_code_api_key_env: bool,
}

impl AuthManager {
    /// Create a new manager loading the initial auth using the provided
    /// preferred auth method. Errors loading auth are swallowed; `auth()` will
    /// simply return `None` in that case so callers can treat it as an
    /// unauthenticated state.
    pub fn new(code_home: PathBuf, preferred_auth_mode: AuthMode, originator: String) -> Self {
        let mut effective_mode = preferred_auth_mode;
        let auth = if let Some(api_key) = read_code_api_key_from_env() {
            effective_mode = AuthMode::ApiKey;
            Some(CodexAuth::from_api_key(&api_key))
        } else {
            CodexAuth::from_code_home(&code_home, preferred_auth_mode, &originator)
                .ok()
                .flatten()
        };
        Self {
            code_home,
            originator,
            inner: RwLock::new(CachedAuth {
                preferred_auth_mode: effective_mode,
                auth,
            }),
            enable_code_api_key_env: true,
        }
    }

    /// Create an AuthManager with a specific CodexAuth, for testing only.
    pub fn from_auth_for_testing(auth: CodexAuth) -> Arc<Self> {
        let preferred_auth_mode = auth.mode;
        let cached = CachedAuth {
            preferred_auth_mode,
            auth: Some(auth),
        };
        Arc::new(Self {
            code_home: PathBuf::new(),
            originator: "code_cli_rs".to_string(),
            inner: RwLock::new(cached),
            enable_code_api_key_env: false,
        })
    }

    pub fn from_auth(auth: CodexAuth, code_home: PathBuf, originator: String) -> Arc<Self> {
        let preferred_auth_mode = auth.mode;
        Arc::new(Self {
            code_home,
            originator,
            inner: RwLock::new(CachedAuth {
                preferred_auth_mode,
                auth: Some(auth),
            }),
            enable_code_api_key_env: false,
        })
    }

    /// Current cached auth (clone). May be `None` if not logged in or load failed.
    pub fn auth(&self) -> Option<CodexAuth> {
        self.inner.read().ok().and_then(|c| c.auth.clone())
    }

    /// Preferred auth method used when (re)loading.
    pub fn preferred_auth_method(&self) -> AuthMode {
        self.inner
            .read()
            .map(|c| c.preferred_auth_mode)
            .unwrap_or(AuthMode::ApiKey)
    }

    /// Force a reload using the existing preferred auth method. Returns
    /// whether the auth value changed.
    pub fn reload(&self) -> bool {
        let preferred = self.preferred_auth_method();
        let env_auth = if self.enable_code_api_key_env {
            read_code_api_key_from_env().map(|api_key| CodexAuth::from_api_key(&api_key))
        } else {
            None
        };
        let new_auth = env_auth.clone().or_else(|| {
            CodexAuth::from_code_home(&self.code_home, preferred, &self.originator)
                .ok()
                .flatten()
        });
        if let Ok(mut guard) = self.inner.write() {
            let changed = !AuthManager::auths_equal(&guard.auth, &new_auth);
            guard.auth = new_auth;
            guard.preferred_auth_mode = env_auth
                .as_ref()
                .map(|auth| auth.mode)
                .unwrap_or(preferred);
            changed
        } else {
            false
        }
    }

    fn auths_equal(a: &Option<CodexAuth>, b: &Option<CodexAuth>) -> bool {
        match (a, b) {
            (None, None) => true,
            (Some(a), Some(b)) => a == b,
            _ => false,
        }
    }

    /// Convenience constructor returning an `Arc` wrapper with default auth mode + originator.
    pub fn shared(code_home: PathBuf) -> Arc<Self> {
        Arc::new(Self::new(
            code_home,
            AuthMode::ApiKey,
            crate::default_client::DEFAULT_ORIGINATOR.to_string(),
        ))
    }

    /// Convenience constructor returning an `Arc` wrapper with explicit auth mode and originator.
    pub fn shared_with_mode_and_originator(
        code_home: PathBuf,
        preferred_auth_mode: AuthMode,
        originator: String,
    ) -> Arc<Self> {
        Arc::new(Self::new(code_home, preferred_auth_mode, originator))
    }

    /// Attempt to refresh the current auth token (if any). On success, reload
    /// the auth state from disk so other components observe refreshed token.
    pub async fn refresh_token_classified(&self) -> Result<Option<String>, RefreshTokenError> {
        let auth = match self.auth() {
            Some(a) => a,
            None => return Ok(None),
        };
        match auth.refresh_token().await {
            Ok(token) => {
                // Reload to pick up persisted changes.
                self.reload();
                Ok(Some(token))
            }
            Err(e) => Err(e),
        }
    }

    pub async fn refresh_token(&self) -> std::io::Result<Option<String>> {
        self.refresh_token_classified()
            .await
            .map_err(|err| std::io::Error::other(err))
    }

    /// Log out by deleting the on‑disk auth.json (if present). Returns Ok(true)
    /// if a file was removed, Ok(false) if no auth file existed. On success,
    /// reloads the in‑memory auth cache so callers immediately observe the
    /// unauthenticated state.
    pub fn logout(&self) -> std::io::Result<bool> {
        let removed = super::auth::logout(&self.code_home)?;
        // Always reload to clear any cached auth (even if file absent).
        self.reload();
        Ok(removed)
    }
}
