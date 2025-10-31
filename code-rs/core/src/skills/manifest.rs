use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::{AllowedTool, AllowedTools, SkillId, SkillIdError};

/// Parsed representation of a SKILL.md file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillManifest {
    pub id: SkillId,
    pub name: String,
    pub description: String,
    pub allowed_tools: AllowedTools,
    pub metadata: BTreeMap<String, serde_yaml::Value>,
    pub body: String,
    pub manifest_path: PathBuf,
    pub root: PathBuf,
}

/// Errors that can arise while parsing a SKILL.md manifest.
#[derive(thiserror::Error, Debug)]
pub enum SkillManifestError {
    #[error("skill directory name must be valid UTF-8")]
    NonUnicodeDirectoryName,
    #[error("missing SKILL.md file in {0}")]
    MissingManifest(PathBuf),
    #[error("failed to read SKILL.md at {path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
    #[error("SKILL.md must start with YAML frontmatter delimited by --- lines")]
    MissingFrontmatter,
    #[error("SKILL.md frontmatter is not terminated with ---")]
    UnterminatedFrontmatter,
    #[error("invalid YAML frontmatter: {0}")]
    InvalidYaml(serde_yaml::Error),
    #[error("skill manifest is missing required field: {0}")]
    MissingRequiredField(&'static str),
    #[error("skill name '{manifest_name}' must match directory name '{directory_name}'")]
    NameMismatch { manifest_name: String, directory_name: String },
    #[error("invalid skill name: {0}")]
    InvalidSkillId(#[from] SkillIdError),
}

#[derive(Debug, Deserialize)]
struct Frontmatter {
    name: Option<String>,
    description: Option<String>,
    #[serde(rename = "allowed-tools")]
    allowed_tools: Option<Vec<String>>,
    #[serde(default)]
    metadata: BTreeMap<String, serde_yaml::Value>,
}

/// Parse the SKILL.md manifest located under `skill_root`.
pub fn parse_skill_manifest_from_path(skill_root: &Path) -> Result<SkillManifest, SkillManifestError> {
    let directory_name = skill_root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(SkillManifestError::NonUnicodeDirectoryName)?
        .to_string();

    let manifest_path = skill_root.join("SKILL.md");
    if !manifest_path.exists() {
        return Err(SkillManifestError::MissingManifest(manifest_path));
    }

    let raw = fs::read_to_string(&manifest_path).map_err(|source| SkillManifestError::Io {
        path: manifest_path.clone(),
        source,
    })?;

    let (frontmatter_raw, body) = extract_frontmatter(&raw)?;

    let frontmatter: Frontmatter = serde_yaml::from_str(&frontmatter_raw)
        .map_err(SkillManifestError::InvalidYaml)?;

    let name = frontmatter
        .name
        .ok_or(SkillManifestError::MissingRequiredField("name"))?;
    let description = frontmatter
        .description
        .ok_or(SkillManifestError::MissingRequiredField("description"))?;

    if name != directory_name {
        return Err(SkillManifestError::NameMismatch {
            manifest_name: name,
            directory_name,
        });
    }

    let id = SkillId::try_from(name.clone())?;
    let allowed_tools = frontmatter
        .allowed_tools
        .unwrap_or_default()
        .into_iter()
        .map(AllowedTool::from_label)
        .collect();

    Ok(SkillManifest {
        id,
        name,
        description,
        allowed_tools,
        metadata: frontmatter.metadata,
        body,
        manifest_path,
        root: skill_root.to_path_buf(),
    })
}

fn extract_frontmatter(input: &str) -> Result<(String, String), SkillManifestError> {
    let mut lines = input.lines();
    let Some(first_line) = lines.next() else {
        return Err(SkillManifestError::MissingFrontmatter);
    };

    if first_line.trim() != "---" {
        return Err(SkillManifestError::MissingFrontmatter);
    }

    let mut frontmatter_lines = Vec::new();
    let mut found_terminator = false;

    for line in &mut lines {
        if line.trim() == "---" {
            found_terminator = true;
            break;
        }
        frontmatter_lines.push(line);
    }

    if !found_terminator {
        return Err(SkillManifestError::UnterminatedFrontmatter);
    }

    let body = lines.collect::<Vec<_>>().join("\n");

    Ok((frontmatter_lines.join("\n"), body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::AllowedTool;
    use tempfile::tempdir;

    #[test]
    fn parse_valid_manifest() {
        let temp = tempdir().unwrap();
        let skill_root = temp.path().join("financial-modeling");
        std::fs::create_dir(&skill_root).unwrap();
        let content = r#"---
name: financial-modeling
description: Build budgets.
allowed-tools:
  - browser
  - Bash
metadata:
  owner: finance
---

# Skill Body
"#;
        std::fs::write(skill_root.join("SKILL.md"), content).unwrap();

        let manifest = parse_skill_manifest_from_path(&skill_root).unwrap();

        assert_eq!(manifest.id.as_str(), "financial-modeling");
        assert_eq!(manifest.description, "Build budgets.");
        assert_eq!(manifest.allowed_tools.len(), 2);
        assert_eq!(manifest.allowed_tools[0], AllowedTool::Browser);
        assert_eq!(manifest.allowed_tools[1], AllowedTool::Bash);
        assert_eq!(manifest.metadata.get("owner").unwrap().as_str().unwrap(), "finance");
        assert!(manifest.body.contains("Skill Body"));
    }

    #[test]
    fn manifest_name_mismatch_errors() {
        let temp = tempdir().unwrap();
        let skill_root = temp.path().join("financial-modeling");
        std::fs::create_dir(&skill_root).unwrap();
        let content = r#"---
name: wrong-name
description: Something
---
"#;
        std::fs::write(skill_root.join("SKILL.md"), content).unwrap();

        let err = parse_skill_manifest_from_path(&skill_root).unwrap_err();

        match err {
            SkillManifestError::NameMismatch { manifest_name, directory_name } => {
                assert_eq!(manifest_name, "wrong-name");
                assert_eq!(directory_name, "financial-modeling");
            }
            other => panic!("unexpected error: {:?}", other),
        }
    }
}
