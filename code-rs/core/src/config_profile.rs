use serde::Deserialize;
use std::path::PathBuf;

use crate::config_types::ReasoningEffort;
use crate::config_types::ReasoningSummary;
use crate::config_types::TextVerbosity;
use crate::protocol::AskForApproval;

/// Collection of common configuration options that a user can define as a unit
/// in `config.toml`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct ConfigProfile {
    pub model: Option<String>,
    pub planning_model: Option<String>,
    pub planning_model_reasoning_effort: Option<ReasoningEffort>,
    pub planning_use_chat_model: Option<bool>,
    pub review_model: Option<String>,
    pub review_resolve_model: Option<String>,
    pub review_resolve_model_reasoning_effort: Option<ReasoningEffort>,
    pub review_resolve_use_chat_model: Option<bool>,
    /// The key in the `model_providers` map identifying the
    /// [`ModelProviderInfo`] to use.
    pub model_provider: Option<String>,
    pub approval_policy: Option<AskForApproval>,
    pub disable_response_storage: Option<bool>,
    pub model_reasoning_effort: Option<ReasoningEffort>,
    pub preferred_model_reasoning_effort: Option<ReasoningEffort>,
    pub review_model_reasoning_effort: Option<ReasoningEffort>,
    pub review_use_chat_model: Option<bool>,
    pub auto_review_model: Option<String>,
    pub auto_review_model_reasoning_effort: Option<ReasoningEffort>,
    pub auto_review_use_chat_model: Option<bool>,
    pub auto_review_resolve_model: Option<String>,
    pub auto_review_resolve_model_reasoning_effort: Option<ReasoningEffort>,
    pub auto_review_resolve_use_chat_model: Option<bool>,
    pub model_reasoning_summary: Option<ReasoningSummary>,
    pub model_text_verbosity: Option<TextVerbosity>,
    pub chatgpt_base_url: Option<String>,
    pub experimental_instructions_file: Option<PathBuf>,
    pub compact_prompt_override: Option<String>,
    pub compact_prompt_override_file: Option<PathBuf>,

    /// When true, automatically switch to another connected account when the
    /// current account hits a rate/usage limit.
    pub auto_switch_accounts_on_rate_limit: Option<bool>,

    /// When true, fall back to an API key account only if every connected
    /// ChatGPT account is rate/usage limited.
    pub api_key_fallback_on_all_accounts_limited: Option<bool>,
}
