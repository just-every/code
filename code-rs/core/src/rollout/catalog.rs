//! Session catalog: unified index of all sessions for efficient querying and resume.

use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};

use code_protocol::models::{ContentItem, ResponseItem};
use code_protocol::protocol::{RolloutItem, RolloutLine, SessionSource};
use serde::{Deserialize, Serialize};
use tracing::warn;
use uuid::Uuid;

use super::SESSIONS_SUBDIR;

const INDEX_SUBDIR: &str = "sessions/index";
const CATALOG_FILENAME: &str = "catalog.jsonl";

/// Canonical entry in the session catalog index.
/// Each session has exactly one entry in the catalog, written as a single JSON line.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionIndexEntry {
    /// Unique session identifier
    pub session_id: Uuid,

    /// Path to the rollout JSONL file, relative to code_home
    pub rollout_path: PathBuf,

    /// Optional path to the snapshot JSON file, relative to code_home
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot_path: Option<PathBuf>,

    /// Timestamp when session was created (from SessionMeta.timestamp)
    pub created_at: String,

    /// Timestamp of the newest RolloutLine in the session
    pub last_event_at: String,

    /// Canonical absolute working directory path
    pub cwd_real: PathBuf,

    /// Display-friendly working directory path (may contain ~)
    pub cwd_display: String,

    /// Git project root directory (canonicalized)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_project_root: Option<PathBuf>,

    /// Git branch name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_branch: Option<String>,

    /// Model provider/name used in this session
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_provider: Option<String>,

    /// Session source (CLI, VSCode, Exec, MCP, etc.)
    pub session_source: SessionSource,

    /// Number of messages/turns in the session
    pub message_count: usize,

    /// Number of user messages in the session
    #[serde(default)]
    pub user_message_count: usize,

    /// Snippet from the last user message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_user_snippet: Option<String>,

    /// Device/machine where this session originated (for synced sessions)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sync_origin_device: Option<String>,

    /// Sync version/generation for conflict resolution
    #[serde(default)]
    pub sync_version: u64,

    /// Whether session is archived
    #[serde(default)]
    pub archived: bool,

    /// Whether session is marked as deleted
    #[serde(default)]
    pub deleted: bool,
}

impl SessionIndexEntry {
    /// Global ordering key for sessions: (last_event_at DESC, created_at DESC, session_id DESC)
    pub fn ordering_key(&self) -> (Reverse<String>, Reverse<String>, Reverse<Uuid>) {
        (
            Reverse(self.last_event_at.clone()),
            Reverse(self.created_at.clone()),
            Reverse(self.session_id),
        )
    }
}

/// In-memory session catalog with secondary indexes for efficient querying.
#[derive(Debug, Default, Clone)]
pub struct SessionCatalog {
    /// All entries indexed by session_id
    pub(crate) entries: HashMap<Uuid, SessionIndexEntry>,

    /// Secondary index: cwd_real -> list of session_ids
    by_cwd: HashMap<PathBuf, Vec<Uuid>>,

    /// Secondary index: git_project_root -> list of session_ids
    by_git_root: HashMap<PathBuf, Vec<Uuid>>,

    /// Path to the catalog file
    catalog_path: PathBuf,
}

impl SessionCatalog {
    /// Load the catalog from disk, or create empty if it doesn't exist.
    pub fn load(code_home: &Path) -> io::Result<Self> {
        let catalog_path = code_home.join(INDEX_SUBDIR).join(CATALOG_FILENAME);

        let mut catalog = Self {
            entries: HashMap::new(),
            by_cwd: HashMap::new(),
            by_git_root: HashMap::new(),
            catalog_path: catalog_path.clone(),
        };

        if !catalog_path.exists() {
            return Ok(catalog);
        }

        let file = fs::File::open(&catalog_path)?;
        let reader = BufReader::new(file);

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }

            match serde_json::from_str::<SessionIndexEntry>(&line) {
                Ok(entry) => {
                    catalog.index_entry(entry);
                }
                Err(e) => {
                    warn!("Failed to parse catalog entry: {}", e);
                }
            }
        }

        Ok(catalog)
    }

    /// Add or update an entry in the catalog.
    fn index_entry(&mut self, entry: SessionIndexEntry) {
        let session_id = entry.session_id;

        // Add to cwd index
        self.by_cwd
            .entry(entry.cwd_real.clone())
            .or_default()
            .push(session_id);

        // Add to git root index
        if let Some(ref git_root) = entry.git_project_root {
            self.by_git_root
                .entry(git_root.clone())
                .or_default()
                .push(session_id);
        }

        // Add to main index
        self.entries.insert(session_id, entry);
    }

    /// Save the entire catalog to disk, overwriting the existing file.
    pub fn save(&self) -> io::Result<()> {
        // Ensure the index directory exists
        if let Some(parent) = self.catalog_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Sort entries by ordering key for deterministic output
        let mut entries: Vec<&SessionIndexEntry> = self.entries.values().collect();
        entries.sort_by_key(|e| e.ordering_key());

        // Write all entries
        let mut lines = Vec::new();
        for entry in entries {
            match serde_json::to_string(entry) {
                Ok(json) => lines.push(json),
                Err(e) => warn!("Failed to serialize catalog entry: {}", e),
            }
        }

        fs::write(&self.catalog_path, lines.join("\n") + "\n")?;
        Ok(())
    }

    /// Get an entry by session ID.
    #[allow(dead_code)]
    pub fn get(&self, session_id: &Uuid) -> Option<&SessionIndexEntry> {
        self.entries.get(session_id)
    }

    /// Get all entries sorted by global ordering.
    pub fn all_ordered(&self) -> Vec<&SessionIndexEntry> {
        let mut entries: Vec<&SessionIndexEntry> = self.entries.values().collect();
        entries.sort_by_key(|e| e.ordering_key());
        entries
    }

    /// Get sessions for a specific working directory, sorted by ordering.
    #[allow(dead_code)]
    pub fn by_cwd(&self, cwd: &Path) -> Vec<&SessionIndexEntry> {
        let session_ids = match self.by_cwd.get(cwd) {
            Some(ids) => ids,
            None => return Vec::new(),
        };

        let mut entries: Vec<&SessionIndexEntry> = session_ids
            .iter()
            .filter_map(|id| self.entries.get(id))
            .collect();

        entries.sort_by_key(|e| e.ordering_key());
        entries
    }

    /// Get sessions for a specific git project root, sorted by ordering.
    #[allow(dead_code)]
    pub fn by_git_root(&self, git_root: &Path) -> Vec<&SessionIndexEntry> {
        let session_ids = match self.by_git_root.get(git_root) {
            Some(ids) => ids,
            None => return Vec::new(),
        };

        let mut entries: Vec<&SessionIndexEntry> = session_ids
            .iter()
            .filter_map(|id| self.entries.get(id))
            .collect();

        entries.sort_by_key(|e| e.ordering_key());
        entries
    }

    /// Update or insert an entry, then save to disk.
    pub fn upsert(&mut self, entry: SessionIndexEntry) -> io::Result<()> {
        // Remove old secondary index entries if updating
        let session_id = entry.session_id;
        if let Some(old_entry) = self.entries.get(&session_id).cloned() {
            self.remove_from_indexes(&session_id, &old_entry);
        }

        self.index_entry(entry);
        self.save()
    }

    /// Remove an entry's session_id from secondary indexes.
    fn remove_from_indexes(&mut self, session_id: &Uuid, entry: &SessionIndexEntry) {
        // Remove from cwd index
        if let Some(ids) = self.by_cwd.get_mut(&entry.cwd_real) {
            ids.retain(|id| id != session_id);
        }

        // Remove from git root index
        if let Some(ref git_root) = entry.git_project_root {
            if let Some(ids) = self.by_git_root.get_mut(git_root) {
                ids.retain(|id| id != session_id);
            }
        }
    }

    /// Remove an entry by session ID.
    #[allow(dead_code)]
    pub fn remove(&mut self, session_id: &Uuid) -> io::Result<()> {
        if let Some(entry) = self.entries.remove(session_id) {
            self.remove_from_indexes(session_id, &entry);
            self.save()?;
        }
        Ok(())
    }

    /// Reconcile the catalog against actual rollout files on disk.
    /// This scans the sessions directory and updates/adds entries for any files
    /// that are newer or missing from the catalog.
    #[allow(dead_code)]
    pub async fn reconcile(&mut self, code_home: &Path) -> io::Result<ReconcileResult> {
        let sessions_root = code_home.join(SESSIONS_SUBDIR);

        if !sessions_root.exists() {
            return Ok(ReconcileResult::default());
        }

        let mut result = ReconcileResult::default();
        let discovered_entries = scan_rollout_files(&sessions_root).await?;
        let discovered_ids: HashSet<Uuid> = discovered_entries.keys().copied().collect();
        let mut changed = false;

        // Remove entries that no longer exist on disk.
        let existing_ids: Vec<Uuid> = self.entries.keys().copied().collect();
        for session_id in existing_ids {
            if !discovered_ids.contains(&session_id) {
                if let Some(entry) = self.entries.remove(&session_id) {
                    self.remove_from_indexes(&session_id, &entry);
                    result.removed += 1;
                    changed = true;
                }
            }
        }

        // Upsert discovered entries.
        for (session_id, entry) in discovered_entries {
            if let Some(existing) = self.entries.get(&session_id).cloned() {
                if should_replace(&existing, &entry) {
                    self.remove_from_indexes(&session_id, &existing);
                    self.index_entry(entry);
                    result.updated += 1;
                    changed = true;
                }
            } else {
                self.index_entry(entry);
                result.added += 1;
                changed = true;
            }
        }

        if changed {
            self.save()?;
        }

        Ok(result)
    }

    /// Get the full absolute path to a session's rollout file.
    #[allow(dead_code)]
    pub fn resolve_rollout_path(&self, code_home: &Path, session_id: &Uuid) -> Option<PathBuf> {
        self.entries
            .get(session_id)
            .map(|entry| code_home.join(&entry.rollout_path))
    }

    /// Get the full absolute path to a session's snapshot file.
    #[allow(dead_code)]
    pub fn resolve_snapshot_path(&self, code_home: &Path, session_id: &Uuid) -> Option<PathBuf> {
        self.entries.get(session_id).and_then(|entry| {
            entry
                .snapshot_path
                .as_ref()
                .map(|p| code_home.join(p))
        })
    }
}

/// Result of reconciling the catalog against filesystem.
#[derive(Debug, Default)]
#[allow(dead_code)]
pub struct ReconcileResult {
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
}

/// Scan all rollout files under the sessions directory and build index entries.
#[allow(dead_code)]
async fn scan_rollout_files(
    sessions_root: &Path,
) -> io::Result<HashMap<Uuid, SessionIndexEntry>> {
    use tokio::fs;

    let mut queue = vec![sessions_root.to_path_buf()];
    let mut discovered = HashMap::new();

    while let Some(dir) = queue.pop() {
        let mut read_dir = fs::read_dir(&dir).await?;

        while let Some(entry) = read_dir.next_entry().await? {
            let path = entry.path();
            let metadata = entry.metadata().await?;

            if metadata.is_dir() {
                queue.push(path);
            } else if metadata.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.ends_with(".jsonl") && name.starts_with("rollout-") {
                        if let Some(index_entry) = parse_rollout_file(&path, sessions_root).await {
                            match discovered.get(&index_entry.session_id) {
                                Some(existing) => {
                                    if should_replace(existing, &index_entry) {
                                        discovered.insert(index_entry.session_id, index_entry);
                                    }
                                }
                                None => {
                                    discovered.insert(index_entry.session_id, index_entry);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(discovered)
}

/// Parse a rollout file and extract catalog entry information.
async fn parse_rollout_file(
    path: &Path,
    sessions_root: &Path,
) -> Option<SessionIndexEntry> {
    use tokio::io::{AsyncBufReadExt, BufReader};

    // Read the file
    let file = match tokio::fs::File::open(path).await {
        Ok(f) => f,
        Err(e) => {
            warn!("Failed to open rollout file {:?}: {}", path, e);
            return None;
        }
    };

    let mut reader = BufReader::new(file).lines();

    let mut session_id: Option<Uuid> = None;
    let mut created_at: Option<String> = None;
    let mut last_event_at: Option<String> = None;
    let mut cwd_real: Option<PathBuf> = None;
    let mut git_branch: Option<String> = None;
    let mut git_project_root: Option<PathBuf> = None;
    let mut session_source: Option<SessionSource> = None;
    let mut message_count = 0usize;
    let mut user_message_count = 0usize;
    let mut last_user_snippet: Option<String> = None;

    // Parse the file line by line
    while let Some(line) = reader.next_line().await.ok().flatten() {
        if line.trim().is_empty() {
            continue;
        }

        let rollout_line: RolloutLine = match serde_json::from_str(&line) {
            Ok(rl) => rl,
            Err(_) => continue,
        };

        // Update last_event_at with every line
        last_event_at = Some(rollout_line.timestamp.clone());

        match rollout_line.item {
            RolloutItem::SessionMeta(meta_line) => {
                session_id = Some(Uuid::from(meta_line.meta.id));
                created_at = Some(rollout_line.timestamp.clone());
                cwd_real = Some(meta_line.meta.cwd.clone());
                session_source = Some(meta_line.meta.source);

                if let Some(git_info) = meta_line.git {
                    git_branch = git_info.branch;
                    // Try to derive git project root from cwd and repository structure
                    // For now, just use cwd as approximation
                    git_project_root = Some(meta_line.meta.cwd.clone());
                }
            }
            RolloutItem::ResponseItem(response_item) => {
                message_count += 1;

                if let ResponseItem::Message { role, content, .. } = response_item {
                    if role.eq_ignore_ascii_case("user") {
                        let snippet = snippet_from_content(&content);
                        if snippet
                            .as_deref()
                            .map_or(false, is_system_status_snippet)
                        {
                            continue;
                        }

                        user_message_count += 1;
                        if let Some(snippet) = snippet {
                            last_user_snippet = Some(snippet);
                        }
                    }
                }
            }
            RolloutItem::Event(_event) => {
                // Event lines record internal state changes (tool output, approvals, etc.).
                // They are not a reliable indicator of user-submitted turns, so avoid
                // counting them toward `user_message_count` to keep resume filters strict.
                message_count += 1;
            }
            RolloutItem::Compacted(_) | RolloutItem::TurnContext(_) => {
                message_count += 1;
            }
        }
    }

    // Build the entry
    let session_id = session_id?;
    let created_at = created_at?;
    let last_event_at = last_event_at.unwrap_or_else(|| created_at.clone());
    let cwd_real = cwd_real?;
    let session_source = session_source?;

    // Make rollout_path relative to sessions_root's parent (code_home)
    let code_home = sessions_root.parent()?;
    let rollout_path = path.strip_prefix(code_home).ok()?.to_path_buf();

    // Check for snapshot file
    let snapshot_path = {
        let snapshot_file = path.with_extension("snapshot.json");
        if snapshot_file.exists() {
            snapshot_file
                .strip_prefix(code_home)
                .ok()
                .map(|p| p.to_path_buf())
        } else {
            None
        }
    };

    // Create display-friendly cwd (can be enhanced later with ~ substitution)
    let cwd_display = cwd_real.to_string_lossy().to_string();

    Some(SessionIndexEntry {
        session_id,
        rollout_path,
        snapshot_path,
        created_at,
        last_event_at,
        cwd_real,
        cwd_display,
        git_project_root,
        git_branch,
        model_provider: None, // TODO: extract from session if available
        session_source,
        message_count,
        user_message_count,
        last_user_snippet,
        sync_origin_device: None,
        sync_version: 0,
        archived: false,
        deleted: false,
    })
}

fn should_replace(existing: &SessionIndexEntry, candidate: &SessionIndexEntry) -> bool {
    if existing.last_event_at != candidate.last_event_at {
        return existing.last_event_at < candidate.last_event_at;
    }

    if existing.created_at != candidate.created_at {
        return existing.created_at < candidate.created_at;
    }

    existing.rollout_path != candidate.rollout_path
        || existing.snapshot_path != candidate.snapshot_path
        || existing.cwd_real != candidate.cwd_real
        || existing.session_source != candidate.session_source
        || existing.git_branch != candidate.git_branch
}

/// Update catalog entry for a session after new events are written.
/// This is called by RolloutRecorder after flushing events.
pub async fn update_catalog_entry(
    code_home: &Path,
    rollout_path: &Path,
    session_id: Uuid,
    last_timestamp: &str,
) -> io::Result<()> {
    let mut catalog = SessionCatalog::load(code_home)?;

    // If entry exists, update last_event_at
        if let Some(mut entry) = catalog.entries.remove(&session_id) {
            entry.last_event_at = last_timestamp.to_string();

            // Re-parse file to update message count and snippet
            if let Some(updated) = parse_rollout_file(rollout_path, &code_home.join(SESSIONS_SUBDIR)).await {
                entry.message_count = updated.message_count;
                entry.user_message_count = updated.user_message_count;
                entry.last_user_snippet = updated.last_user_snippet;
            }

            catalog.upsert(entry)?;
        } else {
            // Entry doesn't exist, parse and add it
            if let Some(entry) = parse_rollout_file(rollout_path, &code_home.join(SESSIONS_SUBDIR)).await {
                catalog.upsert(entry)?;
            }
    }

    Ok(())
}

fn snippet_from_content(content: &[ContentItem]) -> Option<String> {
    content.iter().find_map(|item| match item {
        ContentItem::InputText { text }
        | ContentItem::OutputText { text } => Some(truncate_snippet(text)),
        _ => None,
    })
}

fn truncate_snippet(text: &str) -> String {
    text.chars().take(100).collect()
}

fn is_system_status_snippet(text: &str) -> bool {
    text.starts_with("== System Status ==")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_session_index_entry_ordering() {
        let entry1 = SessionIndexEntry {
            session_id: Uuid::new_v4(),
            rollout_path: PathBuf::from("test1.jsonl"),
            snapshot_path: None,
            created_at: "2025-01-01T10:00:00.000Z".to_string(),
            last_event_at: "2025-01-01T10:05:00.000Z".to_string(),
            cwd_real: PathBuf::from("/test"),
            cwd_display: "/test".to_string(),
            git_project_root: None,
            git_branch: None,
            model_provider: None,
            session_source: SessionSource::Cli,
            message_count: 5,
            user_message_count: 2,
            last_user_snippet: None,
            sync_origin_device: None,
            sync_version: 0,
            archived: false,
            deleted: false,
        };

        let entry2 = SessionIndexEntry {
            last_event_at: "2025-01-01T10:10:00.000Z".to_string(),
            ..entry1.clone()
        };

        // entry2 should come before entry1 (newer last_event_at)
        assert!(entry2.ordering_key() < entry1.ordering_key());
    }

    #[tokio::test]
    async fn test_catalog_load_save() -> io::Result<()> {
        let temp = TempDir::new()?;
        let code_home = temp.path();

        let entry = SessionIndexEntry {
            session_id: Uuid::new_v4(),
            rollout_path: PathBuf::from("sessions/2025/01/01/rollout-test.jsonl"),
            snapshot_path: None,
            created_at: "2025-01-01T10:00:00.000Z".to_string(),
            last_event_at: "2025-01-01T10:05:00.000Z".to_string(),
            cwd_real: PathBuf::from("/test"),
            cwd_display: "/test".to_string(),
            git_project_root: None,
            git_branch: None,
            model_provider: None,
            session_source: SessionSource::Cli,
            message_count: 5,
            user_message_count: 1,
            last_user_snippet: Some("test message".to_string()),
            sync_origin_device: None,
            sync_version: 0,
            archived: false,
            deleted: false,
        };

        // Create and save catalog
        let mut catalog = SessionCatalog::load(code_home)?;
        catalog.upsert(entry.clone())?;

        // Load catalog and verify
        let loaded = SessionCatalog::load(code_home)?;
        assert_eq!(loaded.get(&entry.session_id), Some(&entry));

        Ok(())
    }

    #[tokio::test]
    async fn test_catalog_indexes() -> io::Result<()> {
        let temp = TempDir::new()?;
        let code_home = temp.path();

        let cwd = PathBuf::from("/test/project");
        let git_root = PathBuf::from("/test");

        let entry1 = SessionIndexEntry {
            session_id: Uuid::new_v4(),
            rollout_path: PathBuf::from("sessions/test1.jsonl"),
            snapshot_path: None,
            created_at: "2025-01-01T10:00:00.000Z".to_string(),
            last_event_at: "2025-01-01T10:05:00.000Z".to_string(),
            cwd_real: cwd.clone(),
            cwd_display: cwd.to_string_lossy().to_string(),
            git_project_root: Some(git_root.clone()),
            git_branch: Some("main".to_string()),
            model_provider: None,
            session_source: SessionSource::Cli,
            message_count: 5,
            user_message_count: 1,
            last_user_snippet: None,
            sync_origin_device: None,
            sync_version: 0,
            archived: false,
            deleted: false,
        };

        let entry2 = SessionIndexEntry {
            session_id: Uuid::new_v4(),
            rollout_path: PathBuf::from("sessions/test2.jsonl"),
            last_event_at: "2025-01-01T10:10:00.000Z".to_string(),
            ..entry1.clone()
        };

        let mut catalog = SessionCatalog::load(code_home)?;
        catalog.upsert(entry1.clone())?;
        catalog.upsert(entry2.clone())?;

        // Test by_cwd index
        let cwd_sessions = catalog.by_cwd(&cwd);
        assert_eq!(cwd_sessions.len(), 2);
        assert_eq!(cwd_sessions[0].session_id, entry2.session_id); // Newer first

        // Test by_git_root index
        let git_sessions = catalog.by_git_root(&git_root);
        assert_eq!(git_sessions.len(), 2);

        Ok(())
    }

    #[tokio::test]
    async fn test_catalog_upsert_updates_existing() -> io::Result<()> {
        let temp = TempDir::new()?;
        let code_home = temp.path();

        let session_id = Uuid::new_v4();
        let entry1 = SessionIndexEntry {
            session_id,
            rollout_path: PathBuf::from("sessions/test.jsonl"),
            snapshot_path: None,
            created_at: "2025-01-01T10:00:00.000Z".to_string(),
            last_event_at: "2025-01-01T10:05:00.000Z".to_string(),
            cwd_real: PathBuf::from("/test"),
            cwd_display: "/test".to_string(),
            git_project_root: None,
            git_branch: None,
            model_provider: None,
            session_source: SessionSource::Cli,
            message_count: 5,
            user_message_count: 2,
            last_user_snippet: Some("first message".to_string()),
            sync_origin_device: None,
            sync_version: 0,
            archived: false,
            deleted: false,
        };

        let mut catalog = SessionCatalog::load(code_home)?;
        catalog.upsert(entry1.clone())?;

        // Update the entry
        let entry2 = SessionIndexEntry {
            last_event_at: "2025-01-01T10:15:00.000Z".to_string(),
            message_count: 10,
            last_user_snippet: Some("updated message".to_string()),
            ..entry1.clone()
        };

        catalog.upsert(entry2.clone())?;

        // Verify update
        let loaded = SessionCatalog::load(code_home)?;
        let retrieved = loaded.get(&session_id).unwrap();
        assert_eq!(retrieved.last_event_at, "2025-01-01T10:15:00.000Z");
        assert_eq!(retrieved.message_count, 10);
        assert_eq!(retrieved.last_user_snippet, Some("updated message".to_string()));

        Ok(())
    }

    #[tokio::test]
    async fn test_catalog_remove() -> io::Result<()> {
        let temp = TempDir::new()?;
        let code_home = temp.path();

        let entry = SessionIndexEntry {
            session_id: Uuid::new_v4(),
            rollout_path: PathBuf::from("sessions/test.jsonl"),
            snapshot_path: None,
            created_at: "2025-01-01T10:00:00.000Z".to_string(),
            last_event_at: "2025-01-01T10:05:00.000Z".to_string(),
            cwd_real: PathBuf::from("/test"),
            cwd_display: "/test".to_string(),
            git_project_root: None,
            git_branch: None,
            model_provider: None,
            session_source: SessionSource::Cli,
            message_count: 5,
            user_message_count: 1,
            last_user_snippet: None,
            sync_origin_device: None,
            sync_version: 0,
            archived: false,
            deleted: false,
        };

        let mut catalog = SessionCatalog::load(code_home)?;
        catalog.upsert(entry.clone())?;

        // Verify it exists
        assert!(catalog.get(&entry.session_id).is_some());

        // Remove it
        catalog.remove(&entry.session_id)?;

        // Verify it's gone
        let loaded = SessionCatalog::load(code_home)?;
        assert!(loaded.get(&entry.session_id).is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_catalog_ordering_multi_criteria() -> io::Result<()> {
        let temp = TempDir::new()?;
        let code_home = temp.path();

        // Create entries with different timestamps
        let entry1 = SessionIndexEntry {
            session_id: Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
            rollout_path: PathBuf::from("sessions/test1.jsonl"),
            snapshot_path: None,
            created_at: "2025-01-01T10:00:00.000Z".to_string(),
            last_event_at: "2025-01-01T10:05:00.000Z".to_string(),
            cwd_real: PathBuf::from("/test"),
            cwd_display: "/test".to_string(),
            git_project_root: None,
            git_branch: None,
            model_provider: None,
            session_source: SessionSource::Cli,
            message_count: 5,
            user_message_count: 2,
            last_user_snippet: None,
            sync_origin_device: None,
            sync_version: 0,
            archived: false,
            deleted: false,
        };

        let entry2 = SessionIndexEntry {
            session_id: Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap(),
            rollout_path: PathBuf::from("sessions/test2.jsonl"),
            last_event_at: "2025-01-01T10:10:00.000Z".to_string(), // Newer
            ..entry1.clone()
        };

        let entry3 = SessionIndexEntry {
            session_id: Uuid::parse_str("00000000-0000-0000-0000-000000000003").unwrap(),
            rollout_path: PathBuf::from("sessions/test3.jsonl"),
            last_event_at: "2025-01-01T10:10:00.000Z".to_string(), // Same as entry2
            created_at: "2025-01-01T10:01:00.000Z".to_string(), // Later created_at
            ..entry1.clone()
        };

        let mut catalog = SessionCatalog::load(code_home)?;
        catalog.upsert(entry1.clone())?;
        catalog.upsert(entry2.clone())?;
        catalog.upsert(entry3.clone())?;

        // Get all ordered
        let all = catalog.all_ordered();
        assert_eq!(all.len(), 3);

        // entry3 should be first (latest last_event_at, and if tied, latest created_at)
        // entry2 should be second (same last_event_at as entry3 but earlier created_at)
        // entry1 should be last (earliest last_event_at)
        assert_eq!(all[0].session_id, entry3.session_id);
        assert_eq!(all[1].session_id, entry2.session_id);
        assert_eq!(all[2].session_id, entry1.session_id);

        Ok(())
    }

    #[tokio::test]
    async fn test_catalog_resolve_paths() -> io::Result<()> {
        let temp = TempDir::new()?;
        let code_home = temp.path();

        let entry = SessionIndexEntry {
            session_id: Uuid::new_v4(),
            rollout_path: PathBuf::from("sessions/2025/01/01/rollout-test.jsonl"),
            snapshot_path: Some(PathBuf::from("sessions/2025/01/01/rollout-test.snapshot.json")),
            created_at: "2025-01-01T10:00:00.000Z".to_string(),
            last_event_at: "2025-01-01T10:05:00.000Z".to_string(),
            cwd_real: PathBuf::from("/test"),
            cwd_display: "/test".to_string(),
            git_project_root: None,
            git_branch: None,
            model_provider: None,
            session_source: SessionSource::Cli,
            message_count: 5,
            user_message_count: 2,
            last_user_snippet: None,
            sync_origin_device: None,
            sync_version: 0,
            archived: false,
            deleted: false,
        };

        let mut catalog = SessionCatalog::load(code_home)?;
        catalog.upsert(entry.clone())?;

        // Test resolve paths
        let rollout_path = catalog.resolve_rollout_path(code_home, &entry.session_id);
        assert_eq!(
            rollout_path,
            Some(code_home.join("sessions/2025/01/01/rollout-test.jsonl"))
        );

        let snapshot_path = catalog.resolve_snapshot_path(code_home, &entry.session_id);
        assert_eq!(
            snapshot_path,
            Some(code_home.join("sessions/2025/01/01/rollout-test.snapshot.json"))
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_catalog_reconcile_empty_directory() -> io::Result<()> {
        let temp = TempDir::new()?;
        let code_home = temp.path();

        let mut catalog = SessionCatalog::load(code_home)?;
        let result = catalog.reconcile(code_home).await?;

        assert_eq!(result.added, 0);
        assert_eq!(result.updated, 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_catalog_by_cwd_empty() -> io::Result<()> {
        let temp = TempDir::new()?;
        let code_home = temp.path();

        let catalog = SessionCatalog::load(code_home)?;
        let sessions = catalog.by_cwd(&PathBuf::from("/nonexistent"));

        assert_eq!(sessions.len(), 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_catalog_secondary_index_updates() -> io::Result<()> {
        let temp = TempDir::new()?;
        let code_home = temp.path();

        let session_id = Uuid::new_v4();
        let cwd1 = PathBuf::from("/test1");
        let cwd2 = PathBuf::from("/test2");

        // Create entry with cwd1
        let entry1 = SessionIndexEntry {
            session_id,
            rollout_path: PathBuf::from("sessions/test.jsonl"),
            snapshot_path: None,
            created_at: "2025-01-01T10:00:00.000Z".to_string(),
            last_event_at: "2025-01-01T10:05:00.000Z".to_string(),
            cwd_real: cwd1.clone(),
            cwd_display: cwd1.to_string_lossy().to_string(),
            git_project_root: None,
            git_branch: None,
            model_provider: None,
            session_source: SessionSource::Cli,
            message_count: 5,
            user_message_count: 2,
            last_user_snippet: None,
            sync_origin_device: None,
            sync_version: 0,
            archived: false,
            deleted: false,
        };

        let mut catalog = SessionCatalog::load(code_home)?;
        catalog.upsert(entry1.clone())?;

        // Verify it's in cwd1 index
        assert_eq!(catalog.by_cwd(&cwd1).len(), 1);
        assert_eq!(catalog.by_cwd(&cwd2).len(), 0);

        // Update to cwd2
        let entry2 = SessionIndexEntry {
            cwd_real: cwd2.clone(),
            cwd_display: cwd2.to_string_lossy().to_string(),
            ..entry1.clone()
        };

        catalog.upsert(entry2)?;

        // Verify indexes updated
        assert_eq!(catalog.by_cwd(&cwd1).len(), 0);
        assert_eq!(catalog.by_cwd(&cwd2).len(), 1);

        Ok(())
    }
}
