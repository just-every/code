use std::process::Command;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{anyhow, Context, Result};
use codex_core::config::Config;
use codex_core::config_types::ReasoningEffort;
use codex_core::debug_logger::DebugLogger;
use codex_core::model_family::{derive_default_model_family, find_family_for_model};
use codex_core::project_doc::read_auto_drive_docs;
use codex_core::protocol::SandboxPolicy;
use codex_core::{AuthManager, ModelClient, Prompt, ResponseEvent, TextFormat};
use codex_core::error::CodexErr;
use codex_protocol::models::{ContentItem, ReasoningItemContent, ResponseItem};
use futures::StreamExt;
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::{self, json, Value};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::app_event::{AppEvent, AutoCoordinatorStatus, AutoObserverStatus, AutoObserverTelemetry};
use crate::app_event_sender::AppEventSender;
use crate::chatwidget::retry::{retry_with_backoff, RetryDecision, RetryError, RetryOptions};
#[cfg(feature = "dev-faults")]
use crate::chatwidget::faults::{fault_to_error, next_fault, FaultScope, InjectedFault};
use codex_common::elapsed::format_duration;
use chrono::{DateTime, Local, Utc};
use rand::Rng;
use super::auto_observer::{
    build_observer_conversation,
    run_observer_once,
    start_auto_observer,
    AutoObserverCommand,
    ObserverOutcome,
    ObserverReason,
    ObserverTrigger,
    summarize_intervention,
};

const RATE_LIMIT_BUFFER: Duration = Duration::from_secs(120);
const RATE_LIMIT_JITTER_MAX: Duration = Duration::from_secs(30);
const MAX_RETRY_ELAPSED: Duration = Duration::from_secs(7 * 24 * 60 * 60);

#[derive(Debug, thiserror::Error)]
#[error("auto coordinator cancelled")]
struct AutoCoordinatorCancelled;

pub(super) const MODEL_SLUG: &str = "gpt-5";
const SCHEMA_NAME: &str = "auto_coordinator_flow";

#[derive(Debug, Clone)]
pub(super) struct AutoCoordinatorHandle {
    pub tx: Sender<AutoCoordinatorCommand>,
    cancel_token: CancellationToken,
}

impl AutoCoordinatorHandle {
    pub fn send(
        &self,
        command: AutoCoordinatorCommand,
    ) -> std::result::Result<(), mpsc::SendError<AutoCoordinatorCommand>> {
        self.tx.send(command)
    }

    pub fn cancel(&self) {
        self.cancel_token.cancel();
    }

    #[cfg(test)]
    pub(super) fn for_tests(tx: Sender<AutoCoordinatorCommand>) -> Self {
        Self {
            tx,
            cancel_token: CancellationToken::new(),
        }
    }
}

#[derive(Debug)]
pub(super) enum AutoCoordinatorCommand {
    UpdateConversation(Vec<ResponseItem>),
    ObserverResult(ObserverOutcome),
    Stop,
}

#[derive(Debug, Deserialize)]
struct CoordinatorDecision {
    finish_status: String,
    summary: String,
    #[serde(default)]
    prompt: Option<String>,
}

pub(super) fn start_auto_coordinator(
    app_event_tx: AppEventSender,
    goal_text: String,
    conversation: Vec<ResponseItem>,
    config: Config,
    debug_enabled: bool,
    observer_cadence: u32,
) -> Result<AutoCoordinatorHandle> {
    let (cmd_tx, cmd_rx) = mpsc::channel();
    let thread_tx = cmd_tx.clone();
    let loop_tx = cmd_tx.clone();
    let cancel_token = CancellationToken::new();
    let thread_cancel = cancel_token.clone();

    std::thread::spawn(move || {
        if let Err(err) = run_auto_loop(
            app_event_tx,
            goal_text,
            conversation,
            config,
            cmd_rx,
            loop_tx,
            debug_enabled,
            observer_cadence,
            thread_cancel,
        ) {
            tracing::error!("auto coordinator loop error: {err:#}");
        }
    });

    Ok(AutoCoordinatorHandle {
        tx: thread_tx,
        cancel_token,
    })
}

fn run_auto_loop(
    app_event_tx: AppEventSender,
    goal_text: String,
    initial_conversation: Vec<ResponseItem>,
    config: Config,
    cmd_rx: Receiver<AutoCoordinatorCommand>,
    cmd_tx: Sender<AutoCoordinatorCommand>,
    debug_enabled: bool,
    observer_cadence: u32,
    cancel_token: CancellationToken,
) -> Result<()> {
    let preferred_auth = if config.using_chatgpt_auth {
        codex_protocol::mcp_protocol::AuthMode::ChatGPT
    } else {
        codex_protocol::mcp_protocol::AuthMode::ApiKey
    };
    let codex_home = config.codex_home.clone();
    let responses_originator_header = config.responses_originator_header.clone();
    let auth_mgr = AuthManager::shared_with_mode_and_originator(
        codex_home,
        preferred_auth,
        responses_originator_header,
    );
    let model_provider = config.model_provider.clone();
    let model_reasoning_summary = config.model_reasoning_summary;
    let model_text_verbosity = config.model_text_verbosity;
    let sandbox_policy = config.sandbox_policy.clone();
    let config = Arc::new(config);
    let client = Arc::new(ModelClient::new(
        config.clone(),
        Some(auth_mgr),
        None,
        model_provider,
        ReasoningEffort::Medium,
        model_reasoning_summary,
        model_text_verbosity,
        Uuid::new_v4(),
        Arc::new(Mutex::new(
            DebugLogger::new(debug_enabled)
                .unwrap_or_else(|_| DebugLogger::new(false).expect("debug logger")),
        )),
    ));

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("creating runtime for auto coordinator")?;

    let auto_instructions = match runtime.block_on(read_auto_drive_docs(config.as_ref())) {
        Ok(Some(text)) => {
            let trimmed = text.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        }
        Ok(None) => None,
        Err(err) => {
            warn!("failed to read AUTO_AGENTS.md instructions: {err:#}");
            None
        }
    };

    let sandbox_label = if matches!(sandbox_policy, SandboxPolicy::DangerFullAccess) {
        "full access"
    } else {
        "limited sandbox"
    };
    let environment_details = format_environment_details(sandbox_label);
    let (base_developer_intro, primary_goal_message) =
        build_developer_message(&goal_text, &environment_details);
    let schema = build_schema();
    let platform = std::env::consts::OS;
    debug!("[Auto coordinator] starting: goal={goal_text} platform={platform}");

    let mut pending_conversation = Some(initial_conversation);
    let mut stopped = false;
    let mut requests_completed: u64 = 0;
    let mut observer_guidance: Vec<String> = Vec::new();
    let mut observer_telemetry = AutoObserverTelemetry::default();
    let mut observer_handle = if observer_cadence == 0 {
        None
    } else {
        match start_auto_observer(client.clone(), observer_cadence, cmd_tx.clone()) {
            Ok(handle) => Some(handle),
            Err(err) => {
                tracing::error!("failed to start auto observer: {err:#}");
                None
            }
        }
    };
    let observer_cadence = observer_handle
        .as_ref()
        .map(|handle| handle.cadence() as u64);

    loop {
        if stopped {
            break;
        }

        if let Some(conv) = pending_conversation.take() {
            if cancel_token.is_cancelled() {
                stopped = true;
                continue;
            }

            let conv_for_observer = conv.clone();
            let developer_intro =
                compose_developer_intro(&base_developer_intro, &observer_guidance);
            match request_coordinator_decision(
                &runtime,
                client.as_ref(),
                developer_intro.as_str(),
                &primary_goal_message,
                &schema,
                conv,
                auto_instructions.as_deref(),
                &app_event_tx,
                &cancel_token,
            ) {
                Ok((status, summary, prompt_opt, transcript_items)) => {
                    if matches!(status, AutoCoordinatorStatus::Continue) {
                        let prompt_clone = prompt_opt.clone();
                        let event = AppEvent::AutoCoordinatorDecision {
                            status,
                            summary,
                            prompt: prompt_opt,
                            transcript: transcript_items.clone(),
                        };
                        app_event_tx.send(event);

                        if let (Some(handle), Some(cadence)) =
                            (observer_handle.as_ref(), observer_cadence)
                        {
                            if should_trigger_observer(requests_completed, cadence) {
                                let conversation = build_observer_conversation(
                                    conv_for_observer,
                                    prompt_clone.as_deref(),
                                );
                                let trigger = ObserverTrigger {
                                    conversation,
                                    goal_text: goal_text.clone(),
                                    environment_details: environment_details.clone(),
                                    reason: ObserverReason::Cadence,
                                };
                                if handle.tx.send(AutoObserverCommand::Trigger(trigger)).is_err() {
                                    tracing::warn!("failed to trigger auto observer");
                                }
                            }
                        }
                        continue;
                    }

                    let observer_conversation =
                        build_observer_conversation(conv_for_observer.clone(), None);
                    let validation_result = run_final_observer_validation(
                        &runtime,
                        client.clone(),
                        observer_conversation,
                        &goal_text,
                        &environment_details,
                        status,
                    );

                    if let Ok((observer_status, replace_message, additional_instructions)) =
                        &validation_result
                    {
                        let telemetry = AutoObserverTelemetry {
                            trigger_count: observer_telemetry.trigger_count.saturating_add(1),
                            last_status: *observer_status,
                            last_intervention: summarize_intervention(
                                replace_message.as_deref(),
                                additional_instructions.as_deref(),
                            ),
                        };
                        observer_telemetry = telemetry.clone();
                        let observer_event = AppEvent::AutoObserverReport {
                            status: *observer_status,
                            telemetry,
                            replace_message: replace_message.clone(),
                            additional_instructions: additional_instructions.clone(),
                        };
                        app_event_tx.send(observer_event);

                        if matches!(observer_status, AutoObserverStatus::Failing) {
                            if let Some(instr) = additional_instructions
                                .as_deref()
                                .map(str::trim)
                                .filter(|s| !s.is_empty())
                            {
                                if !observer_guidance.iter().any(|existing| existing == instr) {
                                    observer_guidance.push(instr.to_string());
                                }
                            }
                            pending_conversation = Some(conv_for_observer);
                            continue;
                        }
                    } else if let Err(err) = validation_result {
                        tracing::warn!("final observer validation failed: {err:#}");
                    }

                    let event = AppEvent::AutoCoordinatorDecision {
                        status,
                        summary,
                        prompt: prompt_opt,
                        transcript: transcript_items,
                    };
                    app_event_tx.send(event);
                    stopped = true;
                    continue;
                }
                Err(err) => {
                    if err.downcast_ref::<AutoCoordinatorCancelled>().is_some() {
                        stopped = true;
                        continue;
                    }
                    let event = AppEvent::AutoCoordinatorDecision {
                        status: AutoCoordinatorStatus::Failed,
                        summary: format!("Coordinator error: {err}"),
                        prompt: None,
                        transcript: Vec::new(),
                    };
                    app_event_tx.send(event);
                    stopped = true;
                    continue;
                }
            }
        }

        match cmd_rx.recv() {
            Ok(AutoCoordinatorCommand::UpdateConversation(conv)) => {
                requests_completed = requests_completed.saturating_add(1);
                pending_conversation = Some(conv);
            }
            Ok(AutoCoordinatorCommand::ObserverResult(outcome)) => {
                let ObserverOutcome {
                    status,
                    replace_message,
                    additional_instructions,
                    telemetry,
                } = outcome;

                if let Some(instr) = additional_instructions
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    if !observer_guidance.iter().any(|existing| existing == instr) {
                        observer_guidance.push(instr.to_string());
                    }
                }

                observer_telemetry = telemetry.clone();
                let event = AppEvent::AutoObserverReport {
                    status,
                    telemetry,
                    replace_message,
                    additional_instructions,
                };
                app_event_tx.send(event);
            }
            Ok(AutoCoordinatorCommand::Stop) | Err(_) => {
                stopped = true;
            }
        }
    }

    if let Some(handle) = observer_handle.take() {
        let _ = handle.tx.send(AutoObserverCommand::Stop);
    }

    Ok(())
}

fn compose_developer_intro(base: &str, guidance: &[String]) -> String {
    if guidance.is_empty() {
        return base.to_string();
    }

    let mut intro = String::with_capacity(base.len() + guidance.len() * 64);
    intro.push_str(base);
    intro.push_str("\n\n**Observer Guidance**\n");
    for hint in guidance {
        let trimmed = hint.trim();
        if trimmed.is_empty() {
            continue;
        }
        intro.push_str("- ");
        intro.push_str(trimmed);
        intro.push('\n');
    }
    intro
}

fn should_trigger_observer(requests_completed: u64, cadence: u64) -> bool {
    cadence != 0 && requests_completed > 0 && requests_completed % cadence == 0
}

fn run_final_observer_validation(
    runtime: &tokio::runtime::Runtime,
    client: Arc<ModelClient>,
    conversation: Vec<ResponseItem>,
    goal_text: &str,
    environment_details: &str,
    finish_status: AutoCoordinatorStatus,
) -> Result<(AutoObserverStatus, Option<String>, Option<String>)> {
    let trigger = ObserverTrigger {
        conversation,
        goal_text: goal_text.to_string(),
        environment_details: environment_details.to_string(),
        reason: ObserverReason::FinalCheck { finish_status },
    };
    run_observer_once(runtime, client, trigger)
}

fn build_developer_message(goal_text: &str, environment_details: &str) -> (String, String) {
    let intro = format!(
        "You are coordinating prompts sent to a running Code CLI process. You should act like a human maintainer of the project would act. You will see a **Primary Goal** below - this is what you are always working towards.\n\n**JSON Structure**\n- `finish_status`: one of `continue`, `finish_success`, or `finish_failed`.\n  * Use `continue` when another prompt is reasonable. Always prefer this option.\n  * Use `finish_success` when the goal has been completed in its entirety and absolutely no work remains.\n  * Use `finish_failed` when the goal absolutely cannot be satisfied or you are stuck in a loop. This should almost never be used. Try other approaches and gather more information if there is no clear path forward.\n- `summary`: short summary (<= 160 characters) describing what will happen when the CLI performs the next prompt\n- `prompt`: the exact prompt to provide to the Code CLI process. You will receive the response the CLI provides.\n\n**Rules**\n- You set direction, not implementation. Keep the CLI on track, but let it do all the thinking and implementation. You do not have the context the CLI has.\n- When working on an existing code base, start by prompting the CLI to explain the problem and outline plausible approaches. This lets it build context rather than jumping in naively with a solution.\n- Keep every prompt minimal to give the CLI room to make independent decisions.\n- Don't repeat yourself. If something doesn't work, take a different approach. Always push the project forward.\n- Often a simple 'Please continue' or 'Work on feature A next' or 'What do you think is the best approach?' is sufficient. Your job is to keep things running in an appropriate direction. The CLI does all the actual work and thinking. You do not need to know much about the project or codebase, allow the CLI to do all this for you. You are focused on overall direction not implementation details.\n- Only stop when no other options remain. A human is observing your work and will step in if they want to go in a different direction. You should not ask them for assistance - you should use your judgement to move on the most likely path forward. The human may override your message send to the CLI if they choose to go in another direction. This allows you to just guess the best path, knowing an overseer will step in if needed.\n\nUseful command:\n`/review <what to review>` e.g. `/review latest commit` - this spins up a specialist review thread for the CLI which excels at identify issues. This is useful for repeatedly reviewing code changes you make and fixing them.\n\nEnvironment:\n{environment_details}"
    );
    let primary_goal = format!("**Primary Goal**\n{goal_text}");
    (intro, primary_goal)
}

fn format_environment_details(sandbox: &str) -> String {
    let cwd = std::env::current_dir()
        .map(|dir| dir.display().to_string())
        .unwrap_or_else(|_| "<unknown>".to_string());
    let branch = run_git_command(["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_else(|| "<unknown>".to_string());
    let git_status_raw = run_git_command(["status", "--short"]);
    let git_status = match git_status_raw {
        Some(raw) if raw.trim().is_empty() => "  clean".to_string(),
        Some(raw) => raw
            .lines()
            .map(|line| format!("  {line}"))
            .collect::<Vec<_>>()
            .join("\n"),
        None => "  <git status unavailable>".to_string(),
    };
    format!(
        "- Access: {sandbox}\n- Working directory: {cwd}\n- Git branch: {branch}\n- Git status:\n{git_status}"
    )
}

fn run_git_command<const N: usize>(args: [&str; N]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()
        .map(|text| text.trim_end().to_string())
}

fn build_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "finish_status": {
                "type": "string",
                "enum": ["continue", "finish_success", "finish_failed"],
                "description": "Decision on how to proceed"
            },
            "summary": {
                "type": "string",
                "minLength": 1,
                "maxLength": 160,
                "description": "Short summary describing what will happen when the CLI performs the next prompt"
            },
            "prompt": {
                "type": ["string", "null"],
                "minLength": 1,
                "description": "Prompt to send to Code CLI when finish_status is 'continue'"
            }
        },
        "required": ["finish_status", "summary", "prompt"],
        "additionalProperties": false
    })
}

struct RequestStreamResult {
    output_text: String,
    response_items: Vec<ResponseItem>,
}

fn request_coordinator_decision(
    runtime: &tokio::runtime::Runtime,
    client: &ModelClient,
    developer_intro: &str,
    primary_goal: &str,
    schema: &Value,
    conversation: Vec<ResponseItem>,
    auto_instructions: Option<&str>,
    app_event_tx: &AppEventSender,
    cancel_token: &CancellationToken,
) -> Result<(AutoCoordinatorStatus, String, Option<String>, Vec<ResponseItem>)> {
    let (raw, response_items) = request_decision(
        runtime,
        client,
        developer_intro,
        primary_goal,
        schema,
        &conversation,
        auto_instructions,
        app_event_tx,
        cancel_token,
    )?;
    let (decision, value) = parse_decision(&raw)?;
    debug!("[Auto coordinator] model decision: {:?}", value);

    let status = match decision.finish_status.as_str() {
        "continue" => AutoCoordinatorStatus::Continue,
        "finish_success" => AutoCoordinatorStatus::Success,
        "finish_failed" => AutoCoordinatorStatus::Failed,
        other => {
            return Err(anyhow!("unexpected finish_status '{other}'"));
        }
    };

    let prompt_opt = match status {
        AutoCoordinatorStatus::Continue => {
            let prompt_text = decision
                .prompt
                .as_ref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| anyhow!("model response missing prompt for continue"))?;
            let cleaned = strip_role_prefix(prompt_text);
            Some(cleaned.to_string())
        }
        _ => None,
    };

    Ok((
        status,
        decision.summary.trim().to_string(),
        prompt_opt,
        response_items,
    ))
}

fn request_decision(
    runtime: &tokio::runtime::Runtime,
    client: &ModelClient,
    developer_intro: &str,
    primary_goal: &str,
    schema: &Value,
    conversation: &[ResponseItem],
    auto_instructions: Option<&str>,
    app_event_tx: &AppEventSender,
    cancel_token: &CancellationToken,
) -> Result<(String, Vec<ResponseItem>)> {
    match request_decision_with_model(
        runtime,
        client,
        developer_intro,
        primary_goal,
        schema,
        conversation,
        auto_instructions,
        app_event_tx,
        cancel_token,
        MODEL_SLUG,
    ) {
        Ok(result) => Ok((result.output_text, result.response_items)),
        Err(err) => {
            let fallback_slug = client.default_model_slug().to_string();
            if fallback_slug != MODEL_SLUG && should_retry_with_default_model(&err) {
                debug!(
                    preferred = %MODEL_SLUG,
                    fallback = %fallback_slug,
                    "auto coordinator falling back to configured model after invalid model error"
                );
                let original_error = err.to_string();
                return request_decision_with_model(
                    runtime,
                    client,
                    developer_intro,
                    primary_goal,
                    schema,
                    conversation,
                    auto_instructions,
                    app_event_tx,
                    cancel_token,
                    &fallback_slug,
                )
                .map(|res| (res.output_text, res.response_items))
                .map_err(|fallback_err| {
                    fallback_err.context(format!(
                        "coordinator fallback with model '{}' failed after original error: {}",
                        fallback_slug, original_error
                    ))
                });
            }
            Err(err)
        }
    }
}

fn request_decision_with_model(
    runtime: &tokio::runtime::Runtime,
    client: &ModelClient,
    developer_intro: &str,
    primary_goal: &str,
    schema: &Value,
    conversation: &[ResponseItem],
    auto_instructions: Option<&str>,
    app_event_tx: &AppEventSender,
    cancel_token: &CancellationToken,
    model_slug: &str,
) -> Result<RequestStreamResult> {
    let developer_intro = developer_intro.to_string();
    let primary_goal = primary_goal.to_string();
    let schema = schema.clone();
    let conversation: Vec<ResponseItem> = conversation.to_vec();
    let auto_instructions = auto_instructions.map(|text| text.to_string());
    let tx = app_event_tx.clone();
    let cancel = cancel_token.clone();
    let classify = |error: &anyhow::Error| classify_model_error(error);
    let options = RetryOptions::with_defaults(MAX_RETRY_ELAPSED);

    let result = runtime.block_on(async move {
        retry_with_backoff(
            || {
                let instructions = auto_instructions.clone();
                let prompt = build_prompt_for_model(
                    &developer_intro,
                    &primary_goal,
                    &schema,
                    &conversation,
                    model_slug,
                    instructions.as_deref(),
                );
                let tx_inner = tx.clone();
                async move {
                    #[cfg(feature = "dev-faults")]
                    if let Some(fault) = next_fault(FaultScope::AutoDrive) {
                        let err = fault_to_error(fault);
                        return Err(err);
                    }
                    let mut stream = client.stream(&prompt).await?;
                    let mut out = String::new();
                    let mut response_items: Vec<ResponseItem> = Vec::new();
                    let mut reasoning_delta_accumulator = String::new();
                    while let Some(ev) = stream.next().await {
                        match ev {
                            Ok(ResponseEvent::OutputTextDelta { delta, .. }) => {
                                out.push_str(&delta);
                            }
                            Ok(ResponseEvent::OutputItemDone { item, .. }) => {
                                if let ResponseItem::Message { content, .. } = &item {
                                    for c in content {
                                        if let ContentItem::OutputText { text } = c {
                                            out.push_str(text);
                                        }
                                    }
                                }
                                if matches!(item, ResponseItem::Reasoning { .. }) {
                                    reasoning_delta_accumulator.clear();
                                }
                                response_items.push(item);
                            }
                            Ok(ResponseEvent::ReasoningSummaryDelta {
                                delta,
                                summary_index,
                                ..
                            }) => {
                                let cleaned = strip_role_prefix(&delta);
                                reasoning_delta_accumulator.push_str(cleaned);
                                let message = cleaned.to_string();
                                tx_inner.send(AppEvent::AutoCoordinatorThinking {
                                    delta: message,
                                    summary_index,
                                });
                            }
                            Ok(ResponseEvent::ReasoningContentDelta { delta, .. }) => {
                                let cleaned = strip_role_prefix(&delta);
                                reasoning_delta_accumulator.push_str(cleaned);
                                let message = cleaned.to_string();
                                tx_inner.send(AppEvent::AutoCoordinatorThinking {
                                    delta: message,
                                    summary_index: None,
                                });
                            }
                            Ok(ResponseEvent::Completed { .. }) => break,
                            Err(err) => return Err(err.into()),
                            _ => {}
                        }
                    }
                    if !reasoning_delta_accumulator.trim().is_empty()
                        && !response_items
                            .iter()
                            .any(|item| matches!(item, ResponseItem::Reasoning { .. }))
                    {
                        response_items.push(ResponseItem::Reasoning {
                            id: String::new(),
                            summary: Vec::new(),
                            content: Some(vec![ReasoningItemContent::ReasoningText {
                                text: reasoning_delta_accumulator.trim().to_string(),
                            }]),
                            encrypted_content: None,
                        });
                    }
                    Ok(RequestStreamResult {
                        output_text: out,
                        response_items,
                    })
                }
            },
            classify,
            options,
            &cancel,
            |status| {
                let human_delay = status
                    .sleep
                    .map(format_duration)
                    .unwrap_or_else(|| "0s".to_string());
                let elapsed = format_duration(status.elapsed);
                let prefix = if status.is_rate_limit {
                    "Rate limit"
                } else {
                    "Transient error"
                };
                let attempt = status.attempt;
                let resume_str = status.resume_at.and_then(|resume| {
                    let now = Instant::now();
                    if resume <= now {
                        Some("now".to_string())
                    } else {
                        let remaining = resume.duration_since(now);
                        SystemTime::now()
                            .checked_add(remaining)
                            .map(|time| {
                                let local: DateTime<Local> = time.into();
                                local.format("%Y-%m-%d %H:%M:%S").to_string()
                            })
                    }
                });
                let message = format!(
                    "{prefix} (attempt {attempt}): {}; retrying in {human_delay} (elapsed {elapsed}){}",
                    status.reason,
                    resume_str
                        .map(|s| format!("; next attempt at {s}"))
                        .unwrap_or_default()
                );
                let _ = tx.send(AppEvent::AutoCoordinatorThinking {
                    delta: message,
                    summary_index: None,
                });
            },
        )
        .await
    });

    match result {
        Ok(output) => Ok(output),
        Err(RetryError::Aborted) => Err(anyhow!(AutoCoordinatorCancelled)),
        Err(RetryError::Fatal(err)) => Err(err),
        Err(RetryError::Timeout { elapsed, last_error }) => Err(last_error.context(format!(
            "auto coordinator retry window exceeded after {}",
            format_duration(elapsed)
        ))),
    }
}

fn build_prompt_for_model(
    developer_intro: &str,
    primary_goal: &str,
    schema: &Value,
    conversation: &[ResponseItem],
    model_slug: &str,
    auto_instructions: Option<&str>,
) -> Prompt {
    let mut prompt = Prompt::default();
    prompt.store = true;
    if let Some(instructions) = auto_instructions {
        let trimmed = instructions.trim();
        if !trimmed.is_empty() {
            prompt
                .input
                .push(make_message("developer", trimmed.to_string()));
        }
    }
    prompt
        .input
        .push(make_message("developer", developer_intro.to_string()));
    prompt
        .input
        .push(make_message("developer", primary_goal.to_string()));
    prompt.input.extend(conversation.iter().cloned());
    prompt.text_format = Some(TextFormat {
        r#type: "json_schema".to_string(),
        name: Some(SCHEMA_NAME.to_string()),
        strict: Some(true),
        schema: Some(schema.clone()),
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
            if let CodexErr::UnexpectedStatus(status, body) = codex_err {
                if !status.is_client_error() {
                    return false;
                }
                let body_lower = body.to_lowercase();
                return body_lower.contains("invalid model")
                    || body_lower.contains("unknown model")
                    || body_lower.contains("model_not_found")
                    || body_lower.contains("model does not exist");
            }
        }
        false
    })
}

fn classify_model_error(error: &anyhow::Error) -> RetryDecision {
    if let Some(codex_err) = find_in_chain::<CodexErr>(error) {
        match codex_err {
            CodexErr::Stream(message, _) => {
                return RetryDecision::RetryAfterBackoff {
                    reason: format!("model stream error: {message}"),
                };
            }
            CodexErr::Timeout => {
                return RetryDecision::RetryAfterBackoff {
                    reason: "model request timed out".to_string(),
                };
            }
            CodexErr::UnexpectedStatus(status, body) => {
                if *status == StatusCode::REQUEST_TIMEOUT || status.as_u16() == 408 {
                    return RetryDecision::RetryAfterBackoff {
                        reason: format!("provider returned {status}"),
                    };
                }
                if status.as_u16() == 499 {
                    return RetryDecision::RetryAfterBackoff {
                        reason: "client closed request (499)".to_string(),
                    };
                }
                if *status == StatusCode::TOO_MANY_REQUESTS {
                    if let Some(wait_until) = parse_rate_limit_hint(body) {
                        return RetryDecision::RateLimited {
                            wait_until,
                            reason: "rate limited; waiting for reset".to_string(),
                        };
                    }
                    return RetryDecision::RetryAfterBackoff {
                        reason: "rate limited (429)".to_string(),
                    };
                }
                if status.is_client_error() {
                    return RetryDecision::Fatal(anyhow!(error.to_string()));
                }
                if status.is_server_error() {
                    return RetryDecision::RetryAfterBackoff {
                        reason: format!("server error {status}"),
                    };
                }
            }
            CodexErr::UsageLimitReached(limit) => {
                if let Some(seconds) = limit.resets_in_seconds {
                    let wait_until = compute_rate_limit_wait(Duration::from_secs(seconds));
                    return RetryDecision::RateLimited {
                        wait_until,
                        reason: "usage limit reached".to_string(),
                    };
                }
                return RetryDecision::RetryAfterBackoff {
                    reason: "usage limit reached".to_string(),
                };
            }
            CodexErr::UsageNotIncluded => {
                return RetryDecision::Fatal(anyhow!(error.to_string()));
            }
            CodexErr::ServerError(_) => {
                return RetryDecision::RetryAfterBackoff {
                    reason: error.to_string(),
                };
            }
            CodexErr::RetryLimit(status) => {
                return RetryDecision::Fatal(anyhow!("retry limit exceeded (status {status})"));
            }
            CodexErr::Reqwest(req_err) => {
                return classify_reqwest_error(req_err);
            }
            CodexErr::Io(io_err) => {
                if io_err.kind() == std::io::ErrorKind::TimedOut {
                    return RetryDecision::RetryAfterBackoff {
                        reason: "network timeout".to_string(),
                    };
                }
            }
            _ => {}
        }
    }

    if let Some(req_err) = find_in_chain::<reqwest::Error>(error) {
        return classify_reqwest_error(req_err);
    }

    if let Some(io_err) = find_in_chain::<std::io::Error>(error) {
        if io_err.kind() == std::io::ErrorKind::TimedOut {
            return RetryDecision::RetryAfterBackoff {
                reason: "network timeout".to_string(),
            };
        }
    }

    RetryDecision::Fatal(anyhow!(error.to_string()))
}

fn classify_reqwest_error(err: &reqwest::Error) -> RetryDecision {
    if err.is_timeout() || err.is_connect() || err.is_request() && err.status().is_none() {
        return RetryDecision::RetryAfterBackoff {
            reason: format!("network error: {err}"),
        };
    }

    if let Some(status) = err.status() {
        if status == StatusCode::TOO_MANY_REQUESTS {
            return RetryDecision::RetryAfterBackoff {
                reason: "rate limited (429)".to_string(),
            };
        }
        if status == StatusCode::REQUEST_TIMEOUT || status.as_u16() == 408 {
            return RetryDecision::RetryAfterBackoff {
                reason: format!("provider returned {status}"),
            };
        }
        if status.as_u16() == 499 {
            return RetryDecision::RetryAfterBackoff {
                reason: "client closed request (499)".to_string(),
            };
        }
        if status.is_server_error() {
            return RetryDecision::RetryAfterBackoff {
                reason: format!("server error {status}"),
            };
        }
        if status.is_client_error() {
            return RetryDecision::Fatal(anyhow!(err.to_string()));
        }
    }

    RetryDecision::Fatal(anyhow!(err.to_string()))
}

fn parse_rate_limit_hint(body: &str) -> Option<Instant> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let error_obj = value.get("error").unwrap_or(&value);

    if let Some(seconds) = extract_seconds(error_obj) {
        return Some(compute_rate_limit_wait(seconds));
    }

    if let Some(reset_at) = extract_reset_at(error_obj) {
        return Some(reset_at);
    }

    None
}

fn extract_seconds(value: &serde_json::Value) -> Option<Duration> {
    let fields = [
        "reset_seconds",
        "reset_in_seconds",
        "resets_in_seconds",
        "x-ratelimit-reset",
        "x-ratelimit-reset-requests",
    ];
    for key in fields {
        if let Some(seconds) = value.get(key) {
            if let Some(num) = seconds.as_f64() {
                if num.is_sign_negative() {
                    continue;
                }
                return Some(Duration::from_secs_f64(num));
            }
            if let Some(text) = seconds.as_str() {
                if let Ok(num) = text.parse::<f64>() {
                    if num.is_sign_negative() {
                        continue;
                    }
                    return Some(Duration::from_secs_f64(num));
                }
            }
        }
    }
    None
}

fn extract_reset_at(value: &serde_json::Value) -> Option<Instant> {
    let reset_at = value.get("reset_at").and_then(|v| v.as_str())?;
    let parsed = DateTime::parse_from_rfc3339(reset_at)
        .or_else(|_| DateTime::parse_from_str(reset_at, "%+"))
        .ok()?;
    let reset_utc = parsed.with_timezone(&Utc);
    let now = Utc::now();
    let duration = reset_utc.signed_duration_since(now).to_std().unwrap_or_default();
    Some(compute_rate_limit_wait(duration))
}

fn compute_rate_limit_wait(base: Duration) -> Instant {
    let mut wait = if base > Duration::ZERO { base } else { Duration::ZERO };
    wait += RATE_LIMIT_BUFFER;
    wait += random_jitter(RATE_LIMIT_JITTER_MAX);
    Instant::now() + wait
}

fn random_jitter(max: Duration) -> Duration {
    if max.is_zero() {
        return Duration::ZERO;
    }
    let mut rng = rand::rng();
    let jitter = rng.random_range(0.0..max.as_secs_f64());
    Duration::from_secs_f64(jitter)
}

fn find_in_chain<'a, T: std::error::Error + 'static>(error: &'a anyhow::Error) -> Option<&'a T> {
    for cause in error.chain() {
        if let Some(specific) = cause.downcast_ref::<T>() {
            return Some(specific);
        }
    }
    None
}

#[cfg(test)]
pub(crate) use classify_model_error as test_classify_model_error;

#[cfg(test)]
mod tests {
    use super::*;
    use codex_core::error::UsageLimitReachedError;

    #[test]
    fn rate_limit_hint_uses_reset_seconds() {
        let body = r#"{"error":{"reset_seconds":5}}"#;
        let start = Instant::now();
        let wait = parse_rate_limit_hint(body).expect("expected wait instant");
        let remaining = wait
            .checked_duration_since(start)
            .expect("wait should be in the future");
        let rem_secs = remaining.as_secs_f64();
        let min_expected = (RATE_LIMIT_BUFFER + Duration::from_secs(5)).as_secs_f64() - 0.5;
        let max_expected =
            (RATE_LIMIT_BUFFER + Duration::from_secs(5) + RATE_LIMIT_JITTER_MAX).as_secs_f64() + 1.0;
        assert!(rem_secs >= min_expected, "remaining {rem_secs}, min {min_expected}");
        assert!(rem_secs <= max_expected, "remaining {rem_secs}, max {max_expected}");
    }

    #[test]
    fn rate_limit_hint_uses_reset_at() {
        let reset_at = (Utc::now() + chrono::Duration::seconds(10)).to_rfc3339();
        let body = format!("{{\"error\":{{\"reset_at\":\"{reset_at}\"}}}}");
        let wait = parse_rate_limit_hint(&body).expect("expected wait instant");
        let remaining = wait
            .checked_duration_since(Instant::now())
            .expect("wait should be in the future");
        assert!(
            remaining.as_secs_f64()
                >= (RATE_LIMIT_BUFFER + Duration::from_secs(10)).as_secs_f64() - 1.0
        );
    }

    #[test]
    fn classify_identifies_stream_disconnect() {
        let err = anyhow!(CodexErr::Stream("disconnect".into(), None));
        match classify_model_error(&err) {
            RetryDecision::RetryAfterBackoff { reason } => {
                assert!(reason.contains("disconnect"));
            }
            other => panic!("unexpected decision: {:?}", other),
        }
    }

    #[test]
    fn classify_usage_limit_returns_rate_limit() {
        let err = anyhow!(CodexErr::UsageLimitReached(UsageLimitReachedError {
            plan_type: None,
            resets_in_seconds: Some(10),
        }));
        match classify_model_error(&err) {
            RetryDecision::RateLimited { .. } => {}
            other => panic!("expected rate limit, got {:?}", other),
        }
    }

    #[test]
    fn classify_fatal_on_bad_request() {
        let err = anyhow!(CodexErr::UnexpectedStatus(StatusCode::BAD_REQUEST, "bad".to_string()));
        match classify_model_error(&err) {
            RetryDecision::Fatal(_) => {}
            other => panic!("expected fatal, got {:?}", other),
        }
    }

    #[test]
    fn observer_cadence_triggers_every_n_requests() {
        assert!(!should_trigger_observer(0, 5));
        assert!(!should_trigger_observer(1, 5));
        assert!(should_trigger_observer(5, 5));
        assert!(should_trigger_observer(10, 5));
        assert!(!should_trigger_observer(11, 5));
        assert!(!should_trigger_observer(3, 0));
    }

    #[test]
    fn compose_intro_appends_guidance_section() {
        let base = "Intro";
        let guidance = vec!["First".to_string(), "Second".to_string()];
        let combined = compose_developer_intro(base, &guidance);
        assert!(combined.contains("Intro"));
        assert!(combined.contains("**Observer Guidance**"));
        assert!(combined.contains("- First"));
        assert!(combined.contains("- Second"));
    }
}

fn parse_decision(raw: &str) -> Result<(CoordinatorDecision, Value)> {
    let value: Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => {
            let Some(json_blob) = extract_first_json_object(raw) else {
                return Err(anyhow!("model response was not valid JSON"));
            };
            serde_json::from_str(&json_blob).context("parsing JSON from model output")?
        }
    };
    let decision: CoordinatorDecision = serde_json::from_value(value.clone())
        .context("decoding coordinator decision")?;
    Ok((decision, value))
}

pub(super) fn extract_first_json_object(input: &str) -> Option<String> {
    let mut depth = 0usize;
    let mut in_str = false;
    let mut escape = false;
    let mut start: Option<usize> = None;
    for (idx, ch) in input.char_indices() {
        if in_str {
            if escape {
                escape = false;
                continue;
            }
            match ch {
                '"' => in_str = false,
                '\\' => escape = true,
                _ => {}
            }
            continue;
        }
        match ch {
            '"' => in_str = true,
            '{' => {
                if depth == 0 {
                    start = Some(idx);
                }
                depth += 1;
            }
            '}' => {
                if depth == 0 {
                    continue;
                }
                depth -= 1;
                if depth == 0 {
                    let Some(s) = start else { return None; };
                    return Some(input[s..=idx].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

pub(super) fn make_message(role: &str, text: String) -> ResponseItem {
    let content = if role.eq_ignore_ascii_case("assistant") {
        ContentItem::OutputText { text }
    } else {
        ContentItem::InputText { text }
    };

    ResponseItem::Message {
        id: None,
        role: role.to_string(),
        content: vec![content],
    }
}

fn strip_role_prefix(input: &str) -> &str {
    const PREFIXES: [&str; 2] = ["Coordinator:", "CLI:"];
    for prefix in PREFIXES {
        if let Some(head) = input.get(..prefix.len()) {
            if head.eq_ignore_ascii_case(prefix) {
                let rest = input
                    .get(prefix.len()..)
                    .unwrap_or_default();
                return rest.strip_prefix(' ').unwrap_or(rest);
            }
        }
    }
    input
}
