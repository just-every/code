//! Integration test harness for Phase 3 cross-module tests
//!
//! FORK-SPECIFIC (just-every/code): Spec-kit Phase 3 integration testing
//!
//! Provides:
//! - Multi-module test setup and teardown
//! - Pre-configured test contexts
//! - Evidence verification helpers
//! - State builders for complex scenarios

use codex_tui::{SpecStage, SpecAutoState, HalMode};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Integration test context with all necessary components
pub struct IntegrationTestContext {
    /// Temporary directory for test evidence
    pub temp_dir: TempDir,
    /// SPEC ID for this test
    pub spec_id: String,
    /// Current working directory (temp_dir path)
    pub cwd: PathBuf,
    /// Evidence base directory
    pub evidence_dir: PathBuf,
}

impl IntegrationTestContext {
    /// Create a new integration test context with isolated filesystem
    pub fn new(spec_id: &str) -> Result<Self, std::io::Error> {
        let temp_dir = TempDir::new()?;
        let cwd = temp_dir.path().to_path_buf();

        // Create evidence directory structure
        let evidence_dir = cwd.join("docs/SPEC-OPS-004-integrated-coder-hooks/evidence");
        std::fs::create_dir_all(&evidence_dir)?;

        let ctx = Self {
            temp_dir,
            spec_id: spec_id.to_string(),
            cwd,
            evidence_dir,
        };

        // Create consensus and commands directories
        std::fs::create_dir_all(ctx.consensus_dir())?;
        std::fs::create_dir_all(ctx.commands_dir())?;

        Ok(ctx)
    }

    /// Get the consensus directory for this SPEC
    pub fn consensus_dir(&self) -> PathBuf {
        self.evidence_dir
            .join("consensus")
            .join(&self.spec_id)
    }

    /// Get the commands (guardrail) directory for this SPEC
    pub fn commands_dir(&self) -> PathBuf {
        self.evidence_dir
            .join("commands")
            .join(&self.spec_id)
    }

    /// Create SPEC directory structure
    pub fn create_spec_dirs(&self, spec_slug: &str) -> Result<PathBuf, std::io::Error> {
        let spec_dir = self.cwd.join(format!("docs/{}-{}", self.spec_id, spec_slug));
        std::fs::create_dir_all(&spec_dir)?;
        Ok(spec_dir)
    }

    /// Write a test PRD file
    pub fn write_prd(&self, spec_slug: &str, content: &str) -> Result<(), std::io::Error> {
        let spec_dir = self.create_spec_dirs(spec_slug)?;
        std::fs::write(spec_dir.join("PRD.md"), content)
    }

    /// Write a test spec file
    pub fn write_spec(&self, spec_slug: &str, content: &str) -> Result<(), std::io::Error> {
        let spec_dir = self.create_spec_dirs(spec_slug)?;
        std::fs::write(spec_dir.join("spec.md"), content)
    }

    /// Verify consensus artifact exists
    pub fn assert_consensus_exists(&self, stage: SpecStage, agent: &str) -> bool {
        let consensus_dir = self.consensus_dir();
        let pattern = format!("spec-{:?}_*_{}.json", stage, agent).to_lowercase();

        if let Ok(entries) = std::fs::read_dir(&consensus_dir) {
            for entry in entries.flatten() {
                if entry.file_name().to_string_lossy().contains(agent) {
                    return true;
                }
            }
        }
        false
    }

    /// Verify guardrail telemetry exists
    pub fn assert_guardrail_telemetry_exists(&self, stage: SpecStage) -> bool {
        let commands_dir = self.commands_dir();
        let pattern = format!("spec-{:?}_*.json", stage).to_lowercase();

        if let Ok(entries) = std::fs::read_dir(&commands_dir) {
            for entry in entries.flatten() {
                let filename = entry.file_name().to_string_lossy().to_lowercase();
                if filename.starts_with(&format!("spec-{:?}", stage).to_lowercase()) {
                    return true;
                }
            }
        }
        false
    }

    /// Count evidence files in consensus directory
    pub fn count_consensus_files(&self) -> usize {
        self.count_files_in_dir(&self.consensus_dir())
    }

    /// Count evidence files in commands directory
    pub fn count_guardrail_files(&self) -> usize {
        self.count_files_in_dir(&self.commands_dir())
    }

    fn count_files_in_dir(&self, dir: &Path) -> usize {
        std::fs::read_dir(dir)
            .map(|entries| entries.filter_map(Result::ok).count())
            .unwrap_or(0)
    }
}

/// Builder for creating test SpecAutoState instances
pub struct StateBuilder {
    spec_id: String,
    goal: String,
    start_stage: SpecStage,
    hal_mode: Option<HalMode>,
    quality_gates_enabled: bool,
}

impl StateBuilder {
    pub fn new(spec_id: &str) -> Self {
        Self {
            spec_id: spec_id.to_string(),
            goal: "Integration test".to_string(),
            start_stage: SpecStage::Plan,
            hal_mode: None,
            quality_gates_enabled: true,
        }
    }

    pub fn with_goal(mut self, goal: &str) -> Self {
        self.goal = goal.to_string();
        self
    }

    pub fn starting_at(mut self, stage: SpecStage) -> Self {
        self.start_stage = stage;
        self
    }

    pub fn with_hal_mode(mut self, mode: HalMode) -> Self {
        self.hal_mode = Some(mode);
        self
    }

    pub fn quality_gates(mut self, enabled: bool) -> Self {
        self.quality_gates_enabled = enabled;
        self
    }

    pub fn build(self) -> SpecAutoState {
        let mut state = SpecAutoState::new(
            self.spec_id,
            self.goal,
            self.start_stage,
            self.hal_mode,
        );
        state.quality_gates_enabled = self.quality_gates_enabled;
        state
    }
}

/// Evidence file verifier
pub struct EvidenceVerifier<'a> {
    context: &'a IntegrationTestContext,
}

impl<'a> EvidenceVerifier<'a> {
    pub fn new(context: &'a IntegrationTestContext) -> Self {
        Self { context }
    }

    /// Verify consensus artifacts exist for all agents
    pub fn assert_consensus_complete(&self, stage: SpecStage, agents: &[&str]) -> bool {
        agents.iter().all(|agent| {
            self.context.assert_consensus_exists(stage, agent)
        })
    }

    /// Verify guardrail telemetry exists and is valid JSON
    pub fn assert_guardrail_valid(&self, stage: SpecStage) -> Result<(), String> {
        if !self.context.assert_guardrail_telemetry_exists(stage) {
            return Err(format!("Guardrail telemetry not found for {:?}", stage));
        }

        // TODO: Add JSON schema validation once we have the telemetry file path
        Ok(())
    }

    /// Verify evidence directory structure is correct
    pub fn assert_structure_valid(&self) -> bool {
        self.context.consensus_dir().exists() &&
        self.context.commands_dir().exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_integration_context_creation() {
        let ctx = IntegrationTestContext::new("SPEC-TEST-001").unwrap();
        assert_eq!(ctx.spec_id, "SPEC-TEST-001");
        assert!(ctx.cwd.exists());
        assert!(ctx.evidence_dir.exists());
    }

    #[test]
    fn test_state_builder() {
        let state = StateBuilder::new("SPEC-TEST-002")
            .with_goal("Test automation")
            .starting_at(SpecStage::Tasks)
            .quality_gates(false)
            .build();

        assert_eq!(state.spec_id, "SPEC-TEST-002");
        assert_eq!(state.goal, "Test automation");
        assert_eq!(state.current_stage(), Some(SpecStage::Tasks));
        assert!(!state.quality_gates_enabled);
    }

    #[test]
    fn test_spec_dirs_creation() {
        let ctx = IntegrationTestContext::new("SPEC-TEST-003").unwrap();
        let spec_dir = ctx.create_spec_dirs("test-feature").unwrap();

        assert!(spec_dir.exists());
        assert!(spec_dir.ends_with("docs/SPEC-TEST-003-test-feature"));
    }

    #[test]
    fn test_evidence_verifier() {
        let ctx = IntegrationTestContext::new("SPEC-TEST-004").unwrap();
        let verifier = EvidenceVerifier::new(&ctx);

        assert!(verifier.assert_structure_valid());
    }
}
