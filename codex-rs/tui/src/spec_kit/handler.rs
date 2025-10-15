//! Spec-Kit command handlers
//!
//! All spec-kit command handling logic extracted from chatwidget.rs
//! This module will be populated in subsequent refactoring steps

use super::state::SpecAutoState;

/// Handler for all /speckit.* commands
///
/// This struct will contain all the handler methods currently inline in chatwidget.rs
/// Extraction happens in phases to maintain compilation at each step
pub struct SpecKitHandler {
    pub(crate) state: Option<SpecAutoState>,
}

impl SpecKitHandler {
    pub fn new() -> Self {
        Self { state: None }
    }

    pub fn state(&self) -> Option<&SpecAutoState> {
        self.state.as_ref()
    }

    pub fn state_mut(&mut self) -> Option<&mut SpecAutoState> {
        self.state.as_mut()
    }

    pub fn set_state(&mut self, state: Option<SpecAutoState>) {
        self.state = state;
    }

    // Handler methods will be added here in subsequent steps
    // For now, this is just the structure
}

impl Default for SpecKitHandler {
    fn default() -> Self {
        Self::new()
    }
}
