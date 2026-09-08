//! Request-effort lookup must leave live state untouched during compaction.

use super::RequestEffortUsage;
use crate::session::tests::make_session_and_context;
use codex_features::Feature;
use codex_protocol::openai_models::ReasoningEffort;
use pretty_assertions::assert_eq;
use std::sync::Arc;

#[tokio::test]
async fn compaction_effort_lookup_preserves_pin_for_fallback_models() {
    let (mut session, turn_context) = make_session_and_context().await;
    session
        .features
        .enable(Feature::ReasoningEffortOverride)
        .unwrap();
    session
        .state
        .lock()
        .await
        .reasoning_effort_pin
        .pin("original", ReasoningEffort::Low);
    let mut settings = (*turn_context.initial_settings).clone();
    let effort = ReasoningEffort::Medium;
    let model = Arc::make_mut(&mut settings.model_info);
    model.slug = "fallback".to_string();
    model.use_responses_lite = true;
    model.default_reasoning_level = Some(effort.clone());

    assert_eq!(
        session
            .reasoning_effort_for_request(&settings, RequestEffortUsage::Compaction)
            .await,
        Some(effort)
    );
    assert_eq!(
        session
            .state
            .lock()
            .await
            .reasoning_effort_pin
            .get("original"),
        Some(ReasoningEffort::Low)
    );
}
