use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use crate::protocol::RateLimitSnapshotEvent;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json;

use crate::protocol::TokenUsage;

const USAGE_VERSION: u32 = 1;
const USAGE_SUBDIR: &str = "usage";
const HOURLY_HISTORY_DAYS: i64 = 7;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenTotals {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub cached_input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub reasoning_output_tokens: u64,
    #[serde(default)]
    pub total_tokens: u64,
}

impl TokenTotals {
    fn add_usage(&mut self, usage: &TokenUsage) {
        self.input_tokens = self.input_tokens.saturating_add(usage.input_tokens);
        self.cached_input_tokens = self
            .cached_input_tokens
            .saturating_add(usage.cached_input_tokens);
        self.output_tokens = self.output_tokens.saturating_add(usage.output_tokens);
        self.reasoning_output_tokens = self
            .reasoning_output_tokens
            .saturating_add(usage.reasoning_output_tokens);
        self.total_tokens = self.total_tokens.saturating_add(usage.total_tokens);
    }

    fn add_totals(&mut self, other: &TokenTotals) {
        self.input_tokens = self.input_tokens.saturating_add(other.input_tokens);
        self.cached_input_tokens = self
            .cached_input_tokens
            .saturating_add(other.cached_input_tokens);
        self.output_tokens = self.output_tokens.saturating_add(other.output_tokens);
        self.reasoning_output_tokens = self
            .reasoning_output_tokens
            .saturating_add(other.reasoning_output_tokens);
        self.total_tokens = self.total_tokens.saturating_add(other.total_tokens);
    }

    fn from_usage(usage: &TokenUsage) -> Self {
        let mut totals = TokenTotals::default();
        totals.add_usage(usage);
        totals
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TokenWindowEntry {
    timestamp: DateTime<Utc>,
    tokens: TokenTotals,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct RateLimitInfo {
    #[serde(default)]
    snapshot: Option<RateLimitSnapshotEvent>,
    #[serde(default)]
    observed_at: Option<DateTime<Utc>>,
    #[serde(default, alias = "next_reset_at")]
    primary_next_reset_at: Option<DateTime<Utc>>,
    #[serde(default)]
    secondary_next_reset_at: Option<DateTime<Utc>>,
    #[serde(default)]
    last_usage_limit_hit_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AccountUsageData {
    version: u32,
    account_id: String,
    #[serde(default)]
    plan: Option<String>,
    #[serde(default)]
    last_updated: DateTime<Utc>,
    #[serde(default)]
    totals: TokenTotals,
    #[serde(default)]
    hourly_entries: Vec<TokenWindowEntry>,
    #[serde(default)]
    tokens_last_hour: TokenTotals,
    #[serde(default)]
    rate_limit: Option<RateLimitInfo>,
}

impl AccountUsageData {
    fn new(account_id: String) -> Self {
        Self {
            version: USAGE_VERSION,
            account_id,
            plan: None,
            last_updated: Utc::now(),
            totals: TokenTotals::default(),
            hourly_entries: Vec::new(),
            tokens_last_hour: TokenTotals::default(),
            rate_limit: None,
        }
    }

    fn apply_plan(&mut self, plan: Option<&str>) {
        if let Some(plan) = plan {
            if self.plan.as_deref() != Some(plan) {
                self.plan = Some(plan.to_string());
            }
        }
    }

    fn update_last_hour(&mut self, now: DateTime<Utc>) {
        let hourly_cutoff = now - Duration::hours(1);
        let history_cutoff = now - Duration::days(HOURLY_HISTORY_DAYS);
        self.hourly_entries
            .retain(|entry| entry.timestamp >= history_cutoff);

        let mut totals = TokenTotals::default();
        for entry in &self.hourly_entries {
            if entry.timestamp < hourly_cutoff {
                continue;
            }
            totals.add_totals(&entry.tokens);
        }
        self.tokens_last_hour = totals;
    }
}

#[derive(Debug, Clone)]
pub struct StoredRateLimitSnapshot {
    pub account_id: String,
    pub plan: Option<String>,
    pub snapshot: Option<RateLimitSnapshotEvent>,
    pub observed_at: Option<DateTime<Utc>>,
    pub primary_next_reset_at: Option<DateTime<Utc>>,
    pub secondary_next_reset_at: Option<DateTime<Utc>>,
    pub last_usage_limit_hit_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct StoredUsageEntry {
    pub timestamp: DateTime<Utc>,
    pub tokens: TokenTotals,
}

#[derive(Debug, Clone)]
pub struct StoredUsageSummary {
    pub account_id: String,
    pub plan: Option<String>,
    pub totals: TokenTotals,
    pub last_updated: DateTime<Utc>,
    pub hourly_entries: Vec<StoredUsageEntry>,
}

fn usage_dir(codex_home: &Path) -> PathBuf {
    codex_home.join(USAGE_SUBDIR)
}

fn usage_file_path(codex_home: &Path, account_id: &str) -> PathBuf {
    usage_dir(codex_home).join(format!("{account_id}.json"))
}

fn with_usage_file<F>(
    codex_home: &Path,
    account_id: &str,
    plan: Option<&str>,
    mut update: F,
) -> std::io::Result<()>
where
    F: FnMut(&mut AccountUsageData),
{
    let usage_dir = usage_dir(codex_home);
    fs::create_dir_all(&usage_dir)?;

    let path = usage_file_path(codex_home, account_id);
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(&path)?;

    file.lock_exclusive()?;

    let mut data = if file.metadata()?.len() == 0 {
        AccountUsageData::new(account_id.to_string())
    } else {
        let mut contents = String::new();
        file.seek(SeekFrom::Start(0))?;
        file.read_to_string(&mut contents)?;
        if contents.trim().is_empty() {
            AccountUsageData::new(account_id.to_string())
        } else {
            match serde_json::from_str::<AccountUsageData>(&contents) {
                Ok(mut parsed) => {
                    if parsed.version != USAGE_VERSION {
                        parsed.version = USAGE_VERSION;
                    }
                    parsed
                }
                Err(_) => AccountUsageData::new(account_id.to_string()),
            }
        }
    };

    data.apply_plan(plan);
    update(&mut data);

    let json = serde_json::to_string_pretty(&data)?;
    file.seek(SeekFrom::Start(0))?;
    file.set_len(0)?;
    file.write_all(json.as_bytes())?;
    file.flush()?;
    file.unlock()?;
    Ok(())
}

pub fn record_token_usage(
    codex_home: &Path,
    account_id: &str,
    plan: Option<&str>,
    usage: &TokenUsage,
    observed_at: DateTime<Utc>,
) -> std::io::Result<()> {
    with_usage_file(codex_home, account_id, plan, |data| {
        data.last_updated = observed_at;
        data.totals.add_usage(usage);
        data.hourly_entries.push(TokenWindowEntry {
            timestamp: observed_at,
            tokens: TokenTotals::from_usage(usage),
        });
        data.update_last_hour(observed_at);
    })
}

pub fn record_rate_limit_snapshot(
    codex_home: &Path,
    account_id: &str,
    plan: Option<&str>,
    snapshot: &RateLimitSnapshotEvent,
    observed_at: DateTime<Utc>,
) -> std::io::Result<()> {
    with_usage_file(codex_home, account_id, plan, |data| {
        data.last_updated = observed_at;
        let mut info = data.rate_limit.take().unwrap_or_default();
        info.snapshot = Some(snapshot.clone());
        info.observed_at = Some(observed_at);
        info.primary_next_reset_at = snapshot
            .primary_reset_after_seconds
            .map(|seconds| observed_at + Duration::seconds(seconds as i64));
        info.secondary_next_reset_at = snapshot
            .secondary_reset_after_seconds
            .map(|seconds| observed_at + Duration::seconds(seconds as i64));
        data.rate_limit = Some(info);
    })
}

pub fn list_rate_limit_snapshots(
    codex_home: &Path,
) -> std::io::Result<Vec<StoredRateLimitSnapshot>> {
    let usage_dir = usage_dir(codex_home);
    let mut results = Vec::new();

    let entries = match fs::read_dir(&usage_dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(results),
        Err(err) => return Err(err),
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if entry
            .file_type()
            .ok()
            .map(|ft| ft.is_file())
            .unwrap_or(false)
            && path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("json"))
                .unwrap_or(false)
        {
            let contents = match fs::read_to_string(&path) {
                Ok(text) => text,
                Err(_) => continue,
            };
            let data: AccountUsageData = match serde_json::from_str(&contents) {
                Ok(data) => data,
                Err(_) => continue,
                };
            let rate = data.rate_limit.unwrap_or_default();
            let primary_next_reset_at = rate.primary_next_reset_at;
            let secondary_next_reset_at = rate
                .secondary_next_reset_at
                .or(rate.primary_next_reset_at);
            results.push(StoredRateLimitSnapshot {
                account_id: data.account_id,
                plan: data.plan,
                snapshot: rate.snapshot,
                observed_at: rate.observed_at,
                primary_next_reset_at,
                secondary_next_reset_at,
                last_usage_limit_hit_at: rate.last_usage_limit_hit_at,
            });
        }
    }

    Ok(results)
}

pub fn record_usage_limit_hint(
    codex_home: &Path,
    account_id: &str,
    plan: Option<&str>,
    resets_in_seconds: Option<u64>,
    observed_at: DateTime<Utc>,
) -> std::io::Result<()> {
    if resets_in_seconds.is_none() {
        return with_usage_file(codex_home, account_id, plan, |data| {
            data.last_updated = observed_at;
            let mut info = data.rate_limit.take().unwrap_or_default();
            info.last_usage_limit_hit_at = Some(observed_at);
            data.rate_limit = Some(info);
        });
    }

    with_usage_file(codex_home, account_id, plan, |data| {
        data.last_updated = observed_at;
        let mut info = data.rate_limit.take().unwrap_or_default();
        info.last_usage_limit_hit_at = Some(observed_at);
        if let Some(seconds) = resets_in_seconds {
            let reset_at = observed_at + Duration::seconds(seconds as i64);
            info.primary_next_reset_at = Some(reset_at);
            info.secondary_next_reset_at = Some(reset_at);
        }
        data.rate_limit = Some(info);
    })
}

pub fn load_account_usage(
    codex_home: &Path,
    account_id: &str,
) -> std::io::Result<Option<StoredUsageSummary>> {
    let path = usage_file_path(codex_home, account_id);
    let contents = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err),
    };

    let data: AccountUsageData = serde_json::from_str(&contents)?;
    let hourly_entries = data
        .hourly_entries
        .into_iter()
        .map(|entry| StoredUsageEntry {
            timestamp: entry.timestamp,
            tokens: entry.tokens,
        })
        .collect();

    Ok(Some(StoredUsageSummary {
        account_id: data.account_id,
        plan: data.plan,
        totals: data.totals,
        last_updated: data.last_updated,
        hourly_entries,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use crate::protocol::TokenUsage;
    use tempfile::TempDir;

    fn sample_usage() -> TokenUsage {
        TokenUsage {
            input_tokens: 120,
            cached_input_tokens: 20,
            output_tokens: 80,
            reasoning_output_tokens: 10,
            total_tokens: 210,
        }
    }

    #[test]
    fn creates_usage_file_and_accumulates_tokens() {
        let home = TempDir::new().expect("tempdir");
        let now = Utc::now();

        record_token_usage(
            home.path(),
            "acct-1",
            Some("Team"),
            &sample_usage(),
            now,
        )
        .expect("record usage");

        let path = usage_file_path(home.path(), "acct-1");
        let mut contents = String::new();
        File::open(path)
            .expect("open usage file")
            .read_to_string(&mut contents)
            .expect("read usage file");

        let parsed: AccountUsageData = serde_json::from_str(&contents).expect("parse usage json");
        assert_eq!(parsed.account_id, "acct-1");
        assert_eq!(parsed.plan.as_deref(), Some("Team"));
        assert_eq!(parsed.totals.input_tokens, 120);
        assert_eq!(parsed.totals.output_tokens, 80);
        assert_eq!(parsed.tokens_last_hour.total_tokens, 210);
        assert_eq!(parsed.hourly_entries.len(), 1);
    }
}
