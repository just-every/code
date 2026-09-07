//! Gated reasoning-effort updates appended after accepted input.
//!
//! Only trusted harness items establish an effort. Updates append to surviving
//! history without replacing the request-level reasoning effort.

use super::session::Session;
use super::step_context::StepContext;
use super::step_settings::ResolvedStepSettings;
use codex_features::Feature;
use codex_history::CodexHarnessMetadata;
use codex_history::ResponseItemEnvelope;
use codex_protocol::models::ConfigurationReasoning;
use codex_protocol::models::ResponseItem;
use codex_protocol::openai_models::ReasoningEffort;

impl Session {
    /// Establishes the selected effort in surviving history, independent of replayed settings.
    pub(crate) async fn record_reasoning_effort_override(&self, step_context: &StepContext) {
        let settings = &step_context.settings;
        let Some(effort) = self.effort_for_configuration_update(settings).await else {
            return;
        };
        let should_skip = {
            let state = self.state.lock().await;
            let established_effort =
                state
                    .history
                    .annotated_items()
                    .iter()
                    .rev()
                    .find_map(|envelope| {
                        if !envelope
                            .metadata
                            .as_ref()
                            .is_some_and(|metadata| metadata.harness_authored_configuration)
                        {
                            return None;
                        }
                        match &envelope.item {
                            ResponseItem::ConfigurationUpdate { reasoning } => {
                                Some(&reasoning.effort)
                            }
                            _ => None,
                        }
                    });
            established_effort == Some(&effort)
        };
        if should_skip {
            return;
        }

        self.record_annotated_conversation_items(
            step_context.turn.as_ref(),
            vec![ResponseItemEnvelope {
                item: ResponseItem::ConfigurationUpdate {
                    reasoning: ConfigurationReasoning { effort },
                },
                metadata: Some(CodexHarnessMetadata {
                    harness_authored_configuration: true,
                    ..Default::default()
                }),
            }],
        )
        .await;
    }

    async fn effort_for_configuration_update(
        &self,
        settings: &ResolvedStepSettings,
    ) -> Option<ReasoningEffort> {
        if !self.enabled(Feature::ReasoningEffortOverride)
            || !settings.model_info.use_responses_lite
            || !self.provider().await.is_openai()
        {
            return None;
        }
        let effort = settings
            .model_info
            .resolve_reasoning_effort(settings.effective_reasoning_effort()?);
        // Persistent normalizes to "disabled". Keep unknown custom values out of
        // durable updates so injected items stay bounded to known backend modes.
        if matches!(&effort, ReasoningEffort::Custom(value) if value != "disabled") {
            return None;
        }
        Some(effort)
    }
}
