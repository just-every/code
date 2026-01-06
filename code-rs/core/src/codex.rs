// Poisoned mutex should fail the program
#![allow(clippy::unwrap_used)]

use std::borrow::Cow;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::RwLock;
use std::sync::Weak;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use async_channel::Receiver;
use async_channel::Sender;
use base64::Engine;
use code_apply_patch::ApplyPatchAction;
use code_apply_patch::MaybeApplyPatchVerified;
use crate::bridge_client::spawn_bridge_listener;
use code_browser::BrowserConfig as CodexBrowserConfig;
use code_browser::BrowserManager;
use code_otel::otel_event_manager::{
    OtelEventManager,
    ToolDecisionSource,
    TurnLatencyPayload,
    TurnLatencyPhase,
};
use code_protocol::config_types::ReasoningEffort as ProtoReasoningEffort;
use code_protocol::config_types::ReasoningSummary as ProtoReasoningSummary;
use code_protocol::protocol::AskForApproval as ProtoAskForApproval;
use code_protocol::protocol::ReviewDecision as ProtoReviewDecision;
use code_protocol::protocol::SandboxPolicy as ProtoSandboxPolicy;
use code_protocol::protocol::BROWSER_SNAPSHOT_OPEN_TAG;
use code_protocol::protocol::ENVIRONMENT_CONTEXT_CLOSE_TAG;
use code_protocol::protocol::ENVIRONMENT_CONTEXT_DELTA_CLOSE_TAG;
use code_protocol::protocol::ENVIRONMENT_CONTEXT_DELTA_OPEN_TAG;
use code_protocol::protocol::ENVIRONMENT_CONTEXT_OPEN_TAG;
use crate::config_types::ReasoningEffort as ReasoningEffortConfig;
use crate::config_types::ReasoningSummary as ReasoningSummaryConfig;
use crate::config_types::ClientTools;
// unused: AuthManager
// unused: ConversationHistoryResponseEvent
use code_protocol::protocol::TurnAbortReason;
use code_protocol::protocol::TurnAbortedEvent;
use futures::prelude::*;
use mcp_types::CallToolResult;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::oneshot;
use tokio::task::AbortHandle;
use tracing::debug;
use tracing::error;
use tracing::info;
use tracing::trace;
use tracing::warn;
use uuid::Uuid;
use crate::EnvironmentContextEmission;
use crate::AuthManager;
use crate::CodexAuth;
use crate::agent_tool::AgentStatusUpdatePayload;
use crate::remote_models::RemoteModelsManager;
use crate::split_command_and_args;
use crate::git_worktree;
use crate::protocol::ApprovedCommandMatchKind;
use crate::protocol::WebSearchBeginEvent;
use crate::protocol::WebSearchCompleteEvent;
use code_protocol::mcp_protocol::AuthMode;
use crate::account_usage;
use crate::auth_accounts;
use crate::agent_defaults::{agent_model_spec, default_agent_configs, enabled_agent_model_specs};
use code_protocol::models::WebSearchAction;
use code_protocol::protocol::RolloutItem;
use shlex::split as shlex_split;
use shlex::try_join as shlex_try_join;
use chrono::Local;
use chrono::Utc;

pub mod compact;
pub mod compact_remote;
mod events;
mod exec;
mod session;
mod streaming;

pub use session::ApprovedCommandPattern;
pub(crate) use session::{Session, ToolCallCtx};
use self::compact::{build_compacted_history, collect_compaction_snippets};
use self::compact_remote::run_inline_remote_auto_compact_task;
use self::streaming::{add_pending_screenshot, capture_browser_screenshot, submission_loop};

/// Initial submission ID for session configuration
pub(crate) const INITIAL_SUBMIT_ID: &str = "";
const HOOK_OUTPUT_LIMIT: usize = 2048;
const PENDING_ONLY_SENTINEL: &str = "__code_pending_only__";
const MIN_SHELL_TIMEOUT_MS: u64 = 30 * 60 * 1000;

#[derive(Clone, Default)]
struct ConfirmGuardRuntime {
    patterns: Vec<ConfirmGuardPatternRuntime>,
}

#[derive(Clone)]
struct ConfirmGuardPatternRuntime {
    regex: regex_lite::Regex,
    message: Option<String>,
    raw: String,
}

impl ConfirmGuardRuntime {
    fn from_config(config: &crate::config_types::ConfirmGuardConfig) -> Self {
        let mut patterns = Vec::new();
        for pattern in &config.patterns {
            match regex_lite::Regex::new(&pattern.regex) {
                Ok(regex) => patterns.push(ConfirmGuardPatternRuntime {
                    regex,
                    message: pattern.message.clone(),
                    raw: pattern.regex.clone(),
                }),
                Err(err) => {
                    tracing::warn!("Skipping confirm guard pattern `{}`: {err}", pattern.regex);
                }
            }
        }
        Self { patterns }
    }

    fn matched_pattern(&self, input: &str) -> Option<&ConfirmGuardPatternRuntime> {
        self.patterns.iter().find(|pat| pat.regex.is_match(input))
    }

    fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }
}

impl ConfirmGuardPatternRuntime {
    fn guidance(&self, original_label: &str, original_value: &str, suggested: &str) -> String {
        let header = self
            .message
            .clone()
            .unwrap_or_else(|| {
                format!(
                    "Blocked command matching confirm guard pattern `{}`. Resend with 'confirm:' if you intend to proceed.",
                    self.raw
                )
            });
        format!("{header}\n\n{original_label}: {original_value}\nresend_exact_argv: {suggested}")
    }
}

fn to_proto_reasoning_effort(effort: ReasoningEffortConfig) -> ProtoReasoningEffort {
    match effort {
        ReasoningEffortConfig::Minimal => ProtoReasoningEffort::Minimal,
        ReasoningEffortConfig::Low => ProtoReasoningEffort::Low,
        ReasoningEffortConfig::Medium => ProtoReasoningEffort::Medium,
        ReasoningEffortConfig::High => ProtoReasoningEffort::High,
        ReasoningEffortConfig::XHigh => ProtoReasoningEffort::XHigh,
        ReasoningEffortConfig::None => ProtoReasoningEffort::Minimal,
    }
}

fn to_proto_reasoning_summary(summary: ReasoningSummaryConfig) -> ProtoReasoningSummary {
    match summary {
        ReasoningSummaryConfig::Auto => ProtoReasoningSummary::Auto,
        ReasoningSummaryConfig::Concise => ProtoReasoningSummary::Concise,
        ReasoningSummaryConfig::Detailed => ProtoReasoningSummary::Detailed,
        ReasoningSummaryConfig::None => ProtoReasoningSummary::None,
    }
}

fn to_proto_approval_policy(policy: AskForApproval) -> ProtoAskForApproval {
    match policy {
        AskForApproval::UnlessTrusted => ProtoAskForApproval::UnlessTrusted,
        AskForApproval::OnFailure => ProtoAskForApproval::OnFailure,
        AskForApproval::OnRequest => ProtoAskForApproval::OnRequest,
        AskForApproval::Never => ProtoAskForApproval::Never,
    }
}

fn to_proto_sandbox_policy(policy: SandboxPolicy) -> ProtoSandboxPolicy {
    match policy {
        SandboxPolicy::DangerFullAccess => ProtoSandboxPolicy::DangerFullAccess,
        SandboxPolicy::ReadOnly => ProtoSandboxPolicy::ReadOnly,
        SandboxPolicy::WorkspaceWrite {
            writable_roots,
            network_access,
            exclude_tmpdir_env_var,
            exclude_slash_tmp,
            allow_git_writes,
        } => ProtoSandboxPolicy::WorkspaceWrite {
            writable_roots,
            network_access,
            exclude_tmpdir_env_var,
            exclude_slash_tmp,
            allow_git_writes,
        },
    }
}

fn to_proto_review_decision(decision: ReviewDecision) -> ProtoReviewDecision {
    match decision {
        ReviewDecision::Approved => ProtoReviewDecision::Approved,
        ReviewDecision::ApprovedForSession => ProtoReviewDecision::ApprovedForSession,
        ReviewDecision::Denied => ProtoReviewDecision::Denied,
        ReviewDecision::Abort => ProtoReviewDecision::Abort,
    }
}

#[allow(dead_code)]
trait MutexExt<T> {
    fn lock_unchecked(&self) -> std::sync::MutexGuard<'_, T>;
}

#[allow(dead_code)]
impl<T> MutexExt<T> for Mutex<T> {
    fn lock_unchecked(&self) -> std::sync::MutexGuard<'_, T> {
        #[expect(clippy::expect_used)]
        self.lock().expect("poisoned lock")
    }
}

#[derive(Clone)]
pub(crate) struct TurnContext {
    pub(crate) client: ModelClient,
    pub(crate) cwd: PathBuf,
    pub(crate) base_instructions: Option<String>,
    pub(crate) user_instructions: Option<String>,
    pub(crate) demo_developer_message: Option<String>,
    pub(crate) compact_prompt_override: Option<String>,
    pub(crate) approval_policy: AskForApproval,
    pub(crate) sandbox_policy: SandboxPolicy,
    pub(crate) shell_environment_policy: ShellEnvironmentPolicy,
    pub(crate) is_review_mode: bool,
    pub(crate) text_format_override: Option<TextFormat>,
    pub(crate) final_output_json_schema: Option<Value>,
}

/// Gather ephemeral, per-turn context that should not be persisted to history.
/// Combines environment info and (when enabled) a live browser snapshot and status.
struct EphemeralJar {
    items: Vec<ResponseItem>,
}

impl EphemeralJar {
    fn new() -> Self {
        Self { items: Vec::new() }
    }

    fn into_items(self) -> Vec<ResponseItem> {
        self.items
    }
}

/// Convert a vector of core `InputItem`s into a single `ResponseInputItem`
/// suitable for sending to the model. Handles images (local and pre‑encoded)
/// and our fork's ephemeral image variant by inlining a brief metadata marker
/// followed by the image as a data URL.
fn response_input_from_core_items(items: Vec<InputItem>) -> ResponseInputItem {
    let mut content_items = Vec::new();

    for item in items {
        match item {
            InputItem::Text { text } => {
                content_items.push(ContentItem::InputText { text });
            }
            InputItem::Image { image_url } => {
                content_items.push(ContentItem::InputImage { image_url });
            }
            InputItem::LocalImage { path } => match std::fs::read(&path) {
                Ok(bytes) => {
                    let mime = mime_guess::from_path(&path)
                        .first()
                        .map(|m| m.essence_str().to_owned())
                        .unwrap_or_else(|| "application/octet-stream".to_string());
                    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
                    content_items.push(ContentItem::InputImage {
                        image_url: format!("data:{mime};base64,{encoded}"),
                    });
                }
                Err(err) => {
                    tracing::warn!(
                        "Skipping image {} – could not read file: {}",
                        path.display(),
                        err
                    );
                }
            },
            InputItem::EphemeralImage { path, metadata } => {
                tracing::info!(
                    "Processing ephemeral image: {} with metadata: {:?}",
                    path.display(),
                    metadata
                );

                if let Some(meta) = metadata {
                    content_items.push(ContentItem::InputText {
                        text: format!("[EPHEMERAL:{}]", meta),
                    });
                }

                match std::fs::read(&path) {
                    Ok(bytes) => {
                        let mime = mime_guess::from_path(&path)
                            .first()
                            .map(|m| m.essence_str().to_owned())
                            .unwrap_or_else(|| "application/octet-stream".to_string());
                        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
                        tracing::info!("Created ephemeral image data URL with mime: {}", mime);
                        content_items.push(ContentItem::InputImage {
                            image_url: format!("data:{mime};base64,{encoded}"),
                        });
                    }
                    Err(err) => {
                        tracing::error!(
                            "Failed to read ephemeral image {} – {}",
                            path.display(),
                            err
                        );
                    }
                }
            }
        }
    }

    ResponseInputItem::Message {
        role: "user".to_string(),
        content: content_items,
    }
}

fn convert_call_tool_result_to_function_call_output_payload(
    result: &Result<CallToolResult, String>,
) -> FunctionCallOutputPayload {
    match result {
        Ok(ok) => FunctionCallOutputPayload {
            content: serde_json::to_string(ok)
                .unwrap_or_else(|e| format!("JSON serialization error: {e}")),
            success: Some(true),
        },
        Err(e) => FunctionCallOutputPayload {
            content: format!("err: {e:?}"),
            success: Some(false),
        },
    }
}

fn get_git_branch(cwd: &std::path::Path) -> Option<String> {
    let head_path = cwd.join(".git/HEAD");
    if let Ok(contents) = std::fs::read_to_string(&head_path) {
        if let Some(rest) = contents.trim().strip_prefix("ref: ") {
            if let Some(branch) = rest.trim().rsplit('/').next() {
                return Some(branch.to_string());
            }
        }
    }
    None
}

fn maybe_update_from_model_info<T: Copy + PartialEq>(
    field: &mut Option<T>,
    old_default: Option<T>,
    new_default: Option<T>,
) {
    if field.is_none() {
        if let Some(new_val) = new_default {
            *field = Some(new_val);
        }
        return;
    }

    if let (Some(current), Some(old_val)) = (*field, old_default) {
        if current == old_val {
            *field = new_default;
        }
    }
}

#[derive(Clone, Debug)]
struct RunTimeBudget {
    deadline: Instant,
    total: Duration,
    next_nudge_at: Instant,
}

impl RunTimeBudget {
    fn new(deadline: Instant, total: Duration) -> Self {
        let half = total / 2;
        let next_nudge_at = deadline.checked_sub(half).unwrap_or(deadline);
        Self {
            deadline,
            total,
            next_nudge_at,
        }
    }

    fn maybe_nudge(&mut self, now: Instant) -> Option<String> {
        if now < self.next_nudge_at {
            return None;
        }

        let remaining = self.deadline.saturating_duration_since(now);
        let elapsed = self.total.saturating_sub(remaining);

        if elapsed < (self.total / 2) {
            // Avoid time pressure early.
            let half = self.total / 2;
            self.next_nudge_at = self.deadline.checked_sub(half).unwrap_or(self.deadline);
            return None;
        }

        let guidance = if remaining <= Duration::from_secs(30) {
            "Time is nearly up: stop exploring; take the simplest safe path and do one cheap verification before finishing."
        } else if remaining <= Duration::from_secs(120) {
            "Time is tight: parallelize any remaining scouting/verification (batch tool calls) and finish with the cheapest proof."
        } else {
            "Past 50% of the time budget: start converging; parallelize remaining scouting/verification and avoid detours."
        };

        self.next_nudge_at = now + next_budget_nudge_interval(remaining);

        let total_secs = self.total.as_secs();
        let elapsed_secs = elapsed.as_secs();
        let remaining_secs = remaining.as_secs();
        Some(format!(
            "== System Status ==\n [automatic message added by system]\n\n time_budget: {total_secs}s\n elapsed: {elapsed_secs}s\n remaining: {remaining_secs}s\n\n Guidance: {guidance}"
        ))
    }
}

fn next_budget_nudge_interval(remaining: Duration) -> Duration {
    if remaining >= Duration::from_secs(30 * 60) {
        Duration::from_secs(5 * 60)
    } else if remaining >= Duration::from_secs(10 * 60) {
        Duration::from_secs(2 * 60)
    } else if remaining >= Duration::from_secs(5 * 60) {
        Duration::from_secs(60)
    } else if remaining >= Duration::from_secs(2 * 60) {
        Duration::from_secs(30)
    } else if remaining >= Duration::from_secs(60) {
        Duration::from_secs(15)
    } else if remaining >= Duration::from_secs(30) {
        Duration::from_secs(10)
    } else if remaining >= Duration::from_secs(10) {
        Duration::from_secs(5)
    } else {
        Duration::from_secs(2)
    }
}

fn maybe_time_budget_status_item(sess: &Session) -> Option<ResponseItem> {
    let mut guard = sess.time_budget.lock().unwrap();
    let budget = guard.as_mut()?;
    let text = budget.maybe_nudge(Instant::now())?;
    Some(ResponseItem::Message {
        id: Some(format!("run-budget-{}", sess.id)),
        role: "user".to_string(),
        content: vec![ContentItem::InputText { text }],
    })
}

async fn build_turn_status_items(sess: &Session) -> Vec<ResponseItem> {
    if sess.env_ctx_v2 {
        build_turn_status_items_v2(sess).await
    } else {
        build_turn_status_items_legacy(sess).await
    }
}

async fn build_turn_status_items_legacy(sess: &Session) -> Vec<ResponseItem> {
    let mut jar = EphemeralJar::new();

    // Collect environment context
    let cwd = sess.cwd.to_string_lossy().to_string();
    let branch = get_git_branch(&sess.cwd).unwrap_or_else(|| "unknown".to_string());
    let reasoning_effort = sess.client.get_reasoning_effort();

    // Build current system status (UI-only; not persisted)
    let mut current_status = format!(
        r#"== System Status ==
 [automatic message added by system]

 cwd: {cwd}
 branch: {branch}
 reasoning: {reasoning_effort:?}"#
    );

    // Prepare browser context + optional screenshot
    let mut screenshot_content: Option<ContentItem> = None;
    let mut include_screenshot = false;

    if let Some(browser_manager) = code_browser::global::get_browser_manager().await {
        if browser_manager.is_enabled().await {
            if let Some((_, idle_timeout)) = browser_manager.idle_elapsed_past_timeout().await {
                let idle_text = format!(
                    "Browser idle (timeout {:?}); screenshot capture paused until browser_* tools run again.",
                    idle_timeout
                );
                current_status.push_str("\n");
                current_status.push_str(&idle_text);
            } else {
                // Get current URL and browser info
                let url = browser_manager
                    .get_current_url()
                    .await
                    .unwrap_or_else(|| "unknown".to_string());

                // Try to get a tab title if available
                let title = match browser_manager.get_or_create_page().await {
                    Ok(page) => page.get_title().await,
                    Err(_) => None,
                };

                // Get browser type description
                let browser_type = browser_manager.get_browser_type().await;

                // Get viewport dimensions
                let (viewport_width, viewport_height) = browser_manager.get_viewport_size().await;
                let viewport_info = format!(" | Viewport: {}x{}", viewport_width, viewport_height);

                // Get cursor position
                let cursor_info = match browser_manager.get_cursor_position().await {
                    Ok((x, y)) => format!(
                        " | Mouse position: ({:.0}, {:.0}) [shown as a blue cursor in the screenshot]",
                        x, y
                    ),
                    Err(_) => String::new(),
                };

                // Try to capture screenshot and compare with last one
                let screenshot_status = match capture_browser_screenshot(sess).await {
                    Ok((screenshot_path, _url)) => {
                        // Always update the UI with the latest screenshot, even if unchanged for LLM payload
                        // This ensures the user sees that a fresh capture occurred each turn.
                        add_pending_screenshot(sess, screenshot_path.clone(), url.clone());
                        // Check if screenshot has changed using image hashing
                        let mut last_screenshot_info = sess.last_screenshot_info.lock().unwrap();

                        // Compute hash for current screenshot
                        let current_hash =
                            crate::image_comparison::compute_image_hash(&screenshot_path).ok();

                        let should_include_screenshot = if let (
                            Some((_last_path, last_phash, last_dhash)),
                            Some((cur_phash, cur_dhash)),
                        ) =
                            (last_screenshot_info.as_ref(), current_hash.as_ref())
                        {
                            // Compare hashes to see if screenshots are similar
                            let similar = crate::image_comparison::are_hashes_similar(
                                last_phash, last_dhash, cur_phash, cur_dhash,
                            );

                            if !similar {
                                // Screenshot has changed, include it
                                *last_screenshot_info = Some((
                                    screenshot_path.clone(),
                                    cur_phash.clone(),
                                    cur_dhash.clone(),
                                ));
                                true
                            } else {
                                // Screenshot unchanged
                                false
                            }
                        } else {
                            // No previous screenshot or hash computation failed, include it
                            if let Some((phash, dhash)) = current_hash {
                                *last_screenshot_info = Some((screenshot_path.clone(), phash, dhash));
                            }
                            true
                        };

                        if should_include_screenshot {
                            if let Ok(bytes) = std::fs::read(&screenshot_path) {
                                let mime = mime_guess::from_path(&screenshot_path)
                                    .first()
                                    .map(|m| m.to_string())
                                    .unwrap_or_else(|| "image/png".to_string());
                                let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
                                screenshot_content = Some(ContentItem::InputImage {
                                    image_url: format!("data:{mime};base64,{encoded}"),
                                });
                                include_screenshot = true;
                                ""
                            } else {
                                " [Screenshot file read failed]"
                            }
                        } else {
                            " [Screenshot unchanged]"
                        }
                    }
                    Err(err_msg) => {
                        // Include error message so LLM knows screenshot failed
                        format!(" [Screenshot unavailable: {}]", err_msg).leak()
                    }
                };

                let status_line = if let Some(t) = title {
                    format!(
                        "Browser url: {} — {} ({}){}{}{}. You can interact with it using browser_* tools.",
                        url, t, browser_type, viewport_info, cursor_info, screenshot_status
                    )
                } else {
                    format!(
                        "Browser url: {} ({}){}{}{}. You can interact with it using browser_* tools.",
                        url, browser_type, viewport_info, cursor_info, screenshot_status
                    )
                };
                current_status.push_str("\n");
                current_status.push_str(&status_line);
            }
        }
    }

    // Check if system status has changed
    let mut last_status = sess.last_system_status.lock().unwrap();
    let status_changed = last_status.as_ref() != Some(&current_status);

    if status_changed {
        // Update last status
        *last_status = Some(current_status.clone());
    }

    // Only include items if something has changed or is new
    let mut content: Vec<ContentItem> = Vec::new();

    if status_changed {
        content.push(ContentItem::InputText {
            text: current_status,
        });
    }

    if include_screenshot {
        if let Some(image) = screenshot_content {
            content.push(image);
        }
    }

    if !content.is_empty() {
        jar.items.push(ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content,
        });
    }

    if let Some(item) = maybe_time_budget_status_item(sess) {
        jar.items.push(item);
    }

    jar.into_items()
}

async fn build_turn_status_items_v2(sess: &Session) -> Vec<ResponseItem> {
    let mut items = Vec::new();

    let env_context = EnvironmentContext::new(
        Some(sess.cwd.clone()),
        Some(sess.approval_policy),
        Some(sess.sandbox_policy.clone()),
        Some(sess.user_shell.clone()),
    );

    if let Some(mut env_items) = sess.maybe_emit_env_ctx_messages(
        &env_context,
        get_git_branch(&sess.cwd),
        Some(format!("{:?}", sess.client.get_reasoning_effort())),
    ) {
        items.append(&mut env_items);
    }

    if let Some(item) = maybe_time_budget_status_item(sess) {
        items.push(item);
    }

    if let Some(browser_manager) = code_browser::global::get_browser_manager().await {
        if browser_manager.is_enabled().await {
            let browser_stream_id = {
                let mut state = sess.state.lock().unwrap();
                state
                    .context_stream_ids
                    .browser_stream_id(sess.id)
            };

            if let Some((_, timeout)) = browser_manager.idle_elapsed_past_timeout().await {
                let idle_text = format!(
                    "Browser idle (timeout {:?}); screenshot capture paused until browser_* tools run again.",
                    timeout
                );
                items.push(ResponseItem::Message {
                    id: Some(browser_stream_id),
                    role: "user".to_string(),
                    content: vec![ContentItem::InputText { text: idle_text }],
                });
                return items;
            } else {
                let url = browser_manager
                    .get_current_url()
                    .await
                    .unwrap_or_else(|| "unknown".to_string());

            let title = match browser_manager.get_or_create_page().await {
                Ok(page) => page.get_title().await,
                Err(_) => None,
            };

            let browser_type = browser_manager.get_browser_type().await.to_string();
            let (viewport_width, viewport_height) = browser_manager.get_viewport_size().await;
            let cursor_position = browser_manager.get_cursor_position().await.ok();

            let mut metadata = HashMap::new();
            metadata.insert("browser_type".to_string(), browser_type.clone());
            if let Some((x, y)) = cursor_position {
                metadata.insert("cursor_position".to_string(), format!("{:.0},{:.0}", x, y));
            }

            let viewport = if viewport_width > 0 && viewport_height > 0 {
                Some(ViewportDimensions {
                    width: viewport_width as u32,
                    height: viewport_height as u32,
                })
            } else {
                None
            };

            let mut screenshot_path = None;

            match capture_browser_screenshot(sess).await {
                Ok((path, _)) => {
                    add_pending_screenshot(sess, path.clone(), url.clone());
                    let current_hash = crate::image_comparison::compute_image_hash(&path).ok();
                    let mut last_info = sess.last_screenshot_info.lock().unwrap();
                    let include_screenshot = should_include_browser_screenshot(
                        &mut last_info,
                        &path,
                        current_hash,
                    );
                    drop(last_info);
                    if include_screenshot {
                        screenshot_path = Some(path);
                    }
                }
                Err(err_msg) => {
                    trace!("env_ctx_v2: screenshot capture failed: {}", err_msg);
                }
            }

                if let Some(path) = screenshot_path {
                    let captured_at = OffsetDateTime::now_utc()
                        .format(&Rfc3339)
                        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());

                    let mut snapshot = BrowserSnapshot::new(url.clone(), captured_at);
                snapshot.title = title.clone();
                snapshot.viewport = viewport;
                if !metadata.is_empty() {
                    snapshot.metadata = Some(metadata);
                }

                match snapshot.to_response_item_with_id(Some(&browser_stream_id)) {
                    Ok(item) => items.push(item),
                    Err(err) => warn!("env_ctx_v2: failed to serialize browser_snapshot JSON: {err}"),
                }

                if *crate::flags::CTX_UI {
                    sess.emit_browser_snapshot_event(&browser_stream_id, &snapshot);
                }

                match std::fs::read(&path) {
                    Ok(bytes) => {
                        let mime = mime_guess::from_path(&path)
                            .first()
                            .map(|m| m.to_string())
                            .unwrap_or_else(|| "image/png".to_string());
                        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
                        items.push(ResponseItem::Message {
                            id: Some(browser_stream_id),
                            role: "user".to_string(),
                            content: vec![ContentItem::InputImage {
                                image_url: format!("data:{mime};base64,{encoded}"),
                            }],
                        });
                    }
                    Err(err) => warn!(
                        "env_ctx_v2: failed to read screenshot file {}: {err}",
                        path.display()
                    ),
                }
                }
            }
        }
    }

    items
}

fn should_include_browser_screenshot(
    last_info: &mut Option<(PathBuf, Vec<u8>, Vec<u8>)>,
    path: &PathBuf,
    current_hash: Option<(Vec<u8>, Vec<u8>)>,
) -> bool {
    if let Some((cur_phash, cur_dhash)) = current_hash {
        if let Some((_, last_phash, last_dhash)) = last_info.as_ref() {
            if crate::image_comparison::are_hashes_similar(
                last_phash,
                last_dhash,
                &cur_phash,
                &cur_dhash,
            ) {
                return false;
            }
        }
        *last_info = Some((path.clone(), cur_phash, cur_dhash));
        true
    } else {
        *last_info = Some((path.clone(), Vec::new(), Vec::new()));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codex::streaming::{process_rollout_env_item, TimelineReplayContext};
    use code_protocol::models::ContentItem;
    use pretty_assertions::assert_eq;

    #[test]
    fn screenshot_dedup_tracks_changes() {
        let mut last = None;
        let path = PathBuf::from("/tmp/a.png");
        let hash_one = (vec![0xAAu8; 32], vec![0x55u8; 32]);
        let hash_two = (vec![0xABu8; 32], vec![0x56u8; 32]);

        assert!(should_include_browser_screenshot(&mut last, &path, Some(hash_one.clone())));
        assert!(!should_include_browser_screenshot(&mut last, &path, Some(hash_one.clone())));
        assert!(should_include_browser_screenshot(&mut last, &path, Some(hash_two)));
    }

    fn make_snapshot(cwd: &str) -> EnvironmentContextSnapshot {
        EnvironmentContextSnapshot {
            version: EnvironmentContextSnapshot::VERSION,
            cwd: Some(cwd.to_string()),
            approval_policy: None,
            sandbox_mode: None,
            network_access: None,
            writable_roots: Vec::new(),
            operating_system: None,
            common_tools: Vec::new(),
            shell: None,
            git_branch: Some("main".to_string()),
            reasoning_effort: None,
        }
    }

    #[test]
    fn timeline_rehydrate_round_trip() {
        let baseline = make_snapshot("/repo");
        let delta_snapshot = make_snapshot("/repo-updated");
        let delta = delta_snapshot.diff_from(&baseline);

        let baseline_item = baseline
            .to_response_item()
            .expect("serialize baseline snapshot");
        let delta_item = delta
            .to_response_item()
            .expect("serialize delta snapshot");

        let mut ctx = TimelineReplayContext::default();
        process_rollout_env_item(&mut ctx, &baseline_item);
        process_rollout_env_item(&mut ctx, &delta_item);

        assert!(ctx.timeline.baseline().is_some());
        assert_eq!(ctx.timeline.delta_count(), 1);
        assert_eq!(ctx.next_sequence, 2);
        assert!(ctx.last_snapshot.is_some());
    }

    #[test]
    fn timeline_rehydrate_legacy_baseline() {
        let legacy_item = ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: "== System Status ==\n cwd: /legacy\n branch: main".to_string(),
            }],
        };

        let mut ctx = TimelineReplayContext::default();
        process_rollout_env_item(&mut ctx, &legacy_item);

        assert!(ctx.timeline.is_empty());
        assert!(ctx.legacy_baseline.is_some());
    }

    #[test]
    fn timeline_rehydrate_delta_gap_triggers_reset() {
        let baseline = make_snapshot("/repo");
        let baseline_item = baseline
            .to_response_item()
            .expect("serialize baseline snapshot");

        let mut ctx = TimelineReplayContext::default();
        process_rollout_env_item(&mut ctx, &baseline_item);

        let mut delta = make_snapshot("/other").diff_from(&baseline);
        delta.base_fingerprint = "mismatch".to_string();
        let delta_item = delta
            .to_response_item()
            .expect("serialize delta snapshot");

        process_rollout_env_item(&mut ctx, &delta_item);

        assert!(ctx.timeline.is_empty());
        assert!(ctx.last_snapshot.is_none());
        assert_eq!(ctx.next_sequence, 1);
    }
}
use crate::agent_tool::AGENT_MANAGER;
use crate::agent_tool::AgentStatus;
use crate::agent_tool::AgentToolRequest;
use crate::agent_defaults::model_guide_markdown_with_custom;
use crate::agent_tool::CancelAgentParams;
use crate::agent_tool::CheckAgentStatusParams;
use crate::agent_tool::GetAgentResultParams;
use crate::agent_tool::ListAgentsParams;
use crate::agent_tool::normalize_agent_name;
use crate::agent_tool::RunAgentParams;
use crate::agent_tool::WaitForAgentParams;
use crate::apply_patch::convert_apply_patch_to_protocol;
use crate::apply_patch::get_writable_roots;
use crate::apply_patch::{self, ApplyPatchResult};
use crate::bridge_client::{
    get_effective_subscription, persist_workspace_subscription, send_bridge_control,
    set_session_subscription, set_workspace_subscription,
};
use crate::client::ModelClient;
use crate::client_common::{Prompt, ResponseEvent, TextFormat, REVIEW_PROMPT};
use crate::context_timeline::ContextTimeline;
use crate::environment_context::{
    BrowserSnapshot,
    EnvironmentContext,
    EnvironmentContextDelta,
    EnvironmentContextSnapshot,
    EnvironmentContextTracker,
    ViewportDimensions,
};
use crate::user_instructions::UserInstructions;
use crate::config::{persist_model_selection, Config};
use crate::timeboxed_exec_guidance::{
    AUTO_EXEC_TIMEBOXED_CLI_GUIDANCE,
    AUTO_EXEC_TIMEBOXED_REVIEW_GUIDANCE,
};
use crate::config_types::ProjectHookEvent;
use crate::config_types::ShellEnvironmentPolicy;
use crate::conversation_history::ConversationHistory;
use crate::error::{CodexErr, RetryAfter};
use crate::error::Result as CodexResult;
use crate::error::SandboxErr;
use crate::error::get_error_message_ui;
use crate::exec::ExecParams;
use crate::exec::ExecToolCallOutput;
use crate::exec::SandboxType;
use crate::exec::StdoutStream;
use crate::exec::StreamOutput;
use crate::exec::EXEC_CAPTURE_MAX_BYTES;
use crate::exec::process_exec_tool_call;
use crate::review_format::format_review_findings_block;
use crate::exec_env::create_env;
use crate::mcp_connection_manager::McpConnectionManager;
use crate::mcp_tool_call::handle_mcp_tool_call;
use crate::model_family::{derive_default_model_family, find_family_for_model};
use code_protocol::models::ContentItem;
use code_protocol::models::FunctionCallOutputPayload;
use code_protocol::models::LocalShellAction;
use code_protocol::models::ReasoningItemContent;
use code_protocol::models::ReasoningItemReasoningSummary;
use code_protocol::models::ResponseInputItem;
use code_protocol::models::ResponseItem;
use code_protocol::models::ShellToolCallParams;
use code_protocol::models::SandboxPermissions;
use crate::openai_tools::ToolsConfig;
use crate::openai_tools::get_openai_tools;
use crate::slash_commands::get_enabled_agents;
use crate::dry_run_guard::{analyze_command, DryRunAnalysis, DryRunDisposition, DryRunGuardState};
use crate::parse_command::parse_command;
use crate::plan_tool::handle_update_plan;
use crate::project_doc::get_user_instructions;
use crate::skills::loader::load_skills;
use crate::project_features::{ProjectCommand, ProjectHook, ProjectHooks};
use crate::protocol::AgentMessageDeltaEvent;
use crate::protocol::AgentMessageEvent;
use crate::protocol::AgentReasoningDeltaEvent;
use crate::protocol::AgentReasoningEvent;
use crate::protocol::AgentSourceKind;
use crate::protocol::AgentReasoningRawContentDeltaEvent;
use crate::protocol::AgentReasoningRawContentEvent;
use crate::protocol::AgentReasoningSectionBreakEvent;
use crate::protocol::AgentStatusUpdateEvent;
use crate::protocol::ApplyPatchApprovalRequestEvent;
use crate::protocol::AskForApproval;
use crate::protocol::BackgroundEventEvent;
use crate::protocol::BrowserScreenshotUpdateEvent;
use crate::protocol::ErrorEvent;
use crate::protocol::Event;
use crate::protocol::EventMsg;
use crate::protocol::ExitedReviewModeEvent;
use crate::protocol::ReviewSnapshotInfo;
use crate::protocol::ListCustomPromptsResponseEvent;
use crate::protocol::ListSkillsResponseEvent;
use crate::protocol::{BrowserSnapshotEvent, EnvironmentContextDeltaEvent, EnvironmentContextFullEvent};
use crate::protocol::ExecApprovalRequestEvent;
use crate::protocol::ExecCommandBeginEvent;
use crate::protocol::ExecCommandEndEvent;
use crate::protocol::FileChange;
use crate::protocol::InputItem;
use crate::protocol::Op;
use crate::protocol::PatchApplyBeginEvent;
use crate::protocol::PatchApplyEndEvent;
use crate::protocol::RateLimitSnapshotEvent;
use crate::protocol::TokenCountEvent;
use crate::protocol::TokenUsage;
use crate::protocol::TokenUsageInfo;
use crate::protocol::ReviewDecision;
use crate::protocol::ValidationGroup;
use crate::protocol::ReviewOutputEvent;
use crate::protocol::ReviewRequest;
use crate::protocol::SandboxPolicy;
use crate::protocol::SessionConfiguredEvent;
use crate::protocol::Submission;
use crate::protocol::TaskCompleteEvent;
use std::sync::OnceLock;
use tokio::sync::Notify;
use crate::protocol::TurnDiffEvent;
use crate::rollout::RolloutRecorder;
use crate::safety::SafetyCheck;
use crate::safety::assess_command_safety;
use crate::safety::assess_safety_for_untrusted_command;
use crate::shell;
use crate::turn_diff_tracker::TurnDiffTracker;
use crate::user_notification::UserNotification;
use crate::util::{backoff, wait_for_connectivity};
use code_protocol::protocol::SessionSource;
use crate::rollout::recorder::SessionStateSnapshot;
use serde_json::Value;
use crate::exec_command::ExecSessionManager;

/// The high-level interface to the Codex system.
/// It operates as a queue pair where you send submissions and receive events.
pub struct Codex {
    next_id: AtomicU64,
    tx_sub: Sender<Submission>,
    rx_event: Receiver<Event>,
}

// Allow internal components (like background exec completions) to trigger a new
// turn without fabricating a visible user message. We enqueue an empty
// UserInput; the model will only see queued developer/system items.
static TX_SUB_GLOBAL: OnceLock<Sender<Submission>> = OnceLock::new();
static ANY_BG_NOTIFY: OnceLock<std::sync::Arc<Notify>> = OnceLock::new();

/// Wrapper returned by [`Codex::spawn`] containing the spawned [`Codex`],
/// the submission id for the initial `ConfigureSession` request and the
/// unique session id.
pub struct CodexSpawnOk {
    pub codex: Codex,
    pub init_id: String,
    pub session_id: Uuid,
}

impl Codex {
    /// Spawn a new [`Codex`] and initialize the session.
    pub async fn spawn(config: Config, auth: Option<CodexAuth>) -> CodexResult<CodexSpawnOk> {
        let auth_manager = auth.map(crate::AuthManager::from_auth_for_testing);
        Self::spawn_with_auth_manager(config, auth_manager).await
    }

    pub async fn spawn_with_auth_manager(
        config: Config,
        auth_manager: Option<Arc<AuthManager>>,
    ) -> CodexResult<CodexSpawnOk> {
        // experimental resume path (undocumented)
        let resume_path = config.experimental_resume.clone();
        info!("resume_path: {resume_path:?}");
        // Use an unbounded submission queue to avoid any possibility of back‑pressure
        // between the TUI submit worker and the core loop during interrupts/cancels.
        let (tx_sub, rx_sub) = async_channel::unbounded();
        let (tx_event, rx_event) = async_channel::unbounded();

        let skills_outcome = config.skills_enabled.then(|| load_skills(&config));
        if let Some(outcome) = &skills_outcome {
            for err in &outcome.errors {
                warn!("invalid skill {}: {}", err.path.display(), err.message);
            }
        }

        let user_instructions = get_user_instructions(
            &config,
            skills_outcome.as_ref().map(|outcome| outcome.skills.as_slice()),
        )
        .await;

        let configure_session = Op::ConfigureSession {
            provider: config.model_provider.clone(),
            model: config.model.clone(),
            model_explicit: config.model_explicit,
            model_reasoning_effort: config.model_reasoning_effort,
            preferred_model_reasoning_effort: config.preferred_model_reasoning_effort,
            model_reasoning_summary: config.model_reasoning_summary,
            model_text_verbosity: config.model_text_verbosity,
            user_instructions,
            base_instructions: config.base_instructions.clone(),
            approval_policy: config.approval_policy,
            sandbox_policy: config.sandbox_policy.clone(),
            disable_response_storage: config.disable_response_storage,
            notify: config.notify.clone(),
            cwd: config.cwd.clone(),
            resume_path: resume_path.clone(),
            demo_developer_message: config.demo_developer_message.clone(),
        };

        let config = Arc::new(config);

        // Generate a unique ID for the lifetime of this Codex session.
        let session_id = Uuid::new_v4();

        // This task will run until Op::Shutdown is received.
        tokio::spawn(submission_loop(
            session_id,
            config,
            auth_manager,
            rx_sub,
            tx_event,
        ));
        let codex = Codex {
            next_id: AtomicU64::new(0),
            tx_sub,
            rx_event,
        };
        // Make a clone of tx_sub available for internal auto-turn triggers.
        let _ = TX_SUB_GLOBAL.set(codex.tx_sub.clone());
        let _ = ANY_BG_NOTIFY.set(std::sync::Arc::new(Notify::new()));
        let init_id = codex.submit(configure_session).await?;

        Ok(CodexSpawnOk {
            codex,
            init_id,
            session_id,
        })
    }

    /// Submit the `op` wrapped in a `Submission` with a unique ID.
    pub async fn submit(&self, op: Op) -> CodexResult<String> {
        let id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            .to_string();
        let sub = Submission { id: id.clone(), op };
        self.submit_with_id(sub).await?;
        Ok(id)
    }

    /// Use sparingly: prefer `submit()` so Codex is responsible for generating
    /// unique IDs for each submission.
    pub async fn submit_with_id(&self, sub: Submission) -> CodexResult<()> {
        self.tx_sub
            .send(sub)
            .await
            .map_err(|_| CodexErr::InternalAgentDied)?;
        Ok(())
    }

    pub async fn next_event(&self) -> CodexResult<Event> {
        let event = self
            .rx_event
            .recv()
            .await
            .map_err(|_| CodexErr::InternalAgentDied)?;
        Ok(event)
    }
}
