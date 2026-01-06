use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::config_types::{ProjectCommandConfig, ProjectHookConfig, ProjectHookEvent, ProjectHookType};
use regex_lite::Regex;
use wildmatch::WildMatchPattern;

#[derive(Debug, Clone, PartialEq)]
enum MatcherClause {
    Any,
    Exact(String),
    Wildcard(WildMatchPattern),
    Regex(Regex),
}

#[derive(Debug, Clone, PartialEq)]
pub struct HookMatcher {
    raw: Option<String>,
    clauses: Vec<MatcherClause>,
}

impl HookMatcher {
    pub fn from_raw(raw: &Option<String>) -> Self {
        let raw_trimmed = raw.as_ref().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        let mut clauses = Vec::new();
        if let Some(raw_value) = raw_trimmed.as_ref() {
            for part in raw_value.split('|').map(str::trim).filter(|p| !p.is_empty()) {
                if part == "*" {
                    clauses.push(MatcherClause::Any);
                    continue;
                }
                if looks_like_regex(part) {
                    match Regex::new(part) {
                        Ok(regex) => clauses.push(MatcherClause::Regex(regex)),
                        Err(err) => {
                            tracing::warn!("invalid hook matcher regex `{}`: {}", part, err);
                        }
                    }
                    continue;
                }
                if part.contains('*') || part.contains('?') {
                    clauses.push(MatcherClause::Wildcard(WildMatchPattern::new(part)));
                    continue;
                }
                clauses.push(MatcherClause::Exact(part.to_string()));
            }
        }

        if clauses.is_empty() {
            clauses.push(MatcherClause::Any);
        }

        Self {
            raw: raw_trimmed,
            clauses,
        }
    }

    pub fn matches(&self, subject: &str) -> bool {
        self.clauses.iter().any(|clause| match clause {
            MatcherClause::Any => true,
            MatcherClause::Exact(value) => value == subject,
            MatcherClause::Wildcard(pattern) => pattern.matches(subject),
            MatcherClause::Regex(regex) => regex.is_match(subject),
        })
    }

    pub fn raw(&self) -> Option<&str> {
        self.raw.as_deref()
    }
}

fn looks_like_regex(value: &str) -> bool {
    value
        .chars()
        .any(|c| matches!(c, '.' | '^' | '$' | '[' | ']' | '(' | ')' | '+' | '\\'))
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectHook {
    pub event: ProjectHookEvent,
    pub name: Option<String>,
    pub hook_type: ProjectHookType,
    pub matcher: HookMatcher,
    pub prompt: Option<String>,
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

    pub fn matches(&self, subject: Option<&str>) -> bool {
        let target = subject.unwrap_or("");
        self.matcher.matches(target)
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
            let hook_type = cfg.hook_type;
            match hook_type {
                ProjectHookType::Command => {
                    if cfg.command.is_empty() {
                        continue;
                    }
                }
                ProjectHookType::Prompt => {
                    if cfg
                        .prompt
                        .as_ref()
                        .map(str::trim)
                        .filter(|p| !p.is_empty())
                        .is_none()
                    {
                        continue;
                    }
                }
            }
            let hook = ProjectHook {
                event: cfg.event,
                name: cfg.name.clone(),
                hook_type,
                matcher: HookMatcher::from_raw(&cfg.matcher),
                prompt: cfg.prompt.clone(),
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

    pub fn matching_hooks(&self, event: ProjectHookEvent, subject: Option<&str>) -> Vec<ProjectHook> {
        self.hooks
            .get(&event)
            .into_iter()
            .flat_map(|hooks| hooks.iter())
            .filter(|hook| hook.matches(subject))
            .cloned()
            .collect()
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

pub fn load_project_commands(configs: &[ProjectCommandConfig], project_root: &Path) -> Vec<ProjectCommand> {
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
