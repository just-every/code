use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::Hash;
use std::hash::Hasher;
use std::path::Path;
use std::path::PathBuf;

use thiserror::Error;

const SYSTEM_SKILLS_DIR_NAME: &str = ".system";
const SKILLS_DIR_NAME: &str = "skills";
const SYSTEM_SKILLS_MARKER_FILENAME: &str = ".codex-system-skills.marker";

const EMBEDDED_SYSTEM_SKILLS: &[(&str, &[u8])] = &[
    (
        "plan/SKILL.md",
        include_bytes!("assets/samples/plan/SKILL.md"),
    ),
    (
        "plan/LICENSE.txt",
        include_bytes!("assets/samples/plan/LICENSE.txt"),
    ),
    (
        "plan/scripts/create_plan.py",
        include_bytes!("assets/samples/plan/scripts/create_plan.py"),
    ),
    (
        "plan/scripts/list_plans.py",
        include_bytes!("assets/samples/plan/scripts/list_plans.py"),
    ),
    (
        "plan/scripts/plan_utils.py",
        include_bytes!("assets/samples/plan/scripts/plan_utils.py"),
    ),
    (
        "plan/scripts/read_plan_frontmatter.py",
        include_bytes!("assets/samples/plan/scripts/read_plan_frontmatter.py"),
    ),
    (
        "skill-creator/SKILL.md",
        include_bytes!("assets/samples/skill-creator/SKILL.md"),
    ),
    (
        "skill-creator/license.txt",
        include_bytes!("assets/samples/skill-creator/license.txt"),
    ),
    (
        "skill-creator/scripts/init_skill.py",
        include_bytes!("assets/samples/skill-creator/scripts/init_skill.py"),
    ),
    (
        "skill-creator/scripts/package_skill.py",
        include_bytes!("assets/samples/skill-creator/scripts/package_skill.py"),
    ),
    (
        "skill-creator/scripts/quick_validate.py",
        include_bytes!("assets/samples/skill-creator/scripts/quick_validate.py"),
    ),
    (
        "skill-installer/SKILL.md",
        include_bytes!("assets/samples/skill-installer/SKILL.md"),
    ),
    (
        "skill-installer/LICENSE.txt",
        include_bytes!("assets/samples/skill-installer/LICENSE.txt"),
    ),
    (
        "skill-installer/scripts/github_utils.py",
        include_bytes!("assets/samples/skill-installer/scripts/github_utils.py"),
    ),
    (
        "skill-installer/scripts/install-skill-from-github.py",
        include_bytes!("assets/samples/skill-installer/scripts/install-skill-from-github.py"),
    ),
    (
        "skill-installer/scripts/list-curated-skills.py",
        include_bytes!("assets/samples/skill-installer/scripts/list-curated-skills.py"),
    ),
];

/// Returns the on-disk cache location for embedded system skills.
///
/// This is typically located at `CODEX_HOME/skills/.system`.
pub(crate) fn system_cache_root_dir(code_home: &Path) -> PathBuf {
    code_home.join(SKILLS_DIR_NAME).join(SYSTEM_SKILLS_DIR_NAME)
}

/// Installs embedded system skills into `CODEX_HOME/skills/.system`.
///
/// Clears any existing system skills directory first and then writes the embedded
/// skills directory into place.
///
/// To avoid doing unnecessary work on every startup, a marker file is written
/// with a fingerprint of the embedded directory. When the marker matches, the
/// install is skipped.
pub(crate) fn install_system_skills(code_home: &Path) -> Result<(), SystemSkillsError> {
    let skills_root_dir = code_home.join(SKILLS_DIR_NAME);
    fs::create_dir_all(&skills_root_dir)
        .map_err(|source| SystemSkillsError::io("create skills root dir", source))?;

    let dest_system = system_cache_root_dir(code_home);
    let marker_path = dest_system.join(SYSTEM_SKILLS_MARKER_FILENAME);
    let expected_fingerprint = embedded_system_skills_fingerprint();

    if dest_system.is_dir() && read_marker(&marker_path).is_ok_and(|m| m == expected_fingerprint) {
        return Ok(());
    }

    if dest_system.exists() {
        fs::remove_dir_all(&dest_system)
            .map_err(|source| SystemSkillsError::io("remove existing system skills dir", source))?;
    }

    write_embedded_files(&dest_system)?;
    fs::write(&marker_path, format!("{expected_fingerprint}\n"))
        .map_err(|source| SystemSkillsError::io("write system skills marker", source))?;
    Ok(())
}

fn read_marker(path: &Path) -> Result<String, SystemSkillsError> {
    Ok(fs::read_to_string(path)
        .map_err(|source| SystemSkillsError::io("read system skills marker", source))?
        .trim()
        .to_string())
}

fn embedded_system_skills_fingerprint() -> String {
    let mut items: Vec<(&str, u64)> = EMBEDDED_SYSTEM_SKILLS
        .iter()
        .map(|&(rel, bytes)| {
            let mut file_hasher = DefaultHasher::new();
            bytes.hash(&mut file_hasher);
            (rel, file_hasher.finish())
        })
        .collect();
    items.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));

    let mut hasher = DefaultHasher::new();
    for (path, contents_hash) in items {
        path.hash(&mut hasher);
        contents_hash.hash(&mut hasher);
    }
    format!("{:x}", hasher.finish())
}

fn write_embedded_files(dest_root: &Path) -> Result<(), SystemSkillsError> {
    fs::create_dir_all(dest_root)
        .map_err(|source| SystemSkillsError::io("create system skills dir", source))?;

    for &(rel, bytes) in EMBEDDED_SYSTEM_SKILLS {
        let path = dest_root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|source| SystemSkillsError::io("create system skills file parent", source))?;
        }
        fs::write(&path, bytes)
            .map_err(|source| SystemSkillsError::io("write system skill file", source))?;
    }

    Ok(())
}

#[derive(Debug, Error)]
pub(crate) enum SystemSkillsError {
    #[error("io error while {action}: {source}")]
    Io {
        action: &'static str,
        #[source]
        source: std::io::Error,
    },
}

impl SystemSkillsError {
    fn io(action: &'static str, source: std::io::Error) -> Self {
        Self::Io { action, source }
    }
}
