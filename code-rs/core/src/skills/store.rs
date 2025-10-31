use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use tokio::sync::broadcast;
use tracing::warn;

use super::manifest::parse_skill_manifest_from_path;
use super::{SkillId, SkillManifest, SkillSource};

/// Trait implemented by concrete skill loaders (Anthropic catalog, local paths, etc.).
pub trait SkillLoader: Send + Sync {
    fn load(&self) -> Result<Vec<SkillEntry>, SkillLoaderError>;
}

/// Error while loading skills from a source.
#[derive(thiserror::Error, Debug)]
pub enum SkillLoaderError {
    #[error("skill loader failed: {0}")]
    Message(String),
    #[error(transparent)]
    Manifest(#[from] super::manifest::SkillManifestError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// In-memory registry entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillEntry {
    pub manifest: SkillManifest,
    pub source: SkillSource,
}

/// Notification describing a registry change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillRegistryEvent {
    SkillAdded(SkillEntry),
    SkillRemoved { id: SkillId },
}

/// Errors returned by the in-memory registry.
#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum SkillRegistryError {
    #[error("skill '{0}' already exists in the registry")]
    AlreadyExists(SkillId),
    #[error("skill '{0}' was not found in the registry")]
    NotFound(SkillId),
}

/// Thread-safe registry storing parsed skills and broadcasting change events.
pub struct SkillRegistry {
    inner: RwLock<HashMap<SkillId, SkillEntry>>,
    notifier: broadcast::Sender<SkillRegistryEvent>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        let (notifier, _) = broadcast::channel(32);
        Self {
            inner: RwLock::new(HashMap::new()),
            notifier,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SkillRegistryEvent> {
        self.notifier.subscribe()
    }

    pub fn list(&self) -> Vec<SkillEntry> {
        let map = self.inner.read().unwrap();
        let mut entries: Vec<_> = map.values().cloned().collect();
        entries.sort_by(|a, b| a.manifest.id.cmp(&b.manifest.id));
        entries
    }

    pub fn add_skill(&self, entry: SkillEntry) -> Result<(), SkillRegistryError> {
        let id = entry.manifest.id.clone();

        let mut guard = self.inner.write().unwrap();
        if guard.contains_key(&id) {
            return Err(SkillRegistryError::AlreadyExists(id));
        }
        guard.insert(id.clone(), entry.clone());
        drop(guard);

        let _ = self.notifier.send(SkillRegistryEvent::SkillAdded(entry));
        Ok(())
    }

    pub fn remove_skill(&self, id: &SkillId) -> Result<SkillEntry, SkillRegistryError> {
        let mut guard = self.inner.write().unwrap();
        let Some(entry) = guard.remove(id) else {
            return Err(SkillRegistryError::NotFound(id.clone()));
        };
        drop(guard);

        let _ = self
            .notifier
            .send(SkillRegistryEvent::SkillRemoved { id: id.clone() });

        Ok(entry)
    }
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Loader that scans a local directory for skill bundles.
pub struct LocalDirectorySkillLoader {
    root: PathBuf,
    source: SkillSource,
}

impl LocalDirectorySkillLoader {
    pub fn new(root: impl Into<PathBuf>, source: SkillSource) -> Self {
        Self { root: root.into(), source }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn entry_source(&self, skill_root: &Path) -> SkillSource {
        match &self.source {
            SkillSource::Anthropic { identifier, version } => SkillSource::Anthropic {
                identifier: identifier.clone(),
                version: version.clone(),
            },
            SkillSource::LocalUser { .. } => SkillSource::LocalUser {
                root: skill_root.to_path_buf(),
            },
            SkillSource::Project { .. } => SkillSource::Project {
                root: skill_root.to_path_buf(),
            },
        }
    }
}

impl SkillLoader for LocalDirectorySkillLoader {
    fn load(&self) -> Result<Vec<SkillEntry>, SkillLoaderError> {
        let mut entries = Vec::new();

        let root_manifest = self.root.join("SKILL.md");
        if root_manifest.exists() {
            match parse_skill_manifest_from_path(&self.root) {
                Ok(manifest) => {
                    let source = self.entry_source(&self.root);
                    entries.push(SkillEntry { manifest, source });
                }
                Err(err) => {
                    warn!("failed to parse SKILL.md at {}: {err}", root_manifest.display());
                }
            }
        }

        let read_dir = match fs::read_dir(&self.root) {
            Ok(iter) => iter,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(entries),
            Err(err) => return Err(SkillLoaderError::Io(err)),
        };

        for entry in read_dir {
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    warn!("failed to read entry in {}: {err}", self.root.display());
                    continue;
                }
            };

            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let manifest_path = path.join("SKILL.md");
            if !manifest_path.exists() {
                continue;
            }

            match parse_skill_manifest_from_path(&path) {
                Ok(manifest) => {
                    let source = self.entry_source(&path);
                    entries.push(SkillEntry { manifest, source });
                }
                Err(err) => {
                    warn!(
                        "failed to parse SKILL.md at {}: {err}",
                        manifest_path.display()
                    );
                }
            }
        }

        Ok(entries)
    }
}
