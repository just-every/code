//! State management for spec-kit automation

use crate::spec_prompts::SpecStage;
use std::time::Instant;

/// Phase tracking for /speckit.auto pipeline
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecAutoPhase {
    Plan,
    Tasks,
    Implement,
    Validate,
    Audit,
    Unlock,
    Done,
}

impl SpecAutoPhase {
    pub fn as_stage(self) -> Option<SpecStage> {
        match self {
            SpecAutoPhase::Plan => Some(SpecStage::Plan),
            SpecAutoPhase::Tasks => Some(SpecStage::Tasks),
            SpecAutoPhase::Implement => Some(SpecStage::Implement),
            SpecAutoPhase::Validate => Some(SpecStage::Validate),
            SpecAutoPhase::Audit => Some(SpecStage::Audit),
            SpecAutoPhase::Unlock => Some(SpecStage::Unlock),
            SpecAutoPhase::Done => None,
        }
    }

    pub fn next(self) -> Self {
        match self {
            SpecAutoPhase::Plan => SpecAutoPhase::Tasks,
            SpecAutoPhase::Tasks => SpecAutoPhase::Implement,
            SpecAutoPhase::Implement => SpecAutoPhase::Validate,
            SpecAutoPhase::Validate => SpecAutoPhase::Audit,
            SpecAutoPhase::Audit => SpecAutoPhase::Unlock,
            SpecAutoPhase::Unlock | SpecAutoPhase::Done => SpecAutoPhase::Done,
        }
    }
}

/// Waiting state for guardrail execution
#[derive(Debug, Clone)]
pub struct WaitingGuardrail {
    pub stage: SpecStage,
}

/// State for /speckit.auto pipeline automation
#[derive(Debug, Clone)]
pub struct SpecAutoState {
    pub spec_id: String,
    pub current_phase: SpecAutoPhase,
    pub started_at: Instant,
    pub goal: String,
    pub waiting_guardrail: Option<WaitingGuardrail>,
}

impl SpecAutoState {
    pub fn new(spec_id: String, goal: String, resume_from: SpecStage) -> Self {
        let current_phase = match resume_from {
            SpecStage::Plan => SpecAutoPhase::Plan,
            SpecStage::Tasks => SpecAutoPhase::Tasks,
            SpecStage::Implement => SpecAutoPhase::Implement,
            SpecStage::Validate => SpecAutoPhase::Validate,
            SpecStage::Audit => SpecAutoPhase::Audit,
            SpecStage::Unlock => SpecAutoPhase::Unlock,
        };

        Self {
            spec_id,
            current_phase,
            started_at: Instant::now(),
            goal,
            waiting_guardrail: None,
        }
    }

    pub fn is_done(&self) -> bool {
        self.current_phase == SpecAutoPhase::Done
    }
}
