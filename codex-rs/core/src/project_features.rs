use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::config_types::{ProjectCommandConfig, ProjectHookConfig, ProjectHookEvent};

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectHook {
    pub event: ProjectHookEvent,
    pub name: Option<String>,
    pub command: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub env: HashMap<String, String>,
    pub timeout_ms: Option<u64>,
    pub run_in_background: bool,
}

impl ProjectHook {
    pub fn resolved_cwd(&self, session_cwd: &Path) -> PathBuf {
        match &self.cwd {
            Some(path) if path.is_absolute() => path.clone(),
            Some(path) => session_cwd.join(path),
            None => session_cwd.to_path_buf(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProjectHooks {
    hooks: HashMap<ProjectHookEvent, Vec<ProjectHook>>,
}

impl ProjectHooks {
    pub fn from_configs(configs: &[ProjectHookConfig], project_root: &Path) -> Self {
        let mut map: HashMap<ProjectHookEvent, Vec<ProjectHook>> = HashMap::new();
        for cfg in configs {
            if cfg.command.is_empty() {
                continue;
            }
            let hook = ProjectHook {
                event: cfg.event,
                name: cfg.name.clone(),
                command: cfg.command.clone(),
                cwd: resolve_optional_path(&cfg.cwd, project_root),
                env: cfg.env.clone().unwrap_or_default(),
                timeout_ms: cfg.timeout_ms,
                run_in_background: cfg.run_in_background.unwrap_or(false),
            };
            map.entry(cfg.event).or_default().push(hook);
        }
        Self { hooks: map }
    }

    pub fn is_empty(&self) -> bool {
        self.hooks.values().all(|hooks| hooks.is_empty())
    }

    pub fn hooks_for(&self, event: ProjectHookEvent) -> impl Iterator<Item = &ProjectHook> {
        self.hooks
            .get(&event)
            .into_iter()
            .flat_map(|hooks| hooks.iter())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectCommand {
    pub name: String,
    pub command: Vec<String>,
    pub description: Option<String>,
    pub cwd: Option<PathBuf>,
    pub env: HashMap<String, String>,
    pub timeout_ms: Option<u64>,
}

impl ProjectCommand {
    pub fn matches(&self, candidate: &str) -> bool {
        self.name.eq_ignore_ascii_case(candidate.trim())
    }

    pub fn resolved_cwd(&self, session_cwd: &Path) -> PathBuf {
        match &self.cwd {
            Some(path) if path.is_absolute() => path.clone(),
            Some(path) => session_cwd.join(path),
            None => session_cwd.to_path_buf(),
        }
    }
}

pub fn load_project_commands(
    configs: &[ProjectCommandConfig],
    project_root: &Path,
) -> Vec<ProjectCommand> {
    let mut commands: Vec<ProjectCommand> = Vec::new();
    for cfg in configs {
        let name = cfg.name.trim();
        if name.is_empty() || cfg.command.is_empty() {
            continue;
        }
        let entry = ProjectCommand {
            name: name.to_string(),
            command: cfg.command.clone(),
            description: cfg.description.clone(),
            cwd: resolve_optional_path(&cfg.cwd, project_root),
            env: cfg.env.clone().unwrap_or_default(),
            timeout_ms: cfg.timeout_ms,
        };

        if let Some(existing) = commands.iter_mut().find(|cmd| cmd.matches(name)) {
            *existing = entry;
        } else {
            commands.push(entry);
        }
    }
    commands
}

fn resolve_optional_path(raw: &Option<String>, project_root: &Path) -> Option<PathBuf> {
    let value = raw.as_ref()?.trim();
    if value.is_empty() {
        return None;
    }
    let path = PathBuf::from(value);
    if path.is_absolute() {
        Some(path)
    } else {
        Some(project_root.join(path))
    }
}
