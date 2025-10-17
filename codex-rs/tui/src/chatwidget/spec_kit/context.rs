//! Context trait for spec-kit operations
//!
//! This trait decouples spec-kit from ChatWidget, enabling independent testing
//! and reuse.

use super::state::{GuardrailOutcome, SpecAutoState};
use crate::app_event::BackgroundPlacement;
use crate::history_cell::HistoryCell;
use crate::spec_prompts::SpecStage;
use codex_core::config_types::{AgentConfig, SubagentCommandConfig};
use codex_core::protocol::Op;
use std::path::Path;

/// Minimal context interface required by spec-kit operations
///
/// This trait abstracts away ChatWidget dependencies, allowing spec-kit
/// to work with any UI context that provides these essential operations.
pub trait SpecKitContext {
    // === History Operations ===

    /// Add a cell to the conversation history
    fn history_push(&mut self, cell: impl HistoryCell + 'static);

    /// Add error message to history
    fn push_error(&mut self, message: String) {
        self.history_push(crate::history_cell::new_error_event(message));
    }

    /// Add background event message
    fn push_background(&mut self, message: String, placement: BackgroundPlacement);

    // === UI Operations ===

    /// Request a UI redraw
    fn request_redraw(&mut self);

    // === Agent/Operation Submission ===

    /// Submit an operation to the backend
    fn submit_operation(&self, op: Op);

    /// Submit a prompt with display text
    fn submit_prompt(&mut self, display: String, prompt: String);

    // === Configuration Access ===

    /// Get current working directory
    fn working_directory(&self) -> &Path;

    /// Get agent configuration
    fn agent_config(&self) -> &[AgentConfig];

    /// Get subagent command configuration
    fn subagent_commands(&self) -> &[SubagentCommandConfig];

    // === Spec Auto State ===

    /// Get mutable reference to spec auto state
    fn spec_auto_state_mut(&mut self) -> &mut Option<SpecAutoState>;

    /// Get immutable reference to spec auto state
    fn spec_auto_state(&self) -> &Option<SpecAutoState>;

    /// Take ownership of spec auto state (for cleanup)
    fn take_spec_auto_state(&mut self) -> Option<SpecAutoState> {
        self.spec_auto_state_mut().take()
    }

    // === Guardrail & Consensus Operations (T79-Revised) ===

    /// Collect guardrail outcome for a spec/stage
    fn collect_guardrail_outcome(
        &self,
        spec_id: &str,
        stage: SpecStage,
    ) -> std::result::Result<GuardrailOutcome, String>;

    /// Run consensus checking for a spec/stage
    /// Returns (output_lines, consensus_ok)
    fn run_spec_consensus(
        &mut self,
        spec_id: &str,
        stage: SpecStage,
    ) -> std::result::Result<(Vec<ratatui::text::Line<'static>>, bool), String>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Mock context for testing spec-kit operations in isolation
    pub struct MockSpecKitContext {
        pub cwd: PathBuf,
        pub agents: Vec<AgentConfig>,
        pub subagent_commands: Vec<SubagentCommandConfig>,
        pub history: Vec<String>,
        pub background_events: Vec<(String, BackgroundPlacement)>,
        pub submitted_ops: Vec<String>,
        pub submitted_prompts: Vec<(String, String)>,
        pub spec_auto_state: Option<SpecAutoState>,
        pub redraw_requested: bool,
    }

    impl MockSpecKitContext {
        pub fn new() -> Self {
            Self {
                cwd: PathBuf::from("/test"),
                agents: Vec::new(),
                subagent_commands: Vec::new(),
                history: Vec::new(),
                background_events: Vec::new(),
                submitted_ops: Vec::new(),
                submitted_prompts: Vec::new(),
                spec_auto_state: None,
                redraw_requested: false,
            }
        }

        pub fn with_cwd(mut self, cwd: PathBuf) -> Self {
            self.cwd = cwd;
            self
        }
    }

    impl SpecKitContext for MockSpecKitContext {
        fn history_push(&mut self, _cell: impl HistoryCell + 'static) {
            // Store a simplified representation for testing
            self.history.push("history_cell".to_string());
        }

        fn push_background(&mut self, message: String, placement: BackgroundPlacement) {
            self.background_events.push((message, placement));
        }

        fn request_redraw(&mut self) {
            self.redraw_requested = true;
        }

        fn submit_operation(&self, _op: Op) {
            // Can't mutate self in non-mut method, would need Arc<Mutex> for real impl
            // For testing, we'll track in submitted_ops via interior mutability if needed
        }

        fn submit_prompt(&mut self, display: String, prompt: String) {
            self.submitted_prompts.push((display, prompt));
        }

        fn working_directory(&self) -> &Path {
            &self.cwd
        }

        fn agent_config(&self) -> &[AgentConfig] {
            &self.agents
        }

        fn subagent_commands(&self) -> &[SubagentCommandConfig] {
            &self.subagent_commands
        }

        fn spec_auto_state_mut(&mut self) -> &mut Option<SpecAutoState> {
            &mut self.spec_auto_state
        }

        fn spec_auto_state(&self) -> &Option<SpecAutoState> {
            &self.spec_auto_state
        }

        fn collect_guardrail_outcome(
            &self,
            _spec_id: &str,
            _stage: SpecStage,
        ) -> std::result::Result<GuardrailOutcome, String> {
            // Mock: Return success by default
            Ok(GuardrailOutcome {
                success: true,
                summary: "Mock guardrail success".to_string(),
                telemetry_path: Some(PathBuf::from("/mock/telemetry.json")),
                failures: Vec::new(),
            })
        }

        fn run_spec_consensus(
            &mut self,
            _spec_id: &str,
            _stage: SpecStage,
        ) -> std::result::Result<(Vec<ratatui::text::Line<'static>>, bool), String> {
            // Mock: Return consensus OK
            use ratatui::text::Line;
            Ok((vec![Line::from("Mock consensus OK")], true))
        }
    }

    #[test]
    fn test_mock_context_history() {
        let mut ctx = MockSpecKitContext::new();
        ctx.push_error("test error".to_string());
        assert_eq!(ctx.history.len(), 1);
    }

    #[test]
    fn test_mock_context_background() {
        let mut ctx = MockSpecKitContext::new();
        ctx.push_background("test message".to_string(), BackgroundPlacement::Tail);
        assert_eq!(ctx.background_events.len(), 1);
        assert_eq!(ctx.background_events[0].0, "test message");
    }

    #[test]
    fn test_mock_context_redraw() {
        let mut ctx = MockSpecKitContext::new();
        assert!(!ctx.redraw_requested);
        ctx.request_redraw();
        assert!(ctx.redraw_requested);
    }

    #[test]
    fn test_mock_context_submit_prompt() {
        let mut ctx = MockSpecKitContext::new();
        ctx.submit_prompt("display".to_string(), "prompt".to_string());
        assert_eq!(ctx.submitted_prompts.len(), 1);
        assert_eq!(ctx.submitted_prompts[0].0, "display");
        assert_eq!(ctx.submitted_prompts[0].1, "prompt");
    }

    #[test]
    fn test_mock_context_working_dir() {
        let ctx = MockSpecKitContext::new().with_cwd(PathBuf::from("/custom"));
        assert_eq!(ctx.working_directory(), Path::new("/custom"));
    }

    #[test]
    fn test_mock_context_spec_auto_state() {
        let mut ctx = MockSpecKitContext::new();
        assert!(ctx.spec_auto_state().is_none());

        let state = SpecAutoState::new("SPEC-TEST".to_string(), "test".to_string(), SpecStage::Plan, None);
        *ctx.spec_auto_state_mut() = Some(state);

        assert!(ctx.spec_auto_state().is_some());

        let taken = ctx.take_spec_auto_state();
        assert!(taken.is_some());
        assert!(ctx.spec_auto_state().is_none());
    }

    #[test]
    fn test_mock_context_collect_guardrail() {
        let ctx = MockSpecKitContext::new();

        let result = ctx.collect_guardrail_outcome("SPEC-TEST", SpecStage::Plan);
        assert!(result.is_ok());

        let outcome = result.unwrap();
        assert!(outcome.success);
        assert!(outcome.summary.contains("Mock"));
    }

    #[test]
    fn test_mock_context_run_consensus() {
        let mut ctx = MockSpecKitContext::new();

        let result = ctx.run_spec_consensus("SPEC-TEST", SpecStage::Plan);
        assert!(result.is_ok());

        let (lines, ok) = result.unwrap();
        assert!(ok);
        assert_eq!(lines.len(), 1);
    }
}
