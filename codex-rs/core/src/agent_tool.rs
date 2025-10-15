use chrono::DateTime;
use chrono::Duration;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::RwLock;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::config_types::AgentConfig;
use crate::openai_tools::JsonSchema;
use crate::openai_tools::OpenAiTool;
use crate::openai_tools::ResponsesApiTool;
use crate::protocol::AgentInfo;

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
    #[serde(skip)]
    #[allow(dead_code)]
    pub config: Option<AgentConfig>,
}

// Global agent manager
lazy_static::lazy_static! {
    pub static ref AGENT_MANAGER: Arc<RwLock<AgentManager>> = Arc::new(RwLock::new(AgentManager::new()));
}

pub struct AgentManager {
    agents: HashMap<String, Agent>,
    handles: HashMap<String, JoinHandle<()>>,
    event_sender: Option<mpsc::UnboundedSender<AgentStatusUpdatePayload>>,
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
        }
    }

    pub fn set_event_sender(&mut self, sender: mpsc::UnboundedSender<AgentStatusUpdatePayload>) {
        self.event_sender = Some(sender);
    }

    async fn send_agent_status_update(&self) {
        if let Some(ref sender) = self.event_sender {
            let agents: Vec<AgentInfo> = self
                .agents
                .values()
                .map(|agent| {
                    // Just show the model name - status provides the useful info
                    let name = agent.model.clone();

                    AgentInfo {
                        id: agent.id.clone(),
                        name,
                        status: format!("{:?}", agent.status).to_lowercase(),
                        batch_id: agent.batch_id.clone(),
                        model: Some(agent.model.clone()),
                        last_progress: agent.progress.last().cloned(),
                        result: agent.result.clone(),
                        error: agent.error.clone(),
                    }
                })
                .collect();

            // Get context and task from the first agent (they're all the same)
            let (context, task) = self
                .agents
                .values()
                .next()
                .map(|agent| {
                    let context = agent.context.as_ref().and_then(|value| {
                        if value.trim().is_empty() {
                            None
                        } else {
                            Some(value.clone())
                        }
                    });
                    let task = if agent.prompt.trim().is_empty() {
                        None
                    } else {
                        Some(agent.prompt.clone())
                    };
                    (context, task)
                })
                .unwrap_or((None, None));
            let payload = AgentStatusUpdatePayload {
                agents,
                context,
                task,
            };
            let _ = sender.send(payload);
        }
    }

    pub async fn create_agent(
        &mut self,
        model: String,
        prompt: String,
        context: Option<String>,
        output_goal: Option<String>,
        files: Vec<String>,
        read_only: bool,
        batch_id: Option<String>,
    ) -> String {
        self.create_agent_internal(
            model,
            prompt,
            context,
            output_goal,
            files,
            read_only,
            batch_id,
            None,
        )
        .await
    }

    pub async fn create_agent_with_config(
        &mut self,
        model: String,
        prompt: String,
        context: Option<String>,
        output_goal: Option<String>,
        files: Vec<String>,
        read_only: bool,
        batch_id: Option<String>,
        config: AgentConfig,
    ) -> String {
        self.create_agent_internal(
            model,
            prompt,
            context,
            output_goal,
            files,
            read_only,
            batch_id,
            Some(config),
        )
        .await
    }

    async fn create_agent_internal(
        &mut self,
        model: String,
        prompt: String,
        context: Option<String>,
        output_goal: Option<String>,
        files: Vec<String>,
        read_only: bool,
        batch_id: Option<String>,
        config: Option<AgentConfig>,
    ) -> String {
        let agent_id = Uuid::new_v4().to_string();

        let agent = Agent {
            id: agent_id.clone(),
            batch_id,
            model,
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
            branch_name: None,
            config: config.clone(),
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
            // Send status update event
            self.send_agent_status_update().await;
        }
    }

    pub async fn update_agent_result(&mut self, agent_id: &str, result: Result<String, String>) {
        if let Some(agent) = self.agents.get_mut(agent_id) {
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
            // Send status update event
            self.send_agent_status_update().await;
        }
    }

    pub async fn add_progress(&mut self, agent_id: &str, message: String) {
        if let Some(agent) = self.agents.get_mut(agent_id) {
            agent
                .progress
                .push(format!("{}: {}", Utc::now().format("%H:%M:%S"), message));
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
    let prompt = agent.prompt.clone();
    let read_only = agent.read_only;
    let context = agent.context.clone();
    let output_goal = agent.output_goal.clone();
    let files = agent.files.clone();

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
        full_prompt = format!("Context: {}\n\nAgent: {}", context, full_prompt);
    }
    if let Some(output_goal) = &output_goal {
        full_prompt = format!("{}\n\nDesired output: {}", full_prompt, output_goal);
    }
    if !files.is_empty() {
        full_prompt = format!("{}\n\nFiles to consider: {}", full_prompt, files.join(", "));
    }

    // Setup working directory and execute
    let result = if !read_only {
        // Check git and setup worktree for non-read-only mode
        match get_git_root().await {
            Ok(git_root) => {
                let branch_id = generate_branch_id(&model, &prompt);

                let mut manager = AGENT_MANAGER.write().await;
                manager
                    .add_progress(&agent_id, format!("Creating git worktree: {}", branch_id))
                    .await;
                drop(manager);

                match setup_worktree(&git_root, &branch_id).await {
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

                        // Execute with full permissions in the worktree
                        execute_model_with_permissions(
                            &model,
                            &full_prompt,
                            false,
                            Some(worktree_path),
                            config.clone(),
                        )
                        .await
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
        execute_model_with_permissions(&model, &full_prompt, true, None, config).await
    };

    // Update result
    let mut manager = AGENT_MANAGER.write().await;
    manager.update_agent_result(&agent_id, result).await;
}

async fn execute_model_with_permissions(
    model: &str,
    prompt: &str,
    read_only: bool,
    working_dir: Option<PathBuf>,
    config: Option<AgentConfig>,
) -> Result<String, String> {
    // Helper: cross‑platform check whether an executable is available in PATH
    // and is directly spawnable by std::process::Command (no shell wrappers).
    fn command_exists(cmd: &str) -> bool {
        // Absolute/relative path with separators: check directly (files only).
        if cmd.contains(std::path::MAIN_SEPARATOR) || cmd.contains('/') || cmd.contains('\\') {
            return std::fs::metadata(cmd).map(|m| m.is_file()).unwrap_or(false);
        }

        #[cfg(target_os = "windows")]
        {
            // On Windows, ensure we only accept spawnable extensions. PowerShell
            // scripts like .ps1 are not directly spawnable via Command::new.
            if let Ok(p) = which::which(cmd) {
                if !p.is_file() {
                    return false;
                }
                match p
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|s| s.to_ascii_lowercase())
                {
                    Some(ext) if matches!(ext.as_str(), "exe" | "com" | "cmd" | "bat") => true,
                    _ => false,
                }
            } else {
                false
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            use std::os::unix::fs::PermissionsExt;
            let Some(path_os) = std::env::var_os("PATH") else {
                return false;
            };
            for dir in std::env::split_paths(&path_os) {
                if dir.as_os_str().is_empty() {
                    continue;
                }
                let candidate = dir.join(cmd);
                if let Ok(meta) = std::fs::metadata(&candidate) {
                    if meta.is_file() {
                        let mode = meta.permissions().mode();
                        if mode & 0o111 != 0 {
                            return true;
                        }
                    }
                }
            }
            false
        }
    }

    // Use config command if provided, otherwise use model name
    let command = if let Some(ref cfg) = config {
        cfg.command.clone()
    } else {
        model.to_lowercase()
    };

    // Special case: for the built‑in Codex agent, prefer invoking the currently
    // running executable with the `exec` subcommand rather than relying on a
    // `codex` binary to be present on PATH. This improves portability,
    // especially on Windows where global shims may be missing.
    let model_lower = model.to_lowercase();
    let mut cmd = if (model_lower == "code" || model_lower == "codex") && config.is_none() {
        match std::env::current_exe() {
            Ok(path) => Command::new(path),
            Err(e) => return Err(format!("Failed to resolve current executable: {}", e)),
        }
    } else {
        Command::new(command.clone())
    };

    // Set working directory if provided
    if let Some(dir) = working_dir.clone() {
        cmd.current_dir(dir);
    }

    // Add environment variables from config if provided
    if let Some(ref cfg) = config {
        if let Some(ref env) = cfg.env {
            for (key, value) in env {
                cmd.env(key, value);
            }
        }

        // Add any configured args first, preferring mode‑specific values
        if read_only {
            if let Some(ro) = cfg.args_read_only.as_ref() {
                for arg in ro {
                    cmd.arg(arg);
                }
            } else {
                for arg in &cfg.args {
                    cmd.arg(arg);
                }
            }
        } else if let Some(w) = cfg.args_write.as_ref() {
            for arg in w {
                cmd.arg(arg);
            }
        } else {
            for arg in &cfg.args {
                cmd.arg(arg);
            }
        }
    }

    // Build command based on model and permissions
    // Use command instead of model for matching if config provided
    let model_name = if config.is_some() {
        command.as_str()
    } else {
        model_lower.as_str()
    };

    match model_name {
        "claude" | "gemini" | "qwen" => {
            let mut defaults = crate::agent_defaults::default_params_for(model_name, read_only);
            defaults.push("-p".into());
            defaults.push(prompt.to_string());
            cmd.args(defaults);
        }
        "codex" | "code" => {
            // If config provided explicit args for this mode, do not append defaults.
            let have_mode_args = config
                .as_ref()
                .map(|c| {
                    if read_only {
                        c.args_read_only.is_some()
                    } else {
                        c.args_write.is_some()
                    }
                })
                .unwrap_or(false);
            if have_mode_args {
                cmd.arg(prompt);
            } else {
                let mut defaults = crate::agent_defaults::default_params_for(model_name, read_only);
                defaults.push(prompt.to_string());
                cmd.args(defaults);
            }
        }
        _ => {
            return Err(format!("Unknown model: {}", model));
        }
    }

    // Proactively check for presence of external command before spawn when not
    // using the current executable fallback. This avoids confusing OS errors
    // like "program not found" and lets us surface a cleaner message.
    if model_name != "codex" && model_name != "code" && !command_exists(&command) {
        return Err(format!(
            "Required agent '{}' is not installed or not in PATH",
            command
        ));
    }

    // Agents: run without OS sandboxing; rely on per-branch worktrees for isolation.
    use crate::protocol::SandboxPolicy;
    use crate::spawn::StdioPolicy;
    let output = if !read_only {
        // Build env from current process then overlay any config-provided vars.
        let mut env: std::collections::HashMap<String, String> = std::env::vars().collect();
        let orig_home: Option<String> = env.get("HOME").cloned();
        if let Some(ref cfg) = config {
            if let Some(ref e) = cfg.env {
                for (k, v) in e {
                    env.insert(k.clone(), v.clone());
                }
            }
        }

        // Convenience: map common key names so external CLIs "just work".
        if let Some(google_key) = env.get("GOOGLE_API_KEY").cloned() {
            env.entry("GEMINI_API_KEY".to_string())
                .or_insert(google_key);
        }
        if let Some(claude_key) = env.get("CLAUDE_API_KEY").cloned() {
            env.entry("ANTHROPIC_API_KEY".to_string())
                .or_insert(claude_key);
        }
        if let Some(anthropic_key) = env.get("ANTHROPIC_API_KEY").cloned() {
            env.entry("CLAUDE_API_KEY".to_string())
                .or_insert(anthropic_key);
        }
        if let Some(anthropic_base) = env.get("ANTHROPIC_BASE_URL").cloned() {
            env.entry("CLAUDE_BASE_URL".to_string())
                .or_insert(anthropic_base);
        }
        // Qwen/DashScope convenience: mirror API keys and base URLs both ways so
        // either variable name works across tools.
        if let Some(qwen_key) = env.get("QWEN_API_KEY").cloned() {
            env.entry("DASHSCOPE_API_KEY".to_string())
                .or_insert(qwen_key);
        }
        if let Some(dashscope_key) = env.get("DASHSCOPE_API_KEY").cloned() {
            env.entry("QWEN_API_KEY".to_string())
                .or_insert(dashscope_key);
        }
        if let Some(qwen_base) = env.get("QWEN_BASE_URL").cloned() {
            env.entry("DASHSCOPE_BASE_URL".to_string())
                .or_insert(qwen_base);
        }
        if let Some(ds_base) = env.get("DASHSCOPE_BASE_URL").cloned() {
            env.entry("QWEN_BASE_URL".to_string()).or_insert(ds_base);
        }
        // Reduce startup overhead for Claude CLI: disable auto-updater/telemetry.
        env.entry("DISABLE_AUTOUPDATER".to_string())
            .or_insert("1".to_string());
        env.entry("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".to_string())
            .or_insert("1".to_string());
        env.entry("DISABLE_ERROR_REPORTING".to_string())
            .or_insert("1".to_string());
        // Prefer explicit Claude config dir to avoid touching $HOME/.claude.json.
        // Do not force CLAUDE_CONFIG_DIR here; leave CLI free to use its default
        // (including Keychain) unless we explicitly redirect HOME below.

        // If GEMINI_API_KEY not provided, try pointing to host config for read‑only
        // discovery (Gemini CLI supports GEMINI_CONFIG_DIR). We keep HOME as-is so
        // CLIs that require ~/.gemini and ~/.claude continue to work with your
        // existing config.
        if env.get("GEMINI_API_KEY").is_none() {
            if let Some(h) = orig_home.clone() {
                let host_gem_cfg = std::path::PathBuf::from(&h).join(".gemini");
                if host_gem_cfg.is_dir() {
                    env.insert(
                        "GEMINI_CONFIG_DIR".to_string(),
                        host_gem_cfg.to_string_lossy().to_string(),
                    );
                }
            }
        }

        // No OS sandbox.

        // Resolve the command and args we prepared above into Vec<String> for spawn helpers.
        // Intentionally build args fresh for sandbox helpers; `Command` does not expose argv.
        // Rebuild the invocation as `command` + args set above.
        // We reconstruct to run under our sandbox helpers.
        let program = if (model_lower == "code" || model_lower == "codex") && config.is_none() {
            // Use current exe path
            std::env::current_exe()
                .map_err(|e| format!("Failed to resolve current executable: {}", e))?
        } else {
            // Use program name; PATH resolution will be handled by spawn helper with provided env.
            std::path::PathBuf::from(&command)
        };

        // Rebuild args exactly as above
        let mut args: Vec<String> = Vec::new();
        // Include configured args (mode‑specific preferred) first, to mirror the
        // immediate-Command path above.
        if let Some(ref cfg) = config {
            if read_only {
                if let Some(ro) = cfg.args_read_only.as_ref() {
                    args.extend(ro.iter().cloned());
                } else {
                    args.extend(cfg.args.iter().cloned());
                }
            } else if let Some(w) = cfg.args_write.as_ref() {
                args.extend(w.iter().cloned());
            } else {
                args.extend(cfg.args.iter().cloned());
            }
        }

        match model_name {
            "claude" | "gemini" | "qwen" => {
                let mut defaults = crate::agent_defaults::default_params_for(model_name, read_only);
                defaults.push("-p".into());
                defaults.push(prompt.to_string());
                args.extend(defaults);
            }
            "codex" | "code" => {
                let have_mode_args = config
                    .as_ref()
                    .map(|c| {
                        if read_only {
                            c.args_read_only.is_some()
                        } else {
                            c.args_write.is_some()
                        }
                    })
                    .unwrap_or(false);
                if have_mode_args {
                    args.push(prompt.to_string());
                } else {
                    let mut defaults =
                        crate::agent_defaults::default_params_for(model_name, read_only);
                    defaults.push(prompt.to_string());
                    args.extend(defaults);
                }
            }
            _ => {}
        }

        // Always run agents without OS sandboxing.
        let sandbox_type = crate::exec::SandboxType::None;

        // Spawn via helpers and capture output
        let child_result: std::io::Result<tokio::process::Child> = match sandbox_type {
            crate::exec::SandboxType::None
            | crate::exec::SandboxType::MacosSeatbelt
            | crate::exec::SandboxType::LinuxSeccomp => {
                crate::spawn::spawn_child_async(
                    program.clone(),
                    args.clone(),
                    Some(&program.to_string_lossy()),
                    working_dir.clone().unwrap_or_else(|| {
                        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
                    }),
                    &SandboxPolicy::DangerFullAccess,
                    StdioPolicy::RedirectForShellTool,
                    env.clone(),
                )
                .await
            }
        };

        match child_result {
            Ok(child) => child
                .wait_with_output()
                .await
                .map_err(|e| format!("Failed to read output: {}", e))?,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    return Err(format!(
                        "Required agent '{}' is not installed or not in PATH",
                        command
                    ));
                }
                return Err(format!("Failed to spawn sandboxed agent: {}", e));
            }
        }
    } else {
        // Read-only path: use prior behavior
        match cmd.output().await {
            Ok(o) => o,
            Err(e) => {
                // Only fall back for external CLIs (not the built-in code/codex path)
                if model_name == "codex" || model_name == "code" {
                    return Err(format!("Failed to execute {}: {}", model, e));
                }
                let mut fb = match std::env::current_exe() {
                    Ok(p) => Command::new(p),
                    Err(e2) => {
                        return Err(format!(
                            "Failed to execute {} and could not resolve built-in fallback: {} / {}",
                            model, e, e2
                        ));
                    }
                };
                if read_only {
                    fb.args([
                        "-s",
                        "read-only",
                        "-a",
                        "never",
                        "exec",
                        "--skip-git-repo-check",
                        prompt,
                    ]);
                } else {
                    fb.args([
                        "-s",
                        "workspace-write",
                        "-a",
                        "never",
                        "exec",
                        "--skip-git-repo-check",
                        prompt,
                    ]);
                }
                fb.output().await.map_err(|e2| {
                    format!(
                        "Failed to execute {} ({}). Built-in fallback also failed: {}",
                        model, e, e2
                    )
                })?
            }
        }
    };

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let combined = if stderr.trim().is_empty() {
            stdout.trim().to_string()
        } else if stdout.trim().is_empty() {
            stderr.trim().to_string()
        } else {
            format!("{}\n{}", stderr.trim(), stdout.trim())
        };
        Err(format!("Command failed: {}", combined))
    }
}

// Tool creation functions
pub fn create_run_agent_tool() -> OpenAiTool {
    let mut properties = BTreeMap::new();

    properties.insert(
        "task".to_string(),
        JsonSchema::String {
            description: Some("The task prompt - what to perform (required)".to_string()),
        },
    );

    properties.insert(
        "models".to_string(),
        JsonSchema::Array {
            items: Box::new(JsonSchema::String { description: None }),
            description: Some(
                "Optional: Array of model names (e.g., ['claude','gemini','qwen','code'])"
                    .to_string(),
            ),
        },
    );

    properties.insert(
        "context".to_string(),
        JsonSchema::String {
            description: Some("Optional: Background context for the agent".to_string()),
        },
    );

    properties.insert(
        "output".to_string(),
        JsonSchema::String {
            description: Some("Optional: The desired output/success state".to_string()),
        },
    );

    properties.insert(
        "files".to_string(),
        JsonSchema::Array {
            items: Box::new(JsonSchema::String { description: None }),
            description: Some(
                "Optional: Array of file paths to include in the agent context".to_string(),
            ),
        },
    );

    properties.insert(
        "read_only".to_string(),
        JsonSchema::Boolean {
            description: Some(
                "Optional: When true, agent runs in read-only mode (default: false)".to_string(),
            ),
        },
    );

    OpenAiTool::Function(ResponsesApiTool {
        name: "agent_run".to_string(),
        description: "Start a complex AI task asynchronously. Returns an agent ID immediately to check status and retrieve results. Once an agent is running, enables: agent_check, agent_result, agent_cancel, agent_wait, agent_list.".to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["task".to_string()]),
            additional_properties: Some(false),
        },
    })
}

pub fn create_check_agent_status_tool() -> OpenAiTool {
    let mut properties = BTreeMap::new();

    properties.insert(
        "agent_id".to_string(),
        JsonSchema::String {
            description: Some("The agent ID returned from run_agent".to_string()),
        },
    );

    OpenAiTool::Function(ResponsesApiTool {
        name: "agent_check".to_string(),
        description: "Check the status of a running agent. Returns current status, progress, and partial results if available.".to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["agent_id".to_string()]),
            additional_properties: Some(false),
        },
    })
}

pub fn create_get_agent_result_tool() -> OpenAiTool {
    let mut properties = BTreeMap::new();

    properties.insert(
        "agent_id".to_string(),
        JsonSchema::String {
            description: Some("The agent ID returned from run_agent".to_string()),
        },
    );

    OpenAiTool::Function(ResponsesApiTool {
        name: "agent_result".to_string(),
        description: "Get the final result of a completed agent.".to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["agent_id".to_string()]),
            additional_properties: Some(false),
        },
    })
}

pub fn create_cancel_agent_tool() -> OpenAiTool {
    let mut properties = BTreeMap::new();

    properties.insert(
        "agent_id".to_string(),
        JsonSchema::String {
            description: Some(
                "The agent ID to cancel (required if batch_id not provided)".to_string(),
            ),
        },
    );

    properties.insert(
        "batch_id".to_string(),
        JsonSchema::String {
            description: Some(
                "Cancel all agents with this batch ID (required if agent_id not provided)"
                    .to_string(),
            ),
        },
    );

    OpenAiTool::Function(ResponsesApiTool {
        name: "agent_cancel".to_string(),
        description: "Cancel a pending or running agent, or all agents in a batch.".to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec![]),
            additional_properties: Some(false),
        },
    })
}

pub fn create_wait_for_agent_tool() -> OpenAiTool {
    let mut properties = BTreeMap::new();

    properties.insert(
        "agent_id".to_string(),
        JsonSchema::String {
            description: Some(
                "Wait for this specific agent to complete (required if batch_id not provided)"
                    .to_string(),
            ),
        },
    );

    properties.insert(
        "batch_id".to_string(),
        JsonSchema::String {
            description: Some(
                "Wait for any agent in this batch to complete (required if agent_id not provided)"
                    .to_string(),
            ),
        },
    );

    properties.insert(
        "timeout_seconds".to_string(),
        JsonSchema::Number {
            description: Some(
                "Maximum seconds to wait before timing out (default: 300, max: 600)".to_string(),
            ),
        },
    );

    properties.insert(
        "return_all".to_string(),
        JsonSchema::Boolean {
            description: Some("For batch_id: return all completed agents instead of just the first one (default: false)".to_string()),
        },
    );

    OpenAiTool::Function(ResponsesApiTool {
        name: "agent_wait".to_string(),
        description: "Wait for a agent or any agent in a batch to complete, fail, or be cancelled."
            .to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec![]),
            additional_properties: Some(false),
        },
    })
}

pub fn create_list_agents_tool() -> OpenAiTool {
    let mut properties = BTreeMap::new();

    properties.insert(
        "status_filter".to_string(),
        JsonSchema::String {
            description: Some("Optional: Filter agents by status (pending, running, completed, failed, cancelled)".to_string()),
        },
    );

    properties.insert(
        "batch_id".to_string(),
        JsonSchema::String {
            description: Some("Optional: Filter agents by batch ID".to_string()),
        },
    );

    properties.insert(
        "recent_only".to_string(),
        JsonSchema::Boolean {
            description: Some(
                "Optional: Only show agents from the last 2 hours (default: false)".to_string(),
            ),
        },
    );

    OpenAiTool::Function(ResponsesApiTool {
        name: "agent_list".to_string(),
        description: "List all agents with their current status.".to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec![]),
            additional_properties: Some(false),
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
    pub read_only: Option<bool>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckAgentStatusParams {
    pub agent_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetAgentResultParams {
    pub agent_id: String,
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
