use sha2::Digest;
use sha2::Sha256;

use crate::auth::CodexAuth;
use crate::model_provider_info::ModelProviderInfo;

/// Returns an opaque identity for a catalog's provider and authentication scope.
///
/// The persisted value is a digest. ChatGPT access tokens are omitted when stable account and
/// email metadata are present, so a token refresh retains that account's catalog.
pub(super) fn model_cache_identity(
    provider: &ModelProviderInfo,
    auth: Option<&CodexAuth>,
) -> Option<String> {
    if provider.has_command_auth() {
        return None;
    }

    let mut digest = Sha256::new();
    let mut field = |value: &[u8]| {
        digest.update((value.len() as u64).to_le_bytes());
        digest.update(value);
    };

    field(b"models-cache-v1");
    field(provider.name.as_bytes());
    field(provider.base_url.as_deref().unwrap_or_default().as_bytes());
    field(format!("{:?}", provider.wire_api).as_bytes());
    field(&[u8::from(provider.requires_openai_auth)]);

    let mut query: Vec<_> = provider
        .query_params
        .iter()
        .flat_map(|params| params.iter())
        .collect();
    query.sort();
    field(&(query.len() as u64).to_le_bytes());
    for (name, value) in query {
        field(name.as_bytes());
        field(value.as_bytes());
    }

    let mut headers = provider
        .http_headers
        .iter()
        .flat_map(|configured| configured.iter())
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect::<Vec<_>>();
    if let Some(configured) = provider.env_http_headers.as_ref() {
        for (name, variable) in configured {
            if let Ok(value) = std::env::var(variable)
                && !value.trim().is_empty()
            {
                headers.push((name.clone(), value));
            }
        }
    }
    headers.sort();
    field(&(headers.len() as u64).to_le_bytes());
    for (name, value) in headers {
        field(name.as_bytes());
        field(value.as_bytes());
    }

    field(format!("{:?}", auth.map(|value| value.mode)).as_bytes());
    let account_id = auth.and_then(CodexAuth::get_account_id);
    let email = auth.and_then(CodexAuth::get_account_email);
    field(format!("{account_id:?}").as_bytes());
    field(format!("{email:?}").as_bytes());
    field(format!("{:?}", auth.and_then(CodexAuth::get_plan_type)).as_bytes());
    field(&[u8::from(auth.is_some_and(CodexAuth::is_fedramp_account))]);

    if account_id.is_none() || email.is_none() {
        let credential = auth.and_then(CodexAuth::model_cache_credential);
        field(credential.as_deref().unwrap_or_default().as_bytes());
    }
    field(
        provider
            .experimental_bearer_token
            .as_deref()
            .unwrap_or_default()
            .as_bytes(),
    );
    let provider_api_key = provider.api_key().ok().flatten();
    field(provider_api_key.as_deref().unwrap_or_default().as_bytes());

    Some(format!("{:x}", digest.finalize()))
}
