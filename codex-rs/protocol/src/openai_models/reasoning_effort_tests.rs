use super::super::ReasoningEffortPreset;
use super::super::tests::test_model;
use super::ReasoningEffort;
use pretty_assertions::assert_eq;

#[test]
fn resolves_ultra_using_model_override_and_supported_fallbacks() {
    use ReasoningEffort::High;
    use ReasoningEffort::Low;
    use ReasoningEffort::Max;
    use ReasoningEffort::Medium;
    use ReasoningEffort::Ultra;
    use ReasoningEffort::XHigh;

    for (supported, multi_agent_effort, expected) in [
        (vec![High, Max, Ultra], Some(High), High),
        (vec![High, Max, Ultra], None, Max),
        (vec![High, XHigh, Ultra], None, XHigh),
        (vec![High, XHigh, Ultra], Some(Ultra), XHigh),
        (vec![High, XHigh, Ultra], Some(Low), XHigh),
        (vec![], None, Medium),
    ] {
        let mut model = test_model(/*spec*/ None);
        model.supported_reasoning_levels = supported
            .into_iter()
            .map(|effort| ReasoningEffortPreset {
                effort,
                description: String::new(),
            })
            .collect();
        model.multi_agent_reasoning_effort = multi_agent_effort;
        assert_eq!(model.resolve_reasoning_effort(Ultra), expected);
    }
}

#[test]
fn preserves_native_efforts_and_translates_persistent_mode() {
    let model = test_model(/*spec*/ None);
    for effort in [
        ReasoningEffort::Low,
        ReasoningEffort::Medium,
        ReasoningEffort::High,
        ReasoningEffort::Max,
        ReasoningEffort::Custom("future-effort".to_string()),
    ] {
        assert_eq!(model.resolve_reasoning_effort(effort.clone()), effort);
    }
    assert_eq!(
        model.resolve_reasoning_effort(ReasoningEffort::Persistent),
        ReasoningEffort::Custom("disabled".to_string()),
    );
}
