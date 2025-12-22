use crate::config::Config;
use crate::config::resolve_code_path_for_read;
use crate::git_info::resolve_root_git_project_for_trust;
use crate::skills::model::SkillError;
use crate::skills::model::SkillLoadOutcome;
use crate::skills::model::SkillMetadata;
use crate::skills::model::SkillScope;
use crate::skills::system::system_cache_root_dir;
use crate::skills::system::install_system_skills;
use dunce::canonicalize as normalize_path;
use serde::Deserialize;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use tracing::error;

#[derive(Debug, Deserialize)]
struct SkillFrontmatter {
    name: String,
    description: String,
}

const SKILLS_FILENAME: &str = "SKILL.md";
const SKILLS_DIR_NAME: &str = "skills";
const REPO_ROOT_CONFIG_DIR_NAME: &str = ".codex";
const MAX_NAME_LEN: usize = 64;
const MAX_DESCRIPTION_LEN: usize = 1024;

#[derive(Debug)]
enum SkillParseError {
    Read(std::io::Error),
    MissingFrontmatter,
    InvalidYaml(serde_yaml::Error),
    MissingField(&'static str),
    InvalidField { field: &'static str, reason: String },
}

impl fmt::Display for SkillParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SkillParseError::Read(e) => write!(f, "failed to read file: {e}"),
            SkillParseError::MissingFrontmatter => {
                write!(f, "missing YAML frontmatter delimited by ---")
            }
            SkillParseError::InvalidYaml(e) => write!(f, "invalid YAML: {e}"),
            SkillParseError::MissingField(field) => write!(f, "missing field `{field}`"),
            SkillParseError::InvalidField { field, reason } => {
                write!(f, "invalid {field}: {reason}")
            }
        }
    }
}

impl Error for SkillParseError {}

pub fn load_skills(config: &Config) -> SkillLoadOutcome {
    if let Err(err) = install_system_skills(&config.code_home) {
        tracing::error!("failed to install system skills: {err}");
    }
    load_skills_from_roots(skill_roots(config))
}

pub(crate) struct SkillRoot {
    pub(crate) path: PathBuf,
    pub(crate) scope: SkillScope,
}

pub(crate) fn load_skills_from_roots<I>(roots: I) -> SkillLoadOutcome
where
    I: IntoIterator<Item = SkillRoot>,
{
    let mut outcome = SkillLoadOutcome::default();
    for root in roots {
        discover_skills_under_root(&root.path, root.scope, &mut outcome);
    }

    let mut seen: HashSet<String> = HashSet::new();
    outcome
        .skills
        .retain(|skill| seen.insert(skill.name.clone()));

    outcome
        .skills
        .sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.path.cmp(&b.path)));

    outcome
}

pub(crate) fn user_skills_root(config: &Config) -> SkillRoot {
    let root = resolve_code_path_for_read(&config.code_home, Path::new(SKILLS_DIR_NAME));
    SkillRoot {
        path: root,
        scope: SkillScope::User,
    }
}

pub(crate) fn system_skills_root(config: &Config) -> SkillRoot {
    SkillRoot {
        path: system_cache_root_dir(&config.code_home),
        scope: SkillScope::System,
    }
}

pub(crate) fn repo_skills_root(cwd: &Path) -> Option<SkillRoot> {
    let base = if cwd.is_dir() { cwd } else { cwd.parent()? };
    let base = normalize_path(base).unwrap_or_else(|_| base.to_path_buf());

    let repo_root =
        resolve_root_git_project_for_trust(&base).map(|root| normalize_path(&root).unwrap_or(root));

    let scope = SkillScope::Repo;
    if let Some(repo_root) = repo_root.as_deref() {
        for dir in base.ancestors() {
            let skills_root = dir.join(REPO_ROOT_CONFIG_DIR_NAME).join(SKILLS_DIR_NAME);
            if skills_root.is_dir() {
                return Some(SkillRoot {
                    path: skills_root,
                    scope,
                });
            }

            if dir == repo_root {
                break;
            }
        }
        return None;
    }

    let skills_root = base.join(REPO_ROOT_CONFIG_DIR_NAME).join(SKILLS_DIR_NAME);
    skills_root.is_dir().then_some(SkillRoot {
        path: skills_root,
        scope,
    })
}

fn skill_roots(config: &Config) -> Vec<SkillRoot> {
    let mut roots = Vec::new();

    if let Some(repo_root) = repo_skills_root(&config.cwd) {
        roots.push(repo_root);
    }

    // Load order matters: we dedupe by name, keeping the first occurrence.
    // This makes repo/user skills win over system skills.
    roots.push(user_skills_root(config));
    roots.push(system_skills_root(config));

    roots
}

fn discover_skills_under_root(root: &Path, scope: SkillScope, outcome: &mut SkillLoadOutcome) {
    let Ok(root) = normalize_path(root) else {
        return;
    };

    if !root.is_dir() {
        return;
    }

    let mut queue: VecDeque<PathBuf> = VecDeque::from([root]);
    while let Some(dir) = queue.pop_front() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) => {
                error!("failed to read skills dir {}: {e:#}", dir.display());
                continue;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let file_name = match path.file_name().and_then(|f| f.to_str()) {
                Some(name) => name,
                None => continue,
            };

            if file_name.starts_with('.') {
                continue;
            }

            let Ok(file_type) = entry.file_type() else {
                continue;
            };

            if file_type.is_symlink() {
                continue;
            }

            if file_type.is_dir() {
                queue.push_back(path);
                continue;
            }

            if file_type.is_file() && file_name == SKILLS_FILENAME {
                match parse_skill_file(&path, scope) {
                    Ok(skill) => {
                        outcome.skills.push(skill);
                    }
                    Err(err) => {
                        if scope != SkillScope::System {
                            outcome.errors.push(SkillError {
                                path,
                                message: err.to_string(),
                            });
                        }
                    }
                }
            }
        }
    }
}

fn parse_skill_file(path: &Path, scope: SkillScope) -> Result<SkillMetadata, SkillParseError> {
    let contents = fs::read_to_string(path).map_err(SkillParseError::Read)?;

    let frontmatter = extract_frontmatter(&contents).ok_or(SkillParseError::MissingFrontmatter)?;

    let parsed: SkillFrontmatter =
        serde_yaml::from_str(&frontmatter).map_err(SkillParseError::InvalidYaml)?;

    let name = sanitize_single_line(&parsed.name);
    let description = sanitize_single_line(&parsed.description);

    validate_field(&name, MAX_NAME_LEN, "name")?;
    validate_field(&description, MAX_DESCRIPTION_LEN, "description")?;

    let resolved_path = normalize_path(path).unwrap_or_else(|_| path.to_path_buf());

    Ok(SkillMetadata {
        name,
        description,
        path: resolved_path,
        scope,
        content: contents,
    })
}

fn sanitize_single_line(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn validate_field(
    value: &str,
    max_len: usize,
    field_name: &'static str,
) -> Result<(), SkillParseError> {
    if value.is_empty() {
        return Err(SkillParseError::MissingField(field_name));
    }
    if value.chars().count() > max_len {
        return Err(SkillParseError::InvalidField {
            field: field_name,
            reason: format!("exceeds maximum length of {max_len} characters"),
        });
    }
    Ok(())
}

fn extract_frontmatter(contents: &str) -> Option<String> {
    let mut lines = contents.lines();
    if !matches!(lines.next(), Some(line) if line.trim() == "---") {
        return None;
    }

    let mut frontmatter_lines: Vec<&str> = Vec::new();
    let mut found_closing = false;
    for line in lines.by_ref() {
        if line.trim() == "---" {
            found_closing = true;
            break;
        }
        frontmatter_lines.push(line);
    }

    if frontmatter_lines.is_empty() || !found_closing {
        return None;
    }

    Some(frontmatter_lines.join("\n"))
}
