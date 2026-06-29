use std::collections::HashSet;

use code_common::model_presets::ModelPreset;
use code_common::model_presets::ModelUpgrade;
use code_common::model_presets::ReasoningEffortPreset;
use code_common::model_presets::model_preset_available_for_auth;
use code_core::config_types::TextVerbosity as TextVerbosityConfig;
use code_core::protocol_config_types::ReasoningEffort as ProtocolReasoningEffort;
use code_login::AuthMode;
use code_protocol::openai_models::ModelInfo;
use code_protocol::openai_models::ModelVisibility;
use code_protocol::openai_models::ReasoningEffort as RemoteReasoningEffort;

const REMOTE_TEXT_VERBOSITY_ALL: &[TextVerbosityConfig] = &[
    TextVerbosityConfig::Low,
    TextVerbosityConfig::Medium,
    TextVerbosityConfig::High,
];
const REMOTE_TEXT_VERBOSITY_MEDIUM: &[TextVerbosityConfig] = &[TextVerbosityConfig::Medium];

pub(crate) fn merge_remote_models(
    remote_models: Vec<ModelInfo>,
    local_presets: Vec<ModelPreset>,
    auth_mode: Option<AuthMode>,
    supports_pro_only_models: bool,
) -> Vec<ModelPreset> {
    if remote_models.is_empty() {
        return local_presets;
    }

    let mut remote_models = remote_models;
    remote_models.sort_by(|a, b| a.priority.cmp(&b.priority));
    let mut remote_presets: Vec<ModelPreset> = remote_models.into_iter().map(model_info_to_preset).collect();

    let remote_slugs: HashSet<String> = remote_presets
        .iter()
        .map(|preset| preset.model.to_ascii_lowercase())
        .collect();

    for preset in remote_presets.iter_mut() {
        preset.is_default = false;
    }

    for mut preset in local_presets {
        if remote_slugs.contains(&preset.model.to_ascii_lowercase()) {
            continue;
        }
        if should_skip_chatgpt_account_catalog_fallback(&preset, auth_mode) {
            continue;
        }
        preset.is_default = false;
        remote_presets.push(preset);
    }

    remote_presets.retain(|preset| {
        preset.show_in_picker
            && model_preset_available_for_auth(preset, auth_mode, supports_pro_only_models)
    });
    if let Some(default) = remote_presets.first_mut() {
        default.is_default = true;
    }

    remote_presets
}

fn should_skip_chatgpt_account_catalog_fallback(
    preset: &ModelPreset,
    auth_mode: Option<AuthMode>,
) -> bool {
    if !auth_mode.is_some_and(AuthMode::is_chatgpt) {
        return false;
    }

    matches!(
        preset.model.to_ascii_lowercase().as_str(),
        "gpt-5.2" | "gpt-5.2-codex" | "gpt-5.3-codex" | "gpt-5.3-codex-spark"
    )
}

fn model_info_to_preset(info: ModelInfo) -> ModelPreset {
    let pro_only = info.slug.eq_ignore_ascii_case("gpt-5.3-codex-spark");
    let show_in_picker = info.visibility == ModelVisibility::List
        && !info.slug.eq_ignore_ascii_case("gpt-5.1-codex");

    let supported_text_verbosity = if info.support_verbosity {
        REMOTE_TEXT_VERBOSITY_ALL
    } else {
        REMOTE_TEXT_VERBOSITY_MEDIUM
    };

    let supported_reasoning_efforts = info
        .supported_reasoning_levels
        .into_iter()
        .map(|preset| ReasoningEffortPreset {
            effort: map_reasoning_effort(preset.effort),
            description: preset.description,
        })
        .collect();

    ModelPreset {
        id: info.slug.clone(),
        model: info.slug.clone(),
        display_name: info.display_name,
        description: info.description.unwrap_or_default(),
        default_reasoning_effort: map_reasoning_effort(
            info.default_reasoning_level
                .unwrap_or(RemoteReasoningEffort::None),
        ),
        supported_reasoning_efforts,
        supported_text_verbosity,
        is_default: false,
        upgrade: info.upgrade.map(|upgrade| ModelUpgrade {
            id: upgrade.model,
            reasoning_effort_mapping: None,
            migration_config_key: info.slug,
        }),
        pro_only,
        show_in_picker,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn remote_model(slug: &str, priority: i32) -> ModelInfo {
        serde_json::from_value(serde_json::json!({
            "slug": slug,
            "display_name": slug,
            "description": format!("{slug} desc"),
            "default_reasoning_level": "medium",
            "supported_reasoning_levels": [
                {"effort": "low", "description": "low"},
                {"effort": "medium", "description": "medium"}
            ],
            "shell_type": "shell_command",
            "visibility": "list",
            "supported_in_api": true,
            "priority": priority,
            "upgrade": null,
            "base_instructions": "",
            "supports_reasoning_summaries": false,
            "support_verbosity": false,
            "default_verbosity": null,
            "apply_patch_tool_type": null,
            "truncation_policy": {"mode": "bytes", "limit": 10000},
            "supports_parallel_tool_calls": false,
            "context_window": null,
            "experimental_supported_tools": []
        }))
        .expect("valid model")
    }

    #[test]
    fn chatgpt_remote_catalog_does_not_resurrect_sunset_local_models() {
        let merged = merge_remote_models(
            vec![remote_model("gpt-5.5", 0), remote_model("gpt-5.4", 1)],
            code_common::model_presets::builtin_model_presets(Some(AuthMode::ChatGPT), true),
            Some(AuthMode::ChatGPT),
            true,
        );

        let ids: Vec<&str> = merged.iter().map(|preset| preset.id.as_str()).collect();
        assert!(ids.contains(&"gpt-5.5"));
        assert!(ids.contains(&"gpt-5.4"));
        assert!(!ids.contains(&"gpt-5.2"));
        assert!(!ids.contains(&"gpt-5.2-codex"));
        assert!(!ids.contains(&"gpt-5.3-codex"));
        assert!(!ids.contains(&"gpt-5.3-codex-spark"));
    }

    #[test]
    fn api_key_presets_keep_gpt_5_3_codex_available() {
        let merged = merge_remote_models(
            vec![remote_model("gpt-5.5", 0)],
            code_common::model_presets::builtin_model_presets(Some(AuthMode::ApiKey), false),
            Some(AuthMode::ApiKey),
            false,
        );

        assert!(merged.iter().any(|preset| preset.id == "gpt-5.3-codex"));
    }
}

fn map_reasoning_effort(effort: RemoteReasoningEffort) -> ProtocolReasoningEffort {
    match effort {
        RemoteReasoningEffort::None => ProtocolReasoningEffort::Minimal,
        RemoteReasoningEffort::Minimal => ProtocolReasoningEffort::Minimal,
        RemoteReasoningEffort::Low => ProtocolReasoningEffort::Low,
        RemoteReasoningEffort::Medium => ProtocolReasoningEffort::Medium,
        RemoteReasoningEffort::High => ProtocolReasoningEffort::High,
        RemoteReasoningEffort::XHigh | RemoteReasoningEffort::Max => {
            ProtocolReasoningEffort::XHigh
        }
        RemoteReasoningEffort::Custom(_) => ProtocolReasoningEffort::Medium,
    }
}
