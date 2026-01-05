use chrono::DateTime;
use chrono::Duration;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use uuid::Uuid;
use std::fs::{self, OpenOptions};
use std::io::Write as IoWrite;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};
use tokio::runtime::Builder as TokioRuntimeBuilder;
use tokio::sync::RwLock;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::Duration as TokioDuration;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration as StdDuration, Instant};
use crate::spawn::spawn_tokio_command_with_retry;
use crate::protocol::AgentSourceKind;
use tracing::warn;

#[cfg(target_os = "windows")]
fn default_pathext_or_default() -> Vec<String> {
    std::env::var("PATHEXT")
        .ok()
        .filter(|v| !v.is_empty())
        .map(|v| {
            v.split(';')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_ascii_lowercase())
                .collect()
        })
        // Keep a sane default set even if PATHEXT is missing or empty. Include
        // .ps1 because PowerShell users can invoke scripts without specifying
        // the extension; CreateProcess still resolves fine when we provide the
        // full path with extension.
        .unwrap_or_else(|| vec![
            ".com".into(),
            ".exe".into(),
            ".bat".into(),
            ".cmd".into(),
            ".ps1".into(),
        ])
}

#[cfg(target_os = "windows")]
fn resolve_in_path(command: &str) -> Option<std::path::PathBuf> {
    use std::path::Path;

    let cmd_path = Path::new(command);

    // Absolute or contains separators: respect it directly if it points to a file.
    if cmd_path.is_absolute() || command.contains(['\\', '/']) {
        if cmd_path.is_file() {
            return Some(cmd_path.to_path_buf());
        }
    }

    // Search PATH with PATHEXT semantics and return the first hit.
    let exts = default_pathext_or_default();
    let Some(path_os) = std::env::var_os("PATH") else { return None; };
    let has_ext = cmd_path.extension().is_some();
    for dir in std::env::split_paths(&path_os) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        if has_ext {
            let candidate = dir.join(command);
            if candidate.is_file() {
                return Some(candidate);
            }
        } else {
            for ext in &exts {
                let candidate = dir.join(format!("{command}{ext}"));
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }

    None
}

use crate::agent_defaults::{agent_model_spec, default_params_for};
use shlex::split as shlex_split;
use crate::config_types::AgentConfig;
use crate::openai_tools::JsonSchema;
use crate::openai_tools::OpenAiTool;
use crate::openai_tools::ResponsesApiTool;
use crate::protocol::AgentInfo;

fn current_code_binary_path() -> Result<std::path::PathBuf, String> {
    if let Ok(path) = std::env::var("CODE_BINARY_PATH") {
        let p = std::path::PathBuf::from(path);
        if !p.exists() {
            return Err(format!(
                "CODE_BINARY_PATH points to '{}' but that file is missing. Rebuild with ./build-fast.sh or update CODE_BINARY_PATH.",
                p.display()
            ));
        }
        return Ok(p);
    }
    let exe = std::env::current_exe().map_err(|e| format!("Failed to resolve current executable: {}", e))?;

    // If the kernel reports the path as "(deleted)", strip the suffix and prefer the live file
    // at the same location (common when a rebuild replaces the inode under a long-running process).
    let cleaned = strip_deleted_suffix(&exe);
    if cleaned.exists() {
        return Ok(cleaned);
    }

    if let Some(fallback) = fallback_code_binary_path() {
        return Ok(fallback);
    }

    Err(format!(
        "Current code binary is missing on disk ({}). It may have been deleted while running. Rebuild with ./build-fast.sh or reinstall 'code' to continue.",
        exe.display()
    ))
}

fn strip_deleted_suffix(path: &std::path::Path) -> std::path::PathBuf {
    const DELETED_SUFFIX: &str = " (deleted)";
    let s = path.to_string_lossy();
    if let Some(stripped) = s.strip_suffix(DELETED_SUFFIX) {
        return std::path::PathBuf::from(stripped);
    }
    path.to_path_buf()
}

fn fallback_code_binary_path() -> Option<std::path::PathBuf> {
    // If the running binary was pruned (e.g., shared target cache rotation), try to locate
    // a fresh dev build in the repository, and if missing, trigger a quick rebuild.
    let repo_root = find_repo_root(std::env::current_dir().ok()?)?;
    let workspace = repo_root.join("code-rs");

    // Probe likely build outputs in priority order.
    let mut candidates = vec![
        workspace.join("target/dev-fast/code"),
        workspace.join("target/debug/code"),
        workspace.join("target/release-prod/code"),
        workspace.join("target/release/code"),
        workspace.join("bin/code"),
    ];

    if let Some(found) = candidates.iter().find(|p| p.exists()).cloned() {
        return Some(found);
    }

    // Best-effort rebuild; swallow errors so caller can surface the original message.
    let status = std::process::Command::new("bash")
        .current_dir(&repo_root)
        .args(["-lc", "./build-fast.sh >/dev/null 2>&1"])
        .status()
        .ok();

    if status.map(|s| s.success()).unwrap_or(false) {
        candidates.retain(|p| p.exists());
        if let Some(found) = candidates.first().cloned() {
            return Some(found);
        }
    }

    None
}

fn find_repo_root(start: std::path::PathBuf) -> Option<std::path::PathBuf> {
    let mut dir = Some(start.as_path());
    while let Some(path) = dir {
        if path.join(".git").exists() {
            return Some(path.to_path_buf());
        }
        dir = path.parent();
    }
    None
}

/// Format a helpful error message when an agent command is not found.
/// Provides platform-specific guidance for resolving PATH issues.
fn format_agent_not_found_error(agent_name: &str, command: &str) -> String {
    let mut msg = format!("Agent '{}' could not be found.", agent_name);

    #[cfg(target_os = "windows")]
    {
        msg.push_str(&format!(
            "\n\nTroubleshooting steps:\n\
            1. Check if '{}' is installed and available in your PATH\n\
            2. Try using an absolute path in your config.toml:\n\
               [[agents]]\n\
               name = \"{}\"\n\
               command = \"C:\\\\Users\\\\YourUser\\\\AppData\\\\Roaming\\\\npm\\\\{}.cmd\"\n\
            3. Verify your PATH includes the directory containing '{}'\n\
            4. On Windows, ensure the file has a valid extension (.exe, .cmd, .bat, .com)\n\n\
            For more information, see: https://github.com/just-every/code/blob/main/code-rs/config.md",
            command, agent_name, command, command
        ));
    }

    #[cfg(not(target_os = "windows"))]
    {
        msg.push_str(&format!(
            "\n\nTroubleshooting steps:\n\
            1. Check if '{}' is installed: which {}\n\
            2. Verify '{}' is in your PATH: echo $PATH\n\
            3. Try using an absolute path in your config.toml:\n\
               [[agents]]\n\
               name = \"{}\"\n\
               command = \"/absolute/path/to/{}\"\n\n\
            For more information, see: https://github.com/just-every/code/blob/main/code-rs/config.md",
            command, command, command, agent_name, command
        ));
    }

    msg
}

// Agent status enum
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

// Agent information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: String,
    pub batch_id: Option<String>,
    pub model: String,
    #[serde(default)]
    pub name: Option<String>,
    pub prompt: String,
    pub context: Option<String>,
    pub output_goal: Option<String>,
    pub files: Vec<String>,
    pub read_only: bool,
    pub status: AgentStatus,
    pub result: Option<String>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub progress: Vec<String>,
    pub worktree_path: Option<String>,
    pub branch_name: Option<String>,
    #[serde(default)]
    pub worktree_base: Option<String>,
    #[serde(default)]
    pub source_kind: Option<AgentSourceKind>,
    #[serde(skip)]
    pub log_tag: Option<String>,
    #[serde(skip)]
    #[allow(dead_code)]
    pub config: Option<AgentConfig>,
    pub reasoning_effort: code_protocol::config_types::ReasoningEffort,
    #[serde(skip)]
    pub last_activity: DateTime<Utc>,
}

// Global agent manager
lazy_static::lazy_static! {
    pub static ref AGENT_MANAGER: Arc<RwLock<AgentManager>> = Arc::new(RwLock::new(AgentManager::new()));
}

pub struct AgentManager {
    agents: HashMap<String, Agent>,
    handles: HashMap<String, JoinHandle<()>>,
    event_sender: Option<mpsc::UnboundedSender<AgentStatusUpdatePayload>>,
    debug_log_root: Option<PathBuf>,
    watchdog_handle: Option<JoinHandle<()>>,
    inactivity_timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct AgentStatusUpdatePayload {
    pub agents: Vec<AgentInfo>,
    pub context: Option<String>,
    pub task: Option<String>,
}

impl AgentManager {
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
            handles: HashMap::new(),
            event_sender: None,
            debug_log_root: None,
            watchdog_handle: None,
            inactivity_timeout: Duration::minutes(30),
        }
    }

    pub fn set_event_sender(&mut self, sender: mpsc::UnboundedSender<AgentStatusUpdatePayload>) {
        self.event_sender = Some(sender);
        self.start_watchdog();
    }

    fn start_watchdog(&mut self) {
        if self.watchdog_handle.is_some() {
            return;
        }

        let timeout = self.inactivity_timeout;
        let manager = Arc::downgrade(&AGENT_MANAGER);
        self.watchdog_handle = Some(tokio::spawn(async move {
            let mut ticker = tokio::time::interval(TokioDuration::from_secs(60));
            loop {
                ticker.tick().await;

                let Some(manager_arc) = manager.upgrade() else { break; };

                let mut mgr = manager_arc.write().await;
                let now = Utc::now();
                let timeout_ids: Vec<String> = mgr
                    .agents
                    .iter()
                    .filter(|(_, agent)| matches!(agent.status, AgentStatus::Pending | AgentStatus::Running))
                    .filter(|(_, agent)| now - agent.last_activity > timeout)
                    .map(|(id, _)| id.clone())
                    .collect();

                if timeout_ids.is_empty() {
                    continue;
                }

                for agent_id in timeout_ids.iter() {
                    if let Some(handle) = mgr.handles.remove(agent_id) {
                        handle.abort();
                    }
                    if let Some(agent) = mgr.agents.get_mut(agent_id) {
                        agent.status = AgentStatus::Failed;
                        agent.error = Some(format!(
                            "Agent timed out after {} minutes of inactivity.",
                            timeout.num_minutes()
                        ));
                        agent.completed_at = Some(now);
                        Self::record_activity(agent);
                    }
                }

                // Notify listeners once per sweep.
                mgr.send_agent_status_update().await;
            }
        }));
    }

    pub fn set_debug_log_root(&mut self, root: Option<PathBuf>) {
        self.debug_log_root = root;
    }

    async fn touch_agent(agent_id: &str) {
        if let Some(manager) = Arc::downgrade(&AGENT_MANAGER).upgrade() {
            let mut mgr = manager.write().await;
            if let Some(agent) = mgr.agents.get_mut(agent_id) {
                Self::record_activity(agent);
            }
        }
    }

    fn record_activity(agent: &mut Agent) {
        agent.last_activity = Utc::now();
    }

    fn append_agent_log(&self, log_tag: &str, line: &str) {
        let Some(root) = &self.debug_log_root else { return; };
        let dir = root.join(log_tag);
        if let Err(err) = fs::create_dir_all(&dir) {
            warn!("failed to create agent log dir {:?}: {}", dir, err);
            return;
        }

        let file = dir.join("progress.log");
        match OpenOptions::new().create(true).append(true).open(&file) {
            Ok(mut fh) => {
                if let Err(err) = writeln!(fh, "{}", line) {
                    warn!("failed to write agent log {:?}: {}", file, err);
                }
            }
            Err(err) => warn!("failed to open agent log {:?}: {}", file, err),
        }
    }

    async fn send_agent_status_update(&self) {
        if let Some(ref sender) = self.event_sender {
            let now = Utc::now();
            let agents: Vec<AgentInfo> = self
                .agents
                .values()
                .map(|agent| {
                    // Just show the model name - status provides the useful info
                    let name = agent
                        .name
                        .as_ref()
                        .map(|value| value.clone())
                        .unwrap_or_else(|| agent.model.clone());
                    let start = agent.started_at.unwrap_or(agent.created_at);
                    let end = agent.completed_at.unwrap_or(now);
                    let elapsed_ms = match end.signed_duration_since(start).num_milliseconds() {
                        value if value >= 0 => Some(value as u64),
                        _ => None,
                    };

                    AgentInfo {
                        id: agent.id.clone(),
                        name,
                        status: format!("{:?}", agent.status).to_lowercase(),
                        batch_id: agent.batch_id.clone(),
                        model: Some(agent.model.clone()),
                        last_progress: agent.progress.last().cloned(),
                        result: agent.result.clone(),
                        error: agent.error.clone(),
                        elapsed_ms,
                        token_count: None,
                        last_activity_at: match agent.status {
                            AgentStatus::Pending | AgentStatus::Running => {
                                Some(agent.last_activity.to_rfc3339())
                            }
                            _ => None,
                        },
                        seconds_since_last_activity: match agent.status {
                            AgentStatus::Pending | AgentStatus::Running => Some(
                                Utc::now()
                                    .signed_duration_since(agent.last_activity)
                                    .num_seconds()
                                    .max(0) as u64,
                            ),
                            _ => None,
                        },
                        source_kind: agent.source_kind.clone(),
                    }
                })
                .collect();

            // Get context and task from the first agent (they're all the same)
            let (context, task) = self
                .agents
                .values()
                .next()
                .map(|agent| {
                    let context = agent
                        .context
                        .as_ref()
                        .and_then(|value| if value.trim().is_empty() {
                            None
                        } else {
                            Some(value.clone())
                        });
                    let task = if agent.prompt.trim().is_empty() {
                        None
                    } else {
                        Some(agent.prompt.clone())
                    };
                    (context, task)
                })
                .unwrap_or((None, None));
            let payload = AgentStatusUpdatePayload { agents, context, task };
            let _ = sender.send(payload);
        }
    }

    pub async fn create_agent(
        &mut self,
        model: String,
        name: Option<String>,
        prompt: String,
        context: Option<String>,
        output_goal: Option<String>,
        files: Vec<String>,
        read_only: bool,
        batch_id: Option<String>,
        reasoning_effort: code_protocol::config_types::ReasoningEffort,
    ) -> String {
        self.create_agent_internal(
            model,
            name,
            prompt,
            context,
            output_goal,
            files,
            read_only,
            batch_id,
            None,
            None,
            None,
            None,
            reasoning_effort,
        )
        .await
    }

    pub async fn create_agent_with_config(
        &mut self,
        model: String,
        name: Option<String>,
        prompt: String,
        context: Option<String>,
        output_goal: Option<String>,
        files: Vec<String>,
        read_only: bool,
        batch_id: Option<String>,
        config: AgentConfig,
        reasoning_effort: code_protocol::config_types::ReasoningEffort,
    ) -> String {
        self.create_agent_internal(
            model,
            name,
            prompt,
            context,
            output_goal,
            files,
            read_only,
            batch_id,
            Some(config),
            None,
            None,
            None,
            reasoning_effort,
        )
        .await
    }

    #[allow(dead_code)]
    pub async fn create_agent_with_options(
        &mut self,
        model: String,
        name: Option<String>,
        prompt: String,
        context: Option<String>,
        output_goal: Option<String>,
        files: Vec<String>,
        read_only: bool,
        batch_id: Option<String>,
        config: Option<AgentConfig>,
        worktree_branch: Option<String>,
        worktree_base: Option<String>,
        source_kind: Option<AgentSourceKind>,
        reasoning_effort: code_protocol::config_types::ReasoningEffort,
    ) -> String {
        self
            .create_agent_internal(
                model,
                name,
                prompt,
                context,
                output_goal,
                files,
                read_only,
                batch_id,
                config,
                worktree_branch,
                worktree_base,
                source_kind,
                reasoning_effort,
            )
            .await
    }

    async fn create_agent_internal(
        &mut self,
        model: String,
        name: Option<String>,
        prompt: String,
        context: Option<String>,
        output_goal: Option<String>,
        files: Vec<String>,
        read_only: bool,
        batch_id: Option<String>,
        config: Option<AgentConfig>,
        worktree_branch: Option<String>,
        worktree_base: Option<String>,
        source_kind: Option<AgentSourceKind>,
        reasoning_effort: code_protocol::config_types::ReasoningEffort,
    ) -> String {
        let agent_id = Uuid::new_v4().to_string();

        let log_tag = match source_kind {
            Some(AgentSourceKind::AutoReview) => {
                Some(format!("agents/auto-review/{}", agent_id))
            }
            _ => None,
        };

        let agent = Agent {
            id: agent_id.clone(),
            batch_id,
            model,
            name: normalize_agent_name(name),
            prompt,
            context,
            output_goal,
            files,
            read_only,
            status: AgentStatus::Pending,
            result: None,
            error: None,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            progress: Vec::new(),
            worktree_path: None,
            branch_name: worktree_branch,
            worktree_base,
            source_kind,
            log_tag,
            config: config.clone(),
            reasoning_effort,
            last_activity: Utc::now(),
        };

        self.agents.insert(agent_id.clone(), agent.clone());

        // Send initial status update
        self.send_agent_status_update().await;

        // Spawn async agent
        let agent_id_clone = agent_id.clone();
        let handle = tokio::spawn(async move {
            execute_agent(agent_id_clone, config).await;
        });

        self.handles.insert(agent_id.clone(), handle);

        agent_id
    }

    pub fn get_agent(&self, agent_id: &str) -> Option<Agent> {
        self.agents.get(agent_id).cloned()
    }

    pub fn get_all_agents(&self) -> impl Iterator<Item = &Agent> {
        self.agents.values()
    }

    pub fn list_agents(
        &self,
        status_filter: Option<AgentStatus>,
        batch_id: Option<String>,
        recent_only: bool,
    ) -> Vec<Agent> {
        let cutoff = if recent_only {
            Some(Utc::now() - Duration::hours(2))
        } else {
            None
        };

        self.agents
            .values()
            .filter(|agent| {
                if let Some(ref filter) = status_filter {
                    if agent.status != *filter {
                        return false;
                    }
                }
                if let Some(ref batch) = batch_id {
                    if agent.batch_id.as_ref() != Some(batch) {
                        return false;
                    }
                }
                if let Some(cutoff) = cutoff {
                    if agent.created_at < cutoff {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect()
    }

    pub fn has_active_agents(&self) -> bool {
        self.agents
            .values()
            .any(|agent| matches!(agent.status, AgentStatus::Pending | AgentStatus::Running))
    }

    pub async fn cancel_agent(&mut self, agent_id: &str) -> bool {
        if let Some(handle) = self.handles.remove(agent_id) {
            handle.abort();
            if let Some(agent) = self.agents.get_mut(agent_id) {
                agent.status = AgentStatus::Cancelled;
                agent.completed_at = Some(Utc::now());
            }
            true
        } else {
            false
        }
    }

    pub async fn cancel_batch(&mut self, batch_id: &str) -> usize {
        let agent_ids: Vec<String> = self
            .agents
            .values()
            .filter(|agent| agent.batch_id.as_ref() == Some(&batch_id.to_string()))
            .map(|agent| agent.id.clone())
            .collect();

        let mut count = 0;
        for agent_id in agent_ids {
            if self.cancel_agent(&agent_id).await {
                count += 1;
            }
        }
        count
    }

    pub async fn update_agent_status(&mut self, agent_id: &str, status: AgentStatus) {
        if let Some(agent) = self.agents.get_mut(agent_id) {
            agent.status = status;
            if agent.status == AgentStatus::Running && agent.started_at.is_none() {
                agent.started_at = Some(Utc::now());
            }
            if matches!(
                agent.status,
                AgentStatus::Completed | AgentStatus::Failed | AgentStatus::Cancelled
            ) {
                agent.completed_at = Some(Utc::now());
            }
            Self::record_activity(agent);
            // Send status update event
            self.send_agent_status_update().await;
        }
    }

    pub async fn update_agent_result(&mut self, agent_id: &str, result: Result<String, String>) {
        let debug_enabled = self.debug_log_root.is_some();

        if let Some((log_tag, log_lines)) = self.agents.get_mut(agent_id).map(|agent| {
            let log_tag = if debug_enabled { agent.log_tag.clone() } else { None };

            let mut log_lines: Vec<String> = Vec::new();
            if debug_enabled {
                let stamp = Utc::now().format("%H:%M:%S");
                match &result {
                    Ok(output) => {
                        log_lines.push(format!("{stamp}: [result] completed"));
                        if !output.trim().is_empty() {
                            log_lines.push(output.trim_end().to_string());
                        }
                    }
                    Err(error) => {
                        log_lines.push(format!("{stamp}: [result] failed"));
                        log_lines.push(error.clone());
                    }
                }
            }

            match result {
                Ok(output) => {
                    agent.result = Some(output);
                    agent.status = AgentStatus::Completed;
                }
                Err(error) => {
                    agent.error = Some(error);
                    agent.status = AgentStatus::Failed;
                }
            }
            agent.completed_at = Some(Utc::now());
            Self::record_activity(agent);

            (log_tag, log_lines)
        }) {
            if let Some(tag) = log_tag {
                for line in log_lines {
                    self.append_agent_log(&tag, &line);
                }
            }
            // Send status update event
            self.send_agent_status_update().await;
        }
    }

    pub async fn add_progress(&mut self, agent_id: &str, message: String) {
        let debug_enabled = self.debug_log_root.is_some();

        if let Some((log_tag, entry)) = self.agents.get_mut(agent_id).map(|agent| {
            let entry = format!("{}: {}", Utc::now().format("%H:%M:%S"), message);
            let log_tag = if debug_enabled { agent.log_tag.clone() } else { None };
            agent.progress.push(entry.clone());
            Self::record_activity(agent);
            (log_tag, entry)
        }) {
            if let Some(tag) = log_tag {
                self.append_agent_log(&tag, &entry);
            }
            // Send updated agent status with the latest progress
            self.send_agent_status_update().await;
        }
    }

    pub async fn update_worktree_info(
        &mut self,
        agent_id: &str,
        worktree_path: String,
        branch_name: String,
    ) {
        if let Some(agent) = self.agents.get_mut(agent_id) {
            agent.worktree_path = Some(worktree_path);
            agent.branch_name = Some(branch_name);
        }
    }
}

async fn get_git_root() -> Result<PathBuf, String> {
    let output = Command::new("git")
        .args(&["rev-parse", "--show-toplevel"])
        .output()
        .await
        .map_err(|e| format!("Git not installed or not in a git repository: {}", e))?;

    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(PathBuf::from(path))
    } else {
        Err("Not in a git repository".to_string())
    }
}

use crate::git_worktree::sanitize_ref_component;

fn generate_branch_id(model: &str, agent: &str) -> String {
    // Extract first few meaningful words from agent for the branch name
    let stop = ["the", "and", "for", "with", "from", "into", "goal"]; // skip boilerplate
    let words: Vec<&str> = agent
        .split_whitespace()
        .filter(|w| w.len() > 2 && !stop.contains(&w.to_ascii_lowercase().as_str()))
        .take(3)
        .collect();

    let raw_suffix = if words.is_empty() {
        Uuid::new_v4()
            .to_string()
            .split('-')
            .next()
            .unwrap_or("agent")
            .to_string()
    } else {
        words.join("-")
    };

    // Sanitize both model and suffix for safety
    let model_s = sanitize_ref_component(model);
    let mut suffix_s = sanitize_ref_component(&raw_suffix);

    // Constrain length to keep branch names readable
    if suffix_s.len() > 40 {
        suffix_s.truncate(40);
        suffix_s = suffix_s.trim_matches('-').to_string();
        if suffix_s.is_empty() {
            suffix_s = "agent".to_string();
        }
    }

    format!("code-{}-{}", model_s, suffix_s)
}

use crate::git_worktree::setup_worktree;

async fn execute_agent(agent_id: String, config: Option<AgentConfig>) {
    let mut manager = AGENT_MANAGER.write().await;

    // Get agent details
    let agent = match manager.get_agent(&agent_id) {
        Some(t) => t,
        None => return,
    };

    // Update status to running
    manager
        .update_agent_status(&agent_id, AgentStatus::Running)
        .await;
    manager
        .add_progress(
            &agent_id,
            format!("Starting agent with model: {}", agent.model),
        )
        .await;

    let model = agent.model.clone();
    let model_spec = agent_model_spec(&model);
    let prompt = agent.prompt.clone();
    let read_only = agent.read_only;
    let context = agent.context.clone();
    let output_goal = agent.output_goal.clone();
    let files = agent.files.clone();
    let reasoning_effort = agent.reasoning_effort;
    let source_kind = agent.source_kind.clone();
    let log_tag = agent.log_tag.clone();

    drop(manager); // Release the lock before executing

    // Build the full prompt with context
    let mut full_prompt = prompt.clone();
    // Prepend any per-agent instructions from config when available
    if let Some(cfg) = config.as_ref() {
        if let Some(instr) = cfg.instructions.as_ref() {
            if !instr.trim().is_empty() {
                full_prompt = format!("{}\n\n{}", instr.trim(), full_prompt);
            }
        }
    }
    if let Some(context) = &context {
        let trimmed = full_prompt.trim_start();
        if trimmed.starts_with('/') {
            // Preserve leading slash commands so downstream executors can parse them.
            full_prompt = format!("{full_prompt}\n\nContext: {context}");
        } else {
            full_prompt = format!("Context: {context}\n\nAgent: {full_prompt}");
        }
    }
    if let Some(output_goal) = &output_goal {
        full_prompt = format!("{}\n\nDesired output: {}", full_prompt, output_goal);
    }
    if !files.is_empty() {
        full_prompt = format!("{}\n\nFiles to consider: {}", full_prompt, files.join(", "));
    }

    // Setup working directory and execute
    let gating_error_message = |spec: &crate::agent_defaults::AgentModelSpec| {
        if let Some(flag) = spec.gating_env {
            format!(
                "agent model '{}' is disabled; set {}=1 to enable it",
                spec.slug, flag
            )
        } else {
            format!("agent model '{}' is disabled", spec.slug)
        }
    };

    // Track optional review output path for /review agents (AutoReview)
    let mut review_output_json_path_capture: Option<PathBuf> = None;

    let result = if !read_only {
        // Check git and setup worktree for non-read-only mode
        match get_git_root().await {
            Ok(git_root) => {
                let branch_id = agent
                    .branch_name
                    .clone()
                    .unwrap_or_else(|| generate_branch_id(&model, &prompt));

                let mut manager = AGENT_MANAGER.write().await;
                manager
                    .add_progress(&agent_id, format!("Creating git worktree: {}", branch_id))
                    .await;
                drop(manager);

                match setup_worktree(&git_root, &branch_id, agent.worktree_base.as_deref()).await {
                    Ok((worktree_path, used_branch)) => {
                        let mut manager = AGENT_MANAGER.write().await;
                        manager
                            .add_progress(
                                &agent_id,
                                format!("Executing in worktree: {}", worktree_path.display()),
                            )
                            .await;
                        manager
                            .update_worktree_info(
                                &agent_id,
                                worktree_path.display().to_string(),
                                used_branch.clone(),
                            )
                            .await;
                        drop(manager);

                        // Prepare optional review-output JSON path for /review agents
                        let review_output_json_path: Option<PathBuf> = agent
                            .source_kind
                            .as_ref()
                            .and_then(|kind| matches!(kind, AgentSourceKind::AutoReview).then(|| {
                                let filename = format!("{}.review-output.json", agent_id);
                                std::env::temp_dir().join(filename)
                            }));
                        review_output_json_path_capture = review_output_json_path.clone();

                        // Execute with full permissions in the worktree
                        let use_built_in_cloud = config.is_none()
                            && model_spec
                                .map(|spec| spec.cli.eq_ignore_ascii_case("cloud"))
                                .unwrap_or_else(|| model.eq_ignore_ascii_case("cloud"));

                        if use_built_in_cloud {
                            if let Some(spec) = model_spec {
                                if !spec.is_enabled() {
                                    Err(gating_error_message(spec))
                                } else {
                                    execute_cloud_built_in_streaming(
                                        &agent_id,
                                        &full_prompt,
                                        Some(worktree_path),
                                        config.clone(),
                                        spec.slug,
                                    )
                                    .await
                                }
                            } else {
                                execute_cloud_built_in_streaming(
                                    &agent_id,
                                    &full_prompt,
                                    Some(worktree_path),
                                    config.clone(),
                                    model.as_str(),
                                )
                                .await
                            }
                        } else {
                            execute_model_with_permissions(
                                &agent_id,
                                &model,
                                &full_prompt,
                                false,
                                Some(worktree_path),
                                config.clone(),
                                reasoning_effort,
                                review_output_json_path.as_ref(),
                                source_kind.clone(),
                                log_tag.as_deref(),
                            )
                            .await
                        }
                    }
                    Err(e) => Err(format!("Failed to setup worktree: {}", e)),
                }
            }
            Err(e) => Err(format!("Git is required for non-read-only agents: {}", e)),
        }
    } else {
        // Execute in read-only mode
        full_prompt = format!(
            "{}\n\n[Running in read-only mode - no modifications allowed]",
            full_prompt
        );
        let use_built_in_cloud = config.is_none()
            && model_spec
                .map(|spec| spec.cli.eq_ignore_ascii_case("cloud"))
                .unwrap_or_else(|| model.eq_ignore_ascii_case("cloud"));

        if use_built_in_cloud {
            if let Some(spec) = model_spec {
                if !spec.is_enabled() {
                    Err(gating_error_message(spec))
                } else {
                    execute_cloud_built_in_streaming(&agent_id, &full_prompt, None, config, spec.slug).await
                }
            } else {
                execute_cloud_built_in_streaming(&agent_id, &full_prompt, None, config, model.as_str()).await
            }
        } else {
            execute_model_with_permissions(
                &agent_id,
                &model,
                &full_prompt,
                true,
                None,
                config,
                reasoning_effort,
                None,
                source_kind,
                log_tag.as_deref(),
            )
            .await
        }
    };

    // Update result; if a review-output JSON was produced, prefer its contents.
    let final_result = prefer_json_result(review_output_json_path_capture.as_ref(), result);
    let mut manager = AGENT_MANAGER.write().await;
    manager.update_agent_result(&agent_id, final_result).await;
}

fn prefer_json_result(path: Option<&PathBuf>, fallback: Result<String, String>) -> Result<String, String> {
    if let Some(p) = path {
        if let Ok(json) = std::fs::read_to_string(p) {
            return Ok(json);
        }
    }
    fallback
}

async fn execute_model_with_permissions(
    agent_id: &str,
    model: &str,
    prompt: &str,
    read_only: bool,
    working_dir: Option<PathBuf>,
    config: Option<AgentConfig>,
    reasoning_effort: code_protocol::config_types::ReasoningEffort,
    review_output_json_path: Option<&PathBuf>,
    source_kind: Option<AgentSourceKind>,
    log_tag: Option<&str>,
) -> Result<String, String> {
    // Helper: cross‑platform check whether an executable is available in PATH
    // and is directly spawnable by std::process::Command (no shell wrappers).
fn command_exists(cmd: &str) -> bool {
        // Absolute/relative path with separators: check directly (files only).
        if cmd.contains(std::path::MAIN_SEPARATOR) || cmd.contains('/') || cmd.contains('\\') {
            let path = std::path::Path::new(cmd);
            if path.extension().is_some() {
                return std::fs::metadata(path).map(|m| m.is_file()).unwrap_or(false);
            }

            #[cfg(target_os = "windows")]
            {
                const DEFAULT_EXTS: &[&str] = &[".exe", ".com", ".cmd", ".bat"];
                for ext in default_pathext_or_default() {
                    let candidate = path.with_extension("");
                    let candidate = candidate.with_extension(ext.trim_start_matches('.'));
                    if std::fs::metadata(&candidate)
                        .map(|m| m.is_file())
                        .unwrap_or(false)
                    {
                        return true;
                    }
                }
            }

            return std::fs::metadata(path).map(|m| m.is_file()).unwrap_or(false);
        }

        #[cfg(target_os = "windows")]
        {
            let exts = default_pathext_or_default();
            let path_var = std::env::var_os("PATH");
            let path_iter = path_var
                .as_ref()
                .map(std::env::split_paths)
                .into_iter()
                .flatten();

            let candidates: Vec<String> = if std::path::Path::new(cmd).extension().is_some() {
                vec![cmd.to_string()]
            } else {
                exts
                    .iter()
                    .map(|ext| format!("{cmd}{ext}"))
                    .collect()
            };

            for dir in path_iter {
                for candidate in &candidates {
                    let p = dir.join(candidate);
                    if p.is_file() {
                        return true;
                    }
                }
            }

            false
        }

        #[cfg(not(target_os = "windows"))]
        {
            use std::os::unix::fs::PermissionsExt;
            let Some(path_os) = std::env::var_os("PATH") else { return false; };
            for dir in std::env::split_paths(&path_os) {
                if dir.as_os_str().is_empty() { continue; }
                let candidate = dir.join(cmd);
                if let Ok(meta) = std::fs::metadata(&candidate) {
                    if meta.is_file() {
                        let mode = meta.permissions().mode();
                        if mode & 0o111 != 0 { return true; }
                    }
                }
            }
            false
        }
    }

    let spec_opt = agent_model_spec(model)
        .or_else(|| config.as_ref().and_then(|cfg| agent_model_spec(&cfg.name)))
        .or_else(|| config.as_ref().and_then(|cfg| agent_model_spec(&cfg.command)));

    if let Some(spec) = spec_opt {
        if !spec.is_enabled() {
            if let Some(flag) = spec.gating_env {
                return Err(format!(
                    "agent model '{}' is disabled; set {}=1 to enable it",
                    spec.slug, flag
                ));
            }
            return Err(format!("agent model '{}' is disabled", spec.slug));
        }
    }

    // Use config command if provided, otherwise fall back to the spec CLI (or the
    // lowercase model string).
    let command = if let Some(ref cfg) = config {
        let cmd = cfg.command.trim();
        if !cmd.is_empty() {
            cfg.command.clone()
        } else if let Some(spec) = spec_opt {
            spec.cli.to_string()
        } else {
            cfg.name.clone()
        }
    } else if let Some(spec) = spec_opt {
        spec.cli.to_string()
    } else {
        model.to_lowercase()
    };

    let (command_base, command_extra_args) = split_command_and_args(&command);
    let command_for_spawn = if command_base.is_empty() {
        command.clone()
    } else {
        command_base.clone()
    };

    // Special case: for the built‑in Codex agent, prefer invoking the currently
    // running executable with the `exec` subcommand rather than relying on a
    // `codex` binary to be present on PATH. This improves portability,
    // especially on Windows where global shims may be missing.
    let model_lower = model.to_lowercase();
    let command_lower = command_for_spawn.to_ascii_lowercase();
    fn is_known_family(s: &str) -> bool {
        matches!(s, "claude" | "gemini" | "qwen" | "codex" | "code" | "cloud" | "coder")
    }

    let slug_for_defaults = spec_opt.map(|spec| spec.slug).unwrap_or(model);
    let spec_family = spec_opt.map(|spec| spec.family);
    let family = if let Some(spec_family) = spec_family {
        spec_family
    } else if is_known_family(model_lower.as_str()) {
        model_lower.as_str()
    } else if is_known_family(command_lower.as_str()) {
        command_lower.as_str()
    } else {
        model_lower.as_str()
    };

    let command_missing = !command_exists(&command_for_spawn);
    let use_current_exe = should_use_current_exe_for_agent(family, command_missing, config.as_ref());

    let mut final_args: Vec<String> = command_extra_args;

    if let Some(ref cfg) = config {
        if read_only {
            if let Some(ro) = cfg.args_read_only.as_ref() {
                final_args.extend(ro.iter().cloned());
            } else {
                final_args.extend(cfg.args.iter().cloned());
            }
        } else if let Some(w) = cfg.args_write.as_ref() {
            final_args.extend(w.iter().cloned());
        } else {
            final_args.extend(cfg.args.iter().cloned());
        }
    }

    strip_model_flags(&mut final_args);

    let spec_model_args: Vec<String> = if let Some(spec) = spec_opt {
        spec.model_args.iter().map(|arg| (*arg).to_string()).collect()
    } else {
        Vec::new()
    };

    let built_in_cloud = family == "cloud" && config.is_none();

    // Clamp reasoning effort to what the target model supports.
    let clamped_effort = match reasoning_effort {
        code_protocol::config_types::ReasoningEffort::XHigh => {
            let lower = slug_for_defaults.to_ascii_lowercase();
            if lower.contains("max") {
                reasoning_effort
            } else {
                code_protocol::config_types::ReasoningEffort::High
            }
        }
        other => other,
    };

    // Configuration overrides for Codex CLI families. External CLIs (claude,
    // gemini, qwen) do not understand our config flags, so only attach these
    // when launching Codex binaries.
    let effort_override = format!(
        "model_reasoning_effort={}",
        clamped_effort.to_string().to_ascii_lowercase()
    );
    let auto_effort_override = format!(
        "auto_drive.model_reasoning_effort={}",
        clamped_effort.to_string().to_ascii_lowercase()
    );
    match family {
        "claude" | "gemini" | "qwen" => {
            let mut defaults = default_params_for(slug_for_defaults, read_only);
            strip_model_flags(&mut defaults);
            final_args.extend(defaults);
            final_args.extend(spec_model_args.iter().cloned());
            final_args.push("-p".into());
            final_args.push(prompt.to_string());
        }
        "codex" | "code" => {
            let have_mode_args = config
                .as_ref()
                .map(|c| if read_only { c.args_read_only.is_some() } else { c.args_write.is_some() })
                .unwrap_or(false);
            if !have_mode_args {
                let mut defaults = default_params_for(slug_for_defaults, read_only);
                strip_model_flags(&mut defaults);
                final_args.extend(defaults);
            }
            final_args.extend(spec_model_args.iter().cloned());
            final_args.push("-c".into());
            final_args.push(effort_override.clone());
            final_args.push("-c".into());
            final_args.push(auto_effort_override.clone());
            final_args.push(prompt.to_string());
        }
        "cloud" => {
            if built_in_cloud {
                final_args.extend(["cloud", "submit", "--wait"].map(String::from));
            }
            let have_mode_args = config
                .as_ref()
                .map(|c| if read_only { c.args_read_only.is_some() } else { c.args_write.is_some() })
                .unwrap_or(false);
            if !have_mode_args {
                let mut defaults = default_params_for(slug_for_defaults, read_only);
                strip_model_flags(&mut defaults);
                final_args.extend(defaults);
            }
            final_args.extend(spec_model_args.iter().cloned());
            final_args.push("-c".into());
            final_args.push(effort_override.clone());
            final_args.push("-c".into());
            final_args.push(auto_effort_override);
            final_args.push(prompt.to_string());
        }
        _ => {
            final_args.extend(spec_model_args.iter().cloned());
            final_args.push(prompt.to_string());
        }
    }

    let log_tag_owned = log_tag.map(str::to_string);
    let debug_subagent = debug_subagents_enabled()
        && matches!(source_kind, Some(AgentSourceKind::AutoReview));
    let child_log_tag: Option<String> = if debug_subagent {
        Some(log_tag_owned.clone().unwrap_or_else(|| format!("agents/{agent_id}")))
    } else {
        log_tag_owned
    };

    if debug_subagent && use_current_exe && !has_debug_flag(&final_args) {
        final_args.insert(0, "--debug".to_string());
    }

    if let Some(path) = review_output_json_path {
        final_args.push("--review-output-json".to_string());
        final_args.push(path.display().to_string());
    }

    if use_current_exe
        && (final_args.iter().any(|arg| arg == "exec") || review_output_json_path.is_some())
    {
        let mut reordered: Vec<String> = Vec::with_capacity(final_args.len() + 1);
        reordered.push("exec".to_string());
        for arg in final_args.into_iter() {
            if arg != "exec" {
                reordered.push(arg);
            }
        }
        final_args = reordered;
    }

    // Proactively check for presence of external command before spawn when not
    // using the current executable fallback. This avoids confusing OS errors
    // like "program not found" and lets us surface a cleaner message.
    if !(family == "codex" || family == "code" || (family == "cloud" && config.is_none()))
        && !command_exists(&command_for_spawn)
    {
        return Err(format_agent_not_found_error(&command, &command_for_spawn));
    }

    // Agents: run without OS sandboxing; rely on per-branch worktrees for isolation.
    use crate::protocol::SandboxPolicy;
    use crate::spawn::StdioPolicy;
    // Build env from current process then overlay any config-provided vars.
    let mut env: std::collections::HashMap<String, String> = std::env::vars().collect();
    let orig_home: Option<String> = env.get("HOME").cloned();
    if let Some(ref cfg) = config {
        if let Some(ref e) = cfg.env { for (k, v) in e { env.insert(k.clone(), v.clone()); } }
    }

    if debug_subagent {
        env.entry("CODE_SUBAGENT_DEBUG".to_string())
            .or_insert_with(|| "1".to_string());
        if let Some(tag) = child_log_tag.as_ref() {
            env.insert("CODE_DEBUG_LOG_TAG".to_string(), tag.clone());
        }
    }

    // Tag OpenAI requests originating from agent runs so server-side telemetry
    // can distinguish subagent traffic.
    if use_current_exe || family == "codex" || family == "code" {
        let subagent = match source_kind {
            Some(AgentSourceKind::AutoReview) => "review",
            _ => "agent",
        };
        env.entry("CODE_OPENAI_SUBAGENT".to_string())
            .or_insert_with(|| subagent.to_string());
    }

    // Convenience: map common key names so external CLIs "just work".
    if let Some(google_key) = env.get("GOOGLE_API_KEY").cloned() {
        env.entry("GEMINI_API_KEY".to_string()).or_insert(google_key);
    }
    if let Some(claude_key) = env.get("CLAUDE_API_KEY").cloned() {
        env.entry("ANTHROPIC_API_KEY".to_string()).or_insert(claude_key);
    }
    if let Some(anthropic_key) = env.get("ANTHROPIC_API_KEY").cloned() {
        env.entry("CLAUDE_API_KEY".to_string()).or_insert(anthropic_key);
    }
    if let Some(anthropic_base) = env.get("ANTHROPIC_BASE_URL").cloned() {
        env.entry("CLAUDE_BASE_URL".to_string()).or_insert(anthropic_base);
    }
    // Qwen/DashScope convenience: mirror API keys and base URLs both ways so
    // either variable name works across tools.
    if let Some(qwen_key) = env.get("QWEN_API_KEY").cloned() {
        env.entry("DASHSCOPE_API_KEY".to_string()).or_insert(qwen_key);
    }
    if let Some(dashscope_key) = env.get("DASHSCOPE_API_KEY").cloned() {
        env.entry("QWEN_API_KEY".to_string()).or_insert(dashscope_key);
    }
    if let Some(qwen_base) = env.get("QWEN_BASE_URL").cloned() {
        env.entry("DASHSCOPE_BASE_URL".to_string()).or_insert(qwen_base);
    }
    if let Some(ds_base) = env.get("DASHSCOPE_BASE_URL").cloned() {
        env.entry("QWEN_BASE_URL".to_string()).or_insert(ds_base);
    }
    if family == "qwen" {
        env.insert("OPENAI_API_KEY".to_string(), String::new());
    }
    // Reduce startup overhead for Claude CLI: disable auto-updater/telemetry.
    env.entry("DISABLE_AUTOUPDATER".to_string()).or_insert("1".to_string());
    env.entry("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".to_string()).or_insert("1".to_string());
    env.entry("DISABLE_ERROR_REPORTING".to_string()).or_insert("1".to_string());
    // Prefer explicit Claude config dir to avoid touching $HOME/.claude.json.
    // Do not force CLAUDE_CONFIG_DIR here; leave CLI free to use its default
    // (including Keychain) unless we explicitly redirect HOME below.

    // If GEMINI_API_KEY not provided, try pointing to host config for read‑only
    // discovery (Gemini CLI supports GEMINI_CONFIG_DIR). We keep HOME as-is so
    // CLIs that require ~/.gemini and ~/.claude continue to work with your
    // existing config.
    maybe_set_gemini_config_dir(&mut env, orig_home.clone());

    let output = if !read_only {
        // Resolve the command and args we prepared above into Vec<String> for spawn helpers.
        let program = resolve_program_path(use_current_exe, &command_for_spawn)?;
        let args = final_args.clone();

        let child_result: std::io::Result<tokio::process::Child> = crate::spawn::spawn_child_async(
            program.clone(),
            args.clone(),
            Some(program.to_string_lossy().as_ref()),
            working_dir.clone().unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))),
            &SandboxPolicy::DangerFullAccess,
            StdioPolicy::RedirectForShellTool,
            env.clone(),
        )
        .await;

        match child_result {
            Ok(child) => stream_child_output(agent_id, child).await?,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    return Err(format_agent_not_found_error(&command, &command_for_spawn));
                }
                return Err(format!("Failed to spawn sandboxed agent: {}", e));
            }
        }
    } else {
        // Read-only path: must honor resolve_program_path (and CODE_BINARY_PATH) just
        // like the write path; skipping this can regress to PATH resolution and
        // launch the npm shim on Windows (issue #497).
        let program = resolve_program_path(use_current_exe, &command_for_spawn)?;
        let mut cmd = Command::new(program);

        if let Some(dir) = working_dir.clone() {
            cmd.current_dir(dir);
        }

        cmd.args(final_args.clone());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        for (k, v) in &env {
            cmd.env(k, v);
        }

        // Ensure the child is terminated if this process dies unexpectedly.
        cmd.kill_on_drop(true);

        match spawn_tokio_command_with_retry(&mut cmd).await {
            Ok(child) => stream_child_output(agent_id, child).await?,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    return Err(format_agent_not_found_error(&command, &command_for_spawn));
                }

                return Err(format!("Failed to execute {}: {}", model, e));
            }
        }
    };

    let (status, stdout_buf, stderr_buf) = output;

    if status.success() {
        Ok(stdout_buf)
    } else {
        let stderr = stderr_buf.trim();
        let stdout = stdout_buf.trim();
        let combined = if stderr.is_empty() {
            stdout.to_string()
        } else if stdout.is_empty() {
            stderr.to_string()
        } else {
            format!("{}\n{}", stderr, stdout)
        };
        Err(format!("Command failed: {}", combined))
    }
}

const STREAM_PROGRESS_INTERVAL: StdDuration = StdDuration::from_secs(2);
const STREAM_PROGRESS_BYTES: usize = 2 * 1024;

async fn stream_child_output(
    agent_id: &str,
    mut child: tokio::process::Child,
) -> Result<(std::process::ExitStatus, String, String), String> {
    let agent_id_owned = agent_id.to_string();
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_clone = stop_flag.clone();
    let heartbeat = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(TokioDuration::from_secs(30));
        loop {
            ticker.tick().await;
            if stop_clone.load(Ordering::Relaxed) {
                break;
            }
            AgentManager::touch_agent(&agent_id_owned).await;
        }
    });

    let stdout_task = child.stdout.take().map(|stdout| {
        let agent = agent_id.to_string();
        tokio::spawn(async move { stream_reader_to_progress(agent, "stdout", stdout).await })
    });

    let stderr_task = child.stderr.take().map(|stderr| {
        let agent = agent_id.to_string();
        tokio::spawn(async move { stream_reader_to_progress(agent, "stderr", stderr).await })
    });

    let status = child
        .wait()
        .await
        .map_err(|e| format!("Failed to wait for agent process: {e}"))?;

    let stdout_buf = match stdout_task {
        Some(handle) => handle
            .await
            .map_err(|e| format!("Failed to read agent stdout: {e}"))?,
        None => String::new(),
    };

    let stderr_buf = match stderr_task {
        Some(handle) => handle
            .await
            .map_err(|e| format!("Failed to read agent stderr: {e}"))?,
        None => String::new(),
    };

    stop_flag.store(true, Ordering::Relaxed);
    heartbeat.abort();

    Ok((status, stdout_buf, stderr_buf))
}

async fn stream_reader_to_progress<R>(agent_id: String, label: &str, reader: R) -> String
where
    R: AsyncRead + Unpin,
{
    let mut lines = BufReader::new(reader).lines();
    let mut full = String::new();
    let mut chunk = String::new();
    let mut last_flush = Instant::now();

    while let Ok(Some(line)) = lines.next_line().await {
        let clean = line.trim_end_matches('\r');
        full.push_str(clean);
        full.push('\n');
        chunk.push_str(clean);
        chunk.push('\n');

        if chunk.len() >= STREAM_PROGRESS_BYTES || last_flush.elapsed() >= STREAM_PROGRESS_INTERVAL {
            flush_progress(&agent_id, label, &mut chunk).await;
            last_flush = Instant::now();
        }
    }

    if !chunk.is_empty() {
        flush_progress(&agent_id, label, &mut chunk).await;
    }

    full
}

async fn flush_progress(agent_id: &str, label: &str, chunk: &mut String) {
    let message = format!("[{label}] {}", chunk.trim_end());
    let mut mgr = AGENT_MANAGER.write().await;
    mgr.add_progress(agent_id, message).await;
    chunk.clear();
}

fn debug_subagents_enabled() -> bool {
    match std::env::var("CODE_SUBAGENT_DEBUG") {
        Ok(val) => {
            let lower = val.to_ascii_lowercase();
            matches!(lower.as_str(), "1" | "true" | "yes" | "on")
        }
        Err(_) => false,
    }
}

fn has_debug_flag(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--debug" || arg == "-d")
}

fn maybe_set_gemini_config_dir(env: &mut HashMap<String, String>, orig_home: Option<String>) {
    if env.get("GEMINI_API_KEY").is_some() {
        return;
    }

    let Some(home) = orig_home else { return; };
    let host_gem_cfg = std::path::PathBuf::from(&home).join(".gemini");
    if host_gem_cfg.is_dir() {
        env.insert(
            "GEMINI_CONFIG_DIR".to_string(),
            host_gem_cfg.to_string_lossy().to_string(),
        );
    }
}

pub(crate) fn should_use_current_exe_for_agent(
    family: &str,
    command_missing: bool,
    config: Option<&AgentConfig>,
) -> bool {
    if !matches!(family, "code" | "codex" | "cloud" | "coder") {
        return false;
    }

    // If the command is missing/empty, always use the current binary.
    if command_missing {
        return true;
    }

    if let Some(cfg) = config {
        let trimmed = cfg.command.trim();
        if trimmed.is_empty() {
            return true;
        }

        // If the configured command matches the canonical CLI for this spec, prefer self.
        if let Some(spec) = agent_model_spec(&cfg.name).or_else(|| agent_model_spec(trimmed)) {
            if trimmed.eq_ignore_ascii_case(spec.cli) {
                return true;
            }
        }

        // Otherwise assume the user intentionally set a custom command; do not override.
        false
    } else {
        // No explicit config: built-in families should use the current binary.
        true
    }
}

fn resolve_program_path(use_current_exe: bool, command_for_spawn: &str) -> Result<std::path::PathBuf, String> {
    if use_current_exe {
        return current_code_binary_path();
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(p) = resolve_in_path(command_for_spawn) {
            return Ok(p);
        }
    }

    Ok(std::path::PathBuf::from(command_for_spawn))
}

fn strip_model_flags(args: &mut Vec<String>) {
    let mut i = 0;
    while i < args.len() {
        let lowered = args[i].to_ascii_lowercase();
        if lowered == "--model" || lowered == "-m" {
            args.remove(i);
            if i < args.len() {
                args.remove(i);
            }
            continue;
        }
        if lowered.starts_with("--model=") || lowered.starts_with("-m=") {
            args.remove(i);
            continue;
        }
        i += 1;
    }
}

pub fn split_command_and_args(command: &str) -> (String, Vec<String>) {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return (String::new(), Vec::new());
    }
    if let Some(tokens) = shlex_split(trimmed) {
        if let Some((first, rest)) = tokens.split_first() {
            return (first.clone(), rest.to_vec());
        }
    }

    let tokens: Vec<String> = trimmed.split_whitespace().map(|s| s.to_string()).collect();
    if tokens.is_empty() {
        (String::new(), Vec::new())
    } else {
        let head = tokens[0].clone();
        (head, tokens[1..].to_vec())
    }
}

const AGENT_SMOKE_TEST_PROMPT: &str = "Reply only with the string \"ok\". Do not include any other words.";
const AGENT_SMOKE_TEST_EXPECTED: &str = "ok";
const AGENT_SMOKE_TEST_TIMEOUT: TokioDuration = TokioDuration::from_secs(20);

fn should_validate_in_read_only(_cfg: &AgentConfig) -> bool { true }

async fn run_agent_smoke_test(cfg: AgentConfig) -> Result<String, String> {
    let model_name = cfg.name.clone();
    let read_only = should_validate_in_read_only(&cfg);
    let mut task = tokio::spawn(async move {
        execute_model_with_permissions(
            "agent-smoke-test",
            &model_name,
            AGENT_SMOKE_TEST_PROMPT,
            read_only,
            None,
            Some(cfg),
            code_protocol::config_types::ReasoningEffort::High,
            None,
            None,
            None,
        )
        .await
    });
    let timer = tokio::time::sleep(AGENT_SMOKE_TEST_TIMEOUT);
    tokio::pin!(timer);
    tokio::select! {
        res = &mut task => {
            res.map_err(|e| format!("agent validation task failed: {e}"))?
        }
        _ = timer.as_mut() => {
            task.abort();
            let _ = task.await;
            return Err(format!(
                "agent validation timed out after {}s",
                AGENT_SMOKE_TEST_TIMEOUT.as_secs()
            ));
        }
    }
}

fn summarize_agent_output(output: &str) -> String {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return "<empty response>".to_string();
    }
    const MAX_LEN: usize = 240;
    if trimmed.len() <= MAX_LEN {
        trimmed.to_string()
    } else {
        let mut cutoff = MAX_LEN.min(trimmed.len());
        while cutoff > 0 && !trimmed.is_char_boundary(cutoff) {
            cutoff -= 1;
        }
        if cutoff == 0 {
            // Fallback: take first char to avoid empty slice
            let mut chars = trimmed.chars();
            if let Some(first) = chars.next() {
                format!("{}…", first)
            } else {
                "…".to_string()
            }
        } else {
            format!("{}…", &trimmed[..cutoff])
        }
    }
}

pub async fn smoke_test_agent(cfg: AgentConfig) -> Result<(), String> {
    let output = run_agent_smoke_test(cfg).await?;
    let normalized = output.trim().to_ascii_lowercase();
    if normalized == AGENT_SMOKE_TEST_EXPECTED {
        Ok(())
    } else {
        Err(format!(
            "agent response missing \"ok\": {}",
            summarize_agent_output(&output)
        ))
    }
}

fn run_smoke_test_with_new_runtime(cfg: AgentConfig) -> Result<(), String> {
    TokioRuntimeBuilder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("failed to build validation runtime: {}", e))?
        .block_on(smoke_test_agent(cfg))
}

pub fn smoke_test_agent_blocking(cfg: AgentConfig) -> Result<(), String> {
    if tokio::runtime::Handle::try_current().is_ok() {
        thread::Builder::new()
            .name("agent-smoke-test".into())
            .spawn(move || run_smoke_test_with_new_runtime(cfg))
            .map_err(|e| format!("failed to spawn agent validation thread: {}", e))?
            .join()
            .map_err(|_| "agent validation thread panicked".to_string())?
    } else {
        run_smoke_test_with_new_runtime(cfg)
    }
}

/// Execute the built-in cloud agent via the current `code` binary, streaming
/// stderr lines into the HUD as progress and returning final stdout. Applies a
/// modest truncation cap to very large outputs to keep UI responsive.
async fn execute_cloud_built_in_streaming(
    agent_id: &str,
    prompt: &str,
    working_dir: Option<std::path::PathBuf>,
    _config: Option<AgentConfig>,
    model_slug: &str,
) -> Result<String, String> {
    // Program and argv
    let program = current_code_binary_path()?;
    let mut args: Vec<String> = vec!["cloud".into(), "submit".into(), "--wait".into()];
    if let Some(spec) = agent_model_spec(model_slug) {
        args.extend(spec.model_args.iter().map(|arg| (*arg).to_string()));
    }
    args.push(prompt.into());

    // Baseline env mirrors behavior in execute_model_with_permissions
    let env: std::collections::HashMap<String, String> = std::env::vars().collect();

    use crate::protocol::SandboxPolicy;
    use crate::spawn::StdioPolicy;
    let mut child = crate::spawn::spawn_child_async(
        program.clone(),
        args.clone(),
        Some(program.to_string_lossy().as_ref()),
        working_dir.clone().unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))),
        &SandboxPolicy::DangerFullAccess,
        StdioPolicy::RedirectForShellTool,
        env,
    )
    .await
    .map_err(|e| format!("Failed to spawn cloud submit: {}", e))?;

    // Stream stderr to HUD
    let stderr_task = if let Some(stderr) = child.stderr.take() {
        let agent = agent_id.to_string();
        Some(tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let msg = line.trim();
                if msg.is_empty() { continue; }
                let mut mgr = AGENT_MANAGER.write().await;
                mgr.add_progress(&agent, msg.to_string()).await;
            }
        }))
    } else { None };

    // Collect stdout fully (final result)
    let mut stdout_buf = String::new();
    if let Some(stdout) = child.stdout.take() {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            stdout_buf.push_str(&line);
            stdout_buf.push('\n');
        }
    }

    let status = child.wait().await.map_err(|e| format!("Failed to wait: {}", e))?;
    if let Some(t) = stderr_task { let _ = t.await; }
    if !status.success() {
        return Err(format!("cloud submit exited with status {}", status));
    }

    if let Some(dir) = working_dir.as_ref() {
        let diff_text_opt = if stdout_buf.starts_with("diff --git ") {
            Some(stdout_buf.trim())
        } else {
            stdout_buf
                .find("\ndiff --git ")
                .map(|idx| stdout_buf[idx + 1..].trim())
        };

        if let Some(diff_text) = diff_text_opt {
            if !diff_text.is_empty() {
                let mut apply = Command::new("git");
                apply.arg("apply").arg("--whitespace=nowarn");
                apply.current_dir(dir);
                apply.stdin(Stdio::piped());

                let mut child = spawn_tokio_command_with_retry(&mut apply)
                    .await
                    .map_err(|e| format!("Failed to spawn git apply: {}", e))?;

                if let Some(mut stdin) = child.stdin.take() {
                    stdin
                        .write_all(diff_text.as_bytes())
                        .await
                        .map_err(|e| format!("Failed to write diff to git apply: {}", e))?;
                }

                let status = child
                    .wait()
                    .await
                    .map_err(|e| format!("Failed to wait for git apply: {}", e))?;

                if !status.success() {
                    return Err(format!(
                        "git apply exited with status {} while applying cloud diff",
                        status
                    ));
                }
            }
        }
    }

    // Truncate large outputs
    const MAX_BYTES: usize = 500_000; // ~500 KB
    if stdout_buf.len() > MAX_BYTES {
        let omitted = stdout_buf.len() - MAX_BYTES;
        let mut truncated = String::with_capacity(MAX_BYTES + 128);
        truncated.push_str(&stdout_buf[..MAX_BYTES]);
        truncated.push_str(&format!("\n… [truncated: {} bytes omitted]", omitted));
        Ok(truncated)
    } else {
        Ok(stdout_buf)
    }
}

// Tool creation functions

pub fn create_agent_tool(allowed_models: &[String]) -> OpenAiTool {
    let mut properties = BTreeMap::new();

    properties.insert(
        "action".to_string(),
        JsonSchema::String {
            description: Some(
                "Required: choose one of ['create','status','wait','result','cancel','list']".to_string(),
            ),
            allowed_values: Some(
                ["create", "status", "wait", "result", "cancel", "list"]
                    .into_iter()
                    .map(|value| value.to_string())
                    .collect(),
            ),
        },
    );

    let mut create_properties = BTreeMap::new();
    create_properties.insert(
        "name".to_string(),
        JsonSchema::String {
            description: Some("Display name shown in the UI (e.g., \"Plan TUI Refactor\")".to_string()),
            allowed_values: None,
        },
    );
    create_properties.insert(
        "task".to_string(),
        JsonSchema::String {
            description: Some("Task prompt to execute".to_string()),
            allowed_values: None,
        },
    );
    create_properties.insert(
        "context".to_string(),
        JsonSchema::String {
            description: Some("Optional background context".to_string()),
            allowed_values: None,
        },
    );
    create_properties.insert(
        "models".to_string(),
        JsonSchema::Array {
            items: Box::new(JsonSchema::String {
                description: None,
                allowed_values: if allowed_models.is_empty() {
                    None
                } else {
                    Some(allowed_models.iter().cloned().collect())
                },
            }),
                description: Some(
                    "Optional array of model names (e.g., ['code-gpt-5.2','claude-sonnet-4.5','code-gpt-5.2-codex','gemini-3-flash'])".to_string(),
                ),
        },
    );
    create_properties.insert(
        "files".to_string(),
        JsonSchema::Array {
            items: Box::new(JsonSchema::String {
                description: None,
                allowed_values: None,
            }),
            description: Some(
                "Optional array of file paths to include in context".to_string(),
            ),
        },
    );
    create_properties.insert(
        "output".to_string(),
        JsonSchema::String {
            description: Some("Optional desired output description".to_string()),
            allowed_values: None,
        },
    );
    create_properties.insert(
        "write".to_string(),
        JsonSchema::Boolean {
            description: Some(
                "Enable isolated write worktrees for each agent (default: true). Set false to keep the agent read-only.".to_string(),
            ),
        },
    );
    create_properties.insert(
        "read_only".to_string(),
        JsonSchema::Boolean {
            description: Some(
                "Deprecated: inverse of `write`. Prefer setting `write` instead.".to_string(),
            ),
        },
    );
    properties.insert(
        "create".to_string(),
        JsonSchema::Object {
            properties: create_properties,
            required: Some(vec!["task".to_string()]),
            additional_properties: Some(false.into()),
        },
    );

    let mut status_properties = BTreeMap::new();
    status_properties.insert(
        "agent_id".to_string(),
        JsonSchema::String {
            description: Some("Agent identifier to inspect".to_string()),
            allowed_values: None,
        },
    );
    properties.insert(
        "status".to_string(),
        JsonSchema::Object {
            properties: status_properties,
            required: Some(vec!["agent_id".to_string()]),
            additional_properties: Some(false.into()),
        },
    );

    let mut result_properties = BTreeMap::new();
    result_properties.insert(
        "agent_id".to_string(),
        JsonSchema::String {
            description: Some("Agent identifier whose result should be fetched".to_string()),
            allowed_values: None,
        },
    );
    properties.insert(
        "result".to_string(),
        JsonSchema::Object {
            properties: result_properties,
            required: Some(vec!["agent_id".to_string()]),
            additional_properties: Some(false.into()),
        },
    );

    let mut cancel_properties = BTreeMap::new();
    cancel_properties.insert(
        "agent_id".to_string(),
        JsonSchema::String {
            description: Some("Cancel a specific agent".to_string()),
            allowed_values: None,
        },
    );
    cancel_properties.insert(
        "batch_id".to_string(),
        JsonSchema::String {
            description: Some("Cancel all agents in the batch".to_string()),
            allowed_values: None,
        },
    );
    properties.insert(
        "cancel".to_string(),
        JsonSchema::Object {
            properties: cancel_properties,
            required: Some(Vec::new()),
            additional_properties: Some(false.into()),
        },
    );

    let mut wait_properties = BTreeMap::new();
    wait_properties.insert(
        "agent_id".to_string(),
        JsonSchema::String {
            description: Some("Wait for a specific agent".to_string()),
            allowed_values: None,
        },
    );
    wait_properties.insert(
        "batch_id".to_string(),
        JsonSchema::String {
            description: Some("Wait for any agent in the batch".to_string()),
            allowed_values: None,
        },
    );
    wait_properties.insert(
        "timeout_seconds".to_string(),
        JsonSchema::Number {
            description: Some(
                "Optional timeout before giving up (default 300, max 600)".to_string(),
            ),
        },
    );
    wait_properties.insert(
        "return_all".to_string(),
        JsonSchema::Boolean {
            description: Some(
                "When waiting on a batch, return all completed agents instead of the first".to_string(),
            ),
        },
    );
    properties.insert(
        "wait".to_string(),
        JsonSchema::Object {
            properties: wait_properties,
            required: Some(Vec::new()),
            additional_properties: Some(false.into()),
        },
    );

    let mut list_properties = BTreeMap::new();
    list_properties.insert(
        "status_filter".to_string(),
        JsonSchema::String {
            description: Some(
                "Optional status filter (pending, running, completed, failed, cancelled)".to_string(),
            ),
            allowed_values: None,
        },
    );
    list_properties.insert(
        "batch_id".to_string(),
        JsonSchema::String {
            description: Some("Limit results to a batch".to_string()),
            allowed_values: None,
        },
    );
    list_properties.insert(
        "recent_only".to_string(),
        JsonSchema::Boolean {
            description: Some(
                "When true, only include agents from the last two hours".to_string(),
            ),
        },
    );
    properties.insert(
        "list".to_string(),
        JsonSchema::Object {
            properties: list_properties,
            required: Some(Vec::new()),
            additional_properties: Some(false.into()),
        },
    );

    let required = Some(vec!["action".to_string()]);

    OpenAiTool::Function(ResponsesApiTool {
        name: "agent".to_string(),
        description: "Unified agent manager for launching, monitoring, and collecting results from asynchronous agents.".to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required,
            additional_properties: Some(false.into()),
        },
    })
}

// Parameter structs for handlers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunAgentParams {
    pub task: String,
    #[serde(default, deserialize_with = "deserialize_models_field")]
    pub models: Vec<String>,
    pub context: Option<String>,
    pub output: Option<String>,
    pub files: Option<Vec<String>>,
    #[serde(default)]
    pub write: Option<bool>,
    #[serde(default)]
    pub read_only: Option<bool>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCreateOptions {
    pub task: Option<String>,
    #[serde(default, deserialize_with = "deserialize_models_field")]
    pub models: Vec<String>,
    pub context: Option<String>,
    pub output: Option<String>,
    pub files: Option<Vec<String>>,
    #[serde(default)]
    pub write: Option<bool>,
    #[serde(default)]
    pub read_only: Option<bool>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentIdentifierOptions {
    pub agent_id: Option<String>,
    pub batch_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCancelOptions {
    pub agent_id: Option<String>,
    pub batch_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentWaitOptions {
    pub agent_id: Option<String>,
    pub batch_id: Option<String>,
    pub timeout_seconds: Option<u64>,
    pub return_all: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentListOptions {
    pub status_filter: Option<String>,
    pub batch_id: Option<String>,
    pub recent_only: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentToolRequest {
    pub action: String,
    pub create: Option<AgentCreateOptions>,
    pub status: Option<AgentIdentifierOptions>,
    pub result: Option<AgentIdentifierOptions>,
    pub cancel: Option<AgentCancelOptions>,
    pub wait: Option<AgentWaitOptions>,
    pub list: Option<AgentListOptions>,
}

pub(crate) fn normalize_agent_name(name: Option<String>) -> Option<String> {
    let Some(name) = name.map(|value| value.trim().to_string()) else {
        return None;
    };

    if name.is_empty() {
        return None;
    }

    let canonicalized = canonicalize_agent_word_boundaries(&name);
    let words: Vec<&str> = canonicalized.split_whitespace().collect();
    if words.is_empty() {
        return None;
    }

    Some(
        words
            .into_iter()
            .map(format_agent_word)
            .collect::<Vec<_>>()
            .join(" "),
    )
}

fn canonicalize_agent_word_boundaries(input: &str) -> String {
    let mut tokens: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();
    let mut prev_char: Option<char> = None;
    let mut uppercase_run: usize = 0;

    while let Some(ch) = chars.next() {
        if ch.is_whitespace() || matches!(ch, '_' | '-' | '/' | ':' | '.') {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            prev_char = None;
            uppercase_run = 0;
            continue;
        }

        let next_char = chars.peek().copied();
        let mut split = false;

        if !current.is_empty() {
            if let Some(prev) = prev_char {
                if prev.is_ascii_lowercase() && ch.is_ascii_uppercase() {
                    split = true;
                } else if prev.is_ascii_uppercase()
                    && ch.is_ascii_uppercase()
                    && uppercase_run > 0
                    && next_char.map_or(false, |c| c.is_ascii_lowercase())
                {
                    split = true;
                }
            }
        }

        if split {
            tokens.push(std::mem::take(&mut current));
            uppercase_run = 0;
        }

        current.push(ch);

        if ch.is_ascii_uppercase() {
            uppercase_run += 1;
        } else {
            uppercase_run = 0;
        }

        prev_char = Some(ch);
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens.join(" ")
}

const AGENT_NAME_ACRONYMS: &[&str] = &[
    "AI", "API", "CLI", "CPU", "DB", "GPU", "HTTP", "HTTPS", "ID", "LLM", "SDK", "SQL", "TUI", "UI", "UX",
];

fn format_agent_word(word: &str) -> String {
    if word.is_empty() {
        return String::new();
    }

    let uppercase = word.to_ascii_uppercase();
    if AGENT_NAME_ACRONYMS.contains(&uppercase.as_str()) {
        return uppercase;
    }

    let mut chars = word.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };

    let mut formatted = String::new();
    formatted.extend(first.to_uppercase());
    formatted.push_str(&chars.flat_map(char::to_lowercase).collect::<String>());
    formatted
}

fn deserialize_models_field<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum ModelsInput {
        Seq(Vec<String>),
        One(String),
    }

    let parsed = Option::<ModelsInput>::deserialize(deserializer)?;
    Ok(match parsed {
        Some(ModelsInput::Seq(seq)) => seq,
        Some(ModelsInput::One(single)) => vec![single],
        None => Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::normalize_agent_name;
    use super::maybe_set_gemini_config_dir;
    use super::execute_model_with_permissions;
    use super::resolve_program_path;
    use super::should_use_current_exe_for_agent;
    use super::prefer_json_result;
    use super::current_code_binary_path;
    use crate::config_types::AgentConfig;
    use code_protocol::config_types::ReasoningEffort;
    use std::collections::HashMap;
    use std::ffi::OsString;
    use tempfile::tempdir;
    use std::path::Path;
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};

    #[test]
    fn drops_empty_names() {
        assert_eq!(normalize_agent_name(None), None);
        assert_eq!(normalize_agent_name(Some("   ".into())), None);
    }

    #[test]
    fn title_cases_and_restores_separators() {
        assert_eq!(
            normalize_agent_name(Some("plan_tui_refactor".into())),
            Some("Plan TUI Refactor".into())
        );
        assert_eq!(
            normalize_agent_name(Some("run-ui-tests".into())),
            Some("Run UI Tests".into())
        );
    }

    #[test]
    fn handles_camel_case_and_acronyms() {
        assert_eq!(
            normalize_agent_name(Some("shipCloudAPI".into())),
            Some("Ship Cloud API".into())
        );
    }

    #[test]
    fn prefer_json_result_uses_json_when_available() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("out.json");
        let payload = "{\"findings\":[],\"overall_explanation\":\"ok\"}";
        std::fs::write(&path, payload).unwrap();

        let res = prefer_json_result(Some(&path), Err("fallback".to_string()));
        assert_eq!(res.unwrap(), payload);
    }

    #[test]
    fn prefer_json_result_falls_back_when_missing() {
        let missing = PathBuf::from("/nonexistent/path.json");
        let res = prefer_json_result(Some(&missing), Ok("orig".to_string()));
        assert_eq!(res.unwrap(), "orig");
    }

    fn agent_with_command(command: &str) -> AgentConfig {
        AgentConfig {
            name: "code-gpt-5.2-codex".to_string(),
            command: command.to_string(),
            args: Vec::new(),
            read_only: false,
            enabled: true,
            description: None,
            env: None,
            args_read_only: None,
            args_write: None,
            instructions: None,
        }
    }

    #[test]
    fn code_family_falls_back_when_command_missing() {
        let cfg = agent_with_command("definitely-not-present-429");
        let use_current = should_use_current_exe_for_agent("code", true, Some(&cfg));
        assert!(use_current);
    }

    #[test]
    fn code_family_prefers_current_exe_even_if_coder_in_path() {
        let cfg = agent_with_command("coder");
        let use_current = should_use_current_exe_for_agent("code", false, Some(&cfg));
        assert!(use_current);
    }

    #[test]
    fn code_family_respects_custom_command_override() {
        let cfg = agent_with_command("/usr/local/bin/my-coder");
        let use_current = should_use_current_exe_for_agent("code", false, Some(&cfg));
        assert!(!use_current);
    }

    #[test]
    fn program_path_uses_current_exe_when_requested() {
        let expected = current_code_binary_path().expect("current binary path");
        let resolved = resolve_program_path(true, "coder").expect("resolved program");
        assert_eq!(resolved, expected);

        let custom = resolve_program_path(false, "custom-coder").expect("resolved custom");
        assert_eq!(custom, std::path::PathBuf::from("custom-coder"));
    }

    #[tokio::test]
    async fn read_only_agents_use_code_binary_path() {
        let _lock = env_lock().lock().expect("env lock");
        let _reset_path = EnvReset::capture("PATH");
        let _reset_binary = EnvReset::capture("CODE_BINARY_PATH");

        let dir = tempdir().expect("tempdir");
        let current = script_path(dir.path(), "current");
        let shim = script_path(dir.path(), "coder");

        write_script(&current, "current");
        write_script(&shim, "path");

        unsafe {
            std::env::set_var("CODE_BINARY_PATH", &current);
            std::env::set_var("PATH", prepend_path(dir.path()));
        }

        let output = execute_model_with_permissions(
            "agent-test",
            "code-gpt-5.2-codex",
            "ok",
            true,
            None,
            None,
            ReasoningEffort::Low,
            None,
            None,
            None,
        )
        .await
        .expect("execute read-only agent");

        assert_eq!(output.trim(), "current");
    }

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct EnvReset {
        key: &'static str,
        value: Option<OsString>,
    }

    impl EnvReset {
        fn capture(key: &'static str) -> Self {
            let value = std::env::var_os(key);
            Self { key, value }
        }
    }

    impl Drop for EnvReset {
        fn drop(&mut self) {
            unsafe {
                match &self.value {
                    Some(value) => std::env::set_var(self.key, value),
                    None => std::env::remove_var(self.key),
                }
            }
        }
    }

    fn prepend_path(dir: &Path) -> OsString {
        let original = std::env::var_os("PATH");
        let mut parts: Vec<OsString> = Vec::new();
        parts.push(dir.as_os_str().to_os_string());
        if let Some(orig) = original {
            parts.extend(std::env::split_paths(&orig).map(|p| p.into_os_string()));
        }
        std::env::join_paths(parts).expect("join PATH")
    }

    #[cfg(target_os = "windows")]
    fn script_path(dir: &Path, name: &str) -> PathBuf {
        dir.join(format!("{name}.cmd"))
    }

    #[cfg(not(target_os = "windows"))]
    fn script_path(dir: &Path, name: &str) -> PathBuf {
        dir.join(name)
    }

    #[cfg(target_os = "windows")]
    fn write_script(path: &Path, marker: &str) {
        let script = format!("@echo off\r\necho {marker}\r\nexit /b 0\r\n");
        std::fs::write(path, script).expect("write cmd");
    }

    #[cfg(not(target_os = "windows"))]
    fn write_script(path: &Path, marker: &str) {
        let script = format!("#!/bin/sh\necho {marker}\nexit 0\n");
        std::fs::write(path, script).expect("write script");
        let mut perms = std::fs::metadata(path)
            .expect("script metadata")
            .permissions();
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms).expect("chmod script");
    }

    #[test]
    fn gemini_config_dir_is_injected_when_missing_api_key() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let gem_dir = tmp.path().join(".gemini");
        std::fs::create_dir_all(&gem_dir).expect("create .gemini");

        let mut env: HashMap<String, String> = HashMap::new();
        maybe_set_gemini_config_dir(&mut env, Some(tmp.path().to_string_lossy().to_string()));

        assert_eq!(
            env.get("GEMINI_CONFIG_DIR"),
            Some(&gem_dir.to_string_lossy().to_string())
        );
    }

    #[test]
    fn gemini_config_dir_not_overwritten_when_api_key_present() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut env: HashMap<String, String> = HashMap::new();
        env.insert("GEMINI_API_KEY".to_string(), "abc".to_string());

        maybe_set_gemini_config_dir(&mut env, Some(tmp.path().to_string_lossy().to_string()));

        assert!(!env.contains_key("GEMINI_CONFIG_DIR"));
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckAgentStatusParams {
    pub agent_id: String,
    pub batch_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetAgentResultParams {
    pub agent_id: String,
    pub batch_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelAgentParams {
    pub agent_id: Option<String>,
    pub batch_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaitForAgentParams {
    pub agent_id: Option<String>,
    pub batch_id: Option<String>,
    pub timeout_seconds: Option<u64>,
    pub return_all: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListAgentsParams {
    pub status_filter: Option<String>,
    pub batch_id: Option<String>,
    pub recent_only: Option<bool>,
}
