//! Async-friendly wrapper around the rollout session catalog.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use code_protocol::protocol::SessionSource;
use tokio::task;

use crate::rollout::catalog::{self as rollout_catalog, SessionIndexEntry};

/// Query parameters for catalog lookups.
#[derive(Debug, Clone, Default)]
pub struct SessionQuery {
    /// Filter by canonical working directory (exact match).
    pub cwd: Option<PathBuf>,
    /// Filter by git project root.
    pub git_root: Option<PathBuf>,
    /// Restrict to these sources; empty = all sources.
    pub sources: Vec<SessionSource>,
    /// Include archived sessions.
    pub include_archived: bool,
    /// Include deleted sessions.
    pub include_deleted: bool,
    /// Maximum number of rows to return.
    pub limit: Option<usize>,
}

/// Public catalog facade used by TUI/CLI/Exec entrypoints.
pub struct SessionCatalog {
    code_home: PathBuf,
}

impl SessionCatalog {
    /// Create a catalog facade for the provided code home directory.
    pub fn new(code_home: PathBuf) -> Self {
        Self { code_home }
    }

    /// Query the catalog with the provided filters, returning ordered entries.
    pub async fn query(&self, query: &SessionQuery) -> Result<Vec<SessionIndexEntry>> {
        let catalog = self.load_inner().await?;
        let mut rows = Vec::new();

        for entry in catalog.all_ordered() {
            if !query.include_archived && entry.archived {
                continue;
            }
            if !query.include_deleted && entry.deleted {
                continue;
            }
            if let Some(cwd) = &query.cwd {
                if &entry.cwd_real != cwd {
                    continue;
                }
            }
            if let Some(git_root) = &query.git_root {
                if entry.git_project_root.as_ref() != Some(git_root) {
                    continue;
                }
            }
            if !query.sources.is_empty() && !query.sources.contains(&entry.session_source) {
                continue;
            }

            rows.push(entry.clone());

            if let Some(limit) = query.limit {
                if rows.len() >= limit {
                    break;
                }
            }
        }

        Ok(rows)
    }

    /// Find a session by UUID (prefix matches allowed, case-insensitive).
    pub async fn find_by_id(&self, id_prefix: &str) -> Result<Option<SessionIndexEntry>> {
        let catalog = self.load_inner().await?;
        let needle = id_prefix.to_ascii_lowercase();

        let entry = catalog
            .all_ordered()
            .into_iter()
            .find(|entry| {
                entry
                    .session_id
                    .to_string()
                    .to_ascii_lowercase()
                    .starts_with(&needle)
            })
            .cloned();

        Ok(entry)
    }

    /// Return the newest session matching the query.
    pub async fn get_latest(&self, query: &SessionQuery) -> Result<Option<SessionIndexEntry>> {
        let mut limited = query.clone();
        limited.limit = Some(1);
        let mut rows = self.query(&limited).await?;
        Ok(rows.pop())
    }

    /// Convert a catalog entry to an absolute rollout path.
    pub fn entry_rollout_path(&self, entry: &SessionIndexEntry) -> PathBuf {
        entry_to_rollout_path(&self.code_home, entry)
    }

    async fn load_inner(&self) -> Result<rollout_catalog::SessionCatalog> {
        let code_home = self.code_home.clone();
        let mut catalog = task::spawn_blocking(move || rollout_catalog::SessionCatalog::load(&code_home))
            .await
            .context("catalog task panicked")?
            .context("failed to load session catalog")?;

        catalog
            .reconcile(&self.code_home)
            .await
            .context("failed to reconcile session catalog")?;

        Ok(catalog)
    }
}

/// Helper to convert an entry to an absolute rollout path.
pub fn entry_to_rollout_path(code_home: &Path, entry: &SessionIndexEntry) -> PathBuf {
    code_home.join(&entry.rollout_path)
}
