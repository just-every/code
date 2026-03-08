use crate::config_types::Personality;
use crate::config_types::ContextMode;
use crate::config_types::ReasoningEffort;
use crate::config_types::ReasoningSummary;
use crate::tool_apply_patch::ApplyPatchToolType;
use code_protocol::openai_models::ConfigShellToolType;
use code_protocol::openai_models::ModelInfo;
use code_protocol::openai_models::ModelsResponse;
use code_protocol::openai_models::TruncationMode;
use code_protocol::openai_models::WebSearchToolType;
use code_protocol::protocol::TruncationPolicy;
use once_cell::sync::Lazy;

/// The `instructions` field in the payload sent to a model should always start
/// with this content.
const BASE_INSTRUCTIONS: &str = include_str!("../prompt.md");
const BASE_INSTRUCTIONS_WITH_APPLY_PATCH: &str =
    include_str!("../prompt_with_apply_patch_instructions.md");
const GPT_5_CODEX_INSTRUCTIONS: &str = include_str!("../gpt_5_codex_prompt.md");
const GPT_5_1_INSTRUCTIONS: &str = include_str!("../gpt_5_1_prompt.md");
const GPT_5_2_INSTRUCTIONS: &str = include_str!("../gpt_5_2_prompt.md");
const GPT_5_1_CODEX_MAX_INSTRUCTIONS: &str = include_str!("../gpt-5.1-codex-max_prompt.md");
const GPT_5_2_CODEX_INSTRUCTIONS: &str = include_str!("../gpt-5.2-codex_prompt.md");

const GPT_5_2_CODEX_INSTRUCTIONS_TEMPLATE: &str = include_str!(
    "../templates/model_instructions/gpt-5.2-codex_instructions_template.md",
);
const PERSONALITY_FRIENDLY: &str =
    include_str!("../templates/personalities/gpt-5.2-codex_friendly.md");
const PERSONALITY_PRAGMATIC: &str =
    include_str!("../templates/personalities/gpt-5.2-codex_pragmatic.md");

const CONTEXT_WINDOW_272K: u64 = 272_000;
const CONTEXT_WINDOW_200K: u64 = 200_000;
const CONTEXT_WINDOW_128K: u64 = 128_000;
const CONTEXT_WINDOW_96K: u64 = 96_000;
const CONTEXT_WINDOW_16K: u64 = 16_385;
const CONTEXT_WINDOW_1M: u64 = 1_047_576;
const MAX_OUTPUT_DEFAULT: u64 = 128_000;
const IMAGE_GENERATION_TOOL: &str = "image_generation";

static UPSTREAM_MODELS: Lazy<Vec<ModelInfo>> = Lazy::new(|| {
    serde_json::from_str::<ModelsResponse>(include_str!("../../../codex-rs/core/models.json"))
        .map(|response| response.models)
        .unwrap_or_else(|err| panic!("failed to parse upstream models.json: {err}"))
});

pub const STANDARD_CONTEXT_WINDOW_272K: u64 = CONTEXT_WINDOW_272K;
pub const EXTENDED_CONTEXT_WINDOW_1M: u64 = CONTEXT_WINDOW_1M;

/// A model family is a group of models that share certain characteristics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelFamily {
    /// The full model slug used to derive this model family, e.g.
    /// "gpt-4.1-2025-04-14".
    pub slug: String,

    /// The model family name, e.g. "gpt-4.1".
    pub family: String,

    /// True if the model needs additional instructions on how to use the
    /// "virtual" `apply_patch` CLI.
    pub needs_special_apply_patch_instructions: bool,

    /// Maximum supported context window, if known.
    pub context_window: Option<u64>,

    /// Maximum number of output tokens that can be generated for the model.
    pub max_output_tokens: Option<u64>,

    /// Truncation policy to apply when recording tool outputs in the model context.
    pub truncation_policy: TruncationPolicy,

    /// Token threshold where we should automatically compact history.
    auto_compact_token_limit: Option<i64>,

    // Whether the `reasoning` field can be set when making a request to this
    // model family. Note it has `effort` and `summary` subfields (though
    // `summary` is optional).
    pub supports_reasoning_summaries: bool,

    /// The reasoning effort to use for this model family when none is explicitly chosen.
    pub default_reasoning_effort: Option<ReasoningEffort>,

    /// The reasoning summary setting to use when requests don't override it.
    pub default_reasoning_summary: ReasoningSummary,

    /// Whether this model supports parallel tool calls when using the
    /// Responses API.
    pub supports_parallel_tool_calls: bool,

    /// Prefer websocket transport for this model when supported by the provider.
    pub prefer_websockets: bool,

    // This should be set to true when the model expects a tool named
    // "local_shell" to be provided. Its contract must be understood natively by
    // the model such that its description can be omitted.
    // See https://platform.openai.com/docs/guides/tools-local-shell
    pub uses_local_shell_tool: bool,

    /// Present if the model performs better when `apply_patch` is provided as
    /// a tool call instead of just a bash command
    pub apply_patch_tool_type: Option<ApplyPatchToolType>,

    /// Whether web_search should request text-only or multimodal results.
    pub web_search_tool_type: WebSearchToolType,

    /// Whether responses can use `detail: "original"` for tool-returned images.
    pub supports_image_detail_original: bool,

    /// Whether this model supports image generation via the native Responses tool.
    pub supports_image_generation: bool,

    // Instructions to use for querying the model
    pub base_instructions: String,
}

pub(crate) fn base_instructions_override_for_personality(
    model: &str,
    personality: Option<Personality>,
) -> Option<String> {
    if !(model.starts_with("gpt-5.2-codex")
        || model.starts_with("gpt-5.3-codex")
        || model.starts_with("bengalfox")
        || model.starts_with("exp-codex")
        || model.starts_with("codex-1p"))
    {
        return None;
    }
    let personality_message = match personality {
        Some(Personality::None) => "",
        Some(Personality::Friendly) => PERSONALITY_FRIENDLY,
        Some(Personality::Pragmatic) => PERSONALITY_PRAGMATIC,
        None => "",
    };
    Some(
        GPT_5_2_CODEX_INSTRUCTIONS_TEMPLATE
            .replace("{{ personality }}", personality_message),
    )
}

macro_rules! model_family {
    (
        $slug:expr, $family:expr $(, $key:ident : $value:expr )* $(,)?
    ) => {{
        let slug_value = $slug;
        // defaults
        let mut mf = ModelFamily {
            slug: slug_value.to_string(),
            family: $family.to_string(),
            needs_special_apply_patch_instructions: false,
            context_window: Some(CONTEXT_WINDOW_272K),
            max_output_tokens: Some(MAX_OUTPUT_DEFAULT),
            truncation_policy: TruncationPolicy::Bytes(10_000),
            auto_compact_token_limit: None,
            supports_reasoning_summaries: false,
            default_reasoning_effort: None,
            default_reasoning_summary: ReasoningSummary::Auto,
            supports_parallel_tool_calls: false,
            prefer_websockets: false,
            uses_local_shell_tool: false,
            apply_patch_tool_type: None,
            web_search_tool_type: WebSearchToolType::Text,
            supports_image_detail_original: false,
            supports_image_generation: false,
            base_instructions: BASE_INSTRUCTIONS.to_string(),
        };
        // apply overrides
        $(
            mf.$key = $value;
        )*
        Some(apply_upstream_model_overrides(mf))
    }};
}

fn apply_upstream_model_overrides(mut family: ModelFamily) -> ModelFamily {
    let Some(model_info) = UPSTREAM_MODELS.iter().find(|model| model.slug == family.slug) else {
        return family;
    };

    family.base_instructions = model_info.base_instructions.clone();
    family.context_window = model_info.context_window.and_then(|limit| u64::try_from(limit).ok());
    family.default_reasoning_effort = model_info.default_reasoning_level.map(|effort| match effort {
        code_protocol::openai_models::ReasoningEffort::None
        | code_protocol::openai_models::ReasoningEffort::Minimal => ReasoningEffort::Minimal,
        code_protocol::openai_models::ReasoningEffort::Low => ReasoningEffort::Low,
        code_protocol::openai_models::ReasoningEffort::Medium => ReasoningEffort::Medium,
        code_protocol::openai_models::ReasoningEffort::High => ReasoningEffort::High,
        code_protocol::openai_models::ReasoningEffort::XHigh => ReasoningEffort::XHigh,
    });
    family.default_reasoning_summary = model_info.default_reasoning_summary.into();
    family.supports_reasoning_summaries = model_info.supports_reasoning_summaries;
    family.supports_parallel_tool_calls = model_info.supports_parallel_tool_calls;
    family.web_search_tool_type = model_info.web_search_tool_type;
    family.supports_image_detail_original = model_info.supports_image_detail_original;
    family.supports_image_generation = model_info
        .experimental_supported_tools
        .iter()
        .any(|tool| tool == IMAGE_GENERATION_TOOL);
    family.uses_local_shell_tool = matches!(model_info.shell_type, ConfigShellToolType::Local);
    family.auto_compact_token_limit = model_info.auto_compact_token_limit();
    family.truncation_policy = match model_info.truncation_policy.mode {
        TruncationMode::Bytes => TruncationPolicy::Bytes(
            usize::try_from(model_info.truncation_policy.limit).unwrap_or(10_000),
        ),
        TruncationMode::Tokens => TruncationPolicy::Tokens(
            usize::try_from(model_info.truncation_policy.limit).unwrap_or(10_000),
        ),
    };

    family
}

/// Returns a `ModelFamily` for the given model slug, or `None` if the slug
/// does not match any known model family.
pub fn find_family_for_model(slug: &str) -> Option<ModelFamily> {
    if slug.starts_with("o3") {
        model_family!(
            slug, "o3",
            supports_reasoning_summaries: true,
            needs_special_apply_patch_instructions: true,
            base_instructions: BASE_INSTRUCTIONS_WITH_APPLY_PATCH.to_string(),
            context_window: Some(CONTEXT_WINDOW_200K),
            max_output_tokens: Some(100_000),
        )
    } else if slug.starts_with("o4-mini") {
        model_family!(
            slug, "o4-mini",
            supports_reasoning_summaries: true,
            needs_special_apply_patch_instructions: true,
            base_instructions: BASE_INSTRUCTIONS_WITH_APPLY_PATCH.to_string(),
            context_window: Some(CONTEXT_WINDOW_200K),
            max_output_tokens: Some(100_000),
        )
    } else if slug.starts_with("codex-mini-latest") {
        model_family!(
            slug, "codex-mini-latest",
            supports_reasoning_summaries: true,
            uses_local_shell_tool: true,
            needs_special_apply_patch_instructions: true,
            base_instructions: BASE_INSTRUCTIONS_WITH_APPLY_PATCH.to_string(),
            context_window: Some(CONTEXT_WINDOW_200K),
            max_output_tokens: Some(100_000),
        )
    } else if slug.starts_with("gpt-4.1") {
        model_family!(
            slug, "gpt-4.1",
            needs_special_apply_patch_instructions: true,
            base_instructions: BASE_INSTRUCTIONS_WITH_APPLY_PATCH.to_string(),
            context_window: Some(CONTEXT_WINDOW_1M),
            max_output_tokens: Some(32_768),
        )
    } else if slug.starts_with("gpt-oss") || slug.starts_with("openai/gpt-oss") {
        model_family!(slug, "gpt-oss", apply_patch_tool_type: Some(ApplyPatchToolType::Function),
            uses_local_shell_tool: true,
            context_window: Some(CONTEXT_WINDOW_96K),
            max_output_tokens: Some(32_000))
    } else if slug.starts_with("gpt-4o") {
        model_family!(slug, "gpt-4o", needs_special_apply_patch_instructions: true,
            base_instructions: BASE_INSTRUCTIONS_WITH_APPLY_PATCH.to_string(),
            context_window: Some(CONTEXT_WINDOW_128K),
            max_output_tokens: Some(16_384))
    } else if slug.starts_with("gpt-3.5") {
        model_family!(slug, "gpt-3.5", needs_special_apply_patch_instructions: true,
            base_instructions: BASE_INSTRUCTIONS_WITH_APPLY_PATCH.to_string(),
            context_window: Some(CONTEXT_WINDOW_16K),
            max_output_tokens: Some(4_096))
    } else if slug.starts_with("test-gpt-5") {
        model_family!(
            slug, slug,
            supports_reasoning_summaries: true,
            base_instructions: GPT_5_CODEX_INSTRUCTIONS.to_string(),
            apply_patch_tool_type: Some(ApplyPatchToolType::Freeform),
            supports_parallel_tool_calls: true,
            default_reasoning_effort: Some(ReasoningEffort::Medium),
            truncation_policy: TruncationPolicy::Tokens(10_000),
        )
    } else if slug.starts_with("exp-codex") || slug.starts_with("codex-1p") {
        // Same defaults as gpt-5.2-codex.
        model_family!(
            slug, slug,
            supports_reasoning_summaries: true,
            base_instructions: GPT_5_2_CODEX_INSTRUCTIONS.to_string(),
            apply_patch_tool_type: Some(ApplyPatchToolType::Freeform),
            supports_parallel_tool_calls: true,
            truncation_policy: TruncationPolicy::Tokens(10_000),
        )
    } else if slug.starts_with("exp-") {
        model_family!(
            slug, slug,
            supports_reasoning_summaries: true,
            apply_patch_tool_type: Some(ApplyPatchToolType::Freeform),
            supports_parallel_tool_calls: true,
            default_reasoning_effort: Some(ReasoningEffort::Medium),
            truncation_policy: TruncationPolicy::Bytes(10_000),
        )
    } else if slug.starts_with("gpt-5.1-codex-max") {
        model_family!(
            slug, slug,
            supports_reasoning_summaries: true,
            base_instructions: GPT_5_1_CODEX_MAX_INSTRUCTIONS.to_string(),
            apply_patch_tool_type: Some(ApplyPatchToolType::Freeform),
            context_window: Some(CONTEXT_WINDOW_272K),
            max_output_tokens: Some(MAX_OUTPUT_DEFAULT),
            truncation_policy: TruncationPolicy::Tokens(10_000),
        )
    } else if slug.starts_with("codex-")
        || slug.starts_with("gpt-5-codex")
        || slug.starts_with("gpt-5.1-codex")
    {
        model_family!(
            slug, slug,
            supports_reasoning_summaries: true,
            base_instructions: GPT_5_CODEX_INSTRUCTIONS.to_string(),
            apply_patch_tool_type: Some(ApplyPatchToolType::Freeform),
            context_window: Some(CONTEXT_WINDOW_272K),
            max_output_tokens: Some(MAX_OUTPUT_DEFAULT),
            truncation_policy: TruncationPolicy::Tokens(10_000),
        )
    } else if slug.starts_with("gpt-5.2-codex") {
        // Same defaults as gpt-5.1-codex-max.
        model_family!(
            slug, slug,
            supports_reasoning_summaries: true,
            base_instructions: GPT_5_2_CODEX_INSTRUCTIONS.to_string(),
            apply_patch_tool_type: Some(ApplyPatchToolType::Freeform),
            supports_parallel_tool_calls: true,
            context_window: Some(CONTEXT_WINDOW_272K),
            max_output_tokens: Some(MAX_OUTPUT_DEFAULT),
            truncation_policy: TruncationPolicy::Tokens(10_000),
        )
    } else if slug.starts_with("gpt-5.3-codex") {
        // Same defaults as gpt-5.2-codex.
        model_family!(
            slug, slug,
            supports_reasoning_summaries: true,
            base_instructions: GPT_5_2_CODEX_INSTRUCTIONS.to_string(),
            apply_patch_tool_type: Some(ApplyPatchToolType::Freeform),
            supports_parallel_tool_calls: true,
            context_window: Some(CONTEXT_WINDOW_272K),
            max_output_tokens: Some(MAX_OUTPUT_DEFAULT),
            truncation_policy: TruncationPolicy::Tokens(10_000),
        )
    } else if slug.starts_with("bengalfox") {
        model_family!(
            slug, slug,
            supports_reasoning_summaries: true,
            base_instructions: GPT_5_2_CODEX_INSTRUCTIONS.to_string(),
            apply_patch_tool_type: Some(ApplyPatchToolType::Freeform),
            supports_parallel_tool_calls: true,
            context_window: Some(CONTEXT_WINDOW_272K),
            max_output_tokens: Some(MAX_OUTPUT_DEFAULT),
            truncation_policy: TruncationPolicy::Tokens(10_000),
        )
    } else if slug.starts_with("gpt-5.3") {
        model_family!(
            slug, "gpt-5.3",
            supports_reasoning_summaries: true,
            base_instructions: GPT_5_2_INSTRUCTIONS.to_string(),
            apply_patch_tool_type: Some(ApplyPatchToolType::Freeform),
            default_reasoning_effort: Some(ReasoningEffort::Medium),
            supports_parallel_tool_calls: true,
            context_window: Some(CONTEXT_WINDOW_272K),
            max_output_tokens: Some(MAX_OUTPUT_DEFAULT),
            truncation_policy: TruncationPolicy::Bytes(10_000),
        )
    } else if slug.starts_with("gpt-5.2") {
        model_family!(
            slug, "gpt-5.2",
            supports_reasoning_summaries: true,
            base_instructions: GPT_5_2_INSTRUCTIONS.to_string(),
            apply_patch_tool_type: Some(ApplyPatchToolType::Freeform),
            default_reasoning_effort: Some(ReasoningEffort::Medium),
            supports_parallel_tool_calls: true,
            context_window: Some(CONTEXT_WINDOW_272K),
            max_output_tokens: Some(MAX_OUTPUT_DEFAULT),
            truncation_policy: TruncationPolicy::Bytes(10_000),
        )
    } else if slug.starts_with("boomslang") {
        model_family!(
            slug, slug,
            supports_reasoning_summaries: true,
            base_instructions: GPT_5_2_INSTRUCTIONS.to_string(),
            apply_patch_tool_type: Some(ApplyPatchToolType::Freeform),
            default_reasoning_effort: Some(ReasoningEffort::Medium),
            supports_parallel_tool_calls: true,
            context_window: Some(CONTEXT_WINDOW_272K),
            max_output_tokens: Some(MAX_OUTPUT_DEFAULT),
            truncation_policy: TruncationPolicy::Bytes(10_000),
        )
    } else if slug.starts_with("gpt-5.1") {
        model_family!(
            slug, "gpt-5.1",
            supports_reasoning_summaries: true,
            base_instructions: GPT_5_1_INSTRUCTIONS.to_string(),
            apply_patch_tool_type: Some(ApplyPatchToolType::Freeform),
            default_reasoning_effort: Some(ReasoningEffort::Medium),
            supports_parallel_tool_calls: true,
            context_window: Some(CONTEXT_WINDOW_272K),
            max_output_tokens: Some(MAX_OUTPUT_DEFAULT),
            truncation_policy: TruncationPolicy::Bytes(10_000),
        )
    } else if slug.starts_with("gpt-5") {
        model_family!(
            slug, "gpt-5",
            supports_reasoning_summaries: true,
            base_instructions: BASE_INSTRUCTIONS.to_string(),
            context_window: Some(CONTEXT_WINDOW_272K),
            max_output_tokens: Some(MAX_OUTPUT_DEFAULT),
            truncation_policy: TruncationPolicy::Bytes(10_000),
        )
    } else {
        None
    }
}

pub fn derive_default_model_family(model: &str) -> ModelFamily {
    apply_upstream_model_overrides(ModelFamily {
        slug: model.to_string(),
        family: model.to_string(),
        needs_special_apply_patch_instructions: false,
        context_window: None,
        max_output_tokens: None,
        truncation_policy: TruncationPolicy::Bytes(10_000),
        auto_compact_token_limit: None,
        supports_reasoning_summaries: false,
        default_reasoning_effort: None,
        default_reasoning_summary: ReasoningSummary::Auto,
        supports_parallel_tool_calls: false,
        prefer_websockets: false,
        uses_local_shell_tool: false,
        apply_patch_tool_type: None,
        web_search_tool_type: WebSearchToolType::Text,
        supports_image_detail_original: false,
        supports_image_generation: false,
        base_instructions: BASE_INSTRUCTIONS.to_string(),
    })
}

impl ModelFamily {
    /// Token limit at which we should automatically compact, if known.
    pub fn auto_compact_token_limit(&self) -> Option<i64> {
        self.auto_compact_token_limit
            .or(self.context_window.map(Self::default_auto_compact_limit))
    }

    pub fn set_auto_compact_token_limit(&mut self, limit: Option<i64>) {
        self.auto_compact_token_limit = limit;
    }

    pub fn tool_output_max_bytes(&self) -> usize {
        match self.truncation_policy {
            TruncationPolicy::Bytes(limit) => limit,
            TruncationPolicy::Tokens(limit) => limit.saturating_mul(4),
        }
    }

    pub fn set_truncation_policy(&mut self, policy: TruncationPolicy) {
        self.truncation_policy = policy;
    }

    const fn default_auto_compact_limit(context_window: u64) -> i64 {
        // Match upstream behaviour: 90% of the context window.
        ((context_window as i64) * 9) / 10
    }
}

pub const fn default_auto_compact_limit_for_context_window(context_window: u64) -> i64 {
    ((context_window as i64) * 9) / 10
}

pub fn supports_extended_context(model: &str) -> bool {
    model.eq_ignore_ascii_case("gpt-5.4")
}

pub fn resolve_context_mode_limits(
    model: &str,
    mode: Option<ContextMode>,
    family: &ModelFamily,
) -> (Option<u64>, Option<i64>) {
    match mode {
        Some(ContextMode::OneM | ContextMode::Auto) if supports_extended_context(model) => (
            Some(EXTENDED_CONTEXT_WINDOW_1M),
            Some(default_auto_compact_limit_for_context_window(
                EXTENDED_CONTEXT_WINDOW_1M,
            )),
        ),
        Some(ContextMode::Disabled) => {
            (family.context_window, family.auto_compact_token_limit())
        }
        _ => (family.context_window, family.auto_compact_token_limit()),
    }
}
