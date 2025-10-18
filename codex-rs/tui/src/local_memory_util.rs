//! Local-memory utility types for MCP response parsing
//!
//! STATUS (2025-10-18): ARCH-004 migration complete
//! - ✅ Types: Used by MCP native calls (spec_prompts.rs, consensus.rs)
//! - ✅ Subprocess functions deleted (migrated to native MCP)
//!
//! All local-memory access now via McpConnectionManager::call_tool()

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
