#![allow(dead_code)]

use std::collections::HashMap;

use once_cell::sync::OnceCell;
use serde::Deserialize;

const PROMPTS_JSON: &str = include_str!(
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/spec-kit/prompts.json")
);

#[derive(Debug, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct AgentPrompt {
    #[serde(default)]
    pub role: Option<String>,
    pub prompt: String,
}

#[derive(Debug, Deserialize, Clone, Default, PartialEq, Eq)]
#[serde(default)]
pub struct StagePrompts {
    pub gemini: Option<AgentPrompt>,
    pub claude: Option<AgentPrompt>,
    #[serde(rename = "gpt_pro")]
    pub gpt_pro: Option<AgentPrompt>,
    pub orchestrator_notes: Option<Vec<String>>,
    #[serde(flatten)]
    extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecAgent {
    Gemini,
    Claude,
    GptPro,
}

#[derive(Debug, Clone)]
pub struct PromptRegistry {
    stages: HashMap<String, StagePrompts>,
}

static PROMPT_DATA: OnceCell<PromptRegistry> = OnceCell::new();

impl PromptRegistry {
    fn load() -> Self {
        let stages: HashMap<String, StagePrompts> =
            serde_json::from_str(PROMPTS_JSON).expect("invalid spec-kit prompt json");
        Self { stages }
    }

    pub fn stage(&self, name: &str) -> Option<&StagePrompts> {
        self.stages.get(name)
    }
}

pub fn registry() -> &'static PromptRegistry {
    PROMPT_DATA.get_or_init(PromptRegistry::load)
}

pub fn agent_prompt(stage: &str, agent: SpecAgent) -> Option<AgentPrompt> {
    let stage = registry().stage(stage)?;
    let prompt = match agent {
        SpecAgent::Gemini => stage.gemini.clone(),
        SpecAgent::Claude => stage.claude.clone(),
        SpecAgent::GptPro => stage.gpt_pro.clone(),
    }?;
    Some(prompt)
}

pub fn orchestrator_notes(stage: &str) -> Option<Vec<String>> {
    registry()
        .stage(stage)?
        .orchestrator_notes
        .clone()
}

pub fn render_prompt(stage: &str, agent: SpecAgent, vars: &[(&str, &str)]) -> Option<String> {
    let prompt = agent_prompt(stage, agent)?;
    let mut text = prompt.prompt;
    for (key, value) in vars {
        let placeholder = format!("${{{}}}", key);
        text = text.replace(&placeholder, value);
    }
    Some(text)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecStage {
    Plan,
    Tasks,
    Implement,
    Validate,
    Review,
    Unlock,
}

impl SpecStage {
    pub fn key(self) -> &'static str {
        match self {
            SpecStage::Plan => "spec-plan",
            SpecStage::Tasks => "spec-tasks",
            SpecStage::Implement => "spec-implement",
            SpecStage::Validate => "spec-validate",
            SpecStage::Review => "spec-review",
            SpecStage::Unlock => "spec-unlock",
        }
    }

    pub fn command_name(self) -> &'static str {
        match self {
            SpecStage::Plan => "spec-plan",
            SpecStage::Tasks => "spec-tasks",
            SpecStage::Implement => "spec-implement",
            SpecStage::Validate => "spec-validate",
            SpecStage::Review => "spec-review",
            SpecStage::Unlock => "spec-unlock",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            SpecStage::Plan => "Plan",
            SpecStage::Tasks => "Tasks",
            SpecStage::Implement => "Implement",
            SpecStage::Validate => "Validate",
            SpecStage::Review => "Review",
            SpecStage::Unlock => "Unlock",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PromptBuildError {
    #[error("`/{command}` requires a SPEC ID (e.g. `/{command} SPEC-OPS-005`)")]
    MissingSpecId { command: String },
    #[error("No prompts defined for stage '{0}'")]
    MissingStage(&'static str),
}

pub fn build_stage_prompt(stage: SpecStage, raw_args: &str) -> Result<String, PromptBuildError> {
    let trimmed = raw_args.trim();
    if trimmed.is_empty() {
        return Err(PromptBuildError::MissingSpecId {
            command: stage.command_name().to_string(),
        });
    }

    let mut parts = trimmed.split_whitespace();
    let spec_id = parts.next().unwrap();
    let remainder = trimmed[spec_id.len()..].trim().to_string();

    let context_hint = format!(
        "Use local-memory search (domains: spec-tracker, docs-ops, impl-notes, infra-ci) to gather current context for {spec_id}. Summaries must cite memory IDs or MCP transcripts."
    );

    let goal_hint = if !remainder.is_empty() {
        remainder.clone()
    } else {
        "(no additional goal provided)".to_string()
    };

    let mut replacements: Vec<(String, String)> = vec![
        ("SPEC_ID".into(), spec_id.to_string()),
        ("CONTEXT".into(), context_hint.clone()),
        ("GOAL".into(), goal_hint.clone()),
    ];

    match stage {
        SpecStage::Plan => {
            replacements.push((
                "PREVIOUS_OUTPUTS.gemini".into(),
                "Gemini Ultra findings stored in local-memory (spec-tracker domain).".into(),
            ));
            replacements.push((
                "PREVIOUS_OUTPUTS".into(),
                "Refer to Gemini + Claude outputs captured in local-memory for consensus notes.".into(),
            ));
        }
        SpecStage::Tasks => {
            replacements.push((
                "PREVIOUS_OUTPUTS.gemini".into(),
                "Gemini research from /spec-plan (local-memory spec-tracker).".into(),
            ));
            replacements.push((
                "PREVIOUS_OUTPUTS.plan".into(),
                "Final plan consensus written during /spec-plan.".into(),
            ));
        }
        SpecStage::Implement => {
            replacements.push((
                "PREVIOUS_OUTPUTS.tasks".into(),
                "Latest /spec-tasks consensus stored in docs/SPEC-*/tasks.md and local-memory.".into(),
            ));
        }
        SpecStage::Validate | SpecStage::Review | SpecStage::Unlock => {
            // No extra replacements required
        }
    }

    // Provide fallbacks for placeholders that might appear in prompts
    replacements.push((
        "PREVIOUS_OUTPUTS".into(),
        "See local-memory entries from earlier /spec-* stages.".into(),
    ));
    replacements.push((
        "PREVIOUS_OUTPUTS.plan".into(),
        "Final plan consensus (if available).".into(),
    ));
    replacements.push((
        "PREVIOUS_OUTPUTS.tasks".into(),
        "Task breakdown consensus (if available).".into(),
    ));

    let registry = registry();
    let stage_prompts = registry
        .stage(stage.key())
        .ok_or(PromptBuildError::MissingStage(stage.key()))?;

    let replacement_refs: Vec<(&str, &str)> =
        replacements.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

    let mut bundle = String::new();
    bundle.push_str(&format!("# /{} — {}\n\n", stage.command_name(), spec_id));
    bundle.push_str("Leverage local-memory before starting, then run the three agents in parallel using these prompts. Record outputs back into local-memory (spec-tracker, impl-notes, docs-ops).\n\n");
    if let SpecStage::Plan = stage {
        bundle.push_str(&format!("Goal: {}\n\n", goal_hint));
    }

    if let Some(prompt) = stage_prompts.gemini.clone() {
        let rendered = render_prompt(stage.key(), SpecAgent::Gemini, &replacement_refs)
            .unwrap_or_else(|| prompt.prompt);
        bundle.push_str("## Gemini Ultra — Research\n");
        bundle.push_str(&rendered);
        bundle.push_str("\n\n");
    }
    if let Some(prompt) = stage_prompts.claude.clone() {
        let rendered = render_prompt(stage.key(), SpecAgent::Claude, &replacement_refs)
            .unwrap_or_else(|| prompt.prompt);
        bundle.push_str("## Claude MAX — Synthesis\n");
        bundle.push_str(&rendered);
        bundle.push_str("\n\n");
    }
    if let Some(prompt) = stage_prompts.gpt_pro.clone() {
        let rendered = render_prompt(stage.key(), SpecAgent::GptPro, &replacement_refs)
            .unwrap_or_else(|| prompt.prompt);
        bundle.push_str("## GPT Pro — Execution & QA\n");
        bundle.push_str(&rendered);
        bundle.push_str("\n");
    }

    Ok(bundle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_prompt_is_loaded() {
        let gemini = agent_prompt("spec-plan", SpecAgent::Gemini).expect("gemini prompt");
        assert!(gemini.prompt.contains("Summarize:"));
    }

    #[test]
    fn placeholder_substitution() {
        let rendered = render_prompt(
            "spec-plan",
            SpecAgent::Gemini,
            &[("SPEC_ID", "SPEC-OPS-123"), ("CONTEXT", "<ctx>")],
        )
        .expect("rendered");
        assert!(rendered.contains("SPEC-OPS-123"));
        assert!(rendered.contains("<ctx>"));
    }

    #[test]
    fn orchestrator_notes_present_for_auto() {
        let notes = orchestrator_notes("spec-auto").expect("notes");
        assert!(!notes.is_empty());
    }

    #[test]
    fn build_stage_prompt_requires_spec_id() {
        let err = build_stage_prompt(SpecStage::Plan, " ").unwrap_err();
        assert!(matches!(err, PromptBuildError::MissingSpecId { .. }));
    }

    #[test]
    fn build_stage_prompt_includes_agent_sections() {
        let prompt = build_stage_prompt(SpecStage::Plan, "SPEC-OPS-999 Align rollout").unwrap();
        assert!(prompt.contains("/spec-plan"));
        assert!(prompt.contains("Gemini Ultra"));
        assert!(prompt.contains("Claude MAX"));
        assert!(prompt.contains("GPT Pro"));
    }
}
