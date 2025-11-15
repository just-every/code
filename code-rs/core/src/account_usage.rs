use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Timelike, Utc};
use crate::protocol::RateLimitSnapshotEvent;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json;

use crate::protocol::TokenUsage;

const USAGE_VERSION: u32 = 1;
const USAGE_SUBDIR: &str = "usage";
const HOURLY_HISTORY_DAYS: i64 = 183; // retain ~6 months of hourly usage for history views
const UNKNOWN_RESET_RELOG_INTERVAL: Duration = Duration::hours(24);
const RESET_PASSED_TOLERANCE: Duration = Duration::seconds(5);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RateLimitWarningScope {
    Primary,
    Secondary,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct RateLimitWarningRecord {
    threshold: f64,
    #[serde(default)]
    reset_at: Option<DateTime<Utc>>,
    #[serde(default)]
    logged_at: Option<DateTime<Utc>>,
}

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
struct AggregatedUsageEntry {
    period_start: DateTime<Utc>,
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
    #[serde(default)]
    primary_threshold_logs: Vec<RateLimitWarningRecord>,
    #[serde(default)]
    secondary_threshold_logs: Vec<RateLimitWarningRecord>,
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
    hourly_buckets: Vec<AggregatedUsageEntry>,
    #[serde(default)]
    daily_buckets: Vec<AggregatedUsageEntry>,
    #[serde(default)]
    monthly_buckets: Vec<AggregatedUsageEntry>,
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
            hourly_buckets: Vec::new(),
            daily_buckets: Vec::new(),
            monthly_buckets: Vec::new(),
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
        self.compact_usage(now);
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

    fn compact_usage(&mut self, now: DateTime<Utc>) {
        let recent_cutoff = now - Duration::hours(1);
        let mut rollover: BTreeMap<DateTime<Utc>, TokenTotals> = BTreeMap::new();
        let mut recent: Vec<TokenWindowEntry> = Vec::new();

        for entry in self.hourly_entries.drain(..) {
            if entry.timestamp >= recent_cutoff {
                recent.push(entry);
            } else {
                let bucket = truncate_to_hour(entry.timestamp);
                rollover
                    .entry(bucket)
                    .or_insert_with(TokenTotals::default)
                    .add_totals(&entry.tokens);
            }
        }

        self.hourly_entries = recent;

        for (period_start, tokens) in rollover {
            let day_start = truncate_to_day(period_start);
            add_to_bucket(&mut self.daily_buckets, day_start, tokens.clone());
            add_to_bucket(&mut self.hourly_buckets, period_start, tokens);
        }

        self.compact_hourly_buckets(now);
        self.compact_daily_buckets(now);
    }

    fn compact_hourly_buckets(&mut self, now: DateTime<Utc>) {
        if self.hourly_buckets.is_empty() {
            return;
        }

        let current_hour = truncate_to_hour(now);
        let cutoff = current_hour - Duration::hours(24);
        let mut remaining: Vec<AggregatedUsageEntry> = Vec::new();

        for entry in self.hourly_buckets.drain(..) {
            if entry.period_start >= cutoff {
                remaining.push(entry);
            }
        }

        remaining.sort_by_key(|item| item.period_start);
        self.hourly_buckets = remaining;
    }

    fn compact_daily_buckets(&mut self, now: DateTime<Utc>) {
        if self.daily_buckets.is_empty() {
            return;
        }

        let today = truncate_to_day(now);
        let cutoff = today - Duration::days(30);
        let mut remaining: Vec<AggregatedUsageEntry> = Vec::new();
        let mut monthly_rollover: BTreeMap<DateTime<Utc>, TokenTotals> = BTreeMap::new();

        for entry in self.daily_buckets.drain(..) {
            if entry.period_start < cutoff {
                let month_key = truncate_to_month(entry.period_start);
                monthly_rollover
                    .entry(month_key)
                    .or_insert_with(TokenTotals::default)
                    .add_totals(&entry.tokens);
            } else {
                remaining.push(entry);
            }
        }

        remaining.sort_by_key(|item| item.period_start);
        self.daily_buckets = remaining;

        for (period_start, tokens) in monthly_rollover {
            add_to_bucket(&mut self.monthly_buckets, period_start, tokens);
        }
    }
}

fn add_to_bucket(
    buckets: &mut Vec<AggregatedUsageEntry>,
    period_start: DateTime<Utc>,
    tokens: TokenTotals,
) {
    match buckets.binary_search_by(|entry| entry.period_start.cmp(&period_start)) {
        Ok(idx) => buckets[idx].tokens.add_totals(&tokens),
        Err(idx) => {
            buckets.insert(
                idx,
                AggregatedUsageEntry {
                    period_start,
                    tokens,
                },
            );
        }
    }
}

fn truncate_to_hour(ts: DateTime<Utc>) -> DateTime<Utc> {
    let naive = ts.naive_utc();
    let trimmed = naive
        .with_minute(0)
        .and_then(|dt| dt.with_second(0))
        .and_then(|dt| dt.with_nanosecond(0))
        .expect("valid hour truncation");
    Utc.from_utc_datetime(&trimmed)
}

fn truncate_to_day(ts: DateTime<Utc>) -> DateTime<Utc> {
    let date = ts.date_naive();
    let start = date.and_hms_opt(0, 0, 0).expect("valid day truncation");
    Utc.from_utc_datetime(&start)
}

fn truncate_to_month(ts: DateTime<Utc>) -> DateTime<Utc> {
    let date = ts.date_naive();
    let month_start = NaiveDate::from_ymd_opt(date.year(), date.month(), 1)
        .expect("valid month truncation")
        .and_hms_opt(0, 0, 0)
        .expect("valid month start time");
    Utc.from_utc_datetime(&month_start)
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
pub struct StoredUsageBucket {
    pub period_start: DateTime<Utc>,
    pub tokens: TokenTotals,
}

#[derive(Debug, Clone)]
pub struct StoredUsageSummary {
    pub account_id: String,
    pub plan: Option<String>,
    pub totals: TokenTotals,
    pub last_updated: DateTime<Utc>,
    pub hourly_entries: Vec<StoredUsageEntry>,
    pub hourly_buckets: Vec<StoredUsageBucket>,
    pub daily_buckets: Vec<StoredUsageBucket>,
    pub monthly_buckets: Vec<StoredUsageBucket>,
}

fn usage_dir(code_home: &Path) -> PathBuf {
    code_home.join(USAGE_SUBDIR)
}

fn warning_log_path(code_home: &Path) -> PathBuf {
    usage_dir(code_home).join("rate_limit_warnings.log")
}

fn usage_file_path(code_home: &Path, account_id: &str) -> PathBuf {
    usage_dir(code_home).join(format!("{account_id}.json"))
}

fn with_usage_file<F>(
    code_home: &Path,
    account_id: &str,
    plan: Option<&str>,
    mut update: F,
) -> std::io::Result<()>
where
    F: FnMut(&mut AccountUsageData),
{
    let usage_dir = usage_dir(code_home);
    fs::create_dir_all(&usage_dir)?;

    let path = usage_file_path(code_home, account_id);
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
    let tmp_path = usage_dir.join(format!("{account_id}.json.tmp"));
    {
        let mut tmp = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp_path)?;
        tmp.write_all(json.as_bytes())?;
        tmp.sync_all()?;
    }
    if let Err(err) = fs::rename(&tmp_path, &path) {
        let _ = fs::remove_file(&tmp_path);
        file.unlock()?;
        return Err(err);
    }
    file.unlock()?;
    Ok(())
}

pub fn record_token_usage(
    code_home: &Path,
    account_id: &str,
    plan: Option<&str>,
    usage: &TokenUsage,
    observed_at: DateTime<Utc>,
) -> std::io::Result<()> {
    with_usage_file(code_home, account_id, plan, |data| {
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
    code_home: &Path,
    account_id: &str,
    plan: Option<&str>,
    snapshot: &RateLimitSnapshotEvent,
    observed_at: DateTime<Utc>,
) -> std::io::Result<()> {
    with_usage_file(code_home, account_id, plan, |data| {
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
    code_home: &Path,
) -> std::io::Result<Vec<StoredRateLimitSnapshot>> {
    let usage_dir = usage_dir(code_home);
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
    code_home: &Path,
    account_id: &str,
    plan: Option<&str>,
    resets_in_seconds: Option<u64>,
    observed_at: DateTime<Utc>,
) -> std::io::Result<()> {
    if resets_in_seconds.is_none() {
        return with_usage_file(code_home, account_id, plan, |data| {
            data.last_updated = observed_at;
            let mut info = data.rate_limit.take().unwrap_or_default();
            info.last_usage_limit_hit_at = Some(observed_at);
            data.rate_limit = Some(info);
        });
    }

    with_usage_file(code_home, account_id, plan, |data| {
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

fn record_threshold_log(
    logs: &mut Vec<RateLimitWarningRecord>,
    threshold: f64,
    reset_at: Option<DateTime<Utc>>,
    observed_at: DateTime<Utc>,
) -> bool {
    if let Some(existing) = logs.iter_mut().find(|entry| {
        (entry.threshold - threshold).abs() < f64::EPSILON
    }) {
        let previous_reset = existing.reset_at;
        let previous_logged = existing.logged_at;
        let new_reset = reset_at;

        let reset_moved_earlier = match (previous_reset, new_reset) {
            (Some(prev), Some(next)) => next + RESET_PASSED_TOLERANCE < prev,
            _ => false,
        };

        let logged_after_prev_reset = match (previous_logged, previous_reset) {
            (Some(logged), Some(prev)) => logged >= prev,
            _ => false,
        };

        let prev_reset_elapsed = previous_reset
            .map(|prev| observed_at + RESET_PASSED_TOLERANCE >= prev)
            .unwrap_or(false);

        let unknown_reset_elapsed = (previous_reset.is_none() || new_reset.is_none())
            && previous_logged
                .is_some_and(|logged| observed_at.signed_duration_since(logged) >= UNKNOWN_RESET_RELOG_INTERVAL);

        let mut should_clear = false;

        if reset_moved_earlier {
            should_clear = true;
        } else if prev_reset_elapsed && !logged_after_prev_reset {
            should_clear = true;
        } else if unknown_reset_elapsed {
            should_clear = true;
        }

        existing.reset_at = new_reset;

        if should_clear {
            existing.logged_at = None;
        }

        if existing.logged_at.is_none() {
            existing.logged_at = Some(observed_at);
            return true;
        }

        return false;
    }

    logs.push(RateLimitWarningRecord {
        threshold,
        reset_at,
        logged_at: Some(observed_at),
    });
    true
}

fn append_rate_limit_warning_log(
    code_home: &Path,
    account_id: &str,
    plan: Option<&str>,
    scope: RateLimitWarningScope,
    threshold: f64,
    reset_at: Option<DateTime<Utc>>,
    observed_at: DateTime<Utc>,
    message: &str,
) -> std::io::Result<()> {
    let dir = usage_dir(code_home);
    fs::create_dir_all(&dir)?;
    let path = warning_log_path(code_home);
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .read(true)
        .open(&path)?;
    file.lock_exclusive()?;
    let scope_field = match scope {
        RateLimitWarningScope::Primary => "primary",
        RateLimitWarningScope::Secondary => "secondary",
    };
    let plan_field = plan.unwrap_or("-");
    let reset_field = reset_at
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| "-".to_string());
    let line = format!(
        "{}\t{}\t{}\t{:.0}\t{}\t{}\t{}\n",
        observed_at.to_rfc3339(),
        account_id,
        plan_field,
        threshold,
        scope_field,
        reset_field,
        message,
    );
    let write_res = file.write_all(line.as_bytes());
    let unlock_res = file.unlock();
    write_res?;
    unlock_res?;
    Ok(())
}

pub fn record_rate_limit_warning(
    code_home: &Path,
    account_id: &str,
    plan: Option<&str>,
    scope: RateLimitWarningScope,
    threshold: f64,
    reset_at: Option<DateTime<Utc>>,
    observed_at: DateTime<Utc>,
    message: &str,
) -> std::io::Result<bool> {
    let mut should_log = false;
    with_usage_file(code_home, account_id, plan, |data| {
        data.last_updated = observed_at;
        let mut info = data.rate_limit.take().unwrap_or_default();
        let logs = match scope {
            RateLimitWarningScope::Primary => &mut info.primary_threshold_logs,
            RateLimitWarningScope::Secondary => &mut info.secondary_threshold_logs,
        };
        if record_threshold_log(logs, threshold, reset_at, observed_at) {
            should_log = true;
        }
        data.rate_limit = Some(info);
    })?;

    if should_log {
        append_rate_limit_warning_log(
            code_home,
            account_id,
            plan,
            scope,
            threshold,
            reset_at,
            observed_at,
            message,
        )?;
    }

    Ok(should_log)
}

pub fn load_account_usage(
    code_home: &Path,
    account_id: &str,
) -> std::io::Result<Option<StoredUsageSummary>> {
    let path = usage_file_path(code_home, account_id);
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

    let hourly_buckets = data
        .hourly_buckets
        .into_iter()
        .map(|entry| StoredUsageBucket {
            period_start: entry.period_start,
            tokens: entry.tokens,
        })
        .collect();

    let daily_buckets = data
        .daily_buckets
        .into_iter()
        .map(|entry| StoredUsageBucket {
            period_start: entry.period_start,
            tokens: entry.tokens,
        })
        .collect();

    let monthly_buckets = data
        .monthly_buckets
        .into_iter()
        .map(|entry| StoredUsageBucket {
            period_start: entry.period_start,
            tokens: entry.tokens,
        })
        .collect();

    Ok(Some(StoredUsageSummary {
        account_id: data.account_id,
        plan: data.plan,
        totals: data.totals,
        last_updated: data.last_updated,
        hourly_entries,
        hourly_buckets,
        daily_buckets,
        monthly_buckets,
    }))
}

#[cfg(test)]
mod tests {
    //! Regression coverage for rate-limit warning relogging.
    //!
    //! These cases enforce the desired behaviour:
    //! - **No duplicate within a window**: once a threshold logs, subsequent polls before
    //!   the stored reset timestamp must remain silent even if the backend repeats or
    //!   extends the reset time.
    //! - **Relog after reset passes**: the first poll at or after the recorded reset may
    //!   emit again, regardless of whether the backend has already advanced the window.
    //! - **Relog on earlier reset**: if the backend moves the reset earlier (window
    //!   shrinks), we allow an immediate relog even before the previously stored reset.
    //! - **Unknown reset fallback**: when reset metadata disappears, we rely on the
    //!   24-hour `UNKNOWN_RESET_RELOG_INTERVAL` to unblock further warnings. When the
    //!   backend begins reporting timestamps again, we should also allow a relog provided
    //!   the fallback window has elapsed.
    //! - **Missing metadata alone is not enough**: before the fallback timer elapses we
    //!   must keep warnings muted even if new snapshots omit reset times.
    //!
    //! The helper tests below construct scenarios targeting each rule so the state
    //! machine in `record_threshold_log` can be refactored confidently.
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
    fn usage_limit_hint_updates_last_hit_and_resets() {
        let home = TempDir::new().expect("tempdir");
        let now = Utc::now();

        record_usage_limit_hint(home.path(), "acct-1", Some("Team"), Some(300), now)
            .expect("hint recorded");

        let snapshots = list_rate_limit_snapshots(home.path()).expect("snapshot listing");
        assert_eq!(snapshots.len(), 1, "usage hint should create one snapshot entry");
        let snapshot = &snapshots[0];
        assert_eq!(snapshot.account_id, "acct-1");
        assert_eq!(snapshot.plan.as_deref(), Some("Team"));
        assert_eq!(snapshot.last_usage_limit_hit_at, Some(now));
        let expected_reset = now + Duration::seconds(300);
        assert_eq!(snapshot.primary_next_reset_at, Some(expected_reset));
        assert_eq!(snapshot.secondary_next_reset_at, Some(expected_reset));
    }

    #[test]
    fn token_usage_compacts_old_hourly_entries_into_buckets() {
        let home = TempDir::new().expect("tempdir");
        let account = "acct-compaction";
        let usage = sample_usage();
        let now = Utc::now();

        // Two records that should roll into hourly buckets once a fresh entry is written.
        record_token_usage(
            home.path(),
            account,
            None,
            &usage,
            now - Duration::hours(2),
        )
        .expect("first record");
        record_token_usage(
            home.path(),
            account,
            None,
            &usage,
            now - Duration::minutes(90),
        )
        .expect("second record");

        // Recent usage that should remain in the in-memory hourly entries window.
        record_token_usage(home.path(), account, None, &usage, now)
            .expect("recent record");

        let summary = load_account_usage(home.path(), account)
            .expect("load summary")
            .expect("summary present");

        // Only the most recent entry should remain in the sliding hourly window.
        assert_eq!(summary.hourly_entries.len(), 1);
        assert!(summary.hourly_buckets.len() >= 1, "older usage should compact into buckets");
        assert!(summary.daily_buckets.len() >= 1, "hourly buckets roll into daily aggregates");
    }

    #[test]
    fn rate_limit_warning_only_logs_once_per_reset() {
        let home = TempDir::new().expect("tempdir");
        let now = Utc::now();
        let reset_at = now + Duration::days(7);

        let first = record_rate_limit_warning(
            home.path(),
            "acct-1",
            Some("Team"),
            RateLimitWarningScope::Secondary,
            75.0,
            Some(reset_at),
            now,
            "Secondary usage exceeded 75% of the limit. Run /limits for detailed usage.",
        )
        .expect("first record succeeds");

        assert!(first, "first logging should emit");

        let second = record_rate_limit_warning(
            home.path(),
            "acct-1",
            Some("Team"),
            RateLimitWarningScope::Secondary,
            75.0,
            Some(now + Duration::days(7) + Duration::hours(6)),
            now + Duration::hours(6),
            "Secondary usage exceeded 75% of the limit. Run /limits for detailed usage.",
        )
        .expect("second record succeeds");

        assert!(!second, "duplicate logging before reset should be suppressed");

        let third = record_rate_limit_warning(
            home.path(),
            "acct-1",
            Some("Team"),
            RateLimitWarningScope::Secondary,
            75.0,
            Some(now + Duration::days(15)),
            now + Duration::days(8),
            "Secondary usage exceeded 75% of the limit. Run /limits for detailed usage.",
        )
        .expect("third record succeeds");

        assert!(third, "after reset passes we should emit again");
    }

    #[test]
    fn rate_limit_warning_relogs_after_reset_with_new_timestamp() {
        let home = TempDir::new().expect("tempdir");
        let now = Utc::now();
        let msg = "Secondary usage exceeded 75% of the limit. Run /limits for detailed usage.";

        let first = record_rate_limit_warning(
            home.path(),
            "acct-1",
            Some("Team"),
            RateLimitWarningScope::Secondary,
            75.0,
            Some(now + Duration::hours(1)),
            now,
            msg,
        )
        .expect("first record succeeds");
        assert!(first);

        // Backend extends reset window beyond the old reset time; we should re-emit now that
        // the prior window has expired and a new one started.
        let second = record_rate_limit_warning(
            home.path(),
            "acct-1",
            Some("Team"),
            RateLimitWarningScope::Secondary,
            75.0,
            Some(now + Duration::hours(2)),
            now + Duration::minutes(65),
            msg,
        )
        .expect("second record succeeds");
        assert!(second, "after reset we should log again even if next window is later");

        // Subsequent updates inside the new window should remain suppressed until that reset passes.
        let third = record_rate_limit_warning(
            home.path(),
            "acct-1",
            Some("Team"),
            RateLimitWarningScope::Secondary,
            75.0,
            Some(now + Duration::hours(2)),
            now + Duration::minutes(70),
            msg,
        )
        .expect("third record succeeds");
        assert!(!third, "duplicate logging inside the same window should stay muted");
    }

    #[test]
    fn rate_limit_warning_relogs_after_reset_even_if_logged_just_before() {
        let home = TempDir::new().expect("tempdir");
        let now = Utc::now();
        let reset_at = now + Duration::minutes(1);
        let msg = "Secondary usage exceeded 75% of the limit. Run /limits for detailed usage.";

        let first = record_rate_limit_warning(
            home.path(),
            "acct-1",
            Some("Team"),
            RateLimitWarningScope::Secondary,
            75.0,
            Some(reset_at),
            reset_at - Duration::seconds(3),
            msg,
        )
        .expect("first record succeeds");
        assert!(first);

        // After the reset passes, with a new window scheduled further out, we should relog.
        let second = record_rate_limit_warning(
            home.path(),
            "acct-1",
            Some("Team"),
            RateLimitWarningScope::Secondary,
            75.0,
            Some(reset_at + Duration::hours(1)),
            reset_at + Duration::seconds(45),
            msg,
        )
        .expect("second record succeeds");
        assert!(second, "post-reset poll should emit again even if prior log was moments before reset");
    }

    #[test]
    fn rate_limit_warning_relogs_after_unknown_reset_interval() {
        let home = TempDir::new().expect("tempdir");
        let now = Utc::now();
        let msg = "Secondary usage exceeded 75% of the limit. Run /limits for detailed usage.";

        let first = record_rate_limit_warning(
            home.path(),
            "acct-1",
            Some("Team"),
            RateLimitWarningScope::Secondary,
            75.0,
            Some(now + Duration::hours(1)),
            now,
            msg,
        )
        .expect("first record succeeds");
        assert!(first);

        // Backend stops providing reset info — still within backoff window.
        let second = record_rate_limit_warning(
            home.path(),
            "acct-1",
            Some("Team"),
            RateLimitWarningScope::Secondary,
            75.0,
            None,
            now + Duration::minutes(20),
            msg,
        )
        .expect("second record succeeds");
        assert!(!second, "dropping reset info should keep warning muted initially");

        // After the unknown interval we should allow another log.
        let third = record_rate_limit_warning(
            home.path(),
            "acct-1",
            Some("Team"),
            RateLimitWarningScope::Secondary,
            75.0,
            None,
            now + Duration::hours(25),
            msg,
        )
        .expect("third record succeeds");
        assert!(third, "after backoff expires we should re-emit");
    }

    #[test]
    fn rate_limit_warning_relogs_when_reset_info_returns() {
        let home = TempDir::new().expect("tempdir");
        let now = Utc::now();
        let msg = "Secondary usage exceeded 75% of the limit. Run /limits for detailed usage.";

        let first = record_rate_limit_warning(
            home.path(),
            "acct-1",
            Some("Team"),
            RateLimitWarningScope::Secondary,
            75.0,
            Some(now + Duration::hours(1)),
            now,
            msg,
        )
        .expect("first record succeeds");
        assert!(first);

        let second = record_rate_limit_warning(
            home.path(),
            "acct-1",
            Some("Team"),
            RateLimitWarningScope::Secondary,
            75.0,
            None,
            now + Duration::minutes(20),
            msg,
        )
        .expect("second record succeeds");
        assert!(!second);

        let third = record_rate_limit_warning(
            home.path(),
            "acct-1",
            Some("Team"),
            RateLimitWarningScope::Secondary,
            75.0,
            Some(now + Duration::hours(30)),
            now + Duration::hours(25),
            msg,
        )
        .expect("third record succeeds");
        assert!(third, "restored reset metadata after fallback window should re-log");
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
