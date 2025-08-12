use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use uuid::Uuid;
use chrono::{DateTime, Utc, Duration};

use crate::config_types::AgentConfig;
use crate::openai_tools::{JsonSchema, OpenAiTool, ResponsesApiTool};

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
}

impl AgentManager {
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
            handles: HashMap::new(),
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
        ).await
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
        ).await
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
        let agent_ids: Vec<String> = self.agents
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
            if matches!(agent.status, AgentStatus::Completed | AgentStatus::Failed | AgentStatus::Cancelled) {
                agent.completed_at = Some(Utc::now());
            }
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
        }
    }

    pub async fn add_progress(&mut self, agent_id: &str, message: String) {
        if let Some(agent) = self.agents.get_mut(agent_id) {
            agent.progress.push(format!("{}: {}", Utc::now().format("%H:%M:%S"), message));
        }
    }

    pub async fn update_worktree_info(&mut self, agent_id: &str, worktree_path: String, branch_name: String) {
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

fn generate_branch_id(model: &str, agent: &str) -> String {
    // Extract first few meaningful words from agent for the branch name
    let words: Vec<&str> = agent
        .split_whitespace()
        .filter(|w| w.len() > 2 && !["the", "and", "for", "with", "from", "into"].contains(w))
        .take(3)
        .collect();
    
    let agent_suffix = if words.is_empty() {
        Uuid::new_v4().to_string().split('-').next().unwrap_or("agent").to_string()
    } else {
        words.join("-").to_lowercase()
    };
    
    format!("coder-{}-{}", model, agent_suffix)
}

async fn setup_worktree(git_root: &Path, branch_id: &str) -> Result<PathBuf, String> {
    // Create .coder/branches directory if it doesn't exist
    let coder_dir = git_root.join(".coder").join("branches");
    tokio::fs::create_dir_all(&coder_dir)
        .await
        .map_err(|e| format!("Failed to create .coder/branches directory: {}", e))?;
    
    // Path for this model's worktree
    let worktree_path = coder_dir.join(branch_id);
    
    // Remove existing worktree if it exists (cleanup from previous runs)
    if worktree_path.exists() {
        Command::new("git")
            .args(&["worktree", "remove", worktree_path.to_str().unwrap(), "--force"])
            .output()
            .await
            .ok(); // Ignore errors, it might not be a worktree
    }
    
    // Create new worktree
    let output = Command::new("git")
        .current_dir(git_root)
        .args(&[
            "worktree", 
            "add", 
            "-b", 
            branch_id,
            worktree_path.to_str().unwrap()
        ])
        .output()
        .await
        .map_err(|e| format!("Failed to create git worktree: {}", e))?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to create worktree: {}", stderr));
    }
    
    Ok(worktree_path)
}

async fn execute_agent(agent_id: String, config: Option<AgentConfig>) {
    let mut manager = AGENT_MANAGER.write().await;
    
    // Get agent details
    let agent = match manager.get_agent(&agent_id) {
        Some(t) => t,
        None => return,
    };
    
    // Update status to running
    manager.update_agent_status(&agent_id, AgentStatus::Running).await;
    manager.add_progress(&agent_id, format!("Starting agent with model: {}", agent.model)).await;
    
    let model = agent.model.clone();
    let prompt = agent.prompt.clone();
    let read_only = agent.read_only;
    let context = agent.context.clone();
    let output_goal = agent.output_goal.clone();
    let files = agent.files.clone();
    
    drop(manager); // Release the lock before executing
    
    // Build the full prompt with context
    let mut full_prompt = prompt.clone();
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
                manager.add_progress(&agent_id, format!("Creating git worktree: {}", branch_id)).await;
                drop(manager);
                
                match setup_worktree(&git_root, &branch_id).await {
                    Ok(worktree_path) => {
                        let mut manager = AGENT_MANAGER.write().await;
                        manager.add_progress(&agent_id, format!("Executing in worktree: {}", worktree_path.display())).await;
                        manager.update_worktree_info(&agent_id, worktree_path.display().to_string(), branch_id.clone()).await;
                        drop(manager);
                        
                        // Execute with full permissions in the worktree
                        execute_model_with_permissions(&model, &full_prompt, false, Some(worktree_path), config.clone()).await
                    }
                    Err(e) => Err(format!("Failed to setup worktree: {}", e))
                }
            }
            Err(e) => Err(format!("Git is required for non-read-only agents: {}", e))
        }
    } else {
        // Execute in read-only mode
        full_prompt = format!("{}\n\n[Running in read-only mode - no modifications allowed]", full_prompt);
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
    config: Option<AgentConfig>
) -> Result<String, String> {
    // Use config command if provided, otherwise use model name
    let command = if let Some(ref cfg) = config {
        cfg.command.clone()
    } else {
        model.to_lowercase()
    };
    
    let mut cmd = Command::new(command.clone());
    
    // Set working directory if provided
    if let Some(dir) = working_dir {
        cmd.current_dir(dir);
    }
    
    // Add environment variables from config if provided
    if let Some(ref cfg) = config {
        if let Some(ref env) = cfg.env {
            for (key, value) in env {
                cmd.env(key, value);
            }
        }
        
        // Add any configured args first
        for arg in &cfg.args {
            cmd.arg(arg);
        }
    }
    
    // Build command based on model and permissions
    // Use command instead of model for matching if config provided
    let model_lower = model.to_lowercase();
    let model_name = if config.is_some() {
        command.as_str()
    } else {
        model_lower.as_str()
    };
    
    match model_name {
        "claude" => {
            if read_only {
                cmd.args(&[
                    "--allowedTools",
                    "Bash(ls:*), Bash(cat:*), Bash(grep:*), Bash(git status:*), Bash(git log:*), Bash(find:*), Read, Grep, Glob, LS, WebFetch, TodoRead, TodoWrite, WebSearch",
                    "-p",
                    prompt
                ]);
            } else {
                cmd.args(&["--dangerously-skip-permissions", "-p", prompt]);
            }
        }
        "gemini" => {
            if read_only {
                cmd.args(&["-p", prompt]);
            } else {
                cmd.args(&["-y", "-p", prompt]);
            }
        }
        "codex" => {
            if read_only {
                cmd.args(&["-s", "read-only", "-a", "never", "exec", prompt]);
            } else {
                cmd.args(&["-s", "workspace-write", "-a", "never", "exec", prompt]);
            }
        }
        _ => {
            return Err(format!("Unknown model: {}", model));
        }
    }
    
    let output = cmd
        .output()
        .await
        .map_err(|e| format!("Failed to execute {}: {}", model, e))?;
    
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("Command failed: {}", stderr))
    }
}

// Tool creation functions
pub fn create_run_agent_tool() -> OpenAiTool {
    let mut properties = BTreeMap::new();
    
    properties.insert(
        "agent".to_string(),
        JsonSchema::String {
            description: Some("The agent prompt - what to perform (required)".to_string()),
        },
    );
    
    properties.insert(
        "model".to_string(),
        JsonSchema::String {
            description: Some("Model: 'claude', 'gemini', or 'codex' (or array of models for batch execution)".to_string()),
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
            description: Some("Optional: Array of file paths to include in the agent context".to_string()),
        },
    );
    
    properties.insert(
        "read_only".to_string(),
        JsonSchema::Boolean {
            description: Some("Optional: When true, agent runs in read-only mode (default: false)".to_string()),
        },
    );

    OpenAiTool::Function(ResponsesApiTool {
        name: "run_agent".to_string(),
        description: "Start a complex AI agent asynchronously. Returns a agent ID immediately to check status and retrieve results.".to_string(),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["agent".to_string()]),
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
        name: "check_agent_status".to_string(),
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
        name: "get_agent_result".to_string(),
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
            description: Some("The agent ID to cancel (required if batch_id not provided)".to_string()),
        },
    );
    
    properties.insert(
        "batch_id".to_string(),
        JsonSchema::String {
            description: Some("Cancel all agents with this batch ID (required if agent_id not provided)".to_string()),
        },
    );

    OpenAiTool::Function(ResponsesApiTool {
        name: "cancel_agent".to_string(),
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
            description: Some("Wait for this specific agent to complete (required if batch_id not provided)".to_string()),
        },
    );
    
    properties.insert(
        "batch_id".to_string(),
        JsonSchema::String {
            description: Some("Wait for any agent in this batch to complete (required if agent_id not provided)".to_string()),
        },
    );
    
    properties.insert(
        "timeout_seconds".to_string(),
        JsonSchema::Number {
            description: Some("Maximum seconds to wait before timing out (default: 300, max: 600)".to_string()),
        },
    );
    
    properties.insert(
        "return_all".to_string(),
        JsonSchema::Boolean {
            description: Some("For batch_id: return all completed agents instead of just the first one (default: false)".to_string()),
        },
    );

    OpenAiTool::Function(ResponsesApiTool {
        name: "wait_for_agent".to_string(),
        description: "Wait for a agent or any agent in a batch to complete, fail, or be cancelled.".to_string(),
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
            description: Some("Optional: Only show agents from the last 2 hours (default: false)".to_string()),
        },
    );

    OpenAiTool::Function(ResponsesApiTool {
        name: "list_agents".to_string(),
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
    pub agent: String,
    pub model: Option<serde_json::Value>, // Can be string or array
    pub context: Option<String>,
    pub output: Option<String>,
    pub files: Option<Vec<String>>,
    pub read_only: Option<bool>,
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