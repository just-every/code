use std::collections::HashMap;

use code_app_server_protocol::AuthMode;
use code_core::config_types::TextVerbosity as TextVerbosityConfig;
use code_core::protocol_config_types::ReasoningEffort;
use once_cell::sync::Lazy;

pub const HIDE_GPT5_1_MIGRATION_PROMPT_CONFIG: &str = "hide_gpt5_1_migration_prompt";
pub const HIDE_GPT_5_1_CODEX_MAX_MIGRATION_PROMPT_CONFIG: &str =
    "hide_gpt-5.1-codex-max_migration_prompt";
pub const HIDE_GPT_5_2_MIGRATION_PROMPT_CONFIG: &str = "hide_gpt5_2_migration_prompt";
pub const HIDE_GPT_5_2_CODEX_MIGRATION_PROMPT_CONFIG: &str = "hide_gpt5_2_codex_migration_prompt";

/// A reasoning effort option surfaced for a model.
#[derive(Debug, Clone)]
pub struct ReasoningEffortPreset {
    pub effort: ReasoningEffort,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct ModelUpgrade {
    pub id: String,
    pub reasoning_effort_mapping: Option<HashMap<ReasoningEffort, ReasoningEffort>>,
    pub migration_config_key: String,
}

/// Metadata describing a Code-supported model.
#[derive(Debug, Clone)]
pub struct ModelPreset {
    pub id: String,
    pub model: String,
    pub display_name: String,
    pub description: String,
    pub default_reasoning_effort: ReasoningEffort,
    pub supported_reasoning_efforts: Vec<ReasoningEffortPreset>,
    pub supported_text_verbosity: &'static [TextVerbosityConfig],
    pub is_default: bool,
    pub upgrade: Option<ModelUpgrade>,
    pub show_in_picker: bool,
}

const ALL_TEXT_VERBOSITY: &[TextVerbosityConfig] = &[
    TextVerbosityConfig::Low,
    TextVerbosityConfig::Medium,
    TextVerbosityConfig::High,
];

static PRESETS: Lazy<Vec<ModelPreset>> = Lazy::new(|| {
    vec![
        ModelPreset {
            id: "gpt-5.2-codex".to_string(),
            model: "gpt-5.2-codex".to_string(),
            display_name: "gpt-5.2-codex".to_string(),
            description: "Latest frontier agentic coding model.".to_string(),
            default_reasoning_effort: ReasoningEffort::Medium,
            supported_reasoning_efforts: vec![
                ReasoningEffortPreset {
                    effort: ReasoningEffort::Low,
                    description: "Fast responses with lighter reasoning".to_string(),
                },
                ReasoningEffortPreset {
                    effort: ReasoningEffort::Medium,
                    description: "Balances speed and reasoning depth for everyday tasks".to_string(),
                },
                ReasoningEffortPreset {
                    effort: ReasoningEffort::High,
                    description: "Maximizes reasoning depth for complex problems".to_string(),
                },
                ReasoningEffortPreset {
                    effort: ReasoningEffort::XHigh,
                    description: "Extra high reasoning depth for complex problems".to_string(),
                },
            ],
            supported_text_verbosity: &[TextVerbosityConfig::Medium],
            is_default: true,
            upgrade: None,
            show_in_picker: true,
        },
        ModelPreset {
            id: "gpt-5.2".to_string(),
            model: "gpt-5.2".to_string(),
            display_name: "gpt-5.2".to_string(),
            description:
                "Latest frontier model with improvements across knowledge, reasoning and coding"
                    .to_string(),
            default_reasoning_effort: ReasoningEffort::Medium,
            supported_reasoning_efforts: vec![
                ReasoningEffortPreset {
                    effort: ReasoningEffort::Low,
                    description:
                        "Balances speed with some reasoning; useful for straightforward queries and short explanations"
                            .to_string(),
                },
                ReasoningEffortPreset {
                    effort: ReasoningEffort::Medium,
                    description:
                        "Provides a solid balance of reasoning depth and latency for general-purpose tasks"
                            .to_string(),
                },
                ReasoningEffortPreset {
                    effort: ReasoningEffort::High,
                    description: "Maximizes reasoning depth for complex or ambiguous problems"
                        .to_string(),
                },
                ReasoningEffortPreset {
                    effort: ReasoningEffort::XHigh,
                    description: "Extra high reasoning depth for complex problems".to_string(),
                },
            ],
            supported_text_verbosity: ALL_TEXT_VERBOSITY,
            is_default: false,
            upgrade: Some(ModelUpgrade {
                id: "gpt-5.2-codex".to_string(),
                reasoning_effort_mapping: None,
                migration_config_key: HIDE_GPT_5_2_CODEX_MIGRATION_PROMPT_CONFIG.to_string(),
            }),
            show_in_picker: true,
        },
        ModelPreset {
            id: "bengalfox".to_string(),
            model: "bengalfox".to_string(),
            display_name: "bengalfox".to_string(),
            description: "bengalfox".to_string(),
            default_reasoning_effort: ReasoningEffort::Medium,
            supported_reasoning_efforts: vec![
                ReasoningEffortPreset {
                    effort: ReasoningEffort::Low,
                    description: "Fast responses with lighter reasoning".to_string(),
                },
                ReasoningEffortPreset {
                    effort: ReasoningEffort::Medium,
                    description: "Balances speed and reasoning depth for everyday tasks".to_string(),
                },
                ReasoningEffortPreset {
                    effort: ReasoningEffort::High,
                    description: "Greater reasoning depth for complex problems".to_string(),
                },
                ReasoningEffortPreset {
                    effort: ReasoningEffort::XHigh,
                    description: "Extra high reasoning depth for complex problems".to_string(),
                },
            ],
            supported_text_verbosity: &[TextVerbosityConfig::Medium],
            is_default: false,
            upgrade: None,
            show_in_picker: false,
        },
        ModelPreset {
            id: "boomslang".to_string(),
            model: "boomslang".to_string(),
            display_name: "boomslang".to_string(),
            description: "boomslang".to_string(),
            default_reasoning_effort: ReasoningEffort::Medium,
            supported_reasoning_efforts: vec![
                ReasoningEffortPreset {
                    effort: ReasoningEffort::Low,
                    description:
                        "Balances speed with some reasoning; useful for straightforward queries and short explanations"
                            .to_string(),
                },
                ReasoningEffortPreset {
                    effort: ReasoningEffort::Medium,
                    description:
                        "Provides a solid balance of reasoning depth and latency for general-purpose tasks"
                            .to_string(),
                },
                ReasoningEffortPreset {
                    effort: ReasoningEffort::High,
                    description: "Maximizes reasoning depth for complex or ambiguous problems"
                        .to_string(),
                },
                ReasoningEffortPreset {
                    effort: ReasoningEffort::XHigh,
                    description: "Extra high reasoning for complex problems".to_string(),
                },
            ],
            supported_text_verbosity: ALL_TEXT_VERBOSITY,
            is_default: false,
            upgrade: None,
            show_in_picker: false,
        },
        ModelPreset {
            id: "gpt-5.1-codex-max".to_string(),
            model: "gpt-5.1-codex-max".to_string(),
            display_name: "gpt-5.1-codex-max".to_string(),
            description: "Latest Codex-optimized flagship for deep and fast reasoning.".to_string(),
            default_reasoning_effort: ReasoningEffort::Medium,
            supported_reasoning_efforts: vec![
                ReasoningEffortPreset {
                    effort: ReasoningEffort::Low,
                    description: "Fast responses with lighter reasoning".to_string(),
                },
                ReasoningEffortPreset {
                    effort: ReasoningEffort::Medium,
                    description: "Balances speed and reasoning depth for everyday tasks".to_string(),
                },
                ReasoningEffortPreset {
                    effort: ReasoningEffort::High,
                    description: "Maximizes reasoning depth for complex problems".to_string(),
                },
                ReasoningEffortPreset {
                    effort: ReasoningEffort::XHigh,
                    description: "Extra high reasoning depth for complex problems".to_string(),
                },
            ],
            supported_text_verbosity: &[TextVerbosityConfig::Medium],
            is_default: false,
            upgrade: Some(ModelUpgrade {
                id: "gpt-5.2-codex".to_string(),
                reasoning_effort_mapping: None,
                migration_config_key: HIDE_GPT_5_2_CODEX_MIGRATION_PROMPT_CONFIG.to_string(),
            }),
            show_in_picker: true,
        },
        ModelPreset {
            id: "gpt-5.1-codex".to_string(),
            model: "gpt-5.1-codex".to_string(),
            display_name: "gpt-5.1-codex".to_string(),
            description: "Optimized for Code.".to_string(),
            default_reasoning_effort: ReasoningEffort::Medium,
            supported_reasoning_efforts: vec![
                ReasoningEffortPreset {
                    effort: ReasoningEffort::Low,
                    description: "Fastest responses with limited reasoning".to_string(),
                },
                ReasoningEffortPreset {
                    effort: ReasoningEffort::Medium,
                    description: "Dynamically adjusts reasoning based on the task".to_string(),
                },
                ReasoningEffortPreset {
                    effort: ReasoningEffort::High,
                    description: "Maximizes reasoning depth for complex or ambiguous problems"
                        .to_string(),
                },
            ],
            supported_text_verbosity: ALL_TEXT_VERBOSITY,
            is_default: false,
            upgrade: Some(ModelUpgrade {
                id: "gpt-5.2-codex".to_string(),
                reasoning_effort_mapping: None,
                migration_config_key: HIDE_GPT_5_2_CODEX_MIGRATION_PROMPT_CONFIG.to_string(),
            }),
            show_in_picker: false,
        },
        ModelPreset {
            id: "gpt-5.1-codex-mini".to_string(),
            model: "gpt-5.1-codex-mini".to_string(),
            display_name: "gpt-5.1-codex-mini".to_string(),
            description: "Optimized for Code. Cheaper, faster, but less capable.".to_string(),
            default_reasoning_effort: ReasoningEffort::Medium,
            supported_reasoning_efforts: vec![
                ReasoningEffortPreset {
                    effort: ReasoningEffort::Medium,
                    description: "Dynamically adjusts reasoning based on the task".to_string(),
                },
                ReasoningEffortPreset {
                    effort: ReasoningEffort::High,
                    description: "Maximizes reasoning depth for complex or ambiguous problems"
                        .to_string(),
                },
            ],
            supported_text_verbosity: ALL_TEXT_VERBOSITY,
            is_default: false,
            upgrade: Some(ModelUpgrade {
                id: "gpt-5.2-codex".to_string(),
                reasoning_effort_mapping: None,
                migration_config_key: HIDE_GPT_5_2_CODEX_MIGRATION_PROMPT_CONFIG.to_string(),
            }),
            show_in_picker: true,
        },
        ModelPreset {
            id: "gpt-5.1".to_string(),
            model: "gpt-5.1".to_string(),
            display_name: "gpt-5.1".to_string(),
            description: "Broad world knowledge with strong general reasoning.".to_string(),
            default_reasoning_effort: ReasoningEffort::Medium,
            supported_reasoning_efforts: vec![
                ReasoningEffortPreset {
                    effort: ReasoningEffort::Low,
                    description:
                        "Balances speed with some reasoning; useful for straightforward queries and short explanations"
                            .to_string(),
                },
                ReasoningEffortPreset {
                    effort: ReasoningEffort::Medium,
                    description:
                        "Provides a solid balance of reasoning depth and latency for general-purpose tasks"
                            .to_string(),
                },
                ReasoningEffortPreset {
                    effort: ReasoningEffort::High,
                    description: "Maximizes reasoning depth for complex or ambiguous problems".to_string(),
                },
            ],
            supported_text_verbosity: ALL_TEXT_VERBOSITY,
            is_default: false,
            upgrade: Some(ModelUpgrade {
                id: "gpt-5.2-codex".to_string(),
                reasoning_effort_mapping: None,
                migration_config_key: HIDE_GPT_5_2_CODEX_MIGRATION_PROMPT_CONFIG.to_string(),
            }),
            show_in_picker: false,
        },
        // Deprecated GPT-5 variants kept for migrations / config compatibility.
        ModelPreset {
            id: "gpt-5-codex".to_string(),
            model: "gpt-5-codex".to_string(),
            display_name: "gpt-5-codex".to_string(),
            description: "Optimized for Code.".to_string(),
            default_reasoning_effort: ReasoningEffort::Medium,
            supported_reasoning_efforts: vec![
                ReasoningEffortPreset {
                    effort: ReasoningEffort::Low,
                    description: "Fastest responses with limited reasoning".to_string(),
                },
                ReasoningEffortPreset {
                    effort: ReasoningEffort::Medium,
                    description: "Dynamically adjusts reasoning based on the task".to_string(),
                },
                ReasoningEffortPreset {
                    effort: ReasoningEffort::High,
                    description: "Maximizes reasoning depth for complex or ambiguous problems".to_string(),
                },
            ],
            supported_text_verbosity: ALL_TEXT_VERBOSITY,
            is_default: false,
            upgrade: Some(ModelUpgrade {
                id: "gpt-5.2-codex".to_string(),
                reasoning_effort_mapping: None,
                migration_config_key: HIDE_GPT_5_2_CODEX_MIGRATION_PROMPT_CONFIG.to_string(),
            }),
            show_in_picker: false,
        },
        ModelPreset {
            id: "gpt-5-codex-mini".to_string(),
            model: "gpt-5-codex-mini".to_string(),
            display_name: "gpt-5-codex-mini".to_string(),
            description: "Optimized for Code. Cheaper, faster, but less capable.".to_string(),
            default_reasoning_effort: ReasoningEffort::Medium,
            supported_reasoning_efforts: vec![
                ReasoningEffortPreset {
                    effort: ReasoningEffort::Medium,
                    description: "Dynamically adjusts reasoning based on the task".to_string(),
                },
                ReasoningEffortPreset {
                    effort: ReasoningEffort::High,
                    description: "Maximizes reasoning depth for complex or ambiguous problems".to_string(),
                },
            ],
            supported_text_verbosity: ALL_TEXT_VERBOSITY,
            is_default: false,
            upgrade: Some(ModelUpgrade {
                id: "gpt-5.2-codex".to_string(),
                reasoning_effort_mapping: None,
                migration_config_key: HIDE_GPT_5_2_CODEX_MIGRATION_PROMPT_CONFIG.to_string(),
            }),
            show_in_picker: false,
        },
        ModelPreset {
            id: "gpt-5".to_string(),
            model: "gpt-5".to_string(),
            display_name: "gpt-5".to_string(),
            description: "Broad world knowledge with strong general reasoning.".to_string(),
            default_reasoning_effort: ReasoningEffort::Medium,
            supported_reasoning_efforts: vec![
                ReasoningEffortPreset {
                    effort: ReasoningEffort::Minimal,
                    description: "Fastest responses with little reasoning".to_string(),
                },
                ReasoningEffortPreset {
                    effort: ReasoningEffort::Low,
                    description:
                        "Balances speed with some reasoning; useful for straightforward queries and short explanations"
                            .to_string(),
                },
                ReasoningEffortPreset {
                    effort: ReasoningEffort::Medium,
                    description:
                        "Provides a solid balance of reasoning depth and latency for general-purpose tasks"
                            .to_string(),
                },
                ReasoningEffortPreset {
                    effort: ReasoningEffort::High,
                    description: "Maximizes reasoning depth for complex or ambiguous problems".to_string(),
                },
            ],
            supported_text_verbosity: ALL_TEXT_VERBOSITY,
            is_default: false,
            upgrade: Some(ModelUpgrade {
                id: "gpt-5.2-codex".to_string(),
                reasoning_effort_mapping: None,
                migration_config_key: HIDE_GPT_5_2_CODEX_MIGRATION_PROMPT_CONFIG.to_string(),
            }),
            show_in_picker: false,
        },
    ]
});

pub fn builtin_model_presets(auth_mode: Option<AuthMode>) -> Vec<ModelPreset> {
    PRESETS
        .iter()
        .filter(|preset| match auth_mode {
            Some(AuthMode::ApiKey) => preset.id != "gpt-5.2-codex",
            _ => true,
        })
        .filter(|preset| preset.show_in_picker)
        .cloned()
        .collect()
}

// todo(aibrahim): remove this once we migrate tests
pub fn all_model_presets() -> &'static Vec<ModelPreset> {
    &PRESETS
}

fn find_preset_for_model(model: &str) -> Option<&'static ModelPreset> {
    let model_lower = model.to_ascii_lowercase();

    PRESETS.iter().find(|preset| {
        preset.model.eq_ignore_ascii_case(&model_lower)
            || preset.id.eq_ignore_ascii_case(&model_lower)
            || preset.display_name.eq_ignore_ascii_case(&model_lower)
    })
}

fn reasoning_effort_rank(effort: ReasoningEffort) -> u8 {
    match effort {
        ReasoningEffort::Minimal => 0,
        ReasoningEffort::Low => 1,
        ReasoningEffort::Medium => 2,
        ReasoningEffort::High => 3,
        ReasoningEffort::XHigh => 4,
    }
}

pub fn clamp_reasoning_effort_for_model(
    model: &str,
    requested: ReasoningEffort,
) -> ReasoningEffort {
    let Some(preset) = find_preset_for_model(model) else {
        return requested;
    };

    if preset
        .supported_reasoning_efforts
        .iter()
        .any(|opt| opt.effort == requested)
    {
        return requested;
    }

    let requested_rank = reasoning_effort_rank(requested);

    preset
        .supported_reasoning_efforts
        .iter()
        .min_by_key(|opt| {
            let rank = reasoning_effort_rank(opt.effort);
            (requested_rank.abs_diff(rank), u8::MAX - rank)
        })
        .map(|opt| opt.effort)
        .unwrap_or(requested)
}

pub fn allowed_text_verbosity_for_model(model: &str) -> &'static [TextVerbosityConfig] {
    find_preset_for_model(model)
        .map(|preset| preset.supported_text_verbosity)
        .unwrap_or(ALL_TEXT_VERBOSITY)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_one_default_model_is_configured() {
        assert_eq!(PRESETS.iter().filter(|preset| preset.is_default).count(), 1);
    }

    #[test]
    fn gpt_5_2_codex_hidden_for_api_key_auth() {
        let presets = builtin_model_presets(Some(AuthMode::ApiKey));
        assert!(presets
            .iter()
            .all(|preset| preset.id != "gpt-5.2-codex"));
    }

    #[test]
    fn clamp_reasoning_effort_downgrades_to_supported_level() {
        let clamped = clamp_reasoning_effort_for_model(
            "gpt-5.1-codex",
            ReasoningEffort::XHigh,
        );
        assert_eq!(clamped, ReasoningEffort::High);

        let clamped_minimal =
            clamp_reasoning_effort_for_model("gpt-5.1-codex-mini", ReasoningEffort::Minimal);
        assert_eq!(clamped_minimal, ReasoningEffort::Medium);
    }
}
