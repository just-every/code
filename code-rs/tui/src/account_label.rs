use code_core::auth_accounts::StoredAccount;
use code_login::AuthMode;

const KEY_SUFFIX_LEN: usize = 8;

/// Returns a user-facing label for the given account.
/// Prefers stored labels when present, otherwise formats a sensible default.
pub(crate) fn account_display_label(account: &StoredAccount) -> String {
    if let Some(label) = account.label.as_ref() {
        let trimmed = label.trim();
        if !trimmed.is_empty() {
            match account.mode {
                AuthMode::ChatGPT | AuthMode::ChatgptAuthTokens => {
                    let default_email = account
                        .tokens
                        .as_ref()
                        .and_then(|tokens| tokens.id_token.email.as_deref());
                    if default_email.is_some_and(|email| trimmed.eq_ignore_ascii_case(email)) {
                        // Fall back to the default ChatGPT label format when the stored
                        // label is just the raw email we persist automatically.
                    } else {
                        return trimmed.to_string();
                    }
                }
                AuthMode::ApiKey => {
                    return trimmed.to_string();
                }
            }
        }
    }

    match account.mode {
        AuthMode::ChatGPT | AuthMode::ChatgptAuthTokens => account
            .tokens
            .as_ref()
            .and_then(|tokens| tokens.id_token.email.clone())
            .map(|email| format!("ChatGPT ({email})"))
            .unwrap_or_else(|| "ChatGPT".to_string()),
        AuthMode::ApiKey => account
            .openai_api_key
            .as_ref()
            .map(|key| format!("API key (…{})", key_suffix(key)))
            .unwrap_or_else(|| "API key".to_string()),
    }
}

/// Returns the fixed-length suffix used when displaying sensitive tokens.
pub(crate) fn key_suffix(text: &str) -> String {
    let tail: Vec<char> = text.chars().rev().take(KEY_SUFFIX_LEN).collect();
    tail.into_iter().rev().collect()
}

/// Returns an ordering priority for accounts. ChatGPT accounts should appear first.
pub(crate) fn account_mode_priority(mode: AuthMode) -> u8 {
    match mode {
        AuthMode::ChatGPT | AuthMode::ChatgptAuthTokens => 0,
        AuthMode::ApiKey => 1,
    }
}
