//! Local-memory utility types and subprocess helpers
//!
//! DEPRECATION STATUS (2025-10-18):
//! - ✅ Types (LocalMemorySearchResult, etc.): KEEP - used by MCP response parsing
//! - ⚠️ Functions (search_by_stage, run_local_memory_search): DEPRECATED - subprocess calls
//!
//! REMAINING SUBPROCESS CALLS:
//! - spec_prompts.rs:411 (gather_local_memory_context) - Used for agent prompt context
//! - handler.rs:1386 (GPT-5 validation checking) - Used for quality gate results
//!
//! TODO: Migrate remaining calls to native MCP (ARCH-004 extension or new task)

use std::{env, process::Command};

use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct LocalMemorySearchResponse {
    pub success: bool,
    #[serde(default)]
    pub data: Option<LocalMemorySearchData>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LocalMemorySearchData {
    #[serde(default)]
    pub results: Vec<LocalMemorySearchResult>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LocalMemorySearchResult {
    pub memory: LocalMemoryRecord,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LocalMemoryRecord {
    #[serde(default)]
    pub id: Option<String>,
    pub content: String,
}

/// DEPRECATED: Use native MCP instead (see consensus.rs::fetch_memory_entries)
/// Still used by spec_prompts.rs and handler.rs - migration pending
#[deprecated(since = "2025-10-18", note = "Use MCP manager instead of subprocess")]
pub fn search_by_stage(
    spec_id: &str,
    stage: &str,
    limit: usize,
) -> Result<Vec<LocalMemorySearchResult>, String> {
    let query = format!("{} {}", spec_id, stage);
    let response = run_local_memory_search(&query, spec_id, stage, limit)?;
    Ok(response.data.map(|data| data.results).unwrap_or_default())
}

/// DEPRECATED: Use native MCP instead
#[deprecated(since = "2025-10-18", note = "Use MCP manager instead of subprocess")]
pub fn run_local_memory_search(
    query: &str,
    spec_id: &str,
    stage: &str,
    limit: usize,
) -> Result<LocalMemorySearchResponse, String> {
    let limit_value = if limit == 0 {
        "20".to_string()
    } else {
        limit.to_string()
    };
    let binary = env::var("LOCAL_MEMORY_BIN").unwrap_or_else(|_| "local-memory".to_string());
    let mut cmd = Command::new(binary);
    cmd.arg("search")
        .arg(query)
        .arg("--json")
        .arg("--limit")
        .arg(limit_value)
        .arg("--tags")
        .arg(format!("spec:{}", spec_id))
        .arg("--tags")
        .arg(format!("stage:{}", stage));

    let output = cmd
        .output()
        .map_err(|e| format!("failed to run local-memory search: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "local-memory search failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let response: LocalMemorySearchResponse = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("failed to parse local-memory search output: {e}"))?;

    if !response.success {
        if let Some(err) = response.error.as_ref() {
            return Err(format!("local-memory search error: {err}"));
        }
    }

    Ok(response)
}
