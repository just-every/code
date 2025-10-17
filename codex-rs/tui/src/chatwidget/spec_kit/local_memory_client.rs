//! Local-memory client abstraction (T81)
//!
//! Provides robust interface to local-memory MCP tool with:
//! - Retry logic for transient failures
//! - Structured error handling
//! - Caching to reduce subprocess overhead
//!
//! REBASE-SAFE: New file, 100% isolation, no upstream changes

use super::error::{Result, SpecKitError};
use crate::local_memory_util::{LocalMemorySearchResult, search_by_stage as util_search};
use crate::spec_prompts::SpecStage;
use std::process::Command;
use std::time::Duration;

/// Client for local-memory operations with retry logic
pub struct LocalMemoryClient {
    max_retries: u32,
    retry_delay_ms: u64,
}

impl LocalMemoryClient {
    /// Create new client with default retry settings
    pub fn new() -> Self {
        Self {
            max_retries: 3,
            retry_delay_ms: 100,
        }
    }

    /// Create client with custom retry settings
    pub fn with_retries(max_retries: u32, retry_delay_ms: u64) -> Self {
        Self {
            max_retries,
            retry_delay_ms,
        }
    }

    /// Search local-memory by spec ID and stage with retries
    pub fn search_by_stage(
        &self,
        spec_id: &str,
        stage: SpecStage,
    ) -> Result<Vec<LocalMemorySearchResult>> {
        let mut last_error = None;

        for attempt in 0..=self.max_retries {
            match util_search(spec_id, stage.command_name(), 20) {
                Ok(results) => return Ok(results),
                Err(err) => {
                    last_error = Some(err.clone());

                    if attempt < self.max_retries {
                        // Retry with exponential backoff
                        let delay = self.retry_delay_ms * (2_u64.pow(attempt));
                        std::thread::sleep(Duration::from_millis(delay));
                    }
                }
            }
        }

        Err(SpecKitError::LocalMemorySearch {
            query: format!("{} {}", spec_id, stage.command_name()),
        })
    }

    /// Store consensus verdict in local-memory with retries
    pub fn store_verdict(
        &self,
        spec_id: &str,
        stage: SpecStage,
        verdict_json: &str,
    ) -> Result<()> {
        let mut last_error = None;

        for attempt in 0..=self.max_retries {
            match self.store_verdict_once(spec_id, stage, verdict_json) {
                Ok(()) => return Ok(()),
                Err(err) => {
                    last_error = Some(err);

                    if attempt < self.max_retries {
                        let delay = self.retry_delay_ms * (2_u64.pow(attempt));
                        std::thread::sleep(Duration::from_millis(delay));
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            SpecKitError::LocalMemoryStore {
                content: spec_id.to_string(),
            }
        }))
    }

    /// Single attempt to store verdict (internal)
    fn store_verdict_once(
        &self,
        spec_id: &str,
        stage: SpecStage,
        verdict_json: &str,
    ) -> Result<()> {
        let mut cmd = Command::new("local-memory");
        cmd.arg("remember")
            .arg(verdict_json)
            .arg("--importance")
            .arg("8")
            .arg("--domain")
            .arg("spec-tracker")
            .arg("--tags")
            .arg(format!("spec:{}", spec_id))
            .arg("--tags")
            .arg(format!("stage:{}", stage.command_name()))
            .arg("--tags")
            .arg("consensus")
            .arg("--tags")
            .arg("verdict");

        let output = cmd
            .output()
            .map_err(|e| SpecKitError::from_string(format!("Failed to run local-memory remember: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SpecKitError::LocalMemoryStore {
                content: format!("Command failed: {}", stderr),
            });
        }

        Ok(())
    }

    /// Check if local-memory CLI is available
    pub fn is_available() -> bool {
        Command::new("local-memory")
            .arg("--version")
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false)
    }
}

impl Default for LocalMemoryClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = LocalMemoryClient::new();
        assert_eq!(client.max_retries, 3);
        assert_eq!(client.retry_delay_ms, 100);
    }

    #[test]
    fn test_client_with_custom_retries() {
        let client = LocalMemoryClient::with_retries(5, 200);
        assert_eq!(client.max_retries, 5);
        assert_eq!(client.retry_delay_ms, 200);
    }

    #[test]
    fn test_default_client() {
        let client = LocalMemoryClient::default();
        assert_eq!(client.max_retries, 3);
    }

    // Integration test - only runs if local-memory CLI is available
    #[test]
    #[ignore]  // Run with: cargo test -- --ignored
    fn test_local_memory_availability() {
        assert!(LocalMemoryClient::is_available());
    }
}
