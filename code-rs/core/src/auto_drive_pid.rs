use chrono::Utc;
use serde::Serialize;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// Distinguishes which part of the product started Auto Drive so external
/// tooling can annotate the dump appropriately.
#[derive(Copy, Clone, Debug)]
pub enum AutoDriveMode {
    Exec,
    Tui,
}

impl AutoDriveMode {
    fn as_str(&self) -> &'static str {
        match self {
            AutoDriveMode::Exec => "exec",
            AutoDriveMode::Tui => "tui",
        }
    }
}

#[derive(Serialize)]
struct AutoDrivePidMetadata {
    pid: u32,
    started_at: String,
    mode: &'static str,
    goal: Option<String>,
    cwd: Option<PathBuf>,
    command: Option<String>,
}

/// Small RAII helper that writes `~/.code/auto-drive/pid-<pid>.json` and
/// removes it when dropped or explicitly cleaned up.
pub struct AutoDrivePidFile {
    path: PathBuf,
}

impl AutoDrivePidFile {
    /// Write the PID file under the provided code_home, returning a guard that
    /// will delete it on drop. Errors are swallowed so Auto Drive startup never
    /// fails because of telemetry bookkeeping.
    pub fn write(
        code_home: &Path,
        goal: Option<&str>,
        mode: AutoDriveMode,
    ) -> Option<Self> {
        let dir = code_home.join("auto-drive");
        fs::create_dir_all(&dir).ok()?;

        let pid = std::process::id();
        let cwd = env::current_dir().ok();
        let command = env::args().collect::<Vec<_>>().join(" ");

        let metadata = AutoDrivePidMetadata {
            pid,
            started_at: Utc::now().to_rfc3339(),
            mode: mode.as_str(),
            goal: goal.map(truncate_goal),
            cwd,
            command: if command.is_empty() { None } else { Some(command) },
        };

        let path = dir.join(format!("pid-{pid}.json"));
        let contents = serde_json::to_vec_pretty(&metadata).ok()?;
        fs::write(&path, contents).ok()?;

        Some(Self { path })
    }

    /// Eagerly remove the PID file. Safe to call multiple times.
    pub fn cleanup(self) {
        let _ = fs::remove_file(&self.path);
    }
}

impl Drop for AutoDrivePidFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn truncate_goal(goal: &str) -> String {
    let trimmed = goal.trim();
    if trimmed.len() <= 800 {
        return trimmed.to_string();
    }

    trimmed.chars().take(800).collect()
}
