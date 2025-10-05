use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use codex_core::{error::CodexErr, ModelClient, Prompt, ResponseEvent, TextFormat};
use codex_core::model_family::{derive_default_model_family, find_family_for_model};
use codex_protocol::models::{ContentItem, ResponseItem};
use futures::StreamExt;
use serde::Deserialize;
use serde_json::{self, json, Value};
use tracing::{debug, error, warn};

use crate::app_event::{AutoCoordinatorStatus, AutoObserverStatus, AutoObserverTelemetry};

use super::auto_coordinator::{
    extract_first_json_object,
    make_message,
    AutoCoordinatorCommand,
    MODEL_SLUG,
};

#[derive(Debug)]
pub(super) struct AutoObserverHandle {
    pub tx: Sender<AutoObserverCommand>,
    cadence: u32,
}

impl AutoObserverHandle {
    pub fn cadence(&self) -> u32 {
        self.cadence.max(1)
    }
}

#[derive(Debug)]
pub(super) enum AutoObserverCommand {
    Trigger(ObserverTrigger),
    Stop,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum ObserverReason {
    Cadence,
    FinalCheck { finish_status: AutoCoordinatorStatus },
}

#[derive(Debug, Clone)]
pub(super) struct ObserverTrigger {
    pub conversation: Vec<ResponseItem>,
    pub goal_text: String,
    pub environment_details: String,
    pub reason: ObserverReason,
}

#[derive(Debug, Clone)]
pub(super) struct ObserverOutcome {
    pub status: AutoObserverStatus,
    pub replace_message: Option<String>,
    pub additional_instructions: Option<String>,
    pub telemetry: AutoObserverTelemetry,
}

const OBSERVER_SCHEMA_NAME: &str = "auto_coordinator_observer";

pub(super) fn start_auto_observer(
    client: Arc<ModelClient>,
    cadence: u32,
    coordinator_tx: Sender<AutoCoordinatorCommand>,
) -> Result<AutoObserverHandle> {
    let (tx, rx) = mpsc::channel();
    let thread_tx = tx.clone();

    std::thread::spawn(move || {
        if let Err(err) = run_observer_loop(client, rx, coordinator_tx) {
            error!("auto observer loop error: {err:#}");
        }
    });

    Ok(AutoObserverHandle {
        tx: thread_tx,
        cadence,
    })
}

pub(super) fn run_observer_once(
    runtime: &tokio::runtime::Runtime,
    client: Arc<ModelClient>,
    trigger: ObserverTrigger,
) -> Result<(AutoObserverStatus, Option<String>, Option<String>)> {
    evaluate_observer(runtime, client, trigger)
}

fn run_observer_loop(
    client: Arc<ModelClient>,
    rx: Receiver<AutoObserverCommand>,
    coordinator_tx: Sender<AutoCoordinatorCommand>,
) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("creating runtime for auto observer")?;

    let mut telemetry = AutoObserverTelemetry {
        trigger_count: 0,
        last_status: AutoObserverStatus::Ok,
        last_intervention: None,
    };

    while let Ok(cmd) = rx.recv() {
        match cmd {
            AutoObserverCommand::Trigger(trigger) => {
                telemetry.trigger_count += 1;
                match evaluate_observer(&runtime, client.clone(), trigger) {
                    Ok((status, replace_message, additional_instructions)) => {
                        telemetry.last_status = status;
                        telemetry.last_intervention = summarize_intervention(
                            replace_message.as_deref(),
                            additional_instructions.as_deref(),
                        );

                        let outcome = ObserverOutcome {
                            status,
                            replace_message,
                            additional_instructions,
                            telemetry: telemetry.clone(),
                        };

                        if coordinator_tx
                            .send(AutoCoordinatorCommand::ObserverResult(outcome))
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(err) => {
                        warn!("auto observer evaluation error: {err:#}");
                        telemetry.last_status = AutoObserverStatus::Ok;
                        telemetry.last_intervention = Some(format!("error: {err}"));
                        let outcome = ObserverOutcome {
                            status: AutoObserverStatus::Ok,
                            replace_message: None,
                            additional_instructions: None,
                            telemetry: telemetry.clone(),
                        };
                        if coordinator_tx
                            .send(AutoCoordinatorCommand::ObserverResult(outcome))
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            }
            AutoObserverCommand::Stop => break,
        }
    }

    Ok(())
}

fn evaluate_observer(
    runtime: &tokio::runtime::Runtime,
    client: Arc<ModelClient>,
    trigger: ObserverTrigger,
) -> Result<(AutoObserverStatus, Option<String>, Option<String>)> {
    let prompt = build_observer_prompt(&trigger, MODEL_SLUG);
    match run_observer_prompt(runtime, client.clone(), prompt) {
        Ok(result) => Ok(result),
        Err(err) => {
            let fallback_slug = client.default_model_slug().to_string();
            if should_retry_with_default_model(&err) && fallback_slug != MODEL_SLUG {
                debug!(
                    preferred = %MODEL_SLUG,
                    fallback = %fallback_slug,
                    "auto observer falling back to configured model after invalid model error"
                );
                let fallback_prompt = build_observer_prompt(&trigger, &fallback_slug);
                let original_error = err.to_string();
                return run_observer_prompt(runtime, client, fallback_prompt).map_err(|fallback_err| {
                    fallback_err.context(format!(
                        "observer fallback with model '{}' failed after original error: {}",
                        fallback_slug, original_error
                    ))
                });
            }
            Err(err)
        }
    }
}

fn run_observer_prompt(
    runtime: &tokio::runtime::Runtime,
    client: Arc<ModelClient>,
    prompt: Prompt,
) -> Result<(AutoObserverStatus, Option<String>, Option<String>)> {
    let raw = runtime.block_on(async {
        request_observer_response(client.clone(), &prompt).await
    })?;

    let (response, _value) = parse_observer_response(&raw)?;

    let status = match response.status.as_str() {
        "ok" => AutoObserverStatus::Ok,
        "failing" => AutoObserverStatus::Failing,
        other => return Err(anyhow!("unexpected status '{other}'")),
    };

    let trimmed_replace_message = response
        .replace_message
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let trimmed_additional_instructions = response
        .additional_instructions
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    if matches!(status, AutoObserverStatus::Failing)
        && trimmed_replace_message.is_none()
        && trimmed_additional_instructions.is_none()
    {
        warn!("observer returned failing status without guidance");
    }

    let (replace_message, additional_instructions) = partition_observer_guidance(
        status,
        trimmed_replace_message,
        trimmed_additional_instructions,
    );

    debug!(
        "[Auto observer] status={status:?} replace={} instructions={}",
        replace_message.is_some(),
        additional_instructions.is_some()
    );

    Ok((status, replace_message, additional_instructions))
}

fn partition_observer_guidance(
    status: AutoObserverStatus,
    replace_message: Option<String>,
    additional_instructions: Option<String>,
) -> (Option<String>, Option<String>) {
    if matches!(status, AutoObserverStatus::Failing) {
        (replace_message, additional_instructions)
    } else {
        (None, additional_instructions)
    }
}

fn build_observer_prompt(trigger: &ObserverTrigger, model_slug: &str) -> Prompt {
    let mut prompt = Prompt::default();
    prompt.store = true;

    let instructions = build_observer_instructions(&trigger.environment_details, trigger.reason);
    prompt.input.push(make_message("developer", instructions));
    let goal = format!("Primary Goal\n{}", trigger.goal_text);
    prompt.input.push(make_message("developer", goal));
    prompt.input.extend(trigger.conversation.clone());

    let schema = build_observer_schema();
    prompt.text_format = Some(TextFormat {
        r#type: "json_schema".to_string(),
        name: Some(OBSERVER_SCHEMA_NAME.to_string()),
        strict: Some(true),
        schema: Some(schema),
    });
    prompt.model_override = Some(model_slug.to_string());
    let family = find_family_for_model(model_slug)
        .unwrap_or_else(|| derive_default_model_family(model_slug));
    prompt.model_family_override = Some(family);
    prompt
}

fn should_retry_with_default_model(err: &anyhow::Error) -> bool {
    err.chain().any(|cause| {
        if let Some(codex_err) = cause.downcast_ref::<CodexErr>() {
            if let CodexErr::UnexpectedStatus(err) = codex_err {
                if !err.status.is_client_error() {
                    return false;
                }
                let body_lower = err.body.to_lowercase();
                return body_lower.contains("invalid model")
                    || body_lower.contains("unknown model")
                    || body_lower.contains("model_not_found")
                    || body_lower.contains("model does not exist");
            }
        }
        false
    })
}

async fn request_observer_response(
    client: Arc<ModelClient>,
    prompt: &Prompt,
) -> Result<String> {
    let mut stream = client.stream(prompt).await?;
    let mut out = String::new();
    while let Some(ev) = stream.next().await {
        match ev {
            Ok(ResponseEvent::OutputTextDelta { delta, .. }) => out.push_str(&delta),
            Ok(ResponseEvent::OutputItemDone { item, .. }) => {
                if let ResponseItem::Message { content, .. } = item {
                    for c in content {
                        if let ContentItem::OutputText { text } = c {
                            out.push_str(&text);
                        }
                    }
                }
            }
            Ok(ResponseEvent::Completed { .. }) => break,
            Err(err) => return Err(anyhow!("observer stream error: {err}")),
            _ => {}
        }
    }
    Ok(out)
}

#[derive(Debug, Deserialize)]
struct ObserverResponse {
    status: String,
    #[serde(default)]
    replace_message: Option<String>,
    #[serde(default)]
    additional_instructions: Option<String>,
}

fn parse_observer_response(raw: &str) -> Result<(ObserverResponse, Value)> {
    let value: Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => {
            let Some(blob) = extract_first_json_object(raw) else {
                return Err(anyhow!("observer response was not valid JSON"));
            };
            serde_json::from_str(&blob).context("parsing JSON from observer output")?
        }
    };
    let response: ObserverResponse = serde_json::from_value(value.clone())
        .context("decoding observer response")?;
    Ok((response, value))
}

fn build_observer_instructions(environment_details: &str, reason: ObserverReason) -> String {
    let body = match reason {
        ObserverReason::Cadence => "You are observing a AI Coordinator trying to drive a CLI towards a Primary Goal (shown below).\nPlease critically observe the conversation between the Coordinator and the CLI. Detect either of these issues;\n- Stuck in a loop\n- Not working towards primary goal\nGenerate a response based on this information;\n`status`: one of 'ok' or 'failing' - most of the time it will be 'ok', but use 'failing' when intervention absolutely is needed. When using 'failing' please provide one or both fields below to correct the problem;\n`replace_message`: A message to replace the last Coordinator message\n`additional_instructions`: Instructions to give to the Coordinator for future runs\n**Warning**\nYou almost always want to use `status`: \"ok\". You are a last resort. Avoid setting `status`: \"failing\" for minor issues as it will disrupt the progress of the task.".to_string(),
        ObserverReason::FinalCheck { finish_status } => {
            let finish_phrase = match finish_status {
                AutoCoordinatorStatus::Success => "believes the goal has been fully completed",
                AutoCoordinatorStatus::Failed => "reported that it cannot complete the goal",
                AutoCoordinatorStatus::Continue => "is still mid-run and should not have requested final validation",
            };
            format!(
                "You are performing a final validation run after Auto Drive {finish_phrase}.\nStudy the full conversation and decide if the Primary Goal is truly satisfied.\n- If absolutely everything is done, respond with `status`: 'ok' and leave the other fields null.\n- If any required work remains, respond with `status`: 'failing'. Provide a concise `additional_instructions` developer prompt describing what has already been completed and the specific steps still required. Include enough detail for the coordinator to resume effectively.\n- Use `replace_message` only when the last prompt sent to the CLI must be replaced immediately."
            )
        }
    };
    format!("{body}\nEnvironment:\n{environment_details}")
}

fn build_observer_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "status": {
                "type": "string",
                "enum": ["ok", "failing"],
            },
            "replace_message": {
                "type": ["string", "null"],
                "minLength": 1,
            },
            "additional_instructions": {
                "type": ["string", "null"],
                "minLength": 1,
            }
        },
        "required": ["status", "replace_message", "additional_instructions"],
        "additionalProperties": false
    })
}

pub(super) fn summarize_intervention(
    replace_message: Option<&str>,
    additional_instructions: Option<&str>,
) -> Option<String> {
    let source = replace_message.or(additional_instructions)?;
    let trimmed = source.trim();
    if trimmed.is_empty() {
        return None;
    }
    const MAX_LEN: usize = 160;
    if trimmed.len() > MAX_LEN {
        let mut out = trimmed.chars().take(MAX_LEN).collect::<String>();
        out.push('…');
        Some(out)
    } else {
        Some(trimmed.to_string())
    }
}

// Helper so observer can append the coordinator's latest prompt.
pub(super) fn build_observer_conversation(
    conversation: Vec<ResponseItem>,
    coordinator_prompt: Option<&str>,
) -> Vec<ResponseItem> {
    let mut filtered: Vec<ResponseItem> = Vec::new();

    for item in conversation {
        match item {
            ResponseItem::Message { id, role, content } => {
                if id.as_deref() == Some("auto-drive-reasoning") {
                    continue;
                }

                if role == "assistant" {
                    let mut new_content: Vec<ContentItem> = Vec::new();
                    for entry in content {
                        match entry {
                            ContentItem::InputText { text } => {
                                let prefixed = if text.trim_start().starts_with("Coordinator:") {
                                    text
                                } else {
                                    format!("Coordinator: {text}")
                                };
                                new_content.push(ContentItem::InputText { text: prefixed });
                            }
                            ContentItem::OutputText { text } => {
                                let prefixed = if text.trim_start().starts_with("Coordinator:") {
                                    text
                                } else {
                                    format!("Coordinator: {text}")
                                };
                                new_content.push(ContentItem::OutputText { text: prefixed });
                            }
                            other => new_content.push(other),
                        }
                    }
                    filtered.push(ResponseItem::Message {
                        id: None,
                        role,
                        content: new_content,
                    });
                } else {
                    filtered.push(ResponseItem::Message { id, role, content });
                }
            }
            ResponseItem::Reasoning { .. } => {
                // Observer should not inspect reasoning blocks.
                continue;
            }
            other => filtered.push(other),
        }
    }

    if let Some(prompt) = coordinator_prompt.and_then(|p| {
        let trimmed = p.trim();
        (!trimmed.is_empty()).then_some(trimmed)
    }) {
        let text = if prompt.trim_start().starts_with("Coordinator:") {
            prompt.to_string()
        } else {
            format!("Coordinator: {prompt}")
        };
        filtered.push(ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText { text }],
        });
    }

    filtered
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_protocol::models::{ContentItem, ResponseItem};

    #[test]
    fn build_observer_conversation_appends_prompt() {
        let base = vec![make_message("assistant", "Coordinator: hello".to_string())];
        let conversation = build_observer_conversation(base.clone(), Some("Review the logs"));
        assert_eq!(conversation.len(), base.len() + 1);
        match conversation.last().expect("appended message") {
            ResponseItem::Message { role, content, .. } => {
                assert_eq!(role, "assistant");
                assert_eq!(content.len(), 1);
                match &content[0] {
                    ContentItem::OutputText { text } => {
                        assert!(text.contains("Coordinator: Review the logs"));
                    }
                    other => panic!("unexpected content: {:?}", other),
                }
            }
            other => panic!("unexpected response item: {:?}", other),
        }
    }

    #[test]
    fn parse_observer_response_handles_ok() {
        let raw = r#"{"status":"ok"}"#;
        let (response, _value) = parse_observer_response(raw).expect("parse ok");
        assert_eq!(response.status, "ok");
        assert!(response.replace_message.is_none());
        assert!(response.additional_instructions.is_none());
    }

    #[test]
    fn parse_observer_response_handles_failing() {
        let raw = r#"{"status":"failing","replace_message":"Try a different command","additional_instructions":"Confirm environment variables"}"#;
        let (response, _value) = parse_observer_response(raw).expect("parse failing");
        assert_eq!(response.status, "failing");
        assert_eq!(
            response.replace_message.as_deref(),
            Some("Try a different command")
        );
        assert_eq!(
            response.additional_instructions.as_deref(),
            Some("Confirm environment variables")
        );
    }

    #[test]
    fn observer_ok_status_retains_additional_instructions() {
        let status = AutoObserverStatus::Ok;
        let (replace_message, additional_instructions) = partition_observer_guidance(
            status,
            Some("Should be ignored".to_string()),
            Some("Keep nudging the task".to_string()),
        );

        assert!(replace_message.is_none(), "ok status should not replace prompt");
        assert_eq!(
            additional_instructions.as_deref(),
            Some("Keep nudging the task"),
            "observer notes should be preserved for ok status"
        );
    }

    #[test]
    fn build_observer_conversation_preserves_assistant_roles() {
        let base = vec![
            make_message("assistant", "Coordinator: initial guidance".to_string()),
            make_message("assistant", "Consider running tests".to_string()),
        ];

        let conversation = build_observer_conversation(base.clone(), Some("Review progress"));

        assert_eq!(conversation.len(), base.len() + 1);

        for (idx, item) in conversation.iter().enumerate().take(base.len()) {
            match item {
                ResponseItem::Message { role, content, .. } => {
                    assert_eq!(role, "assistant", "role should remain assistant at index {idx}");
                    assert_eq!(content.len(), 1);
                    match &content[0] {
                        ContentItem::OutputText { text } => {
                            assert!(
                                text.starts_with("Coordinator:"),
                                "coordinator text should be prefixed"
                            );
                        }
                        other => panic!("unexpected content variant: {:?}", other),
                    }
                }
                other => panic!("unexpected response item: {:?}", other),
            }
        }

        match conversation.last().expect("appended prompt") {
            ResponseItem::Message { role, content, .. } => {
                assert_eq!(role, "user");
                assert_eq!(content.len(), 1);
                match &content[0] {
                    ContentItem::InputText { text } => {
                        assert!(text.contains("Coordinator: Review progress"));
                    }
                    other => panic!("unexpected content: {:?}", other),
                }
            }
            other => panic!("unexpected item appended: {:?}", other),
        }
    }
}
