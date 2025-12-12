use crate::config_types::ReasoningEffort;

const GPT5_1_CODEX_MAX_EFFORTS: &[ReasoningEffort] = &[
    ReasoningEffort::Low,
    ReasoningEffort::Medium,
    ReasoningEffort::High,
    ReasoningEffort::XHigh,
];

const GPT5_1_CODEX_EFFORTS: &[ReasoningEffort] = &[
    ReasoningEffort::Low,
    ReasoningEffort::Medium,
    ReasoningEffort::High,
];

const GPT5_1_CODEX_MINI_EFFORTS: &[ReasoningEffort] = &[
    ReasoningEffort::Medium,
    ReasoningEffort::High,
];

const GPT5_1_EFFORTS: &[ReasoningEffort] = &[
    ReasoningEffort::Low,
    ReasoningEffort::Medium,
    ReasoningEffort::High,
];

const GPT5_2_EFFORTS: &[ReasoningEffort] = &[
    ReasoningEffort::Low,
    ReasoningEffort::Medium,
    ReasoningEffort::High,
    ReasoningEffort::XHigh,
];

const GPT5_CODEX_EFFORTS: &[ReasoningEffort] = &[
    ReasoningEffort::Low,
    ReasoningEffort::Medium,
    ReasoningEffort::High,
];

const GPT5_CODEX_MINI_EFFORTS: &[ReasoningEffort] = &[
    ReasoningEffort::Medium,
    ReasoningEffort::High,
];

const GPT5_EFFORTS: &[ReasoningEffort] = &[
    ReasoningEffort::Minimal,
    ReasoningEffort::Low,
    ReasoningEffort::Medium,
    ReasoningEffort::High,
];

const CODEX_FALLBACK_EFFORTS: &[ReasoningEffort] = &[
    ReasoningEffort::Low,
    ReasoningEffort::Medium,
    ReasoningEffort::High,
];

const DEFAULT_EFFORTS: &[ReasoningEffort] = &[
    ReasoningEffort::Minimal,
    ReasoningEffort::Low,
    ReasoningEffort::Medium,
    ReasoningEffort::High,
];

fn reasoning_effort_rank(effort: ReasoningEffort) -> u8 {
    match effort {
        ReasoningEffort::Minimal => 0,
        ReasoningEffort::Low => 1,
        ReasoningEffort::Medium => 2,
        ReasoningEffort::High => 3,
        ReasoningEffort::XHigh => 4,
        ReasoningEffort::None => 5,
    }
}

pub fn supported_reasoning_efforts_for_model(model: &str) -> &'static [ReasoningEffort] {
    let lower = model.to_ascii_lowercase();

    if lower.starts_with("gpt-5.1-codex-max") {
        return GPT5_1_CODEX_MAX_EFFORTS;
    }

    if lower.starts_with("gpt-5.1-codex-mini") {
        return GPT5_1_CODEX_MINI_EFFORTS;
    }

    if lower.starts_with("gpt-5.1-codex") {
        return GPT5_1_CODEX_EFFORTS;
    }

    if lower.starts_with("gpt-5.1") {
        return GPT5_1_EFFORTS;
    }

    if lower.starts_with("gpt-5.2") || lower.starts_with("test-gpt-5.2") {
        return GPT5_2_EFFORTS;
    }

    if lower.starts_with("gpt-5-codex-mini") {
        return GPT5_CODEX_MINI_EFFORTS;
    }

    if lower.starts_with("gpt-5-codex") {
        return GPT5_CODEX_EFFORTS;
    }

    if lower.starts_with("gpt-5") {
        return GPT5_EFFORTS;
    }

    if lower.starts_with("codex-") {
        return CODEX_FALLBACK_EFFORTS;
    }

    DEFAULT_EFFORTS
}

pub fn clamp_reasoning_effort_for_model(
    model: &str,
    requested: ReasoningEffort,
) -> ReasoningEffort {
    let allowed = supported_reasoning_efforts_for_model(model);
    if allowed.iter().any(|effort| *effort == requested) {
        return requested;
    }

    let requested_rank = reasoning_effort_rank(requested);

    allowed
        .iter()
        .min_by_key(|effort| {
            let rank = reasoning_effort_rank(**effort);
            (requested_rank.abs_diff(rank), u8::MAX - rank)
        })
        .copied()
        .unwrap_or(requested)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_xhigh_to_high_for_gpt5() {
        let clamped = clamp_reasoning_effort_for_model("gpt-5", ReasoningEffort::XHigh);
        assert_eq!(clamped, ReasoningEffort::High);
    }

    #[test]
    fn clamps_minimal_up_for_codex_mini() {
        let clamped =
            clamp_reasoning_effort_for_model("gpt-5.1-codex-mini", ReasoningEffort::Minimal);
        assert_eq!(clamped, ReasoningEffort::Medium);
    }

    #[test]
    fn keeps_supported_effort() {
        let clamped = clamp_reasoning_effort_for_model("gpt-5.1-codex-max", ReasoningEffort::High);
        assert_eq!(clamped, ReasoningEffort::High);
    }

    #[test]
    fn gpt5_2_supports_xhigh() {
        let clamped = clamp_reasoning_effort_for_model("gpt-5.2", ReasoningEffort::XHigh);
        assert_eq!(clamped, ReasoningEffort::XHigh);
    }
}
