//! Async-friendly wrapper around the rollout session catalog.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use code_protocol::protocol::SessionSource;
use once_cell::sync::OnceCell;
use tokio::task;
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

use crate::rollout::catalog::{self as rollout_catalog, SessionIndexEntry};
use crate::rollout::{ARCHIVED_SESSIONS_SUBDIR, SESSIONS_SUBDIR};

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

    /// Set or clear a nickname for the given session.
    pub async fn set_nickname(&self, session_id: Uuid, nickname: Option<String>) -> Result<bool> {
        let mut catalog = self.load_inner().await?;
        let updated = catalog
            .set_nickname(session_id, nickname)
            .context("failed to update session nickname")?;
        if updated {
            let mut guard = self.cache.lock().await;
            *guard = Some(catalog);
        }
        Ok(updated)
    }

    /// Archive a session by moving its rollout (and optional snapshot) under
    /// `archived_sessions/` and marking the catalog entry archived.
    pub async fn archive_conversation(
        &self,
        session_id: Uuid,
        rollout_path: &Path,
    ) -> Result<bool> {
        if !rollout_path.exists() {
            return Ok(false);
        }

        let code_home = &self.code_home;
        let rel = rollout_path
            .strip_prefix(code_home)
            .context("rollout_path must be under code_home")?;

        let expected_id = rel
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|stem| stem.rsplit_once('-').map(|(_, id)| id.to_string()));
        if expected_id
            .as_deref()
            .is_some_and(|id| !session_id.to_string().eq_ignore_ascii_case(id))
        {
            anyhow::bail!("conversation_id does not match rollout_path");
        }

        let sessions_prefix = Path::new(SESSIONS_SUBDIR);
        let archived_prefix = Path::new(ARCHIVED_SESSIONS_SUBDIR);

        let already_archived = rel.starts_with(archived_prefix);
        let suffix = if rel.starts_with(sessions_prefix) {
            rel.strip_prefix(sessions_prefix)
                .context("failed to strip sessions prefix")?
        } else if already_archived {
            rel.strip_prefix(archived_prefix)
                .context("failed to strip archived prefix")?
        } else {
            anyhow::bail!("rollout_path must be under sessions/ or archived_sessions/");
        };

        let new_rel = archived_prefix.join(suffix);
        let new_abs = code_home.join(&new_rel);

        if !already_archived {
            if let Some(parent) = new_abs.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .context("failed to create archive directory")?;
            }

            move_file(rollout_path, &new_abs)
                .await
                .context("failed to move rollout file")?;

            let snapshot_old = rollout_path.with_extension("snapshot.json");
            if snapshot_old.exists() {
                let snapshot_new = new_abs.with_extension("snapshot.json");
                if let Some(parent) = snapshot_new.parent() {
                    tokio::fs::create_dir_all(parent)
                        .await
                        .context("failed to create archive directory")?;
                }
                move_file(&snapshot_old, &snapshot_new)
                    .await
                    .context("failed to move snapshot file")?;
            }
        }

        // Update catalog entry.
        let mut catalog = self.load_inner().await?;
        let Some(mut entry) = catalog.entries.get(&session_id).cloned() else {
            return Ok(false);
        };

        entry.archived = true;
        entry.rollout_path = new_rel;
        let snapshot_new_abs = new_abs.with_extension("snapshot.json");
        entry.snapshot_path = snapshot_new_abs
            .exists()
            .then(|| {
                snapshot_new_abs
                    .strip_prefix(code_home)
                    .ok()
                    .map(|p| p.to_path_buf())
            })
            .flatten();

        catalog
            .upsert(entry)
            .context("failed to persist updated catalog entry")?;

        let mut guard = self.cache.lock().await;
        *guard = Some(catalog);
        Ok(true)
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

async fn move_file(old: &Path, new: &Path) -> Result<()> {
    match tokio::fs::rename(old, new).await {
        Ok(()) => Ok(()),
        Err(_) => {
            tokio::fs::copy(old, new)
                .await
                .context("copy failed")?;
            tokio::fs::remove_file(old)
                .await
                .context("remove failed")?;
            Ok(())
        }
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
