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
const AGENTS_DIR_NAME: &str = ".agents";
const REPO_ROOT_CONFIG_DIR_NAME: &str = ".codex";
const ADMIN_SKILLS_ROOT: &str = "/etc/codex/skills";
const MAX_NAME_LEN: usize = 64;
const MAX_DESCRIPTION_LEN: usize = 1024;
const MAX_SCAN_DEPTH: usize = 6;
const MAX_SKILLS_DIRS_PER_ROOT: usize = 2000;

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

pub(crate) fn home_agents_skills_root() -> Option<SkillRoot> {
    let home = dirs::home_dir()?;
    Some(SkillRoot {
        path: home.join(AGENTS_DIR_NAME).join(SKILLS_DIR_NAME),
        scope: SkillScope::User,
    })
}

pub(crate) fn system_skills_root(config: &Config) -> SkillRoot {
    SkillRoot {
        path: system_cache_root_dir(&config.code_home),
        scope: SkillScope::System,
    }
}

pub(crate) fn admin_skills_root() -> SkillRoot {
    SkillRoot {
        path: PathBuf::from(ADMIN_SKILLS_ROOT),
        scope: SkillScope::Admin,
    }
}

fn repo_search_dirs(cwd: &Path) -> Vec<PathBuf> {
    let Some(base) = (if cwd.is_dir() { Some(cwd) } else { cwd.parent() }) else {
        return Vec::new();
    };
    let base = normalize_path(base).unwrap_or_else(|_| base.to_path_buf());

    let repo_root =
        resolve_root_git_project_for_trust(&base).map(|root| normalize_path(&root).unwrap_or(root));

    if let Some(repo_root) = repo_root.as_deref() {
        let mut dirs = Vec::new();
        for dir in base.ancestors() {
            dirs.push(dir.to_path_buf());

            if dir == repo_root {
                break;
            }
        }
        return dirs;
    }

    vec![base]
}

fn repo_skills_roots_for_config_dir(cwd: &Path, config_dir: &str) -> Vec<SkillRoot> {
    repo_search_dirs(cwd)
        .into_iter()
        .map(|dir| dir.join(config_dir).join(SKILLS_DIR_NAME))
        .filter(|path| path.is_dir())
        .map(|path| SkillRoot {
            path,
            scope: SkillScope::Repo,
        })
        .collect()
}

fn dedupe_skill_roots_by_path(roots: &mut Vec<SkillRoot>) {
    let mut seen: HashSet<PathBuf> = HashSet::new();
    roots.retain(|root| {
        let normalized = normalize_path(&root.path).unwrap_or_else(|_| root.path.clone());
        seen.insert(normalized)
    });
}

fn skill_roots(config: &Config) -> Vec<SkillRoot> {
    let mut roots = Vec::new();

    roots.extend(repo_skills_roots_for_config_dir(&config.cwd, AGENTS_DIR_NAME));
    roots.extend(repo_skills_roots_for_config_dir(
        &config.cwd,
        REPO_ROOT_CONFIG_DIR_NAME,
    ));

    if let Some(home_root) = home_agents_skills_root() {
        roots.push(home_root);
    }

    // Load order matters: we dedupe by name, keeping the first occurrence.
    // This makes repo/user skills win over system/admin skills.
    roots.push(user_skills_root(config));
    roots.push(system_skills_root(config));
    roots.push(admin_skills_root());

    dedupe_skill_roots_by_path(&mut roots);

    roots
}

fn discover_skills_under_root(root: &Path, scope: SkillScope, outcome: &mut SkillLoadOutcome) {
    let Ok(root) = normalize_path(root) else {
        return;
    };

    if !root.is_dir() {
        return;
    }

    fn enqueue_dir(
        queue: &mut VecDeque<(PathBuf, usize)>,
        visited_dirs: &mut HashSet<PathBuf>,
        truncated_by_dir_limit: &mut bool,
        path: PathBuf,
        depth: usize,
    ) {
        if depth > MAX_SCAN_DEPTH {
            return;
        }
        if visited_dirs.len() >= MAX_SKILLS_DIRS_PER_ROOT {
            *truncated_by_dir_limit = true;
            return;
        }
        if visited_dirs.insert(path.clone()) {
            queue.push_back((path, depth));
        }
    }

    let follow_symlinks = matches!(scope, SkillScope::Repo | SkillScope::User | SkillScope::Admin);

    let mut visited_dirs: HashSet<PathBuf> = HashSet::new();
    visited_dirs.insert(root.clone());
    let mut queue: VecDeque<(PathBuf, usize)> = VecDeque::from([(root.clone(), 0)]);
    let mut truncated_by_dir_limit = false;

    while let Some((dir, depth)) = queue.pop_front() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) => {
                error!("failed to read skills dir {}: {e:#}", dir.display());
                continue;
            }
        };

        let mut entries: Vec<fs::DirEntry> = entries.flatten().collect();
        entries.sort_by_key(|entry| entry.file_name());

        for entry in entries {
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
                if !follow_symlinks {
                    continue;
                }

                let metadata = match fs::metadata(&path) {
                    Ok(metadata) => metadata,
                    Err(e) => {
                        error!(
                            "failed to stat skills entry {} (symlink): {e:#}",
                            path.display()
                        );
                        continue;
                    }
                };

                if metadata.is_dir() {
                    let Ok(resolved_dir) = normalize_path(&path) else {
                        continue;
                    };
                    enqueue_dir(
                        &mut queue,
                        &mut visited_dirs,
                        &mut truncated_by_dir_limit,
                        resolved_dir,
                        depth + 1,
                    );
                }
                continue;
            }

            if file_type.is_dir() {
                let Ok(resolved_dir) = normalize_path(&path) else {
                    continue;
                };
                enqueue_dir(
                    &mut queue,
                    &mut visited_dirs,
                    &mut truncated_by_dir_limit,
                    resolved_dir,
                    depth + 1,
                );
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

    if truncated_by_dir_limit {
        tracing::warn!(
            "skills scan truncated after {} directories (root: {})",
            MAX_SKILLS_DIRS_PER_ROOT,
            root.display()
        );
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::config::ConfigOverrides;
    use crate::config::ConfigToml;
    use std::ffi::OsString;
    use std::process::Command;

    const AGENTS_DIR_NAME: &str = ".agents";

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<OsString>,
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match self.previous.as_ref() {
                Some(value) => {
                    // SAFETY: tests that mutate process env are serialised.
                    unsafe { std::env::set_var(self.key, value) }
                }
                None => {
                    // SAFETY: tests that mutate process env are serialised.
                    unsafe { std::env::remove_var(self.key) }
                }
            }
        }
    }

    fn set_env_var(key: &'static str, value: &Path) -> EnvVarGuard {
        let previous = std::env::var_os(key);
        // SAFETY: tests that mutate process env are serialised.
        unsafe { std::env::set_var(key, value) };
        EnvVarGuard { key, previous }
    }

    fn make_config_for_cwd(code_home: &Path, cwd: &Path) -> Config {
        Config::load_from_base_config_with_overrides(
            ConfigToml::default(),
            ConfigOverrides {
                cwd: Some(cwd.to_path_buf()),
                ..Default::default()
            },
            code_home.to_path_buf(),
        )
        .expect("build config")
    }

    fn mark_as_git_repo(dir: &Path) {
        let output = Command::new("git")
            .arg("init")
            .current_dir(dir)
            .output()
            .expect("run git init");
        assert!(
            output.status.success(),
            "git init failed: status={:?}",
            output.status.code()
        );
    }

    fn write_skill_at(skills_root: &Path, dir: &str, name: &str, description: &str) -> PathBuf {
        let skill_dir = skills_root.join(dir);
        fs::create_dir_all(&skill_dir).expect("create skill dir");
        let skill_path = skill_dir.join(SKILLS_FILENAME);
        fs::write(
            &skill_path,
            format!(
                "---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n"
            ),
        )
        .expect("write skill file");
        skill_path
    }

    fn normalized(path: &Path) -> PathBuf {
        normalize_path(path).unwrap_or_else(|_| path.to_path_buf())
    }

    #[test]
    fn loads_skills_from_agents_dir_without_codex_dir() {
        let code_home = tempfile::tempdir().expect("tempdir");
        let repo_dir = tempfile::tempdir().expect("tempdir");
        mark_as_git_repo(repo_dir.path());

        let skill_path = write_skill_at(
            &repo_dir.path().join(AGENTS_DIR_NAME).join(SKILLS_DIR_NAME),
            "agents",
            "agents-skill",
            "from agents",
        );
        let cfg = make_config_for_cwd(code_home.path(), repo_dir.path());

        let outcome = load_skills(&cfg);
        assert!(
            outcome.errors.is_empty(),
            "unexpected errors: {:?}",
            outcome.errors
        );
        assert!(
            outcome.skills.iter().any(|skill| {
                skill.name == "agents-skill"
                    && skill.description == "from agents"
                    && skill.path == normalized(&skill_path)
                    && skill.scope == SkillScope::Repo
            }),
            "expected repo .agents skill in outcome: {:?}",
            outcome.skills
        );
    }

    #[test]
    #[serial_test::serial]
    fn loads_skills_from_home_agents_dir_for_user_scope() {
        let home_dir = tempfile::tempdir().expect("tempdir");
        let _home_guard = set_env_var("HOME", home_dir.path());
        let code_home = tempfile::tempdir().expect("tempdir");
        let cwd = tempfile::tempdir().expect("tempdir");

        let skill_path = write_skill_at(
            &home_dir.path().join(AGENTS_DIR_NAME).join(SKILLS_DIR_NAME),
            "home",
            "home-agents-skill",
            "from home agents",
        );

        let cfg = make_config_for_cwd(code_home.path(), cwd.path());

        let outcome = load_skills(&cfg);
        assert!(
            outcome.errors.is_empty(),
            "unexpected errors: {:?}",
            outcome.errors
        );
        assert!(
            outcome.skills.iter().any(|skill| {
                skill.name == "home-agents-skill"
                    && skill.description == "from home agents"
                    && skill.path == normalized(&skill_path)
                    && skill.scope == SkillScope::User
            }),
            "expected home .agents user skill in outcome: {:?}",
            outcome.skills
        );
    }

    #[cfg(unix)]
    #[test]
    fn follows_symlinked_subdir_for_user_scope() {
        let root_dir = tempfile::tempdir().expect("tempdir");
        let shared_dir = tempfile::tempdir().expect("tempdir");

        let skill_path = write_skill_at(
            shared_dir.path(),
            "shared",
            "symlinked-user-skill",
            "from symlink",
        );

        std::os::unix::fs::symlink(shared_dir.path(), root_dir.path().join("shared"))
            .expect("create symlink");

        let outcome = load_skills_from_roots(vec![SkillRoot {
            path: root_dir.path().to_path_buf(),
            scope: SkillScope::User,
        }]);

        assert!(
            outcome.errors.is_empty(),
            "unexpected errors: {:?}",
            outcome.errors
        );
        assert!(
            outcome.skills.iter().any(|skill| {
                skill.name == "symlinked-user-skill"
                    && skill.description == "from symlink"
                    && skill.path == normalized(&skill_path)
                    && skill.scope == SkillScope::User
            }),
            "expected symlinked user skill in outcome: {:?}",
            outcome.skills
        );
    }

    #[test]
    fn loads_skills_from_agents_dirs_between_cwd_and_repo_root() {
        let code_home = tempfile::tempdir().expect("tempdir");
        let repo_dir = tempfile::tempdir().expect("tempdir");
        mark_as_git_repo(repo_dir.path());

        let nested_dir = repo_dir.path().join("nested/inner");
        fs::create_dir_all(&nested_dir).expect("create nested dir");

        let root_skill_path = write_skill_at(
            &repo_dir.path().join(AGENTS_DIR_NAME).join(SKILLS_DIR_NAME),
            "root",
            "root-agents-skill",
            "from root agents",
        );
        let nested_skill_path = write_skill_at(
            &repo_dir
                .path()
                .join("nested")
                .join(AGENTS_DIR_NAME)
                .join(SKILLS_DIR_NAME),
            "nested",
            "nested-agents-skill",
            "from nested agents",
        );

        let cfg = make_config_for_cwd(code_home.path(), &nested_dir);

        let outcome = load_skills(&cfg);
        assert!(
            outcome.errors.is_empty(),
            "unexpected errors: {:?}",
            outcome.errors
        );
        assert!(
            outcome.skills.iter().any(|skill| {
                skill.name == "root-agents-skill"
                    && skill.path == normalized(&root_skill_path)
                    && skill.scope == SkillScope::Repo
            }),
            "expected root .agents skill in outcome: {:?}",
            outcome.skills
        );
        assert!(
            outcome.skills.iter().any(|skill| {
                skill.name == "nested-agents-skill"
                    && skill.path == normalized(&nested_skill_path)
                    && skill.scope == SkillScope::Repo
            }),
            "expected nested .agents skill in outcome: {:?}",
            outcome.skills
        );
    }

    #[test]
    #[serial_test::serial]
    fn prefers_repo_agents_over_user_and_legacy_for_duplicate_name() {
        let home_dir = tempfile::tempdir().expect("tempdir");
        let _home_guard = set_env_var("HOME", home_dir.path());
        let code_home = tempfile::tempdir().expect("tempdir");
        let repo_dir = tempfile::tempdir().expect("tempdir");
        mark_as_git_repo(repo_dir.path());

        let nested_dir = repo_dir.path().join("nested/inner");
        fs::create_dir_all(&nested_dir).expect("create nested dir");

        let repo_agents_path = write_skill_at(
            &repo_dir.path().join(AGENTS_DIR_NAME).join(SKILLS_DIR_NAME),
            "dup",
            "collision-skill",
            "from repo agents",
        );
        write_skill_at(
            &repo_dir
                .path()
                .join(REPO_ROOT_CONFIG_DIR_NAME)
                .join(SKILLS_DIR_NAME),
            "dup",
            "collision-skill",
            "from repo codex",
        );
        write_skill_at(
            &home_dir.path().join(AGENTS_DIR_NAME).join(SKILLS_DIR_NAME),
            "dup",
            "collision-skill",
            "from home agents",
        );
        write_skill_at(
            &code_home.path().join(SKILLS_DIR_NAME),
            "dup",
            "collision-skill",
            "from legacy user",
        );

        let cfg = make_config_for_cwd(code_home.path(), &nested_dir);

        let outcome = load_skills(&cfg);
        assert!(
            outcome.errors.is_empty(),
            "unexpected errors: {:?}",
            outcome.errors
        );

        let matching: Vec<&SkillMetadata> = outcome
            .skills
            .iter()
            .filter(|skill| skill.name == "collision-skill")
            .collect();
        assert_eq!(matching.len(), 1, "expected one deduped collision skill");
        assert_eq!(matching[0].description, "from repo agents");
        assert_eq!(matching[0].scope, SkillScope::Repo);
        assert_eq!(matching[0].path, normalized(&repo_agents_path));
    }

    #[test]
    fn still_loads_legacy_user_skills_root() {
        let code_home = tempfile::tempdir().expect("tempdir");
        let cwd = tempfile::tempdir().expect("tempdir");

        let legacy_path = write_skill_at(
            &code_home.path().join(SKILLS_DIR_NAME),
            "legacy",
            "legacy-user-skill",
            "from legacy user",
        );

        let cfg = make_config_for_cwd(code_home.path(), cwd.path());
        let outcome = load_skills(&cfg);
        assert!(
            outcome.errors.is_empty(),
            "unexpected errors: {:?}",
            outcome.errors
        );
        assert!(
            outcome.skills.iter().any(|skill| {
                skill.name == "legacy-user-skill"
                    && skill.description == "from legacy user"
                    && skill.path == normalized(&legacy_path)
                    && skill.scope == SkillScope::User
            }),
            "expected legacy user skill in outcome: {:?}",
            outcome.skills
        );
    }

    #[test]
    fn admin_root_uses_expected_path() {
        let root = admin_skills_root();
        assert_eq!(root.path, PathBuf::from(ADMIN_SKILLS_ROOT));
        assert_eq!(root.scope, SkillScope::Admin);
    }
}
