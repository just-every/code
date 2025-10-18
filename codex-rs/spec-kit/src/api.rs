//! Async-first API for spec-kit automation
//!
//! FORK-SPECIFIC (just-every/code): Spec-kit async API (MAINT-10)
//!
//! This module defines the public async API that replaces TUI-blocking calls.

use crate::error::Result;
use crate::types::{SpecAgent, SpecStage};
use async_trait::async_trait;
use std::path::{Path, PathBuf};

/// Spec-Kit automation engine (async-first)
///
/// Usage:
/// ```rust
/// let engine = SpecKitEngine::new(cwd, mcp_manager)?;
/// let result = engine.run_consensus("SPEC-KIT-065", SpecStage::Plan).await?;
/// ```
pub struct SpecKitEngine {
    cwd: PathBuf,
    mcp_manager: std::sync::Arc<codex_core::mcp_connection_manager::McpConnectionManager>,
}

impl SpecKitEngine {
    /// Create new spec-kit engine
    pub fn new(
        cwd: PathBuf,
        mcp_manager: std::sync::Arc<codex_core::mcp_connection_manager::McpConnectionManager>,
    ) -> Result<Self> {
        Ok(Self { cwd, mcp_manager })
    }

    /// Run consensus check for spec/stage (async, no blocking)
    ///
    /// Returns: (consensus_summary, degraded_flag)
    pub async fn run_consensus(
        &self,
        spec_id: &str,
        stage: SpecStage,
    ) -> Result<(ConsensusSummary, bool)> {
        // TODO: Move consensus.rs::run_spec_consensus logic here
        todo!("MAINT-10: Migrate consensus logic from TUI")
    }

    /// Execute full spec-auto pipeline (6 stages)
    ///
    /// Requires SpecKitContext trait implementation for UI callbacks
    pub async fn run_auto_pipeline<C: SpecKitContext>(
        &self,
        spec_id: &str,
        context: &mut C,
    ) -> Result<PipelineResult> {
        // TODO: Move handler.rs::handle_spec_auto logic here
        todo!("MAINT-10: Migrate pipeline orchestration from TUI")
    }

    /// Run quality gate checkpoint
    pub async fn run_quality_checkpoint<C: SpecKitContext>(
        &self,
        spec_id: &str,
        checkpoint: QualityCheckpoint,
        context: &mut C,
    ) -> Result<QualityGateResult> {
        // TODO: Move quality_gate_handler.rs logic here
        todo!("MAINT-10: Migrate quality gates from TUI")
    }
}

/// Consensus summary (returned from run_consensus)
#[derive(Debug, Clone)]
pub struct ConsensusSummary {
    pub status: String,
    pub missing_agents: Vec<String>,
    pub agreements: Vec<String>,
    pub conflicts: Vec<String>,
    pub aggregator_agent: Option<String>,
}

/// Pipeline execution result
#[derive(Debug, Clone)]
pub struct PipelineResult {
    pub spec_id: String,
    pub stages_completed: Vec<SpecStage>,
    pub final_status: String,
}

/// Quality gate checkpoint result
#[derive(Debug, Clone)]
pub struct QualityGateResult {
    pub auto_resolved: usize,
    pub escalated: usize,
    pub modified_files: Vec<String>,
}

/// Quality checkpoint types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QualityCheckpoint {
    PrePlanning,
    PostPlan,
    PostTasks,
}

/// Context trait for UI callbacks (decouples from specific UI framework)
///
/// Implementations: TUI (Ratatui), CLI (stdout), API (HTTP responses)
#[async_trait]
pub trait SpecKitContext: Send + Sync {
    /// Display message to user
    async fn display_message(&mut self, message: String);

    /// Display error to user
    async fn display_error(&mut self, error: String);

    /// Submit prompt to agent system
    async fn submit_agent_prompt(&mut self, display: String, prompt: String) -> Result<String>;

    /// Get working directory
    fn working_directory(&self) -> &Path;

    /// Request user input for escalated quality issues
    async fn request_quality_answers(
        &mut self,
        checkpoint: QualityCheckpoint,
        questions: Vec<String>,
    ) -> Result<Vec<String>>;
}

/// TUI-specific context (wraps ChatWidget via SpecKitContext trait from TUI)
///
/// This will be implemented in TUI crate, bridging sync Ratatui to async spec-kit
pub struct TuiContext<'a> {
    _phantom: std::marker::PhantomData<&'a ()>,
    // Will contain: &mut ChatWidget or similar
}
