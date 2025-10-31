//! Skill manifest parsing and in-memory registry primitives.

mod manifest;
mod store;

pub use manifest::{parse_skill_manifest_from_path, SkillManifest, SkillManifestError};
pub use store::{
    LocalDirectorySkillLoader,
    SkillEntry,
    SkillLoader,
    SkillLoaderError,
    SkillRegistry,
    SkillRegistryEvent,
    SkillRegistryError,
};

use std::borrow::Cow;
use std::fmt;
use std::path::PathBuf;

/// Identifier for a Claude skill.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SkillId(String);

impl SkillId {
    /// Create a new skill identifier, validating the provided name.
    pub fn new(id: impl Into<String>) -> Result<Self, SkillIdError> {
        let id = id.into();
        if id.trim().is_empty() {
            return Err(SkillIdError::Empty);
        }

        if !id
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' )
        {
            return Err(SkillIdError::InvalidCharacters(id));
        }

        Ok(Self(id))
    }

    /// Borrow the identifier as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SkillId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<&str> for SkillId {
    type Error = SkillIdError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<String> for SkillId {
    type Error = SkillIdError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

/// Errors that can arise when validating skill identifiers.
#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum SkillIdError {
    #[error("skill name cannot be empty")]
    Empty,
    #[error("skill name contains invalid characters: {0}")]
    InvalidCharacters(String),
}

/// Location a skill originates from.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SkillSource {
    /// Skills provided by Anthropic via the Claude catalog.
    Anthropic { identifier: String, version: Option<String> },
    /// Skills uploaded by the local user (stored under their profile directory).
    LocalUser { root: PathBuf },
    /// Skills checked into the active project.
    Project { root: PathBuf },
}

/// Tool capabilities a skill declares.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AllowedTool {
    Browser,
    Agents,
    Bash,
    Custom(String),
}

impl AllowedTool {
    pub fn from_label(label: impl Into<String>) -> Self {
        let label_owned = label.into();
        match label_owned.trim().to_ascii_lowercase().as_str() {
            "browser" => Self::Browser,
            "agents" => Self::Agents,
            "bash" => Self::Bash,
            _ => Self::Custom(label_owned),
        }
    }

    pub fn label(&self) -> Cow<'_, str> {
        match self {
            Self::Browser => Cow::Borrowed("browser"),
            Self::Agents => Cow::Borrowed("agents"),
            Self::Bash => Cow::Borrowed("bash"),
            Self::Custom(value) => Cow::Borrowed(value.as_str()),
        }
    }
}

pub type AllowedTools = Vec<AllowedTool>;
