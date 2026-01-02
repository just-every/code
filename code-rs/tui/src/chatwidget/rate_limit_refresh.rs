use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use code_core::auth::auth_for_stored_account;
use code_core::auth_accounts::{self, StoredAccount};
use code_core::{AuthManager, ModelClient, Prompt, ResponseEvent};
use code_core::account_usage;
use code_core::config::Config;
use code_core::config_types::ReasoningEffort;
use code_core::debug_logger::DebugLogger;
use code_core::protocol::{Event, EventMsg, RateLimitSnapshotEvent, TokenCountEvent};
use code_protocol::models::{ContentItem, ResponseItem};
use chrono::Utc;
use futures::StreamExt;
use tokio::runtime::Runtime;
use uuid::Uuid;

#[cfg(feature = "code-fork")]
use crate::tui_event_extensions::handle_rate_limit;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::thread_spawner;

/// Fire-and-forget helper that refreshes rate limit data using a dedicated model
/// request. Results are funneled back into the main TUI loop via `AppEvent` so
/// history ordering stays consistent.
pub(super) fn start_rate_limit_refresh(
    app_event_tx: AppEventSender,
    config: Config,
    debug_enabled: bool,
) {
    start_rate_limit_refresh_with_options(
        app_event_tx,
        config,
        debug_enabled,
        None,
        true,
        true,
    );
}

pub(super) fn start_rate_limit_refresh_for_account(
    app_event_tx: AppEventSender,
    config: Config,
    debug_enabled: bool,
    account: StoredAccount,
    emit_ui: bool,
    notify_on_failure: bool,
) {
    start_rate_limit_refresh_with_options(
        app_event_tx,
        config,
        debug_enabled,
        Some(account),
        emit_ui,
        notify_on_failure,
    );
}

fn start_rate_limit_refresh_with_options(
    app_event_tx: AppEventSender,
    config: Config,
    debug_enabled: bool,
    account: Option<StoredAccount>,
    emit_ui: bool,
    notify_on_failure: bool,
) {
    let fallback_tx = app_event_tx.clone();
    if thread_spawner::spawn_lightweight("rate-refresh", move || {
        if let Err(err) = run_refresh(
            app_event_tx.clone(),
            config,
            debug_enabled,
            account,
            emit_ui,
        ) {
            if notify_on_failure {
                let message = format!("Failed to refresh rate limits: {err}");
                app_event_tx.send(AppEvent::RateLimitFetchFailed { message });
            } else {
                tracing::warn!("Failed to refresh rate limits: {err}");
            }
        }
    })
    .is_none()
    {
        if notify_on_failure {
            let message =
                "Failed to refresh rate limits: background worker unavailable".to_string();
            fallback_tx.send(AppEvent::RateLimitFetchFailed { message });
        } else {
            tracing::warn!("Failed to refresh rate limits: background worker unavailable");
        }
    }
}

fn run_refresh(
    app_event_tx: AppEventSender,
    config: Config,
    debug_enabled: bool,
    account: Option<StoredAccount>,
    emit_ui: bool,
) -> Result<()> {
    let runtime = build_runtime()?;
    runtime.block_on(async move {
        let (auth_mgr, stored_account) = match account {
            Some(account) => {
                let auth = auth_for_stored_account(
                    &config.code_home,
                    &account,
                    &config.responses_originator_header,
                )
                .await
                .context("building auth for stored account")?;
                (
                    AuthManager::from_auth(
                        auth,
                        config.code_home.clone(),
                        config.responses_originator_header.clone(),
                    ),
                    Some(account),
                )
            }
            None => {
                let auth_mode = if config.using_chatgpt_auth {
                    code_protocol::mcp_protocol::AuthMode::ChatGPT
                } else {
                    code_protocol::mcp_protocol::AuthMode::ApiKey
                };
                (
                    AuthManager::shared_with_mode_and_originator(
                        config.code_home.clone(),
                        auth_mode,
                        config.responses_originator_header.clone(),
                    ),
                    None,
                )
            }
        };

        let client = build_model_client(&config, auth_mgr, debug_enabled)?;

        let mut prompt = Prompt::default();
        prompt.store = false;
        prompt.user_instructions = config.user_instructions.clone();
        prompt.base_instructions_override = config.base_instructions.clone();
        prompt.input.push(ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: "Yield immediately with only the message \"ok\"".to_string(),
            }],
        });
        prompt.set_log_tag("tui/rate_limit_refresh");

        let mut stream = client
            .stream(&prompt)
            .await
            .context("requesting rate limit snapshot")?;

        let mut snapshot = None;
        while let Some(event) = stream.next().await {
            match event? {
                ResponseEvent::RateLimits(s) => {
                    snapshot = Some(s);
                    break;
                }
                ResponseEvent::Completed { .. } => break,
                _ => {}
            }
        }

        let proto_snapshot = snapshot.context("rate limit snapshot missing from response")?;

        let snapshot: RateLimitSnapshotEvent = proto_snapshot.clone();

        let (record_account_id, record_plan) = if let Some(account) = &stored_account {
            (
                Some(account.id.clone()),
                account
                    .tokens
                    .as_ref()
                    .and_then(|tokens| tokens.id_token.get_chatgpt_plan_type()),
            )
        } else {
            let active_id =
                auth_accounts::get_active_account_id(&config.code_home).ok().flatten();
            let account = active_id
                .as_deref()
                .and_then(|id| auth_accounts::find_account(&config.code_home, id).ok())
                .flatten();
            (
                active_id,
                account
                    .as_ref()
                    .and_then(|acc| acc.tokens.as_ref())
                    .and_then(|tokens| tokens.id_token.get_chatgpt_plan_type()),
            )
        };

        if let Some(account_id) = record_account_id.as_deref() {
            if let Err(err) = account_usage::record_rate_limit_snapshot(
                &config.code_home,
                account_id,
                record_plan.as_deref(),
                &snapshot,
                Utc::now(),
            ) {
                tracing::warn!("Failed to persist rate limit snapshot: {err}");
            }
        }

        #[cfg(feature = "code-fork")]
        handle_rate_limit(&snapshot, &app_event_tx);

        if emit_ui {
            let event = Event {
                id: "rate-limit-refresh".to_string(),
                event_seq: 0,
                msg: EventMsg::TokenCount(TokenCountEvent {
                    info: None,
                    rate_limits: Some(snapshot),
                }),
                order: None,
            };

            app_event_tx.send(AppEvent::CodexEvent(event));
        } else if let Some(account_id) = record_account_id {
            app_event_tx.send(AppEvent::RateLimitSnapshotStored { account_id });
        }
        Ok(())
    })
}

fn build_runtime() -> Result<Runtime> {
    Ok(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .context("building rate limit refresh runtime")?,
    )
}

fn build_model_client(
    config: &Config,
    auth_mgr: Arc<AuthManager>,
    debug_enabled: bool,
) -> Result<ModelClient> {
    let debug_logger = DebugLogger::new(debug_enabled)
        .or_else(|_| DebugLogger::new(false))
        .context("initializing debug logger")?;

    let client = ModelClient::new(
        Arc::new(config.clone()),
        Some(auth_mgr),
        None,
        config.model_provider.clone(),
        ReasoningEffort::Low,
        config.model_reasoning_summary,
        config.model_text_verbosity,
        Uuid::new_v4(),
        Arc::new(Mutex::new(debug_logger)),
    );

    Ok(client)
}
