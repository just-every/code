//! MCP tool discovery and registry (T89)
//!
//! Provides dynamic discovery of MCP tool servers from filesystem:
//! - Scans configured directories for MCP server binaries
//! - Queries tool schemas via MCP protocol
//! - Caches tool definitions for fast lookup
//! - Reduces manual config.toml maintenance
//!
//! REBASE-SAFE: New file, 100% isolation, no upstream MCP changes

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use serde::{Deserialize, Serialize};

/// Tool definition discovered from MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub server_path: PathBuf,
    pub schema: serde_json::Value,
}

/// Registry for dynamically discovered MCP tools
#[derive(Debug)]
pub struct McpToolRegistry {
    /// Discovered tools by name
    tools: HashMap<String, ToolDefinition>,
    /// Search paths for MCP servers
    search_paths: Vec<PathBuf>,
    /// Cache validity duration
    cache_ttl_secs: u64,
}

impl McpToolRegistry {
    /// Create new registry with default search paths
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            search_paths: Self::default_search_paths(),
            cache_ttl_secs: 3600, // 1 hour
        }
    }

    /// Create registry with custom search paths
    pub fn with_paths(paths: Vec<PathBuf>) -> Self {
        Self {
            tools: HashMap::new(),
            search_paths: paths,
            cache_ttl_secs: 3600,
        }
    }

    /// Default search paths for MCP servers
    fn default_search_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // User's local bin
        if let Ok(home) = std::env::var("HOME") {
            let home_path = PathBuf::from(&home);
            paths.push(home_path.join(".code/tools"));
            paths.push(PathBuf::from(home).join(".local/bin"));
        }

        // System paths
        paths.push(PathBuf::from("/usr/local/bin"));

        paths
    }

    /// Discover MCP tools from configured search paths
    pub fn discover_all(&mut self) -> Result<usize, String> {
        let mut discovered_count = 0;

        for search_path in &self.search_paths {
            if !search_path.exists() {
                continue;
            }

            match Self::scan_directory(search_path) {
                Ok(tools) => {
                    discovered_count += tools.len();
                    for tool in tools {
                        self.tools.insert(tool.name.clone(), tool);
                    }
                }
                Err(err) => {
                    tracing::warn!("Failed to scan {}: {}", search_path.display(), err);
                }
            }
        }

        Ok(discovered_count)
    }

    /// Scan directory for MCP server binaries
    fn scan_directory(path: &Path) -> Result<Vec<ToolDefinition>, String> {
        let mut tools = Vec::new();

        let entries = std::fs::read_dir(path)
            .map_err(|e| format!("Failed to read directory {}: {}", path.display(), e))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
            let path = entry.path();

            // Check if executable and looks like MCP server
            if Self::is_mcp_server_candidate(&path) {
                match Self::query_tool_schema(&path) {
                    Ok(tool) => tools.push(tool),
                    Err(err) => {
                        tracing::debug!("Skipping {}: {}", path.display(), err);
                    }
                }
            }
        }

        Ok(tools)
    }

    /// Check if file is likely an MCP server
    fn is_mcp_server_candidate(path: &Path) -> bool {
        if !path.is_file() {
            return false;
        }

        // Check if executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = path.metadata() {
                let mode = metadata.permissions().mode();
                if mode & 0o111 == 0 {
                    return false; // Not executable
                }
            }
        }

        // Check naming patterns
        let name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        name.starts_with("mcp-") || name.ends_with("-mcp") || name.contains("mcp")
    }

    /// Query tool schema from MCP server
    fn query_tool_schema(server_path: &Path) -> Result<ToolDefinition, String> {
        // Try to invoke server with --list-tools or similar
        let output = Command::new(server_path)
            .arg("--schema")
            .output()
            .map_err(|e| format!("Failed to execute {}: {}", server_path.display(), e))?;

        if !output.status.success() {
            return Err(format!("Server returned error: {}", output.status));
        }

        let schema: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|e| format!("Failed to parse schema: {}", e))?;

        let name = schema["name"]
            .as_str()
            .ok_or("Missing 'name' in schema")?
            .to_string();

        let description = schema["description"]
            .as_str()
            .unwrap_or("No description")
            .to_string();

        Ok(ToolDefinition {
            name,
            description,
            server_path: server_path.to_path_buf(),
            schema,
        })
    }

    /// Get tool definition by name
    pub fn get_tool(&self, name: &str) -> Option<&ToolDefinition> {
        self.tools.get(name)
    }

    /// List all discovered tools
    pub fn list_tools(&self) -> Vec<&ToolDefinition> {
        self.tools.values().collect()
    }

    /// Get count of discovered tools
    pub fn tool_count(&self) -> usize {
        self.tools.len()
    }

    /// Check if tool is registered
    pub fn has_tool(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// Clear registry
    pub fn clear(&mut self) {
        self.tools.clear();
    }
}

impl Default for McpToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_creation() {
        let registry = McpToolRegistry::new();
        assert_eq!(registry.tool_count(), 0);
        assert!(!registry.search_paths.is_empty());
    }

    #[test]
    fn test_registry_with_custom_paths() {
        let paths = vec![PathBuf::from("/custom/path")];
        let registry = McpToolRegistry::with_paths(paths.clone());
        assert_eq!(registry.search_paths, paths);
    }

    #[test]
    fn test_default_search_paths_include_common_locations() {
        let paths = McpToolRegistry::default_search_paths();
        // Should have at least system path
        assert!(paths.iter().any(|p| p.to_str().unwrap().contains("bin")));
    }

    #[test]
    fn test_empty_registry_has_no_tools() {
        let registry = McpToolRegistry::new();
        assert!(!registry.has_tool("nonexistent"));
        assert!(registry.get_tool("test").is_none());
        assert_eq!(registry.list_tools().len(), 0);
    }

    #[test]
    fn test_clear_registry() {
        let mut registry = McpToolRegistry::new();
        // Simulate adding a tool
        registry.tools.insert(
            "test".to_string(),
            ToolDefinition {
                name: "test".to_string(),
                description: "Test tool".to_string(),
                server_path: PathBuf::from("/test"),
                schema: serde_json::json!({}),
            },
        );
        assert_eq!(registry.tool_count(), 1);

        registry.clear();
        assert_eq!(registry.tool_count(), 0);
    }

    #[test]
    fn test_default_trait() {
        let registry = McpToolRegistry::default();
        assert_eq!(registry.tool_count(), 0);
    }

    #[test]
    #[cfg(unix)]
    fn test_is_mcp_server_candidate_checks_executable() {
        // Most paths won't be executable, so this should return false
        let path = PathBuf::from("/etc/hosts");
        assert!(!McpToolRegistry::is_mcp_server_candidate(&path));
    }

    // Integration test - requires actual MCP server
    #[test]
    #[ignore]
    fn test_discover_from_directory() {
        let mut registry = McpToolRegistry::new();
        // This would work if MCP servers are installed
        let result = registry.discover_all();
        assert!(result.is_ok());
    }
}
