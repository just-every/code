use codex_app_server_protocol::AuthMode;
use codex_protocol::openai_models::{ModelPreset, ModelUpgrade, ReasoningEffort, ReasoningEffortPreset};
use once_cell::sync::Lazy;

pub const HIDE_GPT5_1_MIGRATION_PROMPT_CONFIG: &str = "hide_gpt5_1_migration_prompt";
pub const HIDE_GPT_5_1_CODEX_MAX_MIGRATION_PROMPT_CONFIG: &str =
    "hide_gpt-5.1-codex-max_migration_prompt";

// Built-in model presets exposed when the backend model list is unavailable.
static PRESETS: Lazy<Vec<ModelPreset>> = Lazy::new(|| {
    vec![
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
            is_default: true,
            upgrade: None,
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
                    description: "Maximizes reasoning depth for complex or ambiguous problems".to_string(),
                },
            ],
            is_default: false,
            upgrade: Some(ModelUpgrade {
                id: "gpt-5.1-codex-max".to_string(),
                reasoning_effort_mapping: None,
                migration_config_key: HIDE_GPT_5_1_CODEX_MAX_MIGRATION_PROMPT_CONFIG.to_string(),
            }),
            show_in_picker: true,
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
                    description: "Maximizes reasoning depth for complex or ambiguous problems".to_string(),
                },
            ],
            is_default: false,
            upgrade: Some(ModelUpgrade {
                id: "gpt-5.1-codex-max".to_string(),
                reasoning_effort_mapping: None,
                migration_config_key: HIDE_GPT_5_1_CODEX_MAX_MIGRATION_PROMPT_CONFIG.to_string(),
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
            is_default: false,
            upgrade: Some(ModelUpgrade {
                id: "gpt-5.1-codex-max".to_string(),
                reasoning_effort_mapping: None,
                migration_config_key: HIDE_GPT_5_1_CODEX_MAX_MIGRATION_PROMPT_CONFIG.to_string(),
            }),
            show_in_picker: true,
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
            is_default: false,
            upgrade: Some(ModelUpgrade {
                id: "gpt-5.1-codex-max".to_string(),
                reasoning_effort_mapping: None,
                migration_config_key: HIDE_GPT_5_1_CODEX_MAX_MIGRATION_PROMPT_CONFIG.to_string(),
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
            is_default: false,
            upgrade: Some(ModelUpgrade {
                id: "gpt-5.1-codex-mini".to_string(),
                reasoning_effort_mapping: None,
                migration_config_key: HIDE_GPT5_1_MIGRATION_PROMPT_CONFIG.to_string(),
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
            is_default: false,
            upgrade: Some(ModelUpgrade {
                id: "gpt-5.1-codex-max".to_string(),
                reasoning_effort_mapping: None,
                migration_config_key: HIDE_GPT_5_1_CODEX_MAX_MIGRATION_PROMPT_CONFIG.to_string(),
            }),
            show_in_picker: false,
        },
    ]
});

pub(crate) fn builtin_model_presets(_auth_mode: Option<AuthMode>) -> Vec<ModelPreset> {
    PRESETS.clone()
}

pub fn all_model_presets() -> &'static Vec<ModelPreset> {
    &PRESETS
}

fn reasoning_effort_rank(effort: ReasoningEffort) -> u8 {
    match effort {
        ReasoningEffort::None => 0,
        ReasoningEffort::Minimal => 1,
        ReasoningEffort::Low => 2,
        ReasoningEffort::Medium => 3,
        ReasoningEffort::High => 4,
        ReasoningEffort::XHigh => 5,
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

fn find_preset_for_model(model: &str) -> Option<&'static ModelPreset> {
    PRESETS
        .iter()
        .find(|preset| preset.model.eq_ignore_ascii_case(model))
}
