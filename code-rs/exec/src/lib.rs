mod cli;
mod event_processor;
mod event_processor_with_human_output;
mod event_processor_with_json_output;
mod slash;

pub use cli::Cli;
use code_auto_drive_core::start_auto_coordinator;
use code_auto_drive_core::AutoCoordinatorCommand;
use code_auto_drive_core::AutoCoordinatorEvent;
use code_auto_drive_core::AutoCoordinatorEventSender;
use code_auto_drive_core::AutoCoordinatorStatus;
use code_auto_drive_core::AutoDriveHistory;
use code_auto_drive_core::AutoTurnAgentsAction;
use code_auto_drive_core::AutoTurnAgentsTiming;
use code_auto_drive_core::AutoTurnCliAction;
use code_auto_drive_core::MODEL_SLUG;
use code_core::AuthManager;
use code_core::BUILT_IN_OSS_MODEL_PROVIDER_ID;
use code_core::ConversationManager;
use code_core::NewConversation;
use code_core::CodexConversation;
use code_core::config::set_default_originator;
use code_core::config::Config;
use code_core::config::ConfigOverrides;
use code_core::config_types::AutoDriveContinueMode;
use code_core::model_family::{derive_default_model_family, find_family_for_model};
use code_core::git_info::get_git_repo_root;
use code_core::git_info::recent_commits;
use code_core::review_coord::{
    bump_snapshot_epoch_for,
    clear_stale_lock_if_dead,
    current_snapshot_epoch_for,
    try_acquire_lock,
};
use code_core::protocol::AskForApproval;
use code_core::protocol::AgentSourceKind;
use code_core::protocol::AgentStatusUpdateEvent;
use code_core::protocol::Event;
use code_core::protocol::EventMsg;
use code_core::protocol::InputItem;
use code_core::protocol::Op;
use code_core::protocol::ReviewOutputEvent;
use code_core::protocol::ReviewRequest;
use code_core::protocol::TaskCompleteEvent;
use code_core::protocol::ReviewContextMetadata;
use code_protocol::models::ContentItem;
use code_protocol::models::ResponseItem;
use code_protocol::protocol::SessionSource;
use code_ollama::DEFAULT_OSS_MODEL;
use code_protocol::config_types::SandboxMode;
use event_processor_with_human_output::EventProcessorWithHumanOutput;
use event_processor_with_json_output::EventProcessorWithJsonOutput;
use event_processor::handle_last_message;
use code_git_tooling::GhostCommit;
use code_git_tooling::CreateGhostCommitOptions;
use code_git_tooling::create_ghost_commit;
use serde_json::Value;
use std::collections::HashSet;
use std::io::IsTerminal;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use supports_color::Stream;
use tokio::time::{Duration, Instant};
use tracing::debug;
use tracing::error;
use tracing::info;
use tracing_subscriber::EnvFilter;

use anyhow::Context;
use crate::cli::Command as ExecCommand;
use crate::event_processor::CodexStatus;
use crate::event_processor::EventProcessor;
use crate::slash::{process_exec_slash_command, SlashContext, SlashDispatch};
use code_auto_drive_core::AUTO_RESOLVE_REVIEW_FOLLOWUP;
use code_auto_drive_core::AutoResolvePhase;
use code_auto_drive_core::AutoResolveState;
use code_core::{entry_to_rollout_path, AutoDriveMode, AutoDrivePidFile, SessionCatalog, SessionQuery};
use code_core::protocol::SandboxPolicy;

fn build_auto_drive_exec_config(config: &Config) -> Config {
	    let mut auto_config = config.clone();
	    auto_config.model = config.auto_drive.model.trim().to_string();
	    if auto_config.model.is_empty() {
	        auto_config.model = MODEL_SLUG.to_string();
	    }
	    auto_config.model_reasoning_effort = config.auto_drive.model_reasoning_effort;
	    auto_config
}
use code_core::git_info::current_branch_name;
use code_core::timeboxed_exec_guidance::{
    AUTO_EXEC_TIMEBOXED_CLI_GUIDANCE,
    AUTO_EXEC_TIMEBOXED_GOAL_SUFFIX,
};

/// How long exec waits after task completion before sending Shutdown when Auto Review
/// may be about to start. Guarded so sub-agents are not delayed.
const AUTO_REVIEW_SHUTDOWN_GRACE_MS: u64 = 1_500;

pub async fn run_main(cli: Cli, code_linux_sandbox_exe: Option<PathBuf>) -> anyhow::Result<()> {
    if let Err(err) = set_default_originator("code_exec") {
        tracing::warn!(?err, "Failed to set codex exec originator override {err:?}");
    }

    let Cli {
        command,
        images,
        model: model_cli_arg,
        oss,
        config_profile,
        full_auto,
        dangerously_bypass_approvals_and_sandbox,
        cwd,
        skip_git_repo_check,
        color,
        last_message_file,
        json: json_mode,
        sandbox_mode: sandbox_mode_cli_arg,
        prompt,
        output_schema: output_schema_path,
        include_plan_tool,
        config_overrides,
        auto_drive,
        auto_review,
        max_seconds,
        turn_cap,
        review_output_json,
        ..
    } = cli;

    let run_deadline = max_seconds.map(|seconds| Instant::now() + Duration::from_secs(seconds));
    let run_deadline_std = run_deadline.map(|deadline| deadline.into_std());

    // Determine the prompt source (parent or subcommand) and read from stdin if needed.
    let prompt_arg = match &command {
        // Allow prompt before the subcommand by falling back to the parent-level prompt
        // when the Resume subcommand did not provide its own prompt.
        Some(ExecCommand::Resume(args)) => args.prompt.clone().or(prompt),
        None => prompt,
    };

    let prompt = match prompt_arg {
        Some(p) if p != "-" => p,
        // Either `-` was passed or no positional arg.
        maybe_dash => {
            // When no arg (None) **and** stdin is a TTY, bail out early – unless the
            // user explicitly forced reading via `-`.
            let force_stdin = matches!(maybe_dash.as_deref(), Some("-"));

            if std::io::stdin().is_terminal() && !force_stdin {
                eprintln!(
                    "No prompt provided. Either specify one as an argument or pipe the prompt into stdin."
                );
                std::process::exit(1);
            }

            // Ensure the user knows we are waiting on stdin, as they may
            // have gotten into this state by mistake. If so, and they are not
            // writing to stdin, Codex will hang indefinitely, so this should
            // help them debug in that case.
            if !force_stdin {
                eprintln!("Reading prompt from stdin...");
            }
            let mut buffer = String::new();
            if let Err(e) = std::io::stdin().read_to_string(&mut buffer) {
                eprintln!("Failed to read prompt from stdin: {e}");
                std::process::exit(1);
            } else if buffer.trim().is_empty() {
                eprintln!("No prompt provided via stdin.");
                std::process::exit(1);
            }
            buffer
        }
    };

    let mut auto_drive_goal: Option<String> = None;
    let trimmed_prompt = prompt.trim();
    if trimmed_prompt.starts_with("/auto") {
        auto_drive_goal = Some(trimmed_prompt.trim_start_matches("/auto").trim().to_string());
    }
    if auto_drive {
        if trimmed_prompt.is_empty() {
            eprintln!("Auto Drive requires a goal. Provide one after --auto or prefix the prompt with /auto.");
            std::process::exit(1);
        }
        if auto_drive_goal
            .as_ref()
            .is_some_and(|goal| goal.is_empty())
        {
            auto_drive_goal = Some(trimmed_prompt.to_string());
        } else if auto_drive_goal.is_none() {
            auto_drive_goal = Some(trimmed_prompt.to_string());
        }
    }

    if auto_drive_goal
        .as_ref()
        .is_some_and(|g| g.trim().is_empty())
    {
        eprintln!("Auto Drive requires a goal. Provide one after /auto or --auto.");
        std::process::exit(1);
    }

    let timeboxed_auto_exec = auto_drive_goal.is_some() && max_seconds.is_some();
    if timeboxed_auto_exec {
        if let Some(goal) = auto_drive_goal.as_mut() {
            *goal = append_timeboxed_auto_drive_goal(goal);
        }
    }

    let mut prompt_to_send = prompt.clone();
    let mut summary_prompt = if let Some(goal) = auto_drive_goal.as_ref() {
        format!("/auto {goal}")
    } else {
        prompt.clone()
    };
    let mut review_request: Option<ReviewRequest> = None;

    let _output_schema = load_output_schema(output_schema_path);

    let (stdout_with_ansi, stderr_with_ansi) = match color {
        cli::Color::Always => (true, true),
        cli::Color::Never => (false, false),
        cli::Color::Auto => (
            supports_color::on_cached(Stream::Stdout).is_some(),
            supports_color::on_cached(Stream::Stderr).is_some(),
        ),
    };

    // Build fmt layer (existing logging) to compose with OTEL layer.
    let default_level = "error";

    // Build env_filter separately and attach via with_filter.
    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(default_level))
        .unwrap_or_else(|_| EnvFilter::new(default_level));

    let _ = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_ansi(stderr_with_ansi)
        .with_writer(|| std::io::stderr())
        .try_init();

    let sandbox_mode = if full_auto {
        Some(SandboxMode::WorkspaceWrite)
    } else if dangerously_bypass_approvals_and_sandbox {
        Some(SandboxMode::DangerFullAccess)
    } else {
        sandbox_mode_cli_arg.map(Into::<SandboxMode>::into)
    };

    // When using `--oss`, let the bootstrapper pick the model (defaulting to
    // gpt-oss:20b) and ensure it is present locally. Also, force the built‑in
    // `oss` model provider.
    let model = if let Some(model) = model_cli_arg {
        Some(model)
    } else if oss {
        Some(DEFAULT_OSS_MODEL.to_owned())
    } else {
        None // No model specified, will use the default.
    };

    let model_provider = if oss {
        Some(BUILT_IN_OSS_MODEL_PROVIDER_ID.to_string())
    } else {
        None // No specific model provider override.
    };

    // Load configuration and determine approval policy
    let overrides = ConfigOverrides {
        model,
        review_model: None,
        config_profile,
        // This CLI is intended to be headless and has no affordances for asking
        // the user for approval.
        approval_policy: Some(AskForApproval::Never),
        sandbox_mode,
        cwd: cwd.map(|p| p.canonicalize().unwrap_or(p)),
        model_provider,
        code_linux_sandbox_exe,
        base_instructions: None,
        include_plan_tool: Some(include_plan_tool),
        include_apply_patch_tool: None,
        include_view_image_tool: None,
        disable_response_storage: None,
        debug: None,
        show_raw_agent_reasoning: oss.then_some(true),
        tools_web_search_request: None,
        mcp_servers: None,
        experimental_client_tools: None,
        compact_prompt_override: None,
        compact_prompt_override_file: None,
    };
    // Parse `-c` overrides.
    let cli_kv_overrides = match config_overrides.parse_overrides() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error parsing -c overrides: {e}");
            std::process::exit(1);
        }
    };

    let mut config = Config::load_with_cli_overrides(cli_kv_overrides, overrides)?;
    config.max_run_seconds = max_seconds;
    config.max_run_deadline = run_deadline_std;
    config.demo_developer_message = cli.demo_developer_message.clone();
    config.timeboxed_exec_mode = timeboxed_auto_exec;
    if timeboxed_auto_exec {
        config.demo_developer_message = merge_developer_message(
            config.demo_developer_message.take(),
            AUTO_EXEC_TIMEBOXED_CLI_GUIDANCE,
        );
    }
    if auto_drive_goal.is_some() {
        // Exec is non-interactive; don't burn time on countdown delays between Auto Drive turns.
        config.auto_drive.continue_mode = AutoDriveContinueMode::Immediate;
        if let Some(turn_cap) = turn_cap {
            config.auto_drive.coordinator_turn_cap = turn_cap;
        }
    }
    let slash_context = SlashContext {
        agents: &config.agents,
        subagent_commands: &config.subagent_commands,
    };

    match process_exec_slash_command(prompt_to_send.trim(), slash_context) {
        Ok(SlashDispatch::NotSlash) => {}
        Ok(SlashDispatch::ExpandedPrompt { prompt, summary }) => {
            prompt_to_send = prompt;
            if auto_drive_goal.is_none() {
                summary_prompt = summary;
            }
        }
        Ok(SlashDispatch::Review { request, summary }) => {
            review_request = Some(request);
            if auto_drive_goal.is_none() {
                summary_prompt = summary;
            }
        }
        Err(msg) => {
            eprintln!("{msg}");
            std::process::exit(1);
        }
    }

    if auto_review {
        if let Some(req) = review_request.as_mut() {
            let mut metadata = req.metadata.clone().unwrap_or_default();
            metadata.auto_review = Some(true);
            req.metadata = Some(metadata);
        }
    }

    let is_auto_review = review_request
        .as_ref()
        .and_then(|req| req.metadata.as_ref())
        .and_then(|meta| meta.auto_review)
        .unwrap_or(auto_review);

    if is_auto_review {
        if config.auto_review_use_chat_model {
            config.review_model = config.model.clone();
            config.review_model_reasoning_effort = config.model_reasoning_effort;
        } else {
            config.review_model = config.auto_review_model.clone();
            config.review_model_reasoning_effort = config.auto_review_model_reasoning_effort;
        }
        config.review_use_chat_model = config.auto_review_use_chat_model;

        if config.auto_review_resolve_use_chat_model {
            config.review_resolve_model = config.model.clone();
            config.review_resolve_model_reasoning_effort = config.model_reasoning_effort;
        } else {
            config.review_resolve_model = config.auto_review_resolve_model.clone();
            config.review_resolve_model_reasoning_effort =
                config.auto_review_resolve_model_reasoning_effort;
        }
        config.review_resolve_use_chat_model = config.auto_review_resolve_use_chat_model;
    }

    let review_auto_resolve_requested = review_request.is_some()
        && if is_auto_review {
            config.tui.auto_review_enabled
        } else {
            config.tui.review_auto_resolve
        };
    if review_auto_resolve_requested && matches!(config.sandbox_policy, SandboxPolicy::ReadOnly) {
        config.sandbox_policy = SandboxPolicy::new_workspace_write_policy();
        eprintln!(
            "Auto-resolve enabled for /review; upgrading sandbox to workspace-write so fixes can be applied."
        );
    }

    let mut review_outputs: Vec<code_core::protocol::ReviewOutputEvent> = Vec::new();
    let mut final_review_snapshot: Option<code_core::protocol::ReviewSnapshotInfo> = None;
    let mut review_runs: u32 = 0;
    let mut last_review_epoch: Option<u64> = None;
    let max_auto_resolve_attempts: u32 = if is_auto_review {
        config.auto_drive.auto_review_followup_attempts.get()
    } else {
        config.auto_drive.auto_resolve_review_attempts.get()
    };
    let mut auto_resolve_state: Option<AutoResolveState> = review_request.as_ref().and_then(|req| {
        if review_auto_resolve_requested {
            Some(AutoResolveState::new_with_limit(
                req.prompt.clone(),
                req.user_facing_hint.clone(),
                req.metadata.clone(),
                max_auto_resolve_attempts,
            ))
        } else {
            None
        }
    });
    let mut auto_resolve_fix_guard: Option<code_core::review_coord::ReviewGuard> = None;
    let mut auto_resolve_followup_guard: Option<code_core::review_coord::ReviewGuard> = None;
    // Base snapshot captured at the start of auto-resolve; each review snapshot is parented to this.
    let mut auto_resolve_base_snapshot: Option<GhostCommit> = None;
    let resolve_model_for_auto_resolve = if is_auto_review {
        if config.auto_review_resolve_use_chat_model {
            config.model.clone()
        } else {
            config.auto_review_resolve_model.clone()
        }
    } else if config.review_resolve_use_chat_model {
        config.model.clone()
    } else {
        config.review_resolve_model.clone()
    };
    let resolve_effort_for_auto_resolve = if is_auto_review {
        if config.auto_review_resolve_use_chat_model {
            config.model_reasoning_effort
        } else {
            config.auto_review_resolve_model_reasoning_effort
        }
    } else if config.review_resolve_use_chat_model {
        config.model_reasoning_effort
    } else {
        config.review_resolve_model_reasoning_effort
    };
    if review_auto_resolve_requested
        && (!resolve_model_for_auto_resolve.eq_ignore_ascii_case(&config.model)
            || resolve_effort_for_auto_resolve != config.model_reasoning_effort)
    {
        let resolve_family = find_family_for_model(&resolve_model_for_auto_resolve)
            .unwrap_or_else(|| derive_default_model_family(&resolve_model_for_auto_resolve));
        config.model = resolve_model_for_auto_resolve.clone();
        config.model_family = resolve_family.clone();
        config.model_reasoning_effort = resolve_effort_for_auto_resolve;
        if let Some(cw) = resolve_family.context_window {
            config.model_context_window = Some(cw);
        }
        if let Some(max) = resolve_family.max_output_tokens {
            config.model_max_output_tokens = Some(max);
        }
        config.model_auto_compact_token_limit = resolve_family.auto_compact_token_limit();
    }
    let stop_on_task_complete = auto_drive_goal.is_none() && auto_resolve_state.is_none();
    let mut event_processor: Box<dyn EventProcessor> = if json_mode {
        Box::new(EventProcessorWithJsonOutput::new(last_message_file.clone()))
    } else {
        Box::new(EventProcessorWithHumanOutput::create_with_ansi(
            stdout_with_ansi,
            &config,
            last_message_file.clone(),
            stop_on_task_complete,
        ))
    };

    if oss {
        code_ollama::ensure_oss_ready(&config)
            .await
            .map_err(|e| anyhow::anyhow!("OSS setup failed: {e}"))?;
    }

    // Print the effective configuration and prompt so users can see what Codex
    // is using.
    let default_cwd = config.cwd.to_path_buf();
    let _default_approval_policy = config.approval_policy;
    let _default_sandbox_policy = config.sandbox_policy.clone();
    let _default_model = config.model.clone();
    let _default_effort = config.model_reasoning_effort;
    let _default_summary = config.model_reasoning_summary;

    if !skip_git_repo_check && get_git_repo_root(&default_cwd).is_none() {
        eprintln!("Not inside a trusted directory and --skip-git-repo-check was not specified.");
        std::process::exit(1);
    }

    let auth_manager = AuthManager::shared_with_mode_and_originator(
        config.code_home.clone(),
        code_protocol::mcp_protocol::AuthMode::ApiKey,
        config.responses_originator_header.clone(),
    );
    let conversation_manager = ConversationManager::new(auth_manager.clone(), SessionSource::Exec);

    // Handle resume subcommand by resolving a rollout path and using explicit resume API.
    let NewConversation {
        conversation_id: _,
        conversation,
        session_configured,
    } = if let Some(ExecCommand::Resume(args)) = command {
        let resume_path = resolve_resume_path(&config, &args).await?;

        if let Some(path) = resume_path {
            conversation_manager
                .resume_conversation_from_rollout(config.clone(), path, auth_manager.clone())
                .await?
        } else {
            conversation_manager
                .new_conversation(config.clone())
                .await?
        }
    } else {
        conversation_manager
            .new_conversation(config.clone())
            .await?
    };
    if auto_drive_goal.is_some() {
        let summary_config = build_auto_drive_exec_config(&config);
        event_processor.print_config_summary(&summary_config, &summary_prompt);
    } else {
        event_processor.print_config_summary(&config, &summary_prompt);
    }
    info!("Codex initialized with event: {session_configured:?}");

    if let Some(goal) = auto_drive_goal {
        return run_auto_drive_session(
            goal,
            images,
            config,
            conversation,
            event_processor,
            last_message_file,
            run_deadline,
        )
        .await;
    }

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Event>();
    {
        let conversation = conversation.clone();
        tokio::spawn(async move {
            #[cfg(unix)]
            let mut sigterm_stream = match tokio::signal::unix::signal(
                tokio::signal::unix::SignalKind::terminate(),
            ) {
                Ok(stream) => Some(stream),
                Err(err) => {
                    tracing::warn!("failed to install SIGTERM handler: {err}");
                    None
                }
            };
            #[cfg(unix)]
            let mut sigterm_requested = false;

            loop {
                #[cfg(unix)]
                {
                    if let Some(stream) = sigterm_stream.as_mut() {
                        tokio::select! {
                            _ = stream.recv() => {
                                tracing::debug!("SIGTERM received; requesting shutdown");
                                conversation.submit(Op::Shutdown).await.ok();
                                sigterm_requested = true;
                                break;
                            }
                            _ = tokio::signal::ctrl_c() => {
                                tracing::debug!("Keyboard interrupt");
                                conversation.submit(Op::Interrupt).await.ok();
                                break;
                            }
                            res = conversation.next_event() => match res {
                                Ok(event) => {
                                    debug!("Received event: {event:?}");

                                    let is_shutdown_complete = matches!(event.msg, EventMsg::ShutdownComplete);
                                    if let Err(e) = tx.send(event) {
                                        error!("Error sending event: {e:?}");
                                        break;
                                    }
                                    if is_shutdown_complete {
                                        info!("Received shutdown event, exiting event loop.");
                                        break;
                                    }
                                },
                                Err(e) => {
                                    error!("Error receiving event: {e:?}");
                                    break;
                                }
                            }
                        }
                    } else {
                        tokio::select! {
                            _ = tokio::signal::ctrl_c() => {
                                tracing::debug!("Keyboard interrupt");
                                conversation.submit(Op::Interrupt).await.ok();
                                break;
                            }
                            res = conversation.next_event() => match res {
                                Ok(event) => {
                                    debug!("Received event: {event:?}");

                                    let is_shutdown_complete = matches!(event.msg, EventMsg::ShutdownComplete);
                                    if let Err(e) = tx.send(event) {
                                        error!("Error sending event: {e:?}");
                                        break;
                                    }
                                    if is_shutdown_complete {
                                        info!("Received shutdown event, exiting event loop.");
                                        break;
                                    }
                                },
                                Err(e) => {
                                    error!("Error receiving event: {e:?}");
                                    break;
                                }
                            }
                        }
                    }
                }
                #[cfg(not(unix))]
                {
                    tokio::select! {
                        _ = tokio::signal::ctrl_c() => {
                            tracing::debug!("Keyboard interrupt");
                            conversation.submit(Op::Interrupt).await.ok();
                            break;
                        }
                        res = conversation.next_event() => match res {
                            Ok(event) => {
                                debug!("Received event: {event:?}");

                                let is_shutdown_complete = matches!(event.msg, EventMsg::ShutdownComplete);
                                if let Err(e) = tx.send(event) {
                                    error!("Error sending event: {e:?}");
                                    break;
                                }
                                if is_shutdown_complete {
                                    info!("Received shutdown event, exiting event loop.");
                                    break;
                                }
                            },
                            Err(e) => {
                                error!("Error receiving event: {e:?}");
                                break;
                            }
                        }
                    }
                }
            }
            #[cfg(unix)]
            drop(sigterm_stream);
            #[cfg(unix)]
            if sigterm_requested {
                unsafe {
                    libc::raise(libc::SIGTERM);
                }
            }
        });
    }

    // Send the prompt.
    let mut _review_guard: Option<code_core::review_coord::ReviewGuard> = None;

    // Clear stale review lock in case a prior process crashed.
    let _ = clear_stale_lock_if_dead(Some(&config.cwd));

    let skip_review_lock = std::env::var("CODE_REVIEW_LOCK_LEASE")
        .map(|v| v == "1")
        .unwrap_or(false);

    let _initial_prompt_task_id = if let Some(mut review_request) = review_request.clone() {
        // Cross-process review coordination
        if !skip_review_lock {
            match try_acquire_lock("review", &config.cwd) {
                Ok(Some(g)) => _review_guard = Some(g),
                Ok(None) => {
                    eprintln!("Another review is already running; skipping this /review.");
                    return Ok(());
                }
                Err(err) => {
                    eprintln!("Warning: could not acquire review lock: {err}");
                }
            }
        }

        if auto_resolve_state.is_some() {
            if auto_resolve_base_snapshot.is_none() {
                auto_resolve_base_snapshot = capture_auto_resolve_snapshot(&config.cwd, None, "auto-resolve base snapshot");
                if let Some(state) = auto_resolve_state.as_mut() {
                    state.snapshot_epoch = Some(current_snapshot_epoch_for(&config.cwd));
                }
            }

            if let Some(base) = auto_resolve_base_snapshot.as_ref() {
                if let Some((snap, diff_paths)) = capture_snapshot_against_base(&config.cwd, base, "auto-resolve working snapshot") {
                    review_request = apply_commit_scope_to_review_request(
                        review_request,
                        snap.id(),
                        base.id(),
                        Some(diff_paths.as_slice()),
                    );
                    if let Some(state) = auto_resolve_state.as_mut() {
                        state.last_reviewed_commit = Some(snap.id().to_string());
                    }
                }
            }
        }

        // Capture baseline epoch after any snapshot creation so we don't trip on our own bumps.
        last_review_epoch = Some(current_snapshot_epoch_for(&config.cwd));

        let event_id = conversation.submit(Op::Review { review_request }).await?;
        if is_auto_review {
            eprintln!("[auto-review] phase: reviewing (started)");
        }
        info!("Sent /review with event ID: {event_id}");
        event_id
    } else {
        let mut items: Vec<InputItem> = Vec::new();
        items.push(InputItem::Text { text: prompt_to_send });
        items.extend(images.into_iter().map(|path| InputItem::LocalImage { path }));
        // Fallback for older core protocol: send only user input items.
        let event_id = conversation
            .submit(Op::UserInput {
                items,
                final_output_json_schema: None,
            })
            .await?;
        info!("Sent prompt with event ID: {event_id}");
        event_id
    };

    // Run the loop until the task is complete.
    // Track whether a fatal error was reported by the server so we can
    // exit with a non-zero status for automation-friendly signaling.
    let mut error_seen = false;
    let mut shutdown_pending = false;
    let mut shutdown_sent = false;
    let mut shutdown_deadline: Option<Instant> = None;
    let auto_review_grace_enabled = config.tui.auto_review_enabled;
    let mut auto_review_tracker = AutoReviewTracker::new(&config.cwd);
    loop {
        tokio::select! {
            _ = async {
                if let Some(deadline) = run_deadline {
                    tokio::time::sleep_until(deadline).await;
                } else {
                    std::future::pending::<()>().await;
                }
            } => {
                eprintln!("Time budget exceeded (--max-seconds={})", max_seconds.unwrap_or_default());
                error_seen = true;
                let _ = conversation.submit(Op::Interrupt).await;
                let _ = conversation.submit(Op::Shutdown).await;
                break;
            }
            maybe_event = rx.recv() => {
                let Some(event) = maybe_event else {
                    break;
                };
        if let EventMsg::AgentStatusUpdate(status) = &event.msg {
            let completions = auto_review_tracker.update(status);
            for completion in completions {
                emit_auto_review_completion(&completion);
            }
        }
        if matches!(event.msg, EventMsg::Error(_)) {
            error_seen = true;
        }

        // Handle review auto-resolve: chain follow-up reviews when enabled.
        match &event.msg {
            EventMsg::ExitedReviewMode(event) => {
                // Any review that just finished should release follow-up locks.
                auto_resolve_followup_guard = None;
                // Release the global review lock as soon as the review finishes so follow-up
                // auto-resolve steps can acquire it.
                _review_guard = None;
                review_runs = review_runs.saturating_add(1);
                    if let Some(output) = event.review_output.as_ref() {
                        review_outputs.push(output.clone());
                    }
                    if let Some(snapshot) = event.snapshot.as_ref() {
                        final_review_snapshot = Some(snapshot.clone());
                        // detect stale snapshot epoch
                        if let Some(start_epoch) = last_review_epoch {
                            let current_epoch = current_snapshot_epoch_for(&config.cwd);
                            if current_epoch != start_epoch {
                                eprintln!("Snapshot epoch changed during review; aborting auto-resolve and requiring restart.");
                                auto_resolve_state = None;
                                auto_resolve_base_snapshot = None;
                                continue;
                            }
                        }
                    }

                // Surface review result to the parent CLI via stderr; avoid injecting
                // synthetic user turns into the /review sub-agent conversation.
                if review_request.is_some() {
                    let findings_count = event
                        .review_output
                        .as_ref()
                        .map(|o| o.findings.len())
                        .unwrap_or(0);
                    let branch = current_branch_name(&config.cwd)
                        .await
                        .unwrap_or_else(|| "unknown".to_string());
                    let worktree = config.cwd.clone();
                    let summary = event.review_output.as_ref().and_then(review_summary_line);

                    if findings_count == 0 {
                        eprintln!(
                            "[developer] Auto-review completed on branch '{branch}' (worktree: {}). No issues reported.",
                            worktree.display()
                        );
                    } else {
                        match summary {
                            Some(ref text) if !text.is_empty() => eprintln!(
                                "[developer] Auto-review found {findings_count} issue(s) on branch '{branch}'. Summary: {text}. Worktree: {}. Merge this worktree/branch to apply fixes.",
                                worktree.display()
                            ),
                            _ => eprintln!(
                                "[developer] Auto-review found {findings_count} issue(s) on branch '{branch}'. Worktree: {}. Merge this worktree/branch to apply fixes.",
                                worktree.display()
                            ),
                        }
                    }
                }

                if let Some(state) = auto_resolve_state.as_mut() {
                    state.attempt = state.attempt.saturating_add(1);
                    state.last_review = event.review_output.clone();
                    state.last_fix_message = None;

                    match event.review_output.as_ref() {
                        Some(output) if output.findings.is_empty() => {
                            eprintln!("Auto-resolve: review reported no actionable findings. Exiting.");
                            auto_resolve_state = None;
                            auto_resolve_base_snapshot = None;
                        }
                        Some(_) if state.max_attempts > 0 && state.attempt > state.max_attempts => {
                            let limit = state.max_attempts;
                            let msg = if limit == 1 {
                                "Auto-resolve: reached the review attempt limit (1 allowed re-review). Handing control back.".to_string()
                            } else {
                                format!(
                                    "Auto-resolve: reached the review attempt limit ({limit} allowed re-reviews). Handing control back."
                                )
                            };
                            eprintln!("{msg}");
                            auto_resolve_state = None;
                            auto_resolve_base_snapshot = None;
                        }
                        Some(output) => {
                            state.phase = AutoResolvePhase::PendingFix {
                                review: output.clone(),
                            };
                        }
                        None => {
                            eprintln!(
                                "Auto-resolve: review ended without findings. Please inspect manually."
                            );
                            auto_resolve_state = None;
                            auto_resolve_base_snapshot = None;
                        }
                    }
                }
            }
            EventMsg::TaskComplete(TaskCompleteEvent { last_agent_message }) => {
                if let Some(state_snapshot) = auto_resolve_state.clone() {
                    let current_epoch = current_snapshot_epoch_for(&config.cwd);
                    match state_snapshot.phase {
                        AutoResolvePhase::PendingFix { review } => {
                            if auto_resolve_fix_guard.is_none() {
                                auto_resolve_fix_guard = try_acquire_lock("auto-resolve-fix", &config.cwd).ok().flatten();
                            }
                            if auto_resolve_fix_guard.is_none() {
                                eprintln!("Auto-resolve: another review is running; skipping fix.");
                                auto_resolve_state = None;
                                auto_resolve_base_snapshot = None;
                                auto_resolve_fix_guard = None;
                                auto_resolve_followup_guard = None;
                                request_shutdown(
                                    &conversation,
                                    &auto_review_tracker,
                                    &mut shutdown_pending,
                                    &mut shutdown_sent,
                                    &mut shutdown_deadline,
                                    auto_review_grace_enabled,
                                )
                                .await?;
                                continue;
                            }
                            if let Some(state) = auto_resolve_state.as_mut() {
                                state.phase = AutoResolvePhase::AwaitingFix {
                                    review: review.clone(),
                                };
                                state.snapshot_epoch = Some(current_epoch);
                            }
                            eprintln!("[auto-review] phase: resolving (started)");
                            dispatch_auto_fix(&conversation, &review).await?;
                        }
                        AutoResolvePhase::AwaitingFix { .. } => {
                            // Fix phase complete; release fix guard so follow-up can take the review lock
                            auto_resolve_fix_guard = None;
                            if let Some(state) = auto_resolve_state.as_mut() {
                                state.last_fix_message = last_agent_message.clone();
                                state.phase = AutoResolvePhase::WaitingForReview;
                            }
                            if auto_resolve_followup_guard.is_none() {
                                auto_resolve_followup_guard =
                                    try_acquire_lock("auto-resolve-followup", &config.cwd).ok().flatten();
                            }
                            if auto_resolve_followup_guard.is_none() {
                                eprintln!("Auto-resolve: another review is running; stopping follow-up review.");
                                auto_resolve_state = None;
                                auto_resolve_base_snapshot = None;
                                auto_resolve_fix_guard = None;
                                auto_resolve_followup_guard = None;
                                request_shutdown(
                                    &conversation,
                                    &auto_review_tracker,
                                    &mut shutdown_pending,
                                    &mut shutdown_sent,
                                    &mut shutdown_deadline,
                                    auto_review_grace_enabled,
                                )
                                .await?;
                                continue;
                            }
                            if let Some(base) = auto_resolve_base_snapshot.as_ref() {
                                if !head_is_ancestor_of_base(&config.cwd, base.id()) {
                                    eprintln!("Auto-resolve: base snapshot no longer matches current HEAD; stopping to avoid stale review.");
                                    auto_resolve_state = None;
                                    auto_resolve_base_snapshot = None;
                                    auto_resolve_followup_guard = None;
                                    auto_resolve_fix_guard = None;
                                    request_shutdown(
                                        &conversation,
                                        &auto_review_tracker,
                                        &mut shutdown_pending,
                                        &mut shutdown_sent,
                                        &mut shutdown_deadline,
                                        auto_review_grace_enabled,
                                    )
                                    .await?;
                                    continue;
                                }
                                // stale epoch check
                                if let Some(state) = auto_resolve_state.as_ref() {
                                    if let Some(baseline) = state.snapshot_epoch {
                                        if current_epoch > baseline {
                                            eprintln!("Auto-resolve: snapshot epoch advanced; aborting follow-up review.");
                                            auto_resolve_state = None;
                                            auto_resolve_base_snapshot = None;
                                            auto_resolve_followup_guard = None;
                                            auto_resolve_fix_guard = None;
                                            request_shutdown(
                                                &conversation,
                                                &auto_review_tracker,
                                                &mut shutdown_pending,
                                                &mut shutdown_sent,
                                                &mut shutdown_deadline,
                                                auto_review_grace_enabled,
                                            )
                                            .await?;
                                            continue;
                                        }
                                    }
                                    if state.metadata.as_ref().and_then(|m| m.commit.as_ref()).is_some()
                                        && state.snapshot_epoch.is_none()
                                    {
                                        eprintln!("Auto-resolve: snapshot epoch advanced; aborting follow-up review.");
                                        auto_resolve_state = None;
                                        auto_resolve_base_snapshot = None;
                                        request_shutdown(
                                            &conversation,
                                            &auto_review_tracker,
                                            &mut shutdown_pending,
                                            &mut shutdown_sent,
                                            &mut shutdown_deadline,
                                            auto_review_grace_enabled,
                                        )
                                        .await?;
                                        continue;
                                    }
                                }
                                match capture_snapshot_against_base(
                                    &config.cwd,
                                    base,
                                    "auto-resolve follow-up snapshot",
                                ) {
                                    Some((snap, diff_paths)) => {
                                        if should_skip_followup(
                                            state_snapshot.last_reviewed_commit.as_deref(),
                                            &snap,
                                        ) {
                                            eprintln!("Auto-resolve: follow-up snapshot is identical to last reviewed commit; ending loop to avoid duplicate review.");
                                            auto_resolve_state = None;
                                            auto_resolve_base_snapshot = None;
                                            auto_resolve_followup_guard = None;
                                            auto_resolve_fix_guard = None;
                                            request_shutdown(
                                                &conversation,
                                                &auto_review_tracker,
                                                &mut shutdown_pending,
                                                &mut shutdown_sent,
                                                &mut shutdown_deadline,
                                                auto_review_grace_enabled,
                                            )
                                            .await?;
                                            continue;
                                        }

                                        if let Some(state) = auto_resolve_state.as_mut() {
                                            state.last_reviewed_commit = Some(snap.id().to_string());
                                            state.snapshot_epoch = Some(current_snapshot_epoch_for(&config.cwd));
                                        }

                                        let followup_request = build_followup_review_request(
                                            &state_snapshot,
                                            &config.cwd,
                                            Some(&snap),
                                            Some(diff_paths.as_slice()),
                                            Some(base.id()),
                                        )
                                        .await;
                                        last_review_epoch = Some(current_snapshot_epoch_for(&config.cwd));
                                        eprintln!("[auto-review] phase: reviewing (started)");
                                        let _ = conversation
                                            .submit(Op::Review {
                                                review_request: followup_request,
                                            })
                                            .await?;
                                    }
                                    None => {
                                        eprintln!("Auto-resolve: failed to capture follow-up snapshot or no diff detected; stopping auto-resolve.");
                                        auto_resolve_state = None;
                                        auto_resolve_base_snapshot = None;
                                        auto_resolve_followup_guard = None;
                                        auto_resolve_fix_guard = None;
                                        request_shutdown(
                                            &conversation,
                                            &auto_review_tracker,
                                            &mut shutdown_pending,
                                            &mut shutdown_sent,
                                            &mut shutdown_deadline,
                                            auto_review_grace_enabled,
                                        )
                                        .await?;
                                    }
                                }
                            }
                        }
                        AutoResolvePhase::AwaitingJudge { .. } => {
                            // Legacy branch: fall back to requesting a follow-up review.
                            if let Some(state) = auto_resolve_state.as_mut() {
                                state.last_fix_message = last_agent_message.clone();
                                state.phase = AutoResolvePhase::WaitingForReview;
                            }
                            if let Some(base) = auto_resolve_base_snapshot.as_ref() {
                                if !head_is_ancestor_of_base(&config.cwd, base.id()) {
                                    eprintln!("Auto-resolve: base snapshot no longer matches current HEAD; stopping to avoid stale review.");
                                    auto_resolve_state = None;
                                    auto_resolve_base_snapshot = None;
                                    request_shutdown(
                                        &conversation,
                                        &auto_review_tracker,
                                        &mut shutdown_pending,
                                        &mut shutdown_sent,
                                        &mut shutdown_deadline,
                                        auto_review_grace_enabled,
                                    )
                                    .await?;
                                    continue;
                                }
                                let current_epoch = current_snapshot_epoch_for(&config.cwd);
                                if let Some(state) = auto_resolve_state.as_ref() {
                                    if state
                                        .metadata
                                        .as_ref()
                                        .and_then(|m| m.commit.as_ref())
                                        .is_some()
                                        && current_epoch > state.attempt as u64
                                    {
                                        eprintln!("Auto-resolve: snapshot epoch advanced; aborting follow-up review.");
                                        auto_resolve_state = None;
                                        auto_resolve_base_snapshot = None;
                                        request_shutdown(
                                            &conversation,
                                            &auto_review_tracker,
                                            &mut shutdown_pending,
                                            &mut shutdown_sent,
                                            &mut shutdown_deadline,
                                            auto_review_grace_enabled,
                                        )
                                        .await?;
                                        continue;
                                    }
                                }
                                match capture_snapshot_against_base(
                                    &config.cwd,
                                    base,
                                    "auto-resolve follow-up snapshot",
                                ) {
                                    Some((snap, diff_paths)) => {
                                        if should_skip_followup(
                                            state_snapshot.last_reviewed_commit.as_deref(),
                                            &snap,
                                        ) {
                                            eprintln!("Auto-resolve: follow-up snapshot is identical to last reviewed commit; ending loop to avoid duplicate review.");
                                            auto_resolve_state = None;
                                            auto_resolve_base_snapshot = None;
                                            request_shutdown(
                                                &conversation,
                                                &auto_review_tracker,
                                                &mut shutdown_pending,
                                                &mut shutdown_sent,
                                                &mut shutdown_deadline,
                                                auto_review_grace_enabled,
                                            )
                                            .await?;
                                            continue;
                                        }

                                        if let Some(state) = auto_resolve_state.as_mut() {
                                            state.last_reviewed_commit = Some(snap.id().to_string());
                                        }

                                        let followup_request = build_followup_review_request(
                                            &state_snapshot,
                                            &config.cwd,
                                            Some(&snap),
                                            Some(diff_paths.as_slice()),
                                            Some(base.id()),
                                        )
                                        .await;
                                        last_review_epoch = Some(current_snapshot_epoch_for(&config.cwd));
                                        let _ = conversation
                                            .submit(Op::Review {
                                                review_request: followup_request,
                                            })
                                            .await?;
                                    }
                                    None => {
                                        eprintln!("Auto-resolve: failed to capture follow-up snapshot or no diff detected; stopping auto-resolve.");
                                        auto_resolve_state = None;
                                        auto_resolve_base_snapshot = None;
                                        auto_resolve_followup_guard = None;
                                        auto_resolve_fix_guard = None;
                                    }
                                }
                            }
                        }
                        AutoResolvePhase::WaitingForReview => {
                            // Task complete from a review; handled in ExitedReviewMode.
                        }
                    }
                }

                if auto_resolve_state.is_none() && !shutdown_sent {
                    auto_resolve_base_snapshot = None;
                    request_shutdown(
                        &conversation,
                        &auto_review_tracker,
                        &mut shutdown_pending,
                        &mut shutdown_sent,
                        &mut shutdown_deadline,
                        auto_review_grace_enabled,
                    )
                    .await?;
                }
            }
            _ => {}
        }

        let shutdown: CodexStatus = event_processor.process_event(event);
        match shutdown {
            CodexStatus::Running => {}
            CodexStatus::InitiateShutdown => {
                request_shutdown(
                    &conversation,
                    &auto_review_tracker,
                    &mut shutdown_pending,
                    &mut shutdown_sent,
                    &mut shutdown_deadline,
                    auto_review_grace_enabled,
                )
                .await?;
            }
            CodexStatus::Shutdown => {
                break;
            }
        }

        if shutdown_pending {
            request_shutdown(
                &conversation,
                &auto_review_tracker,
                &mut shutdown_pending,
                &mut shutdown_sent,
                &mut shutdown_deadline,
                auto_review_grace_enabled,
            )
            .await?;
        }
            }
            _ = tokio::time::sleep_until(shutdown_deadline.unwrap_or_else(Instant::now)),
                if shutdown_pending && shutdown_deadline.is_some() && auto_review_grace_enabled =>
            {
                request_shutdown(
                    &conversation,
                    &auto_review_tracker,
                    &mut shutdown_pending,
                    &mut shutdown_sent,
                    &mut shutdown_deadline,
                    auto_review_grace_enabled,
                )
                .await?;
            }
        }
    }
    if let Some(path) = review_output_json {
        if !review_outputs.is_empty() {
            let _ = write_review_json(path, &review_outputs, final_review_snapshot.as_ref());
        }
    }
    if review_runs > 0 {
        eprintln!("Review runs: {} (auto_resolve={} max_attempts={})", review_runs, config.tui.review_auto_resolve, max_auto_resolve_attempts);
    }
    if error_seen {
        std::process::exit(1);
    }

    Ok(())
}

async fn resolve_resume_path(
    config: &Config,
    args: &crate::cli::ResumeArgs,
) -> anyhow::Result<Option<PathBuf>> {
    if !args.last && args.session_id.is_none() {
        return Ok(None);
    }

    let catalog = SessionCatalog::new(config.code_home.clone());

    if let Some(id_str) = args.session_id.as_deref() {
        let entry = catalog
            .find_by_id(id_str)
            .await
            .context("failed to look up session by id")?;
        Ok(entry.map(|entry| entry_to_rollout_path(&config.code_home, &entry)))
    } else if args.last {
        let query = SessionQuery {
            cwd: None,
            git_root: None,
            sources: vec![SessionSource::Cli, SessionSource::VSCode, SessionSource::Exec],
            min_user_messages: 1,
            include_archived: false,
            include_deleted: false,
            limit: Some(1),
        };
        let entry = catalog
            .get_latest(&query)
            .await
            .context("failed to get latest session from catalog")?;
        Ok(entry.map(|entry| entry_to_rollout_path(&config.code_home, &entry)))
    } else {
        Ok(None)
    }
}

struct TurnResult {
    last_agent_message: Option<String>,
    error_seen: bool,
}

async fn run_auto_drive_session(
    goal: String,
    images: Vec<PathBuf>,
    config: Config,
    conversation: Arc<CodexConversation>,
    mut event_processor: Box<dyn EventProcessor>,
    last_message_path: Option<PathBuf>,
    run_deadline: Option<Instant>,
) -> anyhow::Result<()> {
    let mut final_last_message: Option<String> = None;
    let mut error_seen = false;
    let mut auto_review_tracker = AutoReviewTracker::new(&config.cwd);
    let mut shutdown_sent = false;

    if !images.is_empty() {
        let items: Vec<InputItem> = images
            .into_iter()
            .map(|path| InputItem::LocalImage { path })
            .collect();
        let initial_images_event_id = conversation
            .submit(Op::UserInput {
                items,
                final_output_json_schema: None,
            })
            .await?;
        loop {
            let event = if let Some(deadline) = run_deadline {
                let remaining = deadline.saturating_duration_since(Instant::now());
                match tokio::time::timeout(remaining, conversation.next_event()).await {
                    Ok(event) => event?,
                    Err(_) => {
                        eprintln!(
                            "Time budget exceeded (--max-seconds={})",
                            config.max_run_seconds.unwrap_or_default()
                        );
                        let _ = conversation.submit(Op::Interrupt).await;
                        let _ = conversation.submit(Op::Shutdown).await;
                        return Err(anyhow::anyhow!("Time budget exceeded"));
                    }
                }
            } else {
                conversation.next_event().await?
            };

            let is_complete = event.id == initial_images_event_id
                && matches!(
                    event.msg,
                    EventMsg::TaskComplete(TaskCompleteEvent {
                        last_agent_message: _,
                    })
                );
            let status = event_processor.process_event(event);
            if is_complete || matches!(status, CodexStatus::Shutdown) {
                break;
            }
        }
    }

    let mut history = AutoDriveHistory::new();

    let mut auto_drive_pid_guard =
        AutoDrivePidFile::write(&config.code_home, Some(goal.as_str()), AutoDriveMode::Exec);

	    let auto_config = build_auto_drive_exec_config(&config);

    let (auto_tx, mut auto_rx) = tokio::sync::mpsc::unbounded_channel();
    let sender = AutoCoordinatorEventSender::new(move |event| {
        let _ = auto_tx.send(event);
    });

    let handle = start_auto_coordinator(
        sender,
        goal.clone(),
        history.raw_snapshot(),
        auto_config,
        config.debug,
        false,
    )?;

    loop {
        let maybe_event = if let Some(deadline) = run_deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match tokio::time::timeout(remaining, auto_rx.recv()).await {
                Ok(event) => event,
                Err(_) => {
                    let _ = handle.send(AutoCoordinatorCommand::Stop);
                    handle.cancel();
                    let _ = conversation.submit(Op::Interrupt).await;
                    let _ = conversation.submit(Op::Shutdown).await;
                    return Err(anyhow::anyhow!("Time budget exceeded"));
                }
            }
        } else {
            auto_rx.recv().await
        };

        let Some(event) = maybe_event else {
            break;
        };

        match event {
            AutoCoordinatorEvent::Thinking { delta, .. } => {
                println!("[auto] {delta}");
            }
            AutoCoordinatorEvent::Action { message } => {
                println!("[auto] {message}");
            }
            AutoCoordinatorEvent::TokenMetrics {
                total_usage,
                last_turn_usage,
                turn_count,
                ..
            } => {
                println!(
                    "[auto] turn {} tokens (turn/total): {}/{}",
                    turn_count,
                    last_turn_usage.blended_total(),
                    total_usage.blended_total()
                );
            }
            AutoCoordinatorEvent::CompactedHistory { conversation, .. } => {
                history.replace_all(conversation.to_vec());
            }
            AutoCoordinatorEvent::UserReply {
                user_response,
                cli_command,
            } => {
                if let Some(text) = user_response.filter(|s| !s.trim().is_empty()) {
                    history.append_raw(&[make_assistant_message(text.clone())]);
                    final_last_message = Some(text);
                }

                if let Some(cmd) = cli_command {
                    let prompt_text = cmd.trim();
                    if !prompt_text.is_empty() {
                        history.append_raw(&[make_user_message(prompt_text.to_string())]);
                        let TurnResult {
                            last_agent_message,
                            error_seen: turn_error,
                        } = match submit_and_wait(
                            &conversation,
                            event_processor.as_mut(),
                            &mut auto_review_tracker,
                            prompt_text.to_string(),
                            run_deadline,
                        )
                        .await
                        {
                            Ok(result) => result,
                            Err(err) => {
                                let _ = handle.send(AutoCoordinatorCommand::Stop);
                                handle.cancel();
                                return Err(err);
                            }
                        };
                        error_seen |= turn_error;
                        if let Some(text) = last_agent_message {
                            history.append_raw(&[make_assistant_message(text.clone())]);
                            final_last_message = Some(text);
                        }
                        let _ = handle
                            .send(AutoCoordinatorCommand::UpdateConversation(
                                history.raw_snapshot().into(),
                            ));
                    }
                }
            }
            AutoCoordinatorEvent::Decision {
                seq,
                status,
                status_title,
                status_sent_to_user,
                goal: maybe_goal,
                cli,
                agents_timing,
                agents,
                transcript,
            } => {
                history.append_raw(&transcript);
                let _ = handle.send(AutoCoordinatorCommand::AckDecision { seq });

                if let Some(title) = status_title.filter(|s| !s.trim().is_empty()) {
                    println!("[auto] status: {title}");
                }
                if let Some(sent) = status_sent_to_user.filter(|s| !s.trim().is_empty()) {
                    println!("[auto] update: {sent}");
                }
                if let Some(goal_text) = maybe_goal.filter(|s| !s.trim().is_empty()) {
                    println!("[auto] goal: {goal_text}");
                }

                let Some(cli_action) = cli else {
                    if matches!(status, AutoCoordinatorStatus::Success | AutoCoordinatorStatus::Failed)
                    {
                        let _ = handle.send(AutoCoordinatorCommand::Stop);
                    }
                    continue;
                };

                let prompt_text = build_auto_prompt(&cli_action, &agents, agents_timing);
                history.append_raw(&[make_user_message(prompt_text.clone())]);

                let TurnResult {
                    last_agent_message,
                    error_seen: turn_error,
                } = match submit_and_wait(
                    &conversation,
                    event_processor.as_mut(),
                    &mut auto_review_tracker,
                    prompt_text,
                    run_deadline,
                )
                .await
                {
                    Ok(result) => result,
                    Err(err) => {
                        let _ = handle.send(AutoCoordinatorCommand::Stop);
                        handle.cancel();
                        return Err(err);
                    }
                };
                error_seen |= turn_error;
                if let Some(text) = last_agent_message {
                    history.append_raw(&[make_assistant_message(text.clone())]);
                    final_last_message = Some(text);
                }

                if handle
                    .send(AutoCoordinatorCommand::UpdateConversation(
                        history.raw_snapshot().into(),
                    ))
                    .is_err()
                {
                    break;
                }
            }
            AutoCoordinatorEvent::StopAck => {
                break;
            }
        }
    }

    handle.cancel();

    if !auto_review_tracker.is_running() {
        let grace_deadline = Instant::now() + Duration::from_millis(AUTO_REVIEW_SHUTDOWN_GRACE_MS);
        while Instant::now() < grace_deadline {
            let remaining = grace_deadline.saturating_duration_since(Instant::now());
            match tokio::time::timeout(remaining, conversation.next_event()).await {
                Ok(Ok(event)) => {
                    if let EventMsg::AgentStatusUpdate(status) = &event.msg {
                        let completions = auto_review_tracker.update(status);
                        for completion in completions {
                            emit_auto_review_completion(&completion);
                        }
                    }

                    let processor_status = event_processor.process_event(event);
                    if matches!(processor_status, CodexStatus::Shutdown)
                        || auto_review_tracker.is_running()
                    {
                        break;
                    }
                }
                Ok(Err(err)) => return Err(err.into()),
                Err(_) => break,
            }
        }
    }

    if auto_review_tracker.is_running() {
        loop {
            let event = if let Some(deadline) = run_deadline {
                let remaining = deadline.saturating_duration_since(Instant::now());
                match tokio::time::timeout(remaining, conversation.next_event()).await {
                    Ok(event) => event?,
                    Err(_) => {
                        eprintln!(
                            "Time budget exceeded (--max-seconds={})",
                            config.max_run_seconds.unwrap_or_default()
                        );
                        let _ = conversation.submit(Op::Interrupt).await;
                        let _ = conversation.submit(Op::Shutdown).await;
                        return Err(anyhow::anyhow!("Time budget exceeded"));
                    }
                }
            } else {
                conversation.next_event().await?
            };

            if let EventMsg::AgentStatusUpdate(status) = &event.msg {
                let completions = auto_review_tracker.update(status);
                for completion in completions {
                    emit_auto_review_completion(&completion);
                }
            }

            let status = event_processor.process_event(event);

            if !auto_review_tracker.is_running() {
                break;
            }

            if matches!(status, CodexStatus::Shutdown) {
                break;
            }
        }
    }

    let _ = send_shutdown_if_ready(&conversation, &auto_review_tracker, &mut shutdown_sent).await?;

    loop {
        let event = if let Some(deadline) = run_deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match tokio::time::timeout(remaining, conversation.next_event()).await {
                Ok(event) => event?,
                Err(_) => {
                    eprintln!(
                        "Time budget exceeded (--max-seconds={})",
                        config.max_run_seconds.unwrap_or_default()
                    );
                    let _ = conversation.submit(Op::Interrupt).await;
                    let _ = conversation.submit(Op::Shutdown).await;
                    return Err(anyhow::anyhow!("Time budget exceeded"));
                }
            }
        } else {
            conversation.next_event().await?
        };

        if let EventMsg::AgentStatusUpdate(status) = &event.msg {
            let completions = auto_review_tracker.update(status);
            for completion in completions {
                emit_auto_review_completion(&completion);
            }
        }

        if matches!(event.msg, EventMsg::ShutdownComplete) {
            break;
        }
        let status = event_processor.process_event(event);
        if matches!(status, CodexStatus::Shutdown) {
            break;
        }
    }

    if let Some(path) = last_message_path.as_deref() {
        handle_last_message(final_last_message.as_deref(), path);
    }

    if error_seen {
        if let Some(guard) = auto_drive_pid_guard.take() {
            guard.cleanup();
        }
        std::process::exit(1);
    }

    Ok(())
}

fn append_timeboxed_auto_drive_goal(goal: &str) -> String {
    let trimmed_goal = goal.trim();
    if trimmed_goal.is_empty() {
        return AUTO_EXEC_TIMEBOXED_GOAL_SUFFIX.to_string();
    }

    format!("{trimmed_goal}\n\n{AUTO_EXEC_TIMEBOXED_GOAL_SUFFIX}")
}

fn merge_developer_message(existing: Option<String>, extra: &str) -> Option<String> {
    let extra_trimmed = extra.trim();
    if extra_trimmed.is_empty() {
        return existing;
    }

    match existing {
        Some(mut message) => {
            if !message.trim().is_empty() {
                message.push_str("\n\n");
            }
            message.push_str(extra_trimmed);
            Some(message)
        }
        None => Some(extra_trimmed.to_string()),
    }
}

#[derive(Default, Debug, Clone)]
struct AutoReviewSummary {
    has_findings: bool,
    findings: usize,
    summary: Option<String>,
}

#[derive(Debug, Clone)]
struct AutoReviewCompletion {
    branch: Option<String>,
    worktree_path: Option<PathBuf>,
    summary: AutoReviewSummary,
    error: Option<String>,
}

#[derive(Default)]
struct AutoReviewTracker {
    running: HashSet<String>,
    processed: HashSet<String>,
    git_root: PathBuf,
}

impl AutoReviewTracker {
    fn new(cwd: &Path) -> Self {
        let git_root = get_git_repo_root(cwd).unwrap_or_else(|| cwd.to_path_buf());

        Self {
            running: HashSet::new(),
            processed: HashSet::new(),
            git_root,
        }
    }

    fn update(&mut self, event: &AgentStatusUpdateEvent) -> Vec<AutoReviewCompletion> {
        let mut completions: Vec<AutoReviewCompletion> = Vec::new();

        for agent in event.agents.iter() {
            if !matches!(agent.source_kind, Some(AgentSourceKind::AutoReview)) {
                continue;
            }

            let status = agent.status.to_ascii_lowercase();
            if status == "pending" || status == "running" {
                self.running.insert(agent.id.clone());
                continue;
            }

            let is_terminal = matches!(
                status.as_str(),
                "completed" | "failed" | "cancelled"
            );
            if !is_terminal || self.processed.contains(&agent.id) {
                continue;
            }

            self.running.remove(&agent.id);
            self.processed.insert(agent.id.clone());

            let summary = agent
                .result
                .as_deref()
                .map(parse_auto_review_summary)
                .unwrap_or_default();

            completions.push(AutoReviewCompletion {
                branch: agent.batch_id.clone(),
                worktree_path: agent
                    .batch_id
                    .as_deref()
                    .and_then(|branch| resolve_auto_review_worktree_path(&self.git_root, branch)),
                summary,
                error: agent.error.clone(),
            });
        }

        completions
    }

    fn is_running(&self) -> bool {
        !self.running.is_empty()
    }
}

fn emit_auto_review_completion(completion: &AutoReviewCompletion) {
    let branch = completion.branch.as_deref().unwrap_or("auto-review");

    if let Some(err) = completion.error.as_deref() {
        eprintln!("[auto-review] {branch}: failed: {err}");
        return;
    }

    let summary_text = completion
        .summary
        .summary
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("No issues reported.");

    if completion.summary.has_findings {
        let count = completion.summary.findings.max(1);
        if let Some(path) = completion.worktree_path.as_ref() {
            eprintln!(
                "[auto-review] {branch}: {count} issue(s) found. Merge {} to apply fixes. Summary: {summary_text}",
                path.display()
            );
        } else {
            eprintln!(
                "[auto-review] {branch}: {count} issue(s) found. Summary: {summary_text}"
            );
        }
    } else if summary_text == "No issues reported." {
        eprintln!("[auto-review] {branch}: no issues found.");
    } else {
        eprintln!("[auto-review] {branch}: no issues found. {summary_text}");
    }
}

fn parse_auto_review_summary(raw: &str) -> AutoReviewSummary {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return AutoReviewSummary::default();
    }

    #[derive(serde::Deserialize)]
    struct MultiRun {
        #[serde(flatten)]
        latest: ReviewOutputEvent,
        #[serde(default)]
        runs: Vec<ReviewOutputEvent>,
    }

    if let Ok(wrapper) = serde_json::from_str::<MultiRun>(trimmed) {
        let mut runs = wrapper.runs;
        if runs.is_empty() {
            runs.push(wrapper.latest);
        }
        return summary_from_runs(&runs);
    }

    if let Ok(output) = serde_json::from_str::<ReviewOutputEvent>(trimmed) {
        return summary_from_output(&output);
    }

    if let Some(start) = trimmed.find("```") {
        if let Some((body, _)) = trimmed[start + 3..].split_once("```") {
            let candidate = body.trim_start_matches("json").trim();
            if let Ok(output) = serde_json::from_str::<ReviewOutputEvent>(candidate) {
                return summary_from_output(&output);
            }
        }
    }

    let lowered = trimmed.to_ascii_lowercase();
    let clean_phrases = [
        "no issues",
        "no findings",
        "clean",
        "looks good",
        "nothing to fix",
    ];
    let skip_phrases = [
        "already running",
        "another review",
        "skipping this",
        "skip this",
    ];
    let issue_markers = [
        "issue",
        "issues",
        "finding",
        "findings",
        "bug",
        "bugs",
        "problem",
        "problems",
        "error",
        "errors",
    ];

    if skip_phrases.iter().any(|p| lowered.contains(p)) {
        return AutoReviewSummary {
            has_findings: false,
            findings: 0,
            summary: Some(trimmed.to_string()),
        };
    }

    if clean_phrases.iter().any(|p| lowered.contains(p)) {
        return AutoReviewSummary {
            has_findings: false,
            findings: 0,
            summary: Some(trimmed.to_string()),
        };
    }

    let has_findings = issue_markers.iter().any(|p| lowered.contains(p));

    AutoReviewSummary {
        has_findings,
        findings: 0,
        summary: Some(trimmed.to_string()),
    }
}

fn summary_from_runs(outputs: &[ReviewOutputEvent]) -> AutoReviewSummary {
    if outputs.is_empty() {
        return AutoReviewSummary::default();
    }

    let latest = outputs.last().unwrap();
    let mut summary = summary_from_output(latest);

    if let Some(idx) = outputs.iter().rposition(|o| !o.findings.is_empty()) {
        let with_findings = summary_from_output(&outputs[idx]);
        if with_findings.has_findings {
            summary.has_findings = true;
            summary.findings = with_findings.findings;
            summary.summary = with_findings.summary.or(summary.summary);

            if latest.findings.is_empty() {
                let tail = "Final pass reported no issues after auto-resolve.";
                summary.summary = match summary.summary {
                    Some(ref existing) if existing.contains(tail) => Some(existing.clone()),
                    Some(existing) => Some(format!("{existing} \n{tail}")),
                    None => Some(tail.to_string()),
                };
            }
        }
    }

    summary
}

fn summary_from_output(output: &ReviewOutputEvent) -> AutoReviewSummary {
    let findings = output.findings.len();
    let has_findings = findings > 0;

    let mut parts: Vec<String> = Vec::new();
    if !output.overall_explanation.trim().is_empty() {
        parts.push(output.overall_explanation.trim().to_string());
    }
    if has_findings {
        let titles: Vec<String> = output
            .findings
            .iter()
            .filter_map(|f| {
                let title = f.title.trim();
                (!title.is_empty()).then_some(title.to_string())
            })
            .collect();
        if !titles.is_empty() {
            parts.push(format!("Findings: {}", titles.join("; ")));
        }
    }

    let summary = (!parts.is_empty()).then(|| parts.join(" \n"));

    AutoReviewSummary {
        has_findings,
        findings,
        summary,
    }
}

fn auto_review_branches_dir(git_root: &Path) -> Option<PathBuf> {
    let repo_name = git_root.file_name()?.to_str()?;
    let mut code_home = code_core::config::find_code_home().ok()?;
    code_home = code_home.join("working").join(repo_name).join("branches");
    std::fs::create_dir_all(&code_home).ok()?;
    Some(code_home)
}

fn resolve_auto_review_worktree_path(git_root: &Path, branch: &str) -> Option<PathBuf> {
    if branch.is_empty() {
        return None;
    }

    let branches_dir = auto_review_branches_dir(git_root)?;
    let candidate = branches_dir.join(branch);
    candidate.exists().then_some(candidate)
}

async fn send_shutdown_if_ready(
    conversation: &Arc<CodexConversation>,
    auto_review_tracker: &AutoReviewTracker,
    shutdown_sent: &mut bool,
) -> anyhow::Result<bool> {
    if *shutdown_sent || auto_review_tracker.is_running() {
        return Ok(false);
    }

    conversation.submit(Op::Shutdown).await?;
    *shutdown_sent = true;
    Ok(true)
}

async fn request_shutdown(
    conversation: &Arc<CodexConversation>,
    auto_review_tracker: &AutoReviewTracker,
    shutdown_pending: &mut bool,
    shutdown_sent: &mut bool,
    shutdown_deadline: &mut Option<Instant>,
    auto_review_grace_enabled: bool,
) -> anyhow::Result<()> {
    if *shutdown_sent {
        *shutdown_pending = false;
        *shutdown_deadline = None;
        return Ok(());
    }

    let now = Instant::now();
    let (attempt_send, new_pending, new_deadline) = shutdown_state_after_request(
        auto_review_tracker.is_running(),
        *shutdown_pending,
        *shutdown_deadline,
        now,
        auto_review_grace_enabled,
    );
    *shutdown_pending = new_pending;
    *shutdown_deadline = new_deadline;

    if !attempt_send {
        return Ok(());
    }

    if send_shutdown_if_ready(conversation, auto_review_tracker, shutdown_sent).await? {
        *shutdown_pending = false;
        *shutdown_deadline = None;
    } else {
        *shutdown_pending = true;
        *shutdown_deadline = None;
    }

    Ok(())
}

fn shutdown_state_after_request(
    auto_review_running: bool,
    shutdown_pending: bool,
    shutdown_deadline: Option<Instant>,
    now: Instant,
    grace_enabled: bool,
) -> (bool, bool, Option<Instant>) {
    if auto_review_running {
        return (false, true, None);
    }

    if !grace_enabled {
        return (true, true, None);
    }

    if !shutdown_pending && shutdown_deadline.is_none() {
        let deadline = now + Duration::from_millis(AUTO_REVIEW_SHUTDOWN_GRACE_MS);
        return (false, true, Some(deadline));
    }

    if let Some(deadline) = shutdown_deadline {
        if deadline > now {
            return (false, true, Some(deadline));
        }
    }

    (true, true, None)
}

fn build_auto_prompt(
    cli_action: &AutoTurnCliAction,
    agents: &[AutoTurnAgentsAction],
    agents_timing: Option<AutoTurnAgentsTiming>,
) -> String {
    let mut sections: Vec<String> = Vec::new();

    if let Some(ctx) = cli_action
        .context
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        sections.push(ctx.to_string());
    }

    let cli_prompt = cli_action.prompt.trim();
    if !cli_prompt.is_empty() {
        sections.push(cli_prompt.to_string());
    }

    if !agents.is_empty() {
        let mut lines: Vec<String> = Vec::new();
        lines.push("<agents>".to_string());
        lines.push("Please use agents to help you complete this task.".to_string());

        for action in agents {
            let prompt = action
                .prompt
                .trim()
                .replace('\n', " ")
                .replace('"', "\\\"");
            let write_text = if action.write { "write: true" } else { "write: false" };

            lines.push(String::new());
            lines.push(format!("prompt: \"{prompt}\" ({write_text})"));

            if let Some(ctx) = action
                .context
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                lines.push(format!("context: {}", ctx.replace('\n', " ")));
            }

            if let Some(models) = action.models.as_ref().filter(|list| !list.is_empty()) {
                lines.push(format!("models: {}", models.join(", ")));
            }
        }

        let timing_line = match agents_timing {
            Some(AutoTurnAgentsTiming::Parallel) =>
                "Timing: parallel — continue the CLI prompt while agents run; call agent.wait when ready to merge results.".to_string(),
            Some(AutoTurnAgentsTiming::Blocking) =>
                "Timing: blocking — launch agents first, wait with agent.wait, then continue the CLI prompt.".to_string(),
            None =>
                "Timing: blocking — wait for agent.wait before continuing the CLI prompt.".to_string(),
        };
        lines.push(String::new());
        lines.push(timing_line);
        lines.push("</agents>".to_string());

        sections.push(lines.join("\n"));
    }

    sections.join("\n\n")
}

async fn dispatch_auto_fix(
    conversation: &Arc<CodexConversation>,
    review: &code_core::protocol::ReviewOutputEvent,
) -> anyhow::Result<()> {
    let fix_prompt = build_fix_prompt(review);
    let items: Vec<InputItem> = vec![InputItem::Text { text: fix_prompt }];
    let _ = conversation
        .submit(Op::UserInput {
            items,
            final_output_json_schema: None,
        })
        .await?;
    Ok(())
}

fn capture_auto_resolve_snapshot(
    cwd: &Path,
    parent: Option<&str>,
    message: &'static str,
) -> Option<GhostCommit> {
    let cwd_buf = cwd.to_path_buf();
    let hook = move || bump_snapshot_epoch_for(&cwd_buf);
    let mut options = CreateGhostCommitOptions::new(cwd)
        .message(message)
        .post_commit_hook(&hook);
    if let Some(parent) = parent {
        options = options.parent(parent);
    }
    let snap = create_ghost_commit(&options).ok();
    if snap.is_some() {
        bump_snapshot_epoch_for(cwd);
    }
    snap
}

fn snapshot_parent_diff_paths(cwd: &Path, parent: &str, head: &str) -> Option<Vec<String>> {
    let output = std::process::Command::new("git")
        .current_dir(cwd)
        .args(["diff", "--name-only", parent, head])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let paths: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect();

    Some(paths)
}

fn apply_commit_scope_to_review_request(
    mut request: ReviewRequest,
    commit: &str,
    parent: &str,
    paths: Option<&[String]>,
) -> ReviewRequest {
    let short_commit: String = commit.chars().take(7).collect();
    let short_parent: String = parent.chars().take(7).collect();

    let mut prompt = request.prompt.trim_end().to_string();
    prompt.push_str("\n\nReview scope: changes captured in commit ");
    prompt.push_str(commit);
    prompt.push_str(" (parent ");
    prompt.push_str(parent);
    prompt.push(')');
    prompt.push('.');

    if let Some(paths) = paths {
        if !paths.is_empty() {
            prompt.push_str("\nFiles changed in this snapshot:\n");
            for path in paths {
                prompt.push_str("- ");
                prompt.push_str(path);
                prompt.push('\n');
            }
        }
    }

    request.prompt = prompt;
    request.user_facing_hint = format!("commit {short_commit} (parent {short_parent})");

    let mut metadata = request.metadata.unwrap_or_default();
    metadata.scope = Some("commit".to_string());
    metadata.commit = Some(commit.to_string());
    request.metadata = Some(metadata);
    request
}

fn extract_commit_from_prompt(prompt: &str) -> Option<String> {
    let mut words = prompt.split_whitespace().peekable();
    while let Some(word) = words.next() {
        if word.eq_ignore_ascii_case("commit") {
            if let Some(next) = words.peek() {
                let candidate = next.trim_matches(|c: char| c == '.' || c == ',' || c == ';');
                let len_ok = (7..=40).contains(&candidate.len());
                let is_hex = candidate.chars().all(|c| c.is_ascii_hexdigit());
                if len_ok && is_hex {
                    return Some(candidate.to_string());
                }
            }
        }
    }
    None
}

async fn head_commit_with_subject(cwd: &Path) -> Option<(String, Option<String>)> {
    let mut commits = recent_commits(cwd, 1).await.into_iter();
    let entry = commits.next()?;
    let subject = entry.subject.trim();
    let subject = (!subject.is_empty()).then(|| subject.to_string());
    Some((entry.sha, subject))
}

fn capture_snapshot_against_base(
    cwd: &Path,
    base: &GhostCommit,
    message: &'static str,
) -> Option<(GhostCommit, Vec<String>)> {
    let snapshot = capture_auto_resolve_snapshot(cwd, Some(base.id()), message)?;
    let diff_paths = snapshot_parent_diff_paths(cwd, base.id(), snapshot.id())?;
    if diff_paths.is_empty() {
        return None;
    }
    bump_snapshot_epoch_for(cwd);
    Some((snapshot, diff_paths))
}

fn strip_scope_from_prompt(prompt: &str) -> String {
    let mut base = prompt.trim_end().to_string();
    if let Some(idx) = base.find(AUTO_RESOLVE_REVIEW_FOLLOWUP) {
        base = base[..idx].trim_end().to_string();
    }
    let filtered: Vec<&str> = base
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            !(trimmed.starts_with("Review scope:") || trimmed.starts_with("commit "))
        })
        .collect();
    filtered.join("\n")
}

/// Remove lines that pin the review to specific commit hashes so follow-up
/// reviews can safely re-scope to the newest snapshot.
fn strip_commit_mentions(prompt: &str, commits: &[&str]) -> String {
    prompt
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            if trimmed
                .to_ascii_lowercase()
                .contains("analyze only changes made in commit")
            {
                return false;
            }
            for c in commits {
                if !c.is_empty() && trimmed.contains(c) {
                    return false;
                }
            }
            true
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn should_skip_followup(last_reviewed_commit: Option<&str>, next_snapshot: &GhostCommit) -> bool {
    match last_reviewed_commit {
        Some(prev) => prev == next_snapshot.id(),
        None => false,
    }
}

/// Returns true if the current HEAD is an ancestor of `base_commit`.
///
/// Ghost snapshots are created as children of the then-current HEAD. That means
/// HEAD should be an ancestor of the snapshot immediately after creation. If
/// HEAD moves later (new commits, rebases, etc.) it may no longer be an
/// ancestor, which indicates the snapshot is stale relative to the live branch
/// we plan to patch against.
fn head_is_ancestor_of_base(cwd: &Path, base_commit: &str) -> bool {
    let output = std::process::Command::new("git")
        .current_dir(cwd)
        .args(["merge-base", "--is-ancestor", "HEAD", base_commit])
        .output();

    match output {
        Ok(out) => out.status.success(),
        Err(_) => false,
    }
}

async fn build_followup_review_request(
    state: &AutoResolveState,
    cwd: &Path,
    snapshot: Option<&GhostCommit>,
    diff_paths: Option<&[String]>,
    parent_commit: Option<&str>,
) -> ReviewRequest {
    let mut prompt = strip_scope_from_prompt(&state.prompt);

    let mut user_facing_hint = state.hint.clone();
    let mut metadata = state.metadata.clone().unwrap_or_default();

    if let (Some(snapshot), Some(parent)) = (snapshot, parent_commit) {
        let updated = apply_commit_scope_to_review_request(
            ReviewRequest {
                prompt: prompt.clone(),
                user_facing_hint: user_facing_hint.clone(),
                metadata: Some(metadata.clone()),
            },
            snapshot.id(),
            parent,
            diff_paths,
        );
        prompt = updated.prompt;
        user_facing_hint = updated.user_facing_hint;
        metadata = updated.metadata.unwrap_or_default();
    }

    // Strip lingering references to earlier commits so follow-up /review scopes to
    // the freshly captured snapshot instead of the original hash baked into the
    // user prompt.
    let mut commit_ids: Vec<&str> = Vec::new();
    if let Some(last) = state.last_reviewed_commit.as_deref() {
        commit_ids.push(last);
    }
    if let Some(meta_commit) = metadata.commit.as_deref() {
        commit_ids.push(meta_commit);
    }
    if let Some(parent) = parent_commit {
        commit_ids.push(parent);
    }
    prompt = strip_commit_mentions(&prompt, &commit_ids);

    // Ensure commit metadata matches the snapshot we will review.
    if let Some(snapshot) = snapshot {
        metadata.commit = Some(snapshot.id().to_string());
        metadata.scope = Some("commit".to_string());
    } else if metadata.commit.is_none() {
        if let Some(commit) = extract_commit_from_prompt(&prompt) {
            metadata.commit = Some(commit);
        }
    }

    if metadata.scope.is_none() && metadata.commit.is_some() {
        metadata.scope = Some("commit".to_string());
    }

    let scope_is_commit = metadata
        .scope
        .as_ref()
        .is_some_and(|scope| scope.eq_ignore_ascii_case("commit"));

    if scope_is_commit && metadata.commit.is_none() {
        if let Some((head_sha, _)) = head_commit_with_subject(cwd).await {
            metadata.commit = Some(head_sha.clone());
            if metadata.current_branch.is_none() {
                metadata.current_branch = current_branch_name(cwd).await;
            }
        }
    }

    if let Some(last_review) = state.last_review.as_ref() {
        let recap = format_review_findings(last_review);
        if !recap.is_empty() {
            prompt.push_str("\n\nPreviously reported findings to re-validate:\n");
            prompt.push_str(&recap);
        }
    }

    if !prompt.contains(AUTO_RESOLVE_REVIEW_FOLLOWUP) {
        prompt.push_str("\n\n");
        prompt.push_str(AUTO_RESOLVE_REVIEW_FOLLOWUP);
    }

    let metadata = if metadata == ReviewContextMetadata::default() {
        None
    } else {
        Some(metadata)
    };

    ReviewRequest {
        prompt,
        user_facing_hint,
        metadata,
    }
}

fn build_fix_prompt(review: &code_core::protocol::ReviewOutputEvent) -> String {
    let summary = format_review_findings(review);
    let raw_json = serde_json::to_string_pretty(review).unwrap_or_else(|_| "{}".to_string());
    let mut preface = String::from(
        "You are continuing an automated /review resolution loop. Review the listed findings and determine whether they represent real issues introduced by our changes. If they are, apply the necessary fixes and resolve any similar issues you can identify before responding."
    );
    if !summary.is_empty() {
        preface.push_str("\n\nFindings:\n");
        preface.push_str(&summary);
    }
    preface.push_str("\n\nFull review JSON (includes file paths and line ranges):\n");
    preface.push_str(&raw_json);
    format!(
        "Is this a real issue introduced by our changes? If so, please fix and resolve all similar issues.\n\n{preface}"
    )
}

fn format_review_findings(output: &code_core::protocol::ReviewOutputEvent) -> String {
    if output.findings.is_empty() {
        return String::new();
    }
    let mut parts = Vec::new();
    for (idx, f) in output.findings.iter().enumerate() {
        let title = f.title.trim();
        let body = f.body.trim();
        let location = format!(
            "path: {}:{}-{}",
            f.code_location
                .absolute_file_path
                .to_string_lossy()
                .to_string(),
            f.code_location.line_range.start,
            f.code_location.line_range.end
        );
        if body.is_empty() {
            parts.push(format!("{}. {}\n{}", idx + 1, title, location));
        } else {
            parts.push(format!("{}. {}\n{}\n{}", idx + 1, title, location, body));
        }
    }
    parts.join("\n\n")
}

fn review_summary_line(output: &code_core::protocol::ReviewOutputEvent) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    let explanation = output.overall_explanation.trim();
    if !explanation.is_empty() {
        parts.push(explanation.to_string());
    }

    if !output.findings.is_empty() {
        let titles: Vec<String> = output
            .findings
            .iter()
            .filter_map(|f| {
                let title = f.title.trim();
                (!title.is_empty()).then_some(title.to_string())
            })
            .collect();
        if !titles.is_empty() {
            parts.push(format!("Findings: {}", titles.join("; ")));
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" \n"))
    }
}

fn make_user_message(text: String) -> ResponseItem {
    ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText { text }],
    }
}

fn make_assistant_message(text: String) -> ResponseItem {
    ResponseItem::Message {
        id: None,
        role: "assistant".to_string(),
        content: vec![ContentItem::OutputText { text }],
    }
}

fn write_review_json(
    path: PathBuf,
    outputs: &[code_core::protocol::ReviewOutputEvent],
    snapshot: Option<&code_core::protocol::ReviewSnapshotInfo>,
) -> std::io::Result<()> {
    if outputs.is_empty() {
        return Ok(());
    }

    #[derive(serde::Serialize)]
    struct ReviewRun<'a> {
        index: usize,
        #[serde(flatten)]
        output: &'a code_core::protocol::ReviewOutputEvent,
    }

    #[derive(serde::Serialize)]
    struct ReviewJsonOutput<'a> {
        #[serde(flatten)]
        latest: &'a code_core::protocol::ReviewOutputEvent,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        runs: Vec<ReviewRun<'a>>,
        #[serde(flatten, skip_serializing_if = "Option::is_none")]
        snapshot: Option<&'a code_core::protocol::ReviewSnapshotInfo>,
    }

    let latest = outputs
        .last()
        .expect("outputs is non-empty due to earlier guard");
    let runs: Vec<ReviewRun<'_>> = outputs
        .iter()
        .enumerate()
        .map(|(idx, output)| ReviewRun {
            index: idx + 1,
            output,
        })
        .collect();

    let payload = ReviewJsonOutput {
        latest,
        runs,
        snapshot,
    };
    let json = serde_json::to_string_pretty(&payload)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
    std::fs::write(path, json)
}

async fn submit_and_wait(
    conversation: &Arc<CodexConversation>,
    event_processor: &mut dyn EventProcessor,
    auto_review_tracker: &mut AutoReviewTracker,
    prompt_text: String,
    run_deadline: Option<Instant>,
) -> anyhow::Result<TurnResult> {
    let mut error_seen = false;

    let submit_id = conversation
        .submit(Op::UserInput {
            items: vec![InputItem::Text { text: prompt_text }],
            final_output_json_schema: None,
        })
        .await?;

    loop {
        let res = if let Some(deadline) = run_deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    let _ = conversation.submit(Op::Interrupt).await;
                    return Err(anyhow::anyhow!("Interrupted"));
                }
                res = tokio::time::timeout(remaining, conversation.next_event()) => {
                    match res {
                        Ok(event) => event,
                        Err(_) => {
                            let _ = conversation.submit(Op::Interrupt).await;
                            let _ = conversation.submit(Op::Shutdown).await;
                            return Err(anyhow::anyhow!("Time budget exceeded"));
                        }
                    }
                }
            }
        } else {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    let _ = conversation.submit(Op::Interrupt).await;
                    return Err(anyhow::anyhow!("Interrupted"));
                }
                res = conversation.next_event() => res,
            }
        };

        let event = res?;
        let event_id = event.id.clone();
        if matches!(event.msg, EventMsg::Error(_)) {
            error_seen = true;
        }

        if let EventMsg::AgentStatusUpdate(status) = &event.msg {
            let completions = auto_review_tracker.update(status);
            for completion in completions {
                emit_auto_review_completion(&completion);
            }
        }

        let last_agent_message = if let EventMsg::TaskComplete(TaskCompleteEvent { last_agent_message }) = &event.msg {
            last_agent_message.clone()
        } else {
            None
        };

        let status = event_processor.process_event(event);

        if matches!(status, CodexStatus::Shutdown) {
            return Ok(TurnResult {
                last_agent_message: None,
                error_seen,
            });
        }

        if last_agent_message.is_some() && event_id == submit_id {
            return Ok(TurnResult {
                last_agent_message,
                error_seen,
            });
        }
    }
}

fn load_output_schema(path: Option<PathBuf>) -> Option<Value> {
    let path = path?;

    let schema_str = match std::fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(err) => {
            eprintln!(
                "Failed to read output schema file {}: {err}",
                path.display()
            );
            std::process::exit(1);
        }
    };

    match serde_json::from_str::<Value>(&schema_str) {
        Ok(value) => Some(value),
        Err(err) => {
            eprintln!(
                "Output schema file {} is not valid JSON: {err}",
                path.display()
            );
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime};

    use code_core::config::{ConfigOverrides, ConfigToml};
    use code_protocol::models::{ContentItem, ResponseItem};
    use code_protocol::mcp_protocol::ConversationId;
	    use code_protocol::protocol::{
	        EventMsg as ProtoEventMsg, RecordedEvent, RolloutItem, RolloutLine, SessionMeta,
	        SessionMetaLine, SessionSource, UserMessageEvent,
	    };
	    use filetime::{set_file_mtime, FileTime};
	    use tempfile::TempDir;
	    use uuid::Uuid;

	    #[test]
	    fn shutdown_state_schedules_grace_on_first_request() {
	        let now = Instant::now();
	        let (attempt_send, pending, deadline) = shutdown_state_after_request(
	            false,
	            false,
	            None,
	            now,
	            true,
	        );
	        assert!(!attempt_send);
	        assert!(pending);
	        assert!(deadline.expect("deadline").gt(&now));
	    }

	    #[test]
	    fn shutdown_state_waits_until_deadline() {
	        let now = Instant::now();
	        let future_deadline = now + tokio::time::Duration::from_millis(100);
	        let (attempt_send, pending, deadline) = shutdown_state_after_request(
	            false,
	            true,
	            Some(future_deadline),
	            now,
	            true,
	        );
	        assert!(!attempt_send);
	        assert!(pending);
	        assert_eq!(deadline, Some(future_deadline));
	    }

	    #[test]
	    fn shutdown_state_attempts_send_after_grace_elapses() {
	        let now = Instant::now();
	        let expired_deadline = now - tokio::time::Duration::from_millis(1);
	        let (attempt_send, pending, deadline) = shutdown_state_after_request(
	            false,
	            true,
	            Some(expired_deadline),
	            now,
	            true,
	        );
	        assert!(attempt_send);
	        assert!(pending);
	        assert!(deadline.is_none());
	    }

	    #[test]
	    fn shutdown_state_sends_immediately_without_grace() {
	        let now = Instant::now();
	        let (attempt_send, pending, deadline) = shutdown_state_after_request(
	            false,
	            false,
	            None,
	            now,
	            false,
	        );
	        assert!(attempt_send);
	        assert!(pending);
	        assert!(deadline.is_none());
	    }

    #[test]
    fn write_review_json_includes_snapshot() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("out.json");

        let output = code_core::protocol::ReviewOutputEvent {
            findings: vec![code_core::protocol::ReviewFinding {
                title: "bug".into(),
                body: "details".into(),
                confidence_score: 0.5,
                priority: 1,
                code_location: code_core::protocol::ReviewCodeLocation {
                    absolute_file_path: PathBuf::from("src/lib.rs"),
                    line_range: code_core::protocol::ReviewLineRange { start: 1, end: 2 },
                },
            }],
            overall_correctness: "incorrect".into(),
            overall_explanation: "needs fixes".into(),
            overall_confidence_score: 0.7,
        };

        let snapshot = code_core::protocol::ReviewSnapshotInfo {
            snapshot_commit: Some("abc123".into()),
            branch: Some("auto-review-branch".into()),
            worktree_path: Some(PathBuf::from("/tmp/wt")),
            repo_root: Some(PathBuf::from("/tmp/repo")),
        };

        write_review_json(path.clone(), &[output], Some(&snapshot)).unwrap();

        let content = std::fs::read_to_string(path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(v["branch"], "auto-review-branch");
        assert_eq!(v["snapshot_commit"], "abc123");
        assert_eq!(v["worktree_path"], "/tmp/wt");
        assert_eq!(v["findings"].as_array().unwrap().len(), 1);
        let runs = v["runs"].as_array().unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0]["index"], 1);
    }

    #[test]
    fn write_review_json_keeps_all_runs() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("multi.json");

        let first = code_core::protocol::ReviewOutputEvent {
            findings: vec![code_core::protocol::ReviewFinding {
                title: "bug".into(),
                body: "details".into(),
                confidence_score: 0.6,
                priority: 1,
                code_location: code_core::protocol::ReviewCodeLocation {
                    absolute_file_path: PathBuf::from("src/lib.rs"),
                    line_range: code_core::protocol::ReviewLineRange { start: 1, end: 2 },
                },
            }],
            overall_correctness: "incorrect".into(),
            overall_explanation: "needs fixes".into(),
            overall_confidence_score: 0.7,
        };

        let second = code_core::protocol::ReviewOutputEvent {
            findings: Vec::new(),
            overall_correctness: "correct".into(),
            overall_explanation: "clean".into(),
            overall_confidence_score: 0.9,
        };

        write_review_json(path.clone(), &[first, second], None).unwrap();

        let content = std::fs::read_to_string(path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(v["overall_explanation"], "clean"); // latest run is flattened
        let runs = v["runs"].as_array().unwrap();
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0]["index"], 1);
        assert_eq!(runs[0]["findings"].as_array().unwrap().len(), 1);
        assert_eq!(runs[1]["index"], 2);
        assert_eq!(runs[1]["findings"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn strip_scope_removes_previous_commit_scope() {
        let prompt = format!(
            "Please review.\nReview scope: commit abc123 (parent deadbeef)\nMore text\n\n{}",
            AUTO_RESOLVE_REVIEW_FOLLOWUP
        );
        let cleaned = strip_scope_from_prompt(&prompt);
        assert!(!cleaned.contains("abc123"));
        assert!(!cleaned.contains("Review scope"));
        assert!(!cleaned.contains(AUTO_RESOLVE_REVIEW_FOLLOWUP));
        assert!(cleaned.contains("Please review."));
    }

    #[test]
    fn should_skip_followup_detects_duplicate_snapshot() {
        let temp = TempDir::new().unwrap();
        std::process::Command::new("git")
            .current_dir(temp.path())
            .args(["init"])
            .output()
            .unwrap();
        std::fs::write(temp.path().join("foo.txt"), "hello").unwrap();
        std::process::Command::new("git")
            .current_dir(temp.path())
            .args(["add", "."])
            .output()
            .unwrap();
        std::process::Command::new("git")
            .current_dir(temp.path())
            .args(["commit", "-m", "init"])
            .output()
            .unwrap();

        let base = capture_auto_resolve_snapshot(temp.path(), None, "base").expect("base snapshot");
        let snap = capture_auto_resolve_snapshot(temp.path(), Some(base.id()), "dup").expect("child");

        assert!(should_skip_followup(Some(snap.id()), &snap));
        assert!(!should_skip_followup(Some("different"), &snap));
        assert!(!should_skip_followup(None, &snap));
    }

    #[test]
    fn base_ancestor_check_matches_git_history() {
        let temp = TempDir::new().unwrap();
        let run_git = |args: &[&str]| {
            let output = std::process::Command::new("git")
                .current_dir(temp.path())
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "git {:?} failed: {}",
                args,
                String::from_utf8_lossy(&output.stderr)
            );
            output
        };

        run_git(["init"].as_slice());
        run_git(["config", "user.email", "codex@example.com"].as_slice());
        run_git(["config", "user.name", "Codex Tester"].as_slice());
        std::fs::write(temp.path().join("a.txt"), "a").unwrap();
        run_git(["add", "."].as_slice());
        run_git(["commit", "-m", "c1"].as_slice());

        // second commit (represents a snapshot captured off the current HEAD)
        std::fs::write(temp.path().join("a.txt"), "b").unwrap();
        run_git(["commit", "-am", "c2"].as_slice());
        let base = String::from_utf8_lossy(
            &run_git(["rev-parse", "HEAD"].as_slice()).stdout,
        )
        .trim()
        .to_string();

        assert!(head_is_ancestor_of_base(temp.path(), &base));

        // move HEAD back to check false case
        run_git(["checkout", "HEAD~1"].as_slice());
        assert!(!head_is_ancestor_of_base(temp.path(), "deadbeef"));
    }

    fn test_config(code_home: &Path) -> Config {
        let mut overrides = ConfigOverrides::default();
        let workspace = code_home.join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        overrides.cwd = Some(workspace);
        Config::load_from_base_config_with_overrides(
            ConfigToml::default(),
            overrides,
            code_home.to_path_buf(),
        )
        .unwrap()
    }

	    #[test]
	    fn auto_drive_exec_config_uses_auto_drive_reasoning_effort() {
	        let temp = TempDir::new().unwrap();
	        let mut config = test_config(temp.path());
	        config.model_reasoning_effort = code_core::config_types::ReasoningEffort::Low;
	        config.auto_drive.model = "gpt-5.2".to_string();
	        config.auto_drive.model_reasoning_effort =
	            code_core::config_types::ReasoningEffort::XHigh;

	        let auto_config = build_auto_drive_exec_config(&config);
	        assert_eq!(auto_config.model, "gpt-5.2");
	        assert_eq!(
	            auto_config.model_reasoning_effort,
	            code_core::config_types::ReasoningEffort::XHigh
	        );
	    }

    fn write_rollout(
        code_home: &Path,
        session_id: Uuid,
        created_at: &str,
        last_event_at: &str,
        source: SessionSource,
        message: &str,
    ) -> PathBuf {
        let sessions_dir = code_home.join("sessions").join("2025").join("11").join("16");
        std::fs::create_dir_all(&sessions_dir).unwrap();
        let filename = format!(
            "rollout-{}-{}.jsonl",
            created_at.replace(':', "-"),
            session_id
        );
        let path = sessions_dir.join(filename);

        let session_meta = SessionMeta {
            id: ConversationId::from(session_id),
            timestamp: created_at.to_string(),
            cwd: Path::new("/workspace/project").to_path_buf(),
            originator: "test".to_string(),
            cli_version: "0.0.0-test".to_string(),
            instructions: None,
            source,
        };

        let session_line = RolloutLine {
            timestamp: created_at.to_string(),
            item: RolloutItem::SessionMeta(SessionMetaLine {
                meta: session_meta,
                git: None,
            }),
        };
        let event_line = RolloutLine {
            timestamp: last_event_at.to_string(),
            item: RolloutItem::Event(RecordedEvent {
                id: "event-0".to_string(),
                event_seq: 0,
                order: None,
                msg: ProtoEventMsg::UserMessage(UserMessageEvent {
                    message: message.to_string(),
                    kind: None,
                    images: None,
                }),
            }),
        };
        let user_line = RolloutLine {
            timestamp: last_event_at.to_string(),
            item: RolloutItem::ResponseItem(ResponseItem::Message {
                id: Some(format!("user-{}", session_id)),
                role: "user".to_string(),
                content: vec![ContentItem::InputText {
                    text: message.to_string(),
                }],
            }),
        };

        let assistant_line = RolloutLine {
            timestamp: last_event_at.to_string(),
            item: RolloutItem::ResponseItem(ResponseItem::Message {
                id: Some(format!("msg-{}", session_id)),
                role: "assistant".to_string(),
                content: vec![ContentItem::OutputText {
                    text: format!("Ack: {}", message),
                }],
            }),
        };

        let mut writer = std::io::BufWriter::new(std::fs::File::create(&path).unwrap());
        serde_json::to_writer(&mut writer, &session_line).unwrap();
        writer.write_all(b"\n").unwrap();
        serde_json::to_writer(&mut writer, &event_line).unwrap();
        writer.write_all(b"\n").unwrap();
        serde_json::to_writer(&mut writer, &user_line).unwrap();
        writer.write_all(b"\n").unwrap();
        serde_json::to_writer(&mut writer, &assistant_line).unwrap();
        writer.write_all(b"\n").unwrap();
        writer.flush().unwrap();

        path
    }

    #[tokio::test]
    async fn exec_resolve_last_prefers_latest_timestamp() {
        let temp = TempDir::new().unwrap();
        let config = test_config(temp.path());
        let older = Uuid::parse_str("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa").unwrap();
        let newer = Uuid::parse_str("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb").unwrap();

        write_rollout(
            temp.path(),
            older,
            "2025-11-10T09:00:00Z",
            "2025-11-10T09:05:00Z",
            SessionSource::Cli,
            "older",
        );
        write_rollout(
            temp.path(),
            newer,
            "2025-11-16T09:00:00Z",
            "2025-11-16T09:10:00Z",
            SessionSource::Exec,
            "newer",
        );

        let args = crate::cli::ResumeArgs {
            session_id: None,
            last: true,
            prompt: None,
        };
        let path = resolve_resume_path(&config, &args)
            .await
            .unwrap()
            .expect("path");
        let path_str = path.to_string_lossy();
        assert!(
            path_str.contains("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"),
            "resolved path should reference newer session, got {}",
            path_str
        );
    }

    #[tokio::test]
    async fn exec_resolve_by_id_uses_catalog_bootstrap() {
        let temp = TempDir::new().unwrap();
        let config = test_config(temp.path());
        let session_id = Uuid::parse_str("cccccccc-cccc-4ccc-8ccc-cccccccccccc").unwrap();
        write_rollout(
            temp.path(),
            session_id,
            "2025-11-12T09:00:00Z",
            "2025-11-12T09:05:00Z",
            SessionSource::Cli,
            "resume",
        );

        let args = crate::cli::ResumeArgs {
            session_id: Some("cccccccc".to_string()),
            last: false,
            prompt: None,
        };

        let path = resolve_resume_path(&config, &args)
            .await
            .unwrap()
            .expect("path");
        let path_str = path.to_string_lossy();
        assert!(
            path_str.contains("cccccccc-cccc-4ccc-8ccc-cccccccccccc"),
            "resolved path should match requested session, got {}",
            path_str
        );
    }

    #[tokio::test]
    async fn exec_resolve_last_ignores_mtime_drift() {
        let temp = TempDir::new().unwrap();
        let config = test_config(temp.path());
        let older = Uuid::parse_str("dddddddd-dddd-4ddd-8ddd-dddddddddddd").unwrap();
        let newer = Uuid::parse_str("eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee").unwrap();

        let older_path = write_rollout(
            temp.path(),
            older,
            "2025-11-01T09:00:00Z",
            "2025-11-01T09:05:00Z",
            SessionSource::Cli,
            "old",
        );
        let newer_path = write_rollout(
            temp.path(),
            newer,
            "2025-11-20T09:00:00Z",
            "2025-11-20T09:05:00Z",
            SessionSource::Exec,
            "new",
        );

        let base = SystemTime::now();
        set_file_mtime(&older_path, FileTime::from_system_time(base + Duration::from_secs(500))).unwrap();
        set_file_mtime(&newer_path, FileTime::from_system_time(base + Duration::from_secs(10))).unwrap();

        let args = crate::cli::ResumeArgs {
            session_id: None,
            last: true,
            prompt: None,
        };
        let path = resolve_resume_path(&config, &args)
            .await
            .unwrap()
            .expect("path");
        let path_str = path.to_string_lossy();
        assert!(
            path_str.contains("eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee"),
            "resolved path should ignore mtime drift, got {}",
            path_str
        );
    }

}
