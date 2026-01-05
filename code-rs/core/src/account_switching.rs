use std::collections::{HashMap, HashSet};
use std::io;
use std::path::Path;

use chrono::{DateTime, Utc};
use code_app_server_protocol::AuthMode;

use crate::auth;
use crate::account_usage;
use crate::auth_accounts;

#[derive(Debug, Default)]
pub struct RateLimitSwitchState {
    tried_accounts: HashSet<String>,
    limited_chatgpt_accounts: HashSet<String>,
    blocked_until: HashMap<String, DateTime<Utc>>,
}

impl RateLimitSwitchState {
    pub(crate) fn mark_limited(
        &mut self,
        account_id: &str,
        mode: AuthMode,
        blocked_until: Option<DateTime<Utc>>,
    ) {
        self.tried_accounts.insert(account_id.to_string());

        if mode == AuthMode::ChatGPT {
            self.limited_chatgpt_accounts
                .insert(account_id.to_string());
        }

        if let Some(until) = blocked_until {
            self.blocked_until
                .entry(account_id.to_string())
                .and_modify(|existing| {
                    if until > *existing {
                        *existing = until;
                    }
                })
                .or_insert(until);
        }
    }

    fn blocked_until(&self, account_id: &str) -> Option<DateTime<Utc>> {
        self.blocked_until.get(account_id).copied()
    }

    fn has_tried(&self, account_id: &str) -> bool {
        self.tried_accounts.contains(account_id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct CandidateScore {
    used_percent: f64,
}

fn account_has_credentials(account: &auth_accounts::StoredAccount) -> bool {
    match account.mode {
        AuthMode::ChatGPT => account.tokens.is_some(),
        AuthMode::ApiKey => account.openai_api_key.is_some(),
    }
}

fn usage_reset_blocked_until(
    snapshot: &account_usage::StoredRateLimitSnapshot,
) -> Option<DateTime<Utc>> {
    snapshot
        .primary_next_reset_at
        .into_iter()
        .chain(snapshot.secondary_next_reset_at)
        .max()
}

fn usage_used_percent(snapshot: &account_usage::StoredRateLimitSnapshot) -> Option<f64> {
    let snapshot = snapshot.snapshot.as_ref()?;
    let used = snapshot
        .primary_used_percent
        .max(snapshot.secondary_used_percent);
    if used.is_finite() {
        Some(used)
    } else {
        None
    }
}

fn is_blocked(now: DateTime<Utc>, blocked_until: Option<DateTime<Utc>>) -> bool {
    blocked_until.is_some_and(|until| until > now)
}

pub(crate) fn select_next_account_id(
    code_home: &Path,
    state: &RateLimitSwitchState,
    allow_api_key_fallback: bool,
    now: DateTime<Utc>,
    current_account_id: Option<&str>,
) -> io::Result<Option<String>> {
    let current = match current_account_id {
        Some(id) => Some(id.to_string()),
        None => auth_accounts::get_active_account_id(code_home)?,
    };
    let accounts = auth_accounts::list_accounts(code_home)?;

    let snapshots = account_usage::list_rate_limit_snapshots(code_home).unwrap_or_default();
    let snapshot_map: HashMap<String, account_usage::StoredRateLimitSnapshot> = snapshots
        .into_iter()
        .map(|snap| (snap.account_id.clone(), snap))
        .collect();

    let mut chatgpt_accounts: Vec<&auth_accounts::StoredAccount> = accounts
        .iter()
        .filter(|acc| acc.mode == AuthMode::ChatGPT)
        .filter(|acc| account_has_credentials(acc))
        .collect();
    let mut api_key_accounts: Vec<&auth_accounts::StoredAccount> = accounts
        .iter()
        .filter(|acc| acc.mode == AuthMode::ApiKey)
        .filter(|acc| account_has_credentials(acc))
        .collect();

    // Prefer deterministic ordering.
    chatgpt_accounts.sort_by(|a, b| a.id.cmp(&b.id));
    api_key_accounts.sort_by(|a, b| a.id.cmp(&b.id));

    let current = current.as_deref();

    let mut best_chatgpt: Option<(&auth_accounts::StoredAccount, CandidateScore)> = None;
    for account in &chatgpt_accounts {
        if current.is_some_and(|id| id == account.id) {
            continue;
        }
        if state.has_tried(&account.id) {
            continue;
        }

        let blocked_until = state
            .blocked_until(&account.id)
            .into_iter()
            .chain(snapshot_map.get(&account.id).and_then(usage_reset_blocked_until))
            .max();
        if is_blocked(now, blocked_until) {
            continue;
        }

        let used_percent = snapshot_map
            .get(&account.id)
            .and_then(usage_used_percent)
            .unwrap_or(0.0);
        let score = CandidateScore { used_percent };
        match best_chatgpt {
            None => best_chatgpt = Some((*account, score)),
            Some((_, best_score)) => {
                if score.used_percent < best_score.used_percent {
                    best_chatgpt = Some((*account, score));
                }
            }
        }
    }

    if let Some((account, _)) = best_chatgpt {
        return Ok(Some(account.id.clone()));
    }

    if !allow_api_key_fallback {
        return Ok(None);
    }

    // Only allow API key fallback when every ChatGPT account is either blocked
    // or has already been tried and still rate/usage limited.
    let all_chatgpt_unavailable = chatgpt_accounts.iter().all(|account| {
        let blocked_until = state
            .blocked_until(&account.id)
            .into_iter()
            .chain(snapshot_map.get(&account.id).and_then(usage_reset_blocked_until))
            .max();
        let blocked = is_blocked(now, blocked_until);
        let exhausted = state.limited_chatgpt_accounts.contains(&account.id);
        let tried = state.has_tried(&account.id);
        blocked || (tried && exhausted)
    });

    if !chatgpt_accounts.is_empty() && !all_chatgpt_unavailable {
        return Ok(None);
    }

    for account in &api_key_accounts {
        if current.is_some_and(|id| id == account.id) {
            continue;
        }
        if state.has_tried(&account.id) {
            continue;
        }
        return Ok(Some(account.id.clone()));
    }

    Ok(None)
}

pub fn switch_active_account_on_rate_limit(
    code_home: &Path,
    state: &mut RateLimitSwitchState,
    allow_api_key_fallback: bool,
    now: DateTime<Utc>,
    current_account_id: &str,
    current_mode: AuthMode,
    blocked_until: Option<DateTime<Utc>>,
) -> io::Result<Option<String>> {
    state.mark_limited(current_account_id, current_mode, blocked_until);

    let next_account_id = select_next_account_id(
        code_home,
        state,
        allow_api_key_fallback,
        now,
        Some(current_account_id),
    )?;

    if let Some(next_account_id) = next_account_id.as_deref() {
        auth::activate_account(code_home, next_account_id)?;
    }

    Ok(next_account_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token_data::{IdTokenInfo, TokenData};
    use base64::Engine;
    use chrono::TimeZone;
    use serde::Serialize;
    use tempfile::tempdir;

    fn fake_jwt(email: &str, plan: &str) -> String {
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
            "email": email,
            "https://api.openai.com/auth": {
                "chatgpt_plan_type": plan,
            }
        });

        fn b64url_no_pad(bytes: &[u8]) -> String {
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
        }

        let header_b64 = b64url_no_pad(&serde_json::to_vec(&header).expect("header"));
        let payload_b64 = b64url_no_pad(&serde_json::to_vec(&payload).expect("payload"));
        let signature_b64 = b64url_no_pad(b"sig");
        format!("{header_b64}.{payload_b64}.{signature_b64}")
    }

    fn chatgpt_tokens(account_id: &str, email: &str) -> TokenData {
        TokenData {
            id_token: IdTokenInfo {
                email: Some(email.to_string()),
                chatgpt_plan_type: None,
                raw_jwt: fake_jwt(email, "pro"),
            },
            access_token: "access".to_string(),
            refresh_token: "refresh".to_string(),
            account_id: Some(account_id.to_string()),
        }
    }

    fn fixed_now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2025, 12, 22, 12, 0, 0).unwrap()
    }

    #[test]
    fn selects_another_chatgpt_account_when_available() {
        let home = tempdir().expect("tmp");
        let a = auth_accounts::upsert_chatgpt_account(
            home.path(),
            chatgpt_tokens("acct-a", "a@example.com"),
            Utc::now(),
            None,
            true,
        )
        .expect("insert a");
        let b = auth_accounts::upsert_chatgpt_account(
            home.path(),
            chatgpt_tokens("acct-b", "b@example.com"),
            Utc::now(),
            None,
            false,
        )
        .expect("insert b");

        let mut state = RateLimitSwitchState::default();
        state.mark_limited(&a.id, AuthMode::ChatGPT, None);
        let next = select_next_account_id(
            home.path(),
            &state,
            false,
            fixed_now(),
            Some(a.id.as_str()),
        )
        .expect("select");
        assert_eq!(next.as_deref(), Some(b.id.as_str()));
    }

    #[test]
    fn skips_chatgpt_accounts_blocked_by_reset_time() {
        let home = tempdir().expect("tmp");
        let a = auth_accounts::upsert_chatgpt_account(
            home.path(),
            chatgpt_tokens("acct-a", "a@example.com"),
            Utc::now(),
            None,
            true,
        )
        .expect("insert a");
        let b = auth_accounts::upsert_chatgpt_account(
            home.path(),
            chatgpt_tokens("acct-b", "b@example.com"),
            Utc::now(),
            None,
            false,
        )
        .expect("insert b");

        let now = fixed_now();
        let reset_in = Some(60 * 60);
        account_usage::record_usage_limit_hint(home.path(), &b.id, Some("Pro"), reset_in, now)
            .expect("hint");

        let mut state = RateLimitSwitchState::default();
        state.mark_limited(&a.id, AuthMode::ChatGPT, None);
        let next =
            select_next_account_id(home.path(), &state, false, now, Some(a.id.as_str()))
                .expect("select");
        assert!(next.is_none());
    }

    #[test]
    fn api_key_fallback_requires_all_chatgpt_limited() {
        let home = tempdir().expect("tmp");
        let a = auth_accounts::upsert_chatgpt_account(
            home.path(),
            chatgpt_tokens("acct-a", "a@example.com"),
            Utc::now(),
            None,
            true,
        )
        .expect("insert a");
        let b = auth_accounts::upsert_chatgpt_account(
            home.path(),
            chatgpt_tokens("acct-b", "b@example.com"),
            Utc::now(),
            None,
            false,
        )
        .expect("insert b");
        let api = auth_accounts::upsert_api_key_account(
            home.path(),
            "sk-test".to_string(),
            None,
            false,
        )
        .expect("insert api");

        let now = fixed_now();
        let mut state = RateLimitSwitchState::default();
        state.mark_limited(&a.id, AuthMode::ChatGPT, None);

        let next = select_next_account_id(home.path(), &state, true, now, Some(a.id.as_str()))
            .expect("select");
        assert_eq!(next.as_deref(), Some(b.id.as_str()));

        // After both ChatGPT accounts are exhausted, allow API key fallback.
        state.mark_limited(&b.id, AuthMode::ChatGPT, None);
        let next = select_next_account_id(home.path(), &state, true, now, Some(b.id.as_str()))
            .expect("select");
        assert_eq!(next.as_deref(), Some(api.id.as_str()));
    }

    #[test]
    fn prefers_current_account_override_over_active_account() {
        let home = tempdir().expect("tmp");
        let a = auth_accounts::upsert_chatgpt_account(
            home.path(),
            chatgpt_tokens("acct-a", "a@example.com"),
            Utc::now(),
            None,
            true,
        )
        .expect("insert a");
        let b = auth_accounts::upsert_chatgpt_account(
            home.path(),
            chatgpt_tokens("acct-b", "b@example.com"),
            Utc::now(),
            None,
            false,
        )
        .expect("insert b");

        let mut state = RateLimitSwitchState::default();
        state.mark_limited(&b.id, AuthMode::ChatGPT, None);

        let next = select_next_account_id(
            home.path(),
            &state,
            false,
            fixed_now(),
            Some(b.id.as_str()),
        )
        .expect("select");

        assert_eq!(next.as_deref(), Some(a.id.as_str()));
    }

    #[test]
    fn switches_active_account_on_usage_limit() {
        let home = tempdir().expect("tmp");
        let a = auth_accounts::upsert_chatgpt_account(
            home.path(),
            chatgpt_tokens("acct-a", "a@example.com"),
            Utc::now(),
            None,
            true,
        )
        .expect("insert a");
        let b = auth_accounts::upsert_chatgpt_account(
            home.path(),
            chatgpt_tokens("acct-b", "b@example.com"),
            Utc::now(),
            None,
            false,
        )
        .expect("insert b");

        let mut state = RateLimitSwitchState::default();
        let now = fixed_now();
        let next = switch_active_account_on_rate_limit(
            home.path(),
            &mut state,
            false,
            now,
            a.id.as_str(),
            AuthMode::ChatGPT,
            None,
        )
        .expect("switch");

        assert_eq!(next.as_deref(), Some(b.id.as_str()));

        let active = auth_accounts::get_active_account_id(home.path())
            .expect("active account")
            .expect("active account id");
        assert_eq!(active, b.id);
    }
}
