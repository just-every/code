//! Storage utilities for timeline persistence.
//!
//! Phase 2A: Placeholder for future persistence implementation.
//! Current implementation is in-memory only.

use serde::{Deserialize, Serialize};

use super::timeline::ContextTimeline;

/// Storage configuration for timeline persistence.
///
/// Phase 2A: Not yet implemented, placeholder for future phases.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Path to the storage directory.
    pub storage_path: Option<std::path::PathBuf>,

    /// Whether to enable automatic persistence on updates.
    pub auto_persist: bool,

    /// Whether to compress stored data.
    pub compress: bool,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            storage_path: None,
            auto_persist: false,
            compress: false,
        }
    }
}

#[allow(dead_code)]
impl StorageConfig {
    /// Creates a storage config for in-memory only (no persistence).
    pub fn memory_only() -> Self {
        Self::default()
    }

    /// Creates a storage config with a specific path.
    pub fn with_path(path: std::path::PathBuf) -> Self {
        Self {
            storage_path: Some(path),
            auto_persist: true,
            compress: false,
        }
    }
}

/// Storage operations for timeline.
///
/// Phase 2A: Placeholder implementations that do nothing.
/// Future phases will implement actual persistence.
#[allow(dead_code)]
pub struct TimelineStorage {
    config: StorageConfig,
}

#[allow(dead_code)]
impl TimelineStorage {
    pub fn new(config: StorageConfig) -> Self {
        Self { config }
    }

    /// Loads a timeline from storage.
    ///
    /// Phase 2A: Returns empty timeline, persistence not yet implemented.
    pub fn load(&self) -> Result<ContextTimeline, std::io::Error> {
        Ok(ContextTimeline::new())
    }

    /// Saves a timeline to storage.
    ///
    /// Phase 2A: Does nothing, persistence not yet implemented.
    pub fn save(&self, _timeline: &ContextTimeline) -> Result<(), std::io::Error> {
        Ok(())
    }

    /// Returns the storage config.
    pub fn config(&self) -> &StorageConfig {
        &self.config
    }
}
