use code_core::config::Config;
use code_core::CodexAuth;
use code_app_server_protocol::AuthMode;

use anyhow::Context;
use serde::de::DeserializeOwned;

/// Make a GET request to the ChatGPT backend API.
pub(crate) async fn chatgpt_get_request<T: DeserializeOwned>(
    config: &Config,
    path: String,
) -> anyhow::Result<T> {
    let chatgpt_base_url = &config.chatgpt_base_url;
    let auth = CodexAuth::from_code_home(
        &config.code_home,
        AuthMode::ChatGPT,
        &config.responses_originator_header,
    )?
    .ok_or_else(|| anyhow::anyhow!("ChatGPT auth not available"))?;
    anyhow::ensure!(
        auth.uses_codex_backend(),
        "ChatGPT backend requests require Codex backend auth"
    );

    // Make direct HTTP request to ChatGPT backend API with the token
    let client = code_core::http_client::build_http_client();
    let url = format!(
        "{}/{}",
        chatgpt_base_url.trim_end_matches('/'),
        path.trim_start_matches('/')
    );

    let token = auth.get_token_data().await?;
    let account_id = token.account_id.ok_or_else(|| {
        anyhow::anyhow!("ChatGPT account ID not available, please re-run `code login`")
    })?;

    let mut request = client
        .get(&url)
        .bearer_auth(&token.access_token)
        .header("chatgpt-account-id", account_id)
        .header("Content-Type", "application/json");
    if token.id_token.is_fedramp_account() {
        request = request.header("X-OpenAI-Fedramp", "true");
    }

    let response = request.send().await.context("Failed to send request")?;

    if response.status().is_success() {
        let result: T = response
            .json()
            .await
            .context("Failed to parse JSON response")?;
        Ok(result)
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("Request failed with status {status}: {body}")
    }
}
