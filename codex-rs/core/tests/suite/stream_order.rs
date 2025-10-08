use std::time::Duration;

use codex_core::CodexAuth;
use codex_core::ConversationManager;
use codex_core::built_in_model_providers;
use codex_core::protocol::EventMsg;
use codex_core::protocol::InputItem;
use codex_core::protocol::Op;
use codex_core::protocol::OrderMeta;
use codex_core::spawn::CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR;
use core_test_support::load_default_config_for_test;
use core_test_support::load_sse_fixture_with_id;
use tempfile::TempDir;
use tokio::time::timeout;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

fn sandbox_network_disabled() -> bool {
    std::env::var(CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR).is_ok()
}

fn assert_non_decreasing_order(prev: &OrderMeta, next: &OrderMeta) {
    assert!(
        next.request_ordinal >= prev.request_ordinal,
        "request ordinal regressed: prev={prev:?}, next={next:?}"
    );

    if next.request_ordinal > prev.request_ordinal {
        return;
    }

    let prev_output = prev
        .output_index
        .expect("missing output_index on prev order meta");
    let next_output = next
        .output_index
        .expect("missing output_index on next order meta");
    assert!(
        next_output >= prev_output,
        "output_index regressed: prev={prev:?}, next={next:?}"
    );

    if next_output > prev_output {
        return;
    }

    let prev_seq = prev
        .sequence_number
        .expect("missing sequence_number on prev order meta");
    let next_seq = next
        .sequence_number
        .expect("missing sequence_number on next order meta");
    assert!(
        next_seq >= prev_seq,
        "sequence_number regressed: prev={prev:?}, next={next:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn streaming_events_expose_monotonic_order_meta() {
    if sandbox_network_disabled() {
        println!("Skipping event-order smoke test because network is disabled in this sandbox.");
        return;
    }

    let server = MockServer::start().await;

    let sse_stream = load_sse_fixture_with_id(
        "tests/fixtures/ordered_responses_template.json",
        "resp_order_ok",
    );

    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_raw(sse_stream, "text/event-stream"),
        )
        .expect(1)
        .mount(&server)
        .await;

    let mut provider = built_in_model_providers()["openai"].clone();
    provider.base_url = Some(format!("{}/v1", server.uri()));
    provider.request_max_retries = Some(0);
    provider.stream_max_retries = Some(0);

    let codex_home = TempDir::new().expect("create temp dir");
    let mut config = load_default_config_for_test(&codex_home);
    config.model_provider = provider;

    let conversation_manager =
        ConversationManager::with_auth(CodexAuth::from_api_key("Test API Key"));
    let codex = conversation_manager
        .new_conversation(config)
        .await
        .expect("start conversation")
        .conversation;

    codex
        .submit(Op::UserInput {
            items: vec![InputItem::Text {
                text: "hello".into(),
            }],
        })
        .await
        .expect("submit user input");

    let mut ordered = Vec::new();

    loop {
        let event = timeout(Duration::from_secs(10), codex.next_event())
            .await
            .expect("next_event timeout")
            .expect("event stream ended");

        if let Some(order) = event.order.clone() {
            assert_eq!(order.request_ordinal, 1, "unexpected request ordinal");
            assert!(
                order.output_index.is_some(),
                "order meta missing output_index: {order:?}"
            );
            assert!(
                order.sequence_number.is_some(),
                "order meta missing sequence_number: {order:?}"
            );
            ordered.push(order);
        }

        if matches!(event.msg, EventMsg::TaskComplete(_)) {
            break;
        }
    }

    assert!(
        !ordered.is_empty(),
        "expected at least one ordered streaming event"
    );

    for pair in ordered.windows(2) {
        let prev = &pair[0];
        let next = &pair[1];
        assert_non_decreasing_order(prev, next);
    }
}
