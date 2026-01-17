use std::collections::HashSet;

use code_common::model_presets::ModelPreset;
use code_common::model_presets::ModelUpgrade;
use code_common::model_presets::ReasoningEffortPreset;
use code_core::config_types::TextVerbosity as TextVerbosityConfig;
use code_core::protocol_config_types::ReasoningEffort as ProtocolReasoningEffort;
use code_protocol::openai_models::ModelInfo;
use code_protocol::openai_models::ModelVisibility;
use code_protocol::openai_models::ReasoningEffort as RemoteReasoningEffort;

const REMOTE_TEXT_VERBOSITY_ALL: &[TextVerbosityConfig] = &[
    TextVerbosityConfig::Low,
    TextVerbosityConfig::Medium,
    TextVerbosityConfig::High,
];
const REMOTE_TEXT_VERBOSITY_MEDIUM: &[TextVerbosityConfig] = &[TextVerbosityConfig::Medium];

pub(crate) fn merge_remote_models(remote_models: Vec<ModelInfo>, local_presets: Vec<ModelPreset>) -> Vec<ModelPreset> {
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
        preset.is_default = false;
        remote_presets.push(preset);
    }

    remote_presets.retain(|preset| preset.show_in_picker);
    if let Some(default) = remote_presets.first_mut() {
        default.is_default = true;
    }

    remote_presets
}

fn model_info_to_preset(info: ModelInfo) -> ModelPreset {
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
        show_in_picker,
    }
}

fn map_reasoning_effort(effort: RemoteReasoningEffort) -> ProtocolReasoningEffort {
    match effort {
        RemoteReasoningEffort::None => ProtocolReasoningEffort::Minimal,
        RemoteReasoningEffort::Minimal => ProtocolReasoningEffort::Minimal,
        RemoteReasoningEffort::Low => ProtocolReasoningEffort::Low,
        RemoteReasoningEffort::Medium => ProtocolReasoningEffort::Medium,
        RemoteReasoningEffort::High => ProtocolReasoningEffort::High,
        RemoteReasoningEffort::XHigh => ProtocolReasoningEffort::XHigh,
    }
}
