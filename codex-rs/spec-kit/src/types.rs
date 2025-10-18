//! Core types for spec-kit operations
//!
//! FORK-SPECIFIC (just-every/code): Spec-kit multi-agent automation framework
//!
//! Migrated from tui/src/spec_prompts.rs (MAINT-10)

use serde::{Deserialize, Serialize};

/// Spec-kit workflow stages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SpecStage {
    Plan,
    Tasks,
    Implement,
    Validate,
    Audit,
    Unlock,
}

impl SpecStage {
    /// Command name (e.g., "plan" for /speckit.plan)
    pub fn command_name(&self) -> &'static str {
        match self {
            SpecStage::Plan => "plan",
            SpecStage::Tasks => "tasks",
            SpecStage::Implement => "implement",
            SpecStage::Validate => "validate",
            SpecStage::Audit => "audit",
            SpecStage::Unlock => "unlock",
        }
    }

    /// Display name for UI
    pub fn display_name(&self) -> &'static str {
        match self {
            SpecStage::Plan => "Plan",
            SpecStage::Tasks => "Tasks",
            SpecStage::Implement => "Implement",
            SpecStage::Validate => "Validate",
            SpecStage::Audit => "Audit",
            SpecStage::Unlock => "Unlock",
        }
    }

    /// All stages in order
    pub fn all() -> [Self; 6] {
        [
            SpecStage::Plan,
            SpecStage::Tasks,
            SpecStage::Implement,
            SpecStage::Validate,
            SpecStage::Audit,
            SpecStage::Unlock,
        ]
    }
}

/// Agent types in multi-model consensus
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SpecAgent {
    Gemini,
    Claude,
    Code,
    GptCodex,
    GptPro,
}

impl SpecAgent {
    /// Canonical name for storage/comparison
    pub fn canonical_name(&self) -> &'static str {
        match self {
            SpecAgent::Gemini => "gemini",
            SpecAgent::Claude => "claude",
            SpecAgent::Code => "code",
            SpecAgent::GptCodex => "gpt_codex",
            SpecAgent::GptPro => "gpt_pro",
        }
    }

    /// Parse from string (case-insensitive)
    pub fn from_string(s: &str) -> Option<Self> {
        let normalized = s.to_ascii_lowercase().replace("-", "_").replace(" ", "_");
        match normalized.as_str() {
            "gemini" | "gemini_flash" | "gemini_2.0" => Some(Self::Gemini),
            "claude" | "claude_sonnet" | "claude_4" => Some(Self::Claude),
            "code" | "claude_code" => Some(Self::Code),
            "gpt_codex" | "gptcodex" | "gpt5_codex" | "gpt_5_codex" => Some(Self::GptCodex),
            "gpt_pro" | "gptpro" | "gpt5" | "gpt_5" | "gpt5pro" => Some(Self::GptPro),
            _ => None,
        }
    }

    /// Display name for UI
    pub fn display_name(&self) -> &'static str {
        match self {
            SpecAgent::Gemini => "Gemini",
            SpecAgent::Claude => "Claude",
            SpecAgent::Code => "Claude Code",
            SpecAgent::GptCodex => "GPT-5 Codex",
            SpecAgent::GptPro => "GPT-5 Pro",
        }
    }

    /// All expected agents
    pub fn all() -> [Self; 5] {
        [
            Self::Gemini,
            Self::Claude,
            Self::Code,
            Self::GptCodex,
            Self::GptPro,
        ]
    }
}
