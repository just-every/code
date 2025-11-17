//! Async-friendly wrapper around the rollout session catalog.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use code_protocol::protocol::SessionSource;
use once_cell::sync::OnceCell;
use tokio::task;
use tokio::sync::Mutex as AsyncMutex;

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
    /// Minimum number of user messages required.
    pub min_user_messages: usize,
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
    cache: Arc<AsyncMutex<Option<rollout_catalog::SessionCatalog>>>,
}

impl SessionCatalog {
    /// Create a catalog facade for the provided code home directory.
    pub fn new(code_home: PathBuf) -> Self {
        let cache = catalog_cache_handle(&code_home);
        Self { code_home, cache }
    }

    /// Query the catalog with the provided filters, returning ordered entries.
    pub async fn query(&self, query: &SessionQuery) -> Result<Vec<SessionIndexEntry>> {
        let catalog = self.load_inner().await?;
        let mut rows = Vec::new();

        let candidates: Vec<&SessionIndexEntry> = if let Some(cwd) = &query.cwd {
            catalog.by_cwd(cwd)
        } else if let Some(git_root) = &query.git_root {
            catalog.by_git_root(git_root)
        } else {
            catalog.all_ordered()
        };

        for entry in candidates {
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
            if entry.user_message_count < query.min_user_messages {
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
        {
            let mut guard = self.cache.lock().await;
            if let Some(existing) = guard.as_mut() {
                existing
                    .reconcile(&self.code_home)
                    .await
                    .context("failed to reconcile session catalog")?;
                return Ok(existing.clone());
            }
        }

        let code_home = self.code_home.clone();
        let mut catalog = task::spawn_blocking(move || rollout_catalog::SessionCatalog::load(&code_home))
            .await
            .context("catalog task panicked")?
            .context("failed to load session catalog")?;

        catalog
            .reconcile(&self.code_home)
            .await
            .context("failed to reconcile session catalog")?;

        let mut guard = self.cache.lock().await;
        *guard = Some(catalog.clone());
        Ok(catalog)
    }
}

/// Helper to convert an entry to an absolute rollout path.
pub fn entry_to_rollout_path(code_home: &Path, entry: &SessionIndexEntry) -> PathBuf {
    code_home.join(&entry.rollout_path)
}

type SharedCatalog = Arc<AsyncMutex<Option<rollout_catalog::SessionCatalog>>>;

fn catalog_cache_handle(code_home: &Path) -> SharedCatalog {
    static CACHE: OnceCell<Mutex<HashMap<PathBuf, SharedCatalog>>> = OnceCell::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache.lock().expect("session catalog cache poisoned");
    guard
        .entry(code_home.to_path_buf())
        .or_insert_with(|| Arc::new(AsyncMutex::new(None)))
        .clone()
}
