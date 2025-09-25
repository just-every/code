use chrono::DateTime;
use chrono::Utc;
use once_cell::sync::Lazy;
use serde::Deserialize;
use serde::Serialize;
use std::fs;
use std::io::ErrorKind;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use codex_core::config::resolve_codex_path_for_read;
use codex_core::config::Config;
use codex_core::default_client::create_client;
use tokio::process::Command;
use tracing::{info, warn};

static FORCE_UPGRADE_PREVIEW: Lazy<bool> = Lazy::new(|| {
    std::env::var("SHOW_UPGRADE")
        .map(|value| {
            let normalized = value.trim().to_ascii_lowercase();
            matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
});

pub fn upgrade_ui_enabled() -> bool {
    !cfg!(debug_assertions) || *FORCE_UPGRADE_PREVIEW
}

pub fn auto_upgrade_runtime_enabled() -> bool {
    !cfg!(debug_assertions)
}

pub fn get_upgrade_version(config: &Config) -> Option<String> {
    let version_file = version_filepath(config);
    let read_path = resolve_codex_path_for_read(&config.codex_home, Path::new(VERSION_FILENAME));
    let info = read_version_info(&read_path).ok();
    let originator = config.responses_originator_header.clone();

    // Always refresh the cached latest version in the background so TUI startup
    // isn’t blocked by a network call. The UI reads the previously cached
    // value (if any) for this run; the next run shows the banner if needed.
    tokio::spawn(async move {
        check_for_update(&version_file, &originator)
            .await
            .inspect_err(|e| tracing::error!("Failed to update version: {e}"))
    });

    info.and_then(|info| {
        let current_version = codex_version::version();
        if is_newer(&info.latest_version, current_version).unwrap_or(false) {
            Some(info.latest_version)
        } else {
            None
        }
    })
}

#[derive(Debug, Clone)]
pub struct UpdateCheckInfo {
    pub latest_version: Option<String>,
}

pub async fn check_for_updates_now(config: &Config) -> anyhow::Result<UpdateCheckInfo> {
    let version_file = version_filepath(config);
    let originator = config.responses_originator_header.clone();
    let info = check_for_update(&version_file, &originator).await?;
    let current_version = codex_version::version().to_string();
    let latest_version = if is_newer(&info.latest_version, &current_version).unwrap_or(false) {
        Some(info.latest_version)
    } else {
        None
    };

    Ok(UpdateCheckInfo {
        latest_version,
    })
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct VersionInfo {
    latest_version: String,
    // ISO-8601 timestamp (RFC3339)
    last_checked_at: DateTime<Utc>,
}

#[derive(Deserialize, Debug, Clone)]
struct ReleaseInfo {
    tag_name: String,
}

const VERSION_FILENAME: &str = "version.json";
const LATEST_RELEASE_URL: &str = "https://api.github.com/repos/just-every/code/releases/latest";
pub const CODE_RELEASE_URL: &str = "https://github.com/just-every/code/releases/latest";

const AUTO_UPGRADE_LOCK_FILE: &str = "auto-upgrade.lock";
const AUTO_UPGRADE_LOCK_TTL: Duration = Duration::from_secs(900); // 15 minutes

#[derive(Debug, Clone)]
pub enum UpgradeResolution {
    Command { command: Vec<String>, display: String },
    Manual { instructions: String },
}

fn version_filepath(config: &Config) -> PathBuf {
    config.codex_home.join(VERSION_FILENAME)
}

pub fn resolve_upgrade_resolution() -> UpgradeResolution {
    if std::env::var_os("CODEX_MANAGED_BY_NPM").is_some() {
        return UpgradeResolution::Command {
            command: vec![
                "npm".to_string(),
                "install".to_string(),
                "-g".to_string(),
                "@just-every/code@latest".to_string(),
            ],
            display: "npm install -g @just-every/code@latest".to_string(),
        };
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(exe_path) = std::env::current_exe() {
            if exe_path.starts_with("/opt/homebrew") || exe_path.starts_with("/usr/local") {
                return UpgradeResolution::Command {
                    command: vec![
                        "brew".to_string(),
                        "upgrade".to_string(),
                        "code".to_string(),
                    ],
                    display: "brew upgrade code".to_string(),
                };
            }
        }
    }

    UpgradeResolution::Manual {
        instructions: format!(
            "Download the latest release from {CODE_RELEASE_URL} and replace the installed binary."
        ),
    }
}

pub async fn auto_upgrade_if_enabled(config: &Config) -> anyhow::Result<Option<String>> {
    if !config.auto_upgrade_enabled {
        return Ok(None);
    }

    let resolution = resolve_upgrade_resolution();
    let (command, command_display) = match resolution {
        UpgradeResolution::Command {
            command,
            display: command_display,
        } if !command.is_empty() => (command, command_display),
        _ => {
            info!("auto-upgrade enabled but no managed installer detected; skipping");
            return Ok(None);
        }
    };

    let info = match check_for_updates_now(config).await {
        Ok(info) => info,
        Err(err) => {
            warn!("auto-upgrade: failed to check for updates: {err}");
            return Ok(None);
        }
    };

    let Some(latest_version) = info.latest_version.clone() else {
        // Already up to date
        return Ok(None);
    };

    let lock = match AutoUpgradeLock::acquire(&config.codex_home) {
        Ok(Some(lock)) => lock,
        Ok(None) => {
            info!("auto-upgrade already in progress by another instance; skipping");
            return Ok(None);
        }
        Err(err) => {
            warn!("auto-upgrade: unable to acquire lock: {err}");
            return Ok(None);
        }
    };

    info!(
        command = command_display.as_str(),
        latest_version = latest_version.as_str(),
        "auto-upgrade: running managed installer"
    );
    let result = run_upgrade_command(command).await;
    drop(lock);

    match result {
        Ok(()) => {
            info!("auto-upgrade: successfully installed {latest_version}");
            Ok(Some(latest_version))
        }
        Err(err) => {
            warn!("auto-upgrade: upgrade command failed: {err}");
            Ok(None)
        }
    }
}

struct AutoUpgradeLock {
    path: PathBuf,
}

impl AutoUpgradeLock {
    fn acquire(codex_home: &Path) -> anyhow::Result<Option<Self>> {
        let path = codex_home.join(AUTO_UPGRADE_LOCK_FILE);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                let timestamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                writeln!(file, "{timestamp}")?;
                Ok(Some(Self { path }))
            }
            Err(err) if err.kind() == ErrorKind::AlreadyExists => {
                if Self::is_stale(&path)? {
                    let _ = fs::remove_file(&path);
                    match fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&path)
                    {
                        Ok(mut file) => {
                            let timestamp = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs();
                            writeln!(file, "{timestamp}")?;
                            Ok(Some(Self { path }))
                        }
                        Err(err) if err.kind() == ErrorKind::AlreadyExists => Ok(None),
                        Err(err) => Err(err.into()),
                    }
                } else {
                    Ok(None)
                }
            }
            Err(err) => Err(err.into()),
        }
    }

    fn is_stale(path: &Path) -> anyhow::Result<bool> {
        match fs::read_to_string(path) {
            Ok(contents) => {
                if let Ok(stored) = contents.trim().parse::<u64>() {
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    Ok(now.saturating_sub(stored) > AUTO_UPGRADE_LOCK_TTL.as_secs())
                } else {
                    Ok(true)
                }
            }
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(true),
            Err(err) => {
                warn!("auto-upgrade: failed reading lock file: {err}");
                Ok(true)
            }
        }
    }
}

impl Drop for AutoUpgradeLock {
    fn drop(&mut self) {
        if let Err(err) = fs::remove_file(&self.path) {
            if err.kind() != ErrorKind::NotFound {
                warn!("auto-upgrade: failed to remove lock file {}: {err}", self.path.display());
            }
        }
    }
}

async fn run_upgrade_command(command: Vec<String>) -> anyhow::Result<()> {
    if command.is_empty() {
        anyhow::bail!("upgrade command is empty");
    }

    let mut cmd = Command::new(&command[0]);
    if command.len() > 1 {
        cmd.args(&command[1..]);
    }
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::null());

    let status = cmd.status().await?;
    if !status.success() {
        anyhow::bail!(
            "upgrade command exited with status {}",
            status.code().map_or_else(|| "signal".to_string(), |c| c.to_string())
        );
    }
    Ok(())
}

fn read_version_info(version_file: &Path) -> anyhow::Result<VersionInfo> {
    let contents = std::fs::read_to_string(version_file)?;
    Ok(serde_json::from_str(&contents)?)
}

async fn check_for_update(version_file: &Path, originator: &str) -> anyhow::Result<VersionInfo> {
    let ReleaseInfo {
        tag_name: latest_tag_name,
    } = create_client(originator)
        .get(LATEST_RELEASE_URL)
        .send()
        .await?
        .error_for_status()?
        .json::<ReleaseInfo>()
        .await?;

    // Support both tagging schemes:
    // - "rust-vX.Y.Z" (legacy Rust-release workflow)
    // - "vX.Y.Z" (general release workflow)
    let latest_version = if let Some(v) = latest_tag_name.strip_prefix("rust-v") {
        v.to_string()
    } else if let Some(v) = latest_tag_name.strip_prefix('v') {
        v.to_string()
    } else {
        // As a last resort, accept the raw tag if it looks like semver
        // so we can recover from unexpected tag formats.
        match parse_version(&latest_tag_name) {
            Some(_) => latest_tag_name.clone(),
            None => anyhow::bail!(
                "Failed to parse latest tag name '{}': expected 'rust-vX.Y.Z' or 'vX.Y.Z'",
                latest_tag_name
            ),
        }
    };

    let info = VersionInfo {
        latest_version,
        last_checked_at: Utc::now(),
    };

    let json_line = format!("{}\n", serde_json::to_string(&info)?);
    if let Some(parent) = version_file.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(version_file, json_line).await?;
    Ok(info)
}

fn is_newer(latest: &str, current: &str) -> Option<bool> {
    match (parse_version(latest), parse_version(current)) {
        (Some(l), Some(c)) => Some(l > c),
        _ => None,
    }
}

fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let mut iter = v.trim().split('.');
    let maj = iter.next()?.parse::<u64>().ok()?;
    let min = iter.next()?.parse::<u64>().ok()?;
    let pat = iter.next()?.parse::<u64>().ok()?;
    Some((maj, min, pat))
}

#[cfg(all(test, feature = "legacy_tests"))]
mod tests {
    use super::*;

    #[test]
    fn prerelease_version_is_not_considered_newer() {
        assert_eq!(is_newer("0.11.0-beta.1", "0.11.0"), None);
        assert_eq!(is_newer("1.0.0-rc.1", "1.0.0"), None);
    }

    #[test]
    fn plain_semver_comparisons_work() {
        assert_eq!(is_newer("0.11.1", "0.11.0"), Some(true));
        assert_eq!(is_newer("0.11.0", "0.11.1"), Some(false));
        assert_eq!(is_newer("1.0.0", "0.9.9"), Some(true));
        assert_eq!(is_newer("0.9.9", "1.0.0"), Some(false));
    }

    #[test]
    fn whitespace_is_ignored() {
        assert_eq!(parse_version(" 1.2.3 \n"), Some((1, 2, 3)));
        assert_eq!(is_newer(" 1.2.3 ", "1.2.2"), Some(true));
    }
}
