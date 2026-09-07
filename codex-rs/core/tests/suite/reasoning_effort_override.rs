//! Trusted reasoning-effort updates follow surviving history and the next turn's selected settings.

use codex_features::Feature;
use codex_protocol::openai_models::ReasoningEffort;
use codex_protocol::openai_models::ReasoningEffortPreset;
use codex_protocol::protocol::ThreadSettingsOverrides;
use core_test_support::responses;
use core_test_support::responses::ResponsesRequest;
use core_test_support::skip_if_no_network;
use core_test_support::submit_thread_settings;
use core_test_support::test_codex::TestCodexBuilder;
use core_test_support::test_codex::test_codex;
use pretty_assertions::assert_eq;
use serde_json::Value;
use test_case::test_case;

fn override_builder() -> TestCodexBuilder {
    test_codex()
        .with_model_info_override("gpt-5.4", |model| {
            model.use_responses_lite = true;
        })
        .with_config(|config| {
            config
                .features
                .enable(Feature::ReasoningEffortOverride)
                .expect("enable reasoning effort overrides");
            config.model_reasoning_effort = Some(ReasoningEffort::Medium);
        })
}

fn effort_updates(request: &ResponsesRequest) -> Vec<Value> {
    request
        .input()
        .into_iter()
        .filter(|item| item["type"] == "configuration_update")
        .collect()
}

fn effort_update(effort: ReasoningEffort) -> Value {
    serde_json::json!({
        "type": "configuration_update",
        "reasoning": {"effort": effort},
    })
}

fn message(role: &str, text: &str) -> Value {
    serde_json::json!({
        "type": "message",
        "role": role,
        "content": [{"type": "input_text", "text": text}],
    })
}

#[test_case(ReasoningEffort::High; "high to persistent and back")]
#[test_case(ReasoningEffort::Persistent; "persistent to high and back")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reasoning_effort_override_persistent_transitions(
    initial_effort: ReasoningEffort,
) -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));
    let server = responses::start_mock_server().await;
    let mut mocks = Vec::new();
    for id in ["first", "changed", "unchanged", "restored"] {
        mocks.push(
            responses::mount_sse_once(&server, responses::sse(vec![responses::ev_completed(id)]))
                .await,
        );
    }
    let high = effort_update(ReasoningEffort::High);
    let disabled = effort_update(ReasoningEffort::Custom("disabled".to_string()));
    let (changed_effort, initial_update, changed_update) =
        if initial_effort == ReasoningEffort::Persistent {
            (ReasoningEffort::High, disabled, high)
        } else {
            (ReasoningEffort::Persistent, high, disabled)
        };
    let test = override_builder().build_with_auto_env(&server).await?;
    for effort in [
        initial_effort.clone(),
        changed_effort.clone(),
        changed_effort,
        initial_effort,
    ] {
        submit_thread_settings(
            &test.codex,
            ThreadSettingsOverrides {
                effort: Some(Some(effort)),
                ..Default::default()
            },
        )
        .await?;
        test.submit_text_turn("continue").await?;
    }
    assert_eq!(
        mocks
            .iter()
            .map(|mock| effort_updates(&mock.single_request()))
            .collect::<Vec<_>>(),
        vec![
            vec![initial_update.clone()],
            vec![initial_update.clone(), changed_update.clone()],
            vec![initial_update.clone(), changed_update.clone()],
            vec![initial_update.clone(), changed_update, initial_update],
        ],
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reasoning_effort_override_preserves_prefix_and_only_appends_on_change()
-> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));
    let server = responses::start_mock_server().await;
    let mut mocks = Vec::new();
    for id in ["first", "changed", "unchanged", "lowered"] {
        mocks.push(
            responses::mount_sse_once(&server, responses::sse(vec![responses::ev_completed(id)]))
                .await,
        );
    }
    let test = override_builder().build_with_auto_env(&server).await?;
    test.submit_text_turn("first message").await?;
    submit_thread_settings(
        &test.codex,
        ThreadSettingsOverrides {
            effort: Some(Some(ReasoningEffort::High)),
            ..Default::default()
        },
    )
    .await?;
    test.submit_text_turn("second message").await?;
    test.submit_text_turn("third message").await?;
    submit_thread_settings(
        &test.codex,
        ThreadSettingsOverrides {
            effort: Some(Some(ReasoningEffort::Low)),
            ..Default::default()
        },
    )
    .await?;
    test.submit_text_turn("fourth message").await?;

    let requests = mocks
        .iter()
        .map(responses::ResponseMock::single_request)
        .collect::<Vec<_>>();
    let inputs = requests
        .iter()
        .map(|request| {
            responses::strip_response_item_ids_from_json(responses::strip_metadata_from_json(
                Value::Array(request.input()),
            ))
            .as_array()
            .expect("input array")
            .clone()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        requests
            .iter()
            .map(|request| request.body_json()["reasoning"]["effort"].clone())
            .collect::<Vec<_>>(),
        vec![
            Value::from("medium"),
            Value::from("high"),
            Value::from("high"),
            Value::from("low"),
        ],
    );
    let cache_keys = requests
        .iter()
        .map(|request| request.body_json()["prompt_cache_key"].clone())
        .collect::<Vec<_>>();
    assert_eq!(cache_keys, vec![cache_keys[0].clone(); requests.len()]);
    assert_eq!(
        inputs[0][inputs[0].len() - 2..],
        [
            message("user", "first message"),
            effort_update(ReasoningEffort::Medium)
        ],
    );
    let mut expected = inputs[0].clone();
    expected.extend([
        message("user", "second message"),
        effort_update(ReasoningEffort::High),
    ]);
    assert_eq!(inputs[1], expected);
    expected.push(message("user", "third message"));
    assert_eq!(inputs[2], expected);
    expected.extend([
        message("user", "fourth message"),
        effort_update(ReasoningEffort::Low),
    ]);
    assert_eq!(inputs[3], expected);
    Ok(())
}

#[test_case(ReasoningEffort::High; "model maps ultra to high")]
#[test_case(ReasoningEffort::Max; "model maps ultra to max")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reasoning_effort_override_normalizes_ultra_before_comparing_updates(
    resolved_effort: ReasoningEffort,
) -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));
    let server = responses::start_mock_server().await;
    let first = responses::mount_sse_once(
        &server,
        responses::sse(vec![responses::ev_completed("first")]),
    )
    .await;
    let second = responses::mount_sse_once(
        &server,
        responses::sse(vec![responses::ev_completed("second")]),
    )
    .await;
    let model_effort = resolved_effort.clone();
    let test = override_builder()
        .with_model_info_override("gpt-5.4", move |model| {
            model.multi_agent_reasoning_effort = Some(model_effort.clone());
            model.supported_reasoning_levels = vec![ReasoningEffortPreset {
                effort: model_effort,
                description: "Model effort".to_string(),
            }];
        })
        .with_config(|config| config.model_reasoning_effort = Some(ReasoningEffort::Ultra))
        .build_with_auto_env(&server)
        .await?;
    test.submit_text_turn("ultra selection").await?;
    submit_thread_settings(
        &test.codex,
        ThreadSettingsOverrides {
            effort: Some(Some(resolved_effort.clone())),
            ..Default::default()
        },
    )
    .await?;
    test.submit_text_turn("equivalent explicit effort").await?;
    let requests = [first.single_request(), second.single_request()];
    assert_eq!(
        requests.each_ref().map(effort_updates),
        [
            vec![effort_update(resolved_effort.clone())],
            vec![effort_update(resolved_effort.clone())],
        ]
    );
    assert_eq!(
        requests.map(|request| request.body_json()["reasoning"]["effort"].clone()),
        [
            serde_json::to_value(&resolved_effort)?,
            serde_json::to_value(&resolved_effort)?
        ]
    );
    Ok(())
}

#[derive(Clone, Copy)]
enum OverrideUnavailable {
    FeatureDisabled,
    NonOpenAiProvider,
    ResponsesLiteDisabled,
}

#[test_case(OverrideUnavailable::FeatureDisabled; "feature disabled")]
#[test_case(OverrideUnavailable::NonOpenAiProvider; "unsupported provider")]
#[test_case(OverrideUnavailable::ResponsesLiteDisabled; "responses lite disabled")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reasoning_effort_override_unavailable_uses_request_effort(
    unavailable: OverrideUnavailable,
) -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));
    let server = responses::start_mock_server().await;
    let first = responses::mount_sse_once(
        &server,
        responses::sse(vec![responses::ev_completed("first")]),
    )
    .await;
    let second = responses::mount_sse_once(
        &server,
        responses::sse(vec![responses::ev_completed("second")]),
    )
    .await;
    let mut builder = match unavailable {
        OverrideUnavailable::FeatureDisabled => override_builder().with_config(|config| {
            config
                .features
                .disable(Feature::ReasoningEffortOverride)
                .expect("disable overrides");
        }),
        OverrideUnavailable::NonOpenAiProvider => override_builder().with_config(|config| {
            config.model_provider.name = "unsupported provider".into();
        }),
        OverrideUnavailable::ResponsesLiteDisabled => override_builder()
            .with_model_info_override("gpt-5.4", |model| model.use_responses_lite = false),
    };
    let test = builder.build_with_auto_env(&server).await?;
    test.submit_text_turn("first").await?;
    submit_thread_settings(
        &test.codex,
        ThreadSettingsOverrides {
            effort: Some(Some(ReasoningEffort::High)),
            ..Default::default()
        },
    )
    .await?;
    test.submit_text_turn("second").await?;
    let requests = [first.single_request(), second.single_request()];
    assert_eq!(
        requests.each_ref().map(effort_updates),
        [Vec::<Value>::new(), Vec::new()]
    );
    assert_eq!(
        requests.map(|request| request.body_json()["reasoning"]["effort"].clone()),
        [Value::from("medium"), Value::from("high")],
    );
    Ok(())
}
