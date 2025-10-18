//! Spec-Kit Multi-Agent Automation Framework
//!
//! FORK-SPECIFIC (just-every/code): Extracted from TUI (MAINT-10)
//!
//! Multi-agent consensus system for AI-driven feature development through
//! 6-stage workflows (Plan→Tasks→Implement→Validate→Audit→Unlock).
//!
//! This crate provides async-first APIs for spec-kit automation, enabling
//! usage from TUI, CLI, API servers, and CI/CD pipelines.

#![deny(clippy::print_stdout, clippy::print_stderr)]

pub mod error;
pub mod types;

// Phase 1: Core types and error handling
pub use error::{Result, SpecKitError};
pub use types::{SpecStage, SpecAgent};

/// Spec-Kit version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Evidence base directory (centralized)
pub const DEFAULT_EVIDENCE_BASE: &str = "docs/SPEC-OPS-004-integrated-coder-hooks/evidence";

/// Build consensus directory path
pub fn consensus_dir(cwd: &std::path::Path) -> std::path::PathBuf {
    cwd.join(DEFAULT_EVIDENCE_BASE).join("consensus")
}

/// Build commands directory path
pub fn commands_dir(cwd: &std::path::Path) -> std::path::PathBuf {
    cwd.join(DEFAULT_EVIDENCE_BASE).join("commands")
}
