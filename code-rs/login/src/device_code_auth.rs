use reqwest::header::HeaderMap;
use reqwest::StatusCode;
use serde::Deserialize;
use serde::Serialize;
use serde::de::Deserializer;
use serde::de::{self};
use std::time::Duration;
use std::time::Instant;

use crate::pkce::PkceCodes;
use crate::server::{persist_tokens_async, exchange_code_for_tokens, ServerOptions};
use code_browser::global as browser_global;
use code_core::default_client;
use std::io::Write;
use std::io::{self};

#[derive(Deserialize)]
struct UserCodeResp {
    device_auth_id: String,
    #[serde(alias = "user_code", alias = "usercode")]
    user_code: String,
    #[serde(default, deserialize_with = "deserialize_interval")]
    interval: u64,
}

#[derive(Serialize)]
struct UserCodeReq {
    client_id: String,
}

#[derive(Serialize)]
struct TokenPollReq {
    device_auth_id: String,
    user_code: String,
}

fn deserialize_interval<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    s.trim()
        .parse::<u64>()
        .map_err(|e| de::Error::custom(format!("invalid u64 string: {e}")))
}

#[derive(Deserialize)]
struct CodeSuccessResp {
    authorization_code: String,
    code_challenge: String,
    code_verifier: String,
}

/// Request the user code and polling interval.
async fn request_user_code(
    client: &reqwest::Client,
    auth_base_url: &str,
    base_url: &str,
    client_id: &str,
) -> std::io::Result<UserCodeResp> {
    let url = format!("{auth_base_url}/deviceauth/usercode");
    let body = serde_json::to_string(&UserCodeReq {
        client_id: client_id.to_string(),
    })
    .map_err(std::io::Error::other)?;
    let resp = client
        .post(url)
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await
        .map_err(std::io::Error::other)?;

    let status = resp.status();
    let headers = resp.headers().clone();
    let body_text = resp.text().await.map_err(std::io::Error::other)?;

    if !status.is_success() {
        if status == StatusCode::NOT_FOUND {
            return Err(std::io::Error::other(
                "device code login is not enabled for this Codex server. Use the browser login or verify the server URL.",
            ));
        }

        if looks_like_cloudflare_challenge(status, &headers, &body_text) {
            if let Ok(via_browser) = request_user_code_via_browser(base_url, client_id).await {
                return Ok(via_browser);
            }
        }

        return Err(std::io::Error::other(format!(
            "device code request failed with status {}",
            status
        )));
    }

    serde_json::from_str(&body_text).map_err(std::io::Error::other)
}

/// Poll token endpoint until a code is issued or timeout occurs.
async fn poll_for_token(
    client: &reqwest::Client,
    auth_base_url: &str,
    device_auth_id: &str,
    user_code: &str,
    interval: u64,
) -> std::io::Result<CodeSuccessResp> {
    let url = format!("{auth_base_url}/deviceauth/token");
    let max_wait = Duration::from_secs(15 * 60);
    let start = Instant::now();

    loop {
        let body = serde_json::to_string(&TokenPollReq {
            device_auth_id: device_auth_id.to_string(),
            user_code: user_code.to_string(),
        })
        .map_err(std::io::Error::other)?;
        let resp = client
            .post(&url)
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await
            .map_err(std::io::Error::other)?;

        let status = resp.status();

        if status.is_success() {
            return resp.json().await.map_err(std::io::Error::other);
        }

        if status == StatusCode::FORBIDDEN || status == StatusCode::NOT_FOUND {
            if start.elapsed() >= max_wait {
                return Err(std::io::Error::other(
                    "device auth timed out after 15 minutes",
                ));
            }
            let sleep_for = Duration::from_secs(interval).min(max_wait - start.elapsed());
            tokio::time::sleep(sleep_for).await;
            continue;
        }

        return Err(std::io::Error::other(format!(
            "device auth failed with status {}",
            resp.status()
        )));
    }
}

// Helper to print colored text if terminal supports ANSI
fn print_colored_warning_device_code() {
    // ANSI escape code for bright yellow
    const YELLOW: &str = "\x1b[93m";
    const RESET: &str = "\x1b[0m";
    let warning = "WARN!!! device code authentication has potential risks and\n\
        should be used with caution only in cases where browser support \n\
        is missing. This is prone to attacks.\n\
        \n\
        - This code is valid for 15 minutes.\n\
        - Do not share this code with anyone.\n\
        ";
    let mut stdout = io::stdout().lock();
    let _ = write!(stdout, "{YELLOW}{warning}{RESET}");
    let _ = stdout.flush();
}

/// Full device code login flow.
pub async fn run_device_code_login(opts: ServerOptions) -> std::io::Result<()> {
    print_colored_warning_device_code();
    println!("⏳ Generating a new 9-digit device code for authentication...\n");
    let session = DeviceCodeSession::start(opts).await?;

    println!(
        "To authenticate, visit: {} and enter code: {}",
        session.authorize_url(),
        session.user_code()
    );

    session
        .wait_for_tokens()
        .await
        .map_err(|err| std::io::Error::other(format!("device code exchange failed: {err}")))
}

pub struct DeviceCodeSession {
    client: reqwest::Client,
    opts: ServerOptions,
    api_base_url: String,
    base_url: String,
    device_auth_id: String,
    user_code: String,
    interval: u64,
}

impl DeviceCodeSession {
    pub async fn start(opts: ServerOptions) -> std::io::Result<Self> {
        let client = default_client::create_client(&opts.originator);
        let base_url = opts.issuer.trim_end_matches('/').to_string();
        let api_base_url = format!("{}/api/accounts", base_url);
        let uc = request_user_code(&client, &api_base_url, &base_url, &opts.client_id).await?;

        Ok(Self {
            client,
            api_base_url,
            base_url,
            device_auth_id: uc.device_auth_id,
            user_code: uc.user_code,
            interval: uc.interval,
            opts,
        })
    }

    pub fn authorize_url(&self) -> String {
        format!("{}/deviceauth/authorize", self.api_base_url)
    }

    pub fn user_code(&self) -> &str {
        &self.user_code
    }

    pub async fn wait_for_tokens(self) -> std::io::Result<()> {
        let code_resp = poll_for_token(
            &self.client,
            &self.api_base_url,
            &self.device_auth_id,
            &self.user_code,
            self.interval,
        )
        .await?;

        let pkce = PkceCodes {
            code_verifier: code_resp.code_verifier,
            code_challenge: code_resp.code_challenge,
        };
        let redirect_uri = format!("{}/deviceauth/callback", self.base_url);

        let tokens = exchange_code_for_tokens(
            &self.base_url,
            &self.opts.client_id,
            &redirect_uri,
            &pkce,
            &code_resp.authorization_code,
        )
        .await
        .map_err(|err| std::io::Error::other(format!("device code exchange failed: {err}")))?;

        persist_tokens_async(
            &self.opts.code_home,
            None,
            tokens.id_token,
            tokens.access_token,
            tokens.refresh_token,
        )
        .await
    }
}

fn looks_like_cloudflare_challenge(
    status: StatusCode,
    headers: &HeaderMap,
    body: &str,
) -> bool {
    if status != StatusCode::FORBIDDEN {
        return false;
    }

    let lower = body.to_lowercase();
    if lower.contains("cloudflare")
        || lower.contains("cf-ray")
        || lower.contains("_cf_chl_opt")
        || lower.contains("challenge-platform")
        || lower.contains("just a moment")
        || lower.contains("enable javascript and cookies")
    {
        return true;
    }

    headers.get("cf-ray").is_some()
        || headers.get("cf-mitigated").is_some()
        || headers
            .get("server-timing")
            .and_then(|v| v.to_str().ok())
            .map(|v| v.to_lowercase().contains("chlray"))
            .unwrap_or(false)
        || headers
            .get("set-cookie")
            .and_then(|v| v.to_str().ok())
            .map(|cookie| cookie.contains("__cf_bm="))
            .unwrap_or(false)
}

async fn request_user_code_via_browser(
    base_url: &str,
    client_id: &str,
) -> std::io::Result<UserCodeResp> {
    let issuer = base_url.trim_end_matches('/');
    let authorize_page = format!("{issuer}/codex/device");
    let manager = browser_global::get_or_create_browser_manager().await;

    tokio::time::timeout(Duration::from_secs(30), manager.goto(&authorize_page))
        .await
        .map_err(|_| std::io::Error::other("browser navigation timed out"))?
        .map_err(|err| std::io::Error::other(format!("browser navigation failed: {err}")))?;

    tokio::time::sleep(Duration::from_secs(4)).await;

    let api_url = format!("{issuer}/api/accounts/deviceauth/usercode");
    let payload_literal = serde_json::to_string(&serde_json::json!({ "client_id": client_id }))
        .map_err(std::io::Error::other)?;
    let script = format!(
        r#"(async () => {{
            try {{
                const resp = await fetch("{api_url}", {{
                    method: "POST",
                    credentials: "include",
                    headers: {{ "Content-Type": "application/json" }},
                    body: {payload_literal}
                }});
                const text = await resp.text();
                return {{ ok: resp.ok, status: resp.status, body: text }};
            }} catch (err) {{
                return {{ ok: false, status: 0, body: String(err) }};
            }}
        }})()"#
    );

    for _ in 0..3 {
        let value = tokio::time::timeout(Duration::from_secs(15), manager.execute_javascript(&script))
            .await
            .map_err(|_| std::io::Error::other("browser fetch timed out"))?
            .map_err(|err| std::io::Error::other(format!("browser execution failed: {err}")))?;

        let status = value
            .get("status")
            .and_then(|v| v.as_i64())
            .unwrap_or_default();
        let ok = value.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
        let body = value.get("body").and_then(|v| v.as_str()).unwrap_or("");

        if ok {
            return serde_json::from_str(body).map_err(std::io::Error::other);
        }

        if status == 403 {
            tokio::time::sleep(Duration::from_secs(2)).await;
            continue;
        }

        return Err(std::io::Error::other(format!(
            "device code request failed with status {} while using browser fallback",
            status
        )));
    }

    Err(std::io::Error::other(
        "device code request failed after browser fallback retries",
    ))
}
