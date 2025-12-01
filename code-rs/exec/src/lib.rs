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
use code_core::git_info::get_git_repo_root;
use code_core::protocol::AskForApproval;
use code_core::protocol::Event;
use code_core::protocol::EventMsg;
use code_core::protocol::InputItem;
use code_core::protocol::Op;
use code_core::protocol::ReviewRequest;
use code_core::protocol::TaskCompleteEvent;
use code_protocol::models::ContentItem;
use code_protocol::models::ResponseItem;
use code_protocol::protocol::SessionSource;
use code_ollama::DEFAULT_OSS_MODEL;
use code_protocol::config_types::SandboxMode;
use event_processor_with_human_output::EventProcessorWithHumanOutput;
use event_processor_with_json_output::EventProcessorWithJsonOutput;
use event_processor::handle_last_message;
use serde_json::Value;
use std::io::IsTerminal;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use supports_color::Stream;
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
use code_core::{entry_to_rollout_path, SessionCatalog, SessionQuery};
use code_core::protocol::SandboxPolicy;
use code_core::git_info::current_branch_name;

const AUTO_DRIVE_TEST_SUFFIX: &str = "After planning, but before you start, please ensure you can test the outcome of your changes. Test first to ensure it's failing, then again at the end to ensure it passes. Do not use work arounds or mock code to pass - solve the underlying issue. Create new tests as you work if needed. Once done, clean up your tests unless added to an existing test suite.";

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
        review_output_json,
        ..
    } = cli;

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

    if let Some(goal) = auto_drive_goal.as_mut() {
        *goal = append_auto_drive_test_suffix(goal);
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

    let review_auto_resolve_requested = review_request.is_some() && config.tui.review_auto_resolve;
    if review_auto_resolve_requested && matches!(config.sandbox_policy, SandboxPolicy::ReadOnly) {
        config.sandbox_policy = SandboxPolicy::new_workspace_write_policy();
        eprintln!(
            "Auto-resolve enabled for /review; upgrading sandbox to workspace-write so fixes can be applied."
        );
    }

    let mut review_outputs: Vec<code_core::protocol::ReviewOutputEvent> = Vec::new();
    let mut final_review_snapshot: Option<code_core::protocol::ReviewSnapshotInfo> = None;
    let mut review_runs: u32 = 0;
    let max_auto_resolve_attempts: u32 = config.auto_drive.auto_resolve_review_attempts.get();
    let mut auto_resolve_state: Option<AutoResolveState> = review_request.as_ref().and_then(|req| {
        if config.tui.review_auto_resolve {
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
    event_processor.print_config_summary(&config, &summary_prompt);
    info!("Codex initialized with event: {session_configured:?}");

    if let Some(goal) = auto_drive_goal {
        return run_auto_drive_session(
            goal,
            images,
            config,
            conversation,
            event_processor,
            last_message_file,
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

    // Send images first, if any.
    if !images.is_empty() {
        let items: Vec<InputItem> = images
            .into_iter()
            .map(|path| InputItem::LocalImage { path })
            .collect();
        let initial_images_event_id = conversation.submit(Op::UserInput { items }).await?;
        info!("Sent images with event ID: {initial_images_event_id}");
        while let Ok(event) = conversation.next_event().await {
            if event.id == initial_images_event_id
                && matches!(
                    event.msg,
                    EventMsg::TaskComplete(TaskCompleteEvent {
                        last_agent_message: _,
                    })
                )
            {
                break;
            }
        }
    }

    // Send the prompt.
    let _initial_prompt_task_id = if let Some(review_request) = review_request.clone() {
        let event_id = conversation.submit(Op::Review { review_request }).await?;
        info!("Sent /review with event ID: {event_id}");
        event_id
    } else {
        let items: Vec<InputItem> = vec![InputItem::Text { text: prompt_to_send }];
        // Fallback for older core protocol: send only user input items.
        let event_id = conversation
            .submit(Op::UserInput { items })
            .await?;
        info!("Sent prompt with event ID: {event_id}");
        event_id
    };

    // Run the loop until the task is complete.
    // Track whether a fatal error was reported by the server so we can
    // exit with a non-zero status for automation-friendly signaling.
    let mut error_seen = false;
    let mut shutdown_requested = false;
    while let Some(event) = rx.recv().await {
        if matches!(event.msg, EventMsg::Error(_)) {
            error_seen = true;
        }

        // Handle review auto-resolve: chain follow-up reviews when enabled.
        match &event.msg {
            EventMsg::ExitedReviewMode(event) => {
                review_runs = review_runs.saturating_add(1);
                if let Some(output) = event.review_output.as_ref() {
                    review_outputs.push(output.clone());
                }
                if let Some(snapshot) = event.snapshot.as_ref() {
                    final_review_snapshot = Some(snapshot.clone());
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
                        }
                    }
                }
            }
            EventMsg::TaskComplete(TaskCompleteEvent { last_agent_message }) => {
                if let Some(state_snapshot) = auto_resolve_state.clone() {
                    match state_snapshot.phase {
                        AutoResolvePhase::PendingFix { review } => {
                            if let Some(state) = auto_resolve_state.as_mut() {
                                state.phase = AutoResolvePhase::AwaitingFix {
                                    review: review.clone(),
                                };
                            }
                            dispatch_auto_fix(&conversation, &review).await?;
                        }
                        AutoResolvePhase::AwaitingFix { review } => {
                            if let Some(state) = auto_resolve_state.as_mut() {
                                state.last_fix_message = last_agent_message.clone();
                                state.phase = AutoResolvePhase::AwaitingJudge {
                                    review: review.clone(),
                                };
                            }
                            dispatch_auto_judge(&conversation, &review, last_agent_message.clone()).await?;
                        }
                        AutoResolvePhase::AwaitingJudge { ref review } => {
                            let review = review.clone();
                            let decision = last_agent_message
                                .as_deref()
                                .and_then(parse_auto_resolve_decision);

                            let Some(decision) = decision else {
                                eprintln!(
                                    "Auto-resolve: expected JSON status but received something else. Stopping automation."
                                );
                                auto_resolve_state = None;
                                continue;
                            };

                            let status = decision.status.to_ascii_lowercase();
                            let rationale = decision.rationale.unwrap_or_default();
                            match status.as_str() {
                                "no_issue" => {
                                    let allowed_reviews = state_snapshot.max_attempts.saturating_add(1);
                                    let limit = state_snapshot.max_attempts;
                                    let attempt_limit_reached = state_snapshot.attempt >= allowed_reviews;
                                    if attempt_limit_reached {
                                        let msg = match limit {
                                            0 => "Auto-resolve: agent reported no remaining issues but automation is disabled (limit 0). Handing control back.".to_string(),
                                            1 => "Auto-resolve: agent reported no remaining issues but reached the single allowed re-review. Handing control back.".to_string(),
                                            _ => format!(
                                                "Auto-resolve: agent reported no remaining issues but reached the review attempt limit ({limit} allowed re-reviews). Handing control back."
                                            ),
                                        };
                                        eprintln!("{msg}");
                                        auto_resolve_state = None;
                                    } else {
                                        if rationale.trim().is_empty() {
                                            eprintln!(
                                                "Auto-resolve: no remaining issues. Running follow-up /review to confirm."
                                            );
                                        } else {
                                            eprintln!(
                                                "Auto-resolve: no remaining issues. {rationale} Running follow-up /review to confirm."
                                            );
                                        }
                                        if let Some(state) = auto_resolve_state.as_mut() {
                                            state.phase = AutoResolvePhase::WaitingForReview;
                                        }
                                        let followup_request = build_followup_review_request(&state_snapshot);
                                        let _ = conversation
                                            .submit(Op::Review {
                                                review_request: followup_request,
                                            })
                                            .await?;
                                    }
                                }
                                "continue_fix" => {
                                    if let Some(state) = auto_resolve_state.as_mut() {
                                        state.phase = AutoResolvePhase::AwaitingFix {
                                            review: review.clone(),
                                        };
                                    }
                                    dispatch_auto_continue(&conversation, &review).await?;
                                }
                                "review_again" => {
                                    let allowed_reviews = state_snapshot.max_attempts.saturating_add(1);
                                    if state_snapshot.attempt >= allowed_reviews {
                                        let limit = state_snapshot.max_attempts;
                                        let msg = match limit {
                                            0 => "Auto-resolve: review-again requested but automation is disabled (limit 0). Stopping.".to_string(),
                                            1 => "Auto-resolve: review-again requested but the attempt limit has been reached (1 allowed re-review). Stopping.".to_string(),
                                            _ => format!(
                                                "Auto-resolve: review-again requested but the attempt limit has been reached ({limit} allowed re-reviews). Stopping."
                                            ),
                                        };
                                        eprintln!("{msg}");
                                        auto_resolve_state = None;
                                    } else {
                                        eprintln!("Auto-resolve: running another /review pass.");
                                        if let Some(state) = auto_resolve_state.as_mut() {
                                            state.phase = AutoResolvePhase::WaitingForReview;
                                        }
                                        let followup_request = build_followup_review_request(&state_snapshot);
                                        let _ = conversation
                                            .submit(Op::Review {
                                                review_request: followup_request,
                                            })
                                            .await?;
                                    }
                                }
                                other => {
                                    eprintln!(
                                        "Auto-resolve: unexpected status '{other}'. Stopping automation."
                                    );
                                    auto_resolve_state = None;
                                }
                            }
                        }
                        AutoResolvePhase::WaitingForReview => {
                            // Task complete from a review; handled in ExitedReviewMode.
                        }
                    }
                }

                if auto_resolve_state.is_none() && !shutdown_requested {
                    shutdown_requested = true;
                    conversation.submit(Op::Shutdown).await?;
                }
            }
            _ => {}
        }

        let shutdown: CodexStatus = event_processor.process_event(event);
        match shutdown {
            CodexStatus::Running => continue,
            CodexStatus::InitiateShutdown => {
                if !shutdown_requested {
                    shutdown_requested = true;
                    conversation.submit(Op::Shutdown).await?;
                }
            }
            CodexStatus::Shutdown => {
                break;
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
) -> anyhow::Result<()> {
    let mut final_last_message: Option<String> = None;
    let mut error_seen = false;

    if !images.is_empty() {
        let items: Vec<InputItem> = images
            .into_iter()
            .map(|path| InputItem::LocalImage { path })
            .collect();
        let initial_images_event_id = conversation.submit(Op::UserInput { items }).await?;
        while let Ok(event) = conversation.next_event().await {
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

    let mut auto_config = config.clone();
    auto_config.model = config.auto_drive.model.trim().to_string();
    if auto_config.model.is_empty() {
        auto_config.model = MODEL_SLUG.to_string();
    }
    auto_config.model_reasoning_effort = config.auto_drive.model_reasoning_effort;

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

    while let Some(event) = auto_rx.recv().await {
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
                history.replace_all(conversation);
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
                        } = submit_and_wait(&conversation, event_processor.as_mut(), prompt_text.to_string()).await?;
                        error_seen |= turn_error;
                        if let Some(text) = last_agent_message {
                            history.append_raw(&[make_assistant_message(text.clone())]);
                            final_last_message = Some(text);
                        }
                        let _ = handle
                            .send(AutoCoordinatorCommand::UpdateConversation(history.raw_snapshot()));
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
                } = submit_and_wait(&conversation, event_processor.as_mut(), prompt_text).await?;
                error_seen |= turn_error;
                if let Some(text) = last_agent_message {
                    history.append_raw(&[make_assistant_message(text.clone())]);
                    final_last_message = Some(text);
                }

                if handle
                    .send(AutoCoordinatorCommand::UpdateConversation(history.raw_snapshot()))
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
    let _ = conversation.submit(Op::Shutdown).await;
    while let Ok(event) = conversation.next_event().await {
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
        std::process::exit(1);
    }

    Ok(())
}

fn append_auto_drive_test_suffix(goal: &str) -> String {
    let trimmed_goal = goal.trim();
    if trimmed_goal.is_empty() {
        return AUTO_DRIVE_TEST_SUFFIX.to_string();
    }

    format!("{trimmed_goal}\n\n{AUTO_DRIVE_TEST_SUFFIX}")
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
    let _ = conversation.submit(Op::UserInput { items }).await?;
    Ok(())
}

async fn dispatch_auto_judge(
    conversation: &Arc<CodexConversation>,
    review: &code_core::protocol::ReviewOutputEvent,
    fix_message: Option<String>,
) -> anyhow::Result<()> {
    let judge_prompt = build_judge_prompt(review, fix_message);
    let items: Vec<InputItem> = vec![InputItem::Text { text: judge_prompt }];
    let _ = conversation.submit(Op::UserInput { items }).await?;
    Ok(())
}

async fn dispatch_auto_continue(
    conversation: &Arc<CodexConversation>,
    review: &code_core::protocol::ReviewOutputEvent,
) -> anyhow::Result<()> {
    let continue_prompt = build_continue_prompt(review);
    let items: Vec<InputItem> = vec![InputItem::Text { text: continue_prompt }];
    let _ = conversation.submit(Op::UserInput { items }).await?;
    Ok(())
}

fn build_followup_review_request(state: &AutoResolveState) -> ReviewRequest {
    let mut prompt = state.prompt.trim_end().to_string();
    if let Some(idx) = prompt.find(AUTO_RESOLVE_REVIEW_FOLLOWUP) {
        prompt = prompt[..idx].trim_end().to_string();
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

    ReviewRequest {
        prompt,
        user_facing_hint: state.hint.clone(),
        metadata: state.metadata.clone(),
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

#[derive(serde::Deserialize)]
struct AutoResolveDecision {
    status: String,
    #[serde(default)]
    rationale: Option<String>,
}

fn parse_auto_resolve_decision(raw: &str) -> Option<AutoResolveDecision> {
    if let Ok(decision) = serde_json::from_str::<AutoResolveDecision>(raw) {
        return Some(decision);
    }
    if let Some(start) = raw.find('{') {
        if let Some(end) = raw.rfind('}') {
            let slice = &raw[start..=end];
            if let Ok(decision) = serde_json::from_str::<AutoResolveDecision>(slice) {
                return Some(decision);
            }
        }
    }
    if let Some(json_start) = raw.find("```") {
        if let Some(rest) = raw[json_start + 3..].split_once("```") {
            let candidate = rest.0.trim_start_matches("json").trim();
            if let Ok(decision) = serde_json::from_str::<AutoResolveDecision>(candidate) {
                return Some(decision);
            }
        }
    }
    None
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

fn build_judge_prompt(review: &code_core::protocol::ReviewOutputEvent, fix_message: Option<String>) -> String {
    let summary = format_review_findings(review);
    let raw_json = serde_json::to_string_pretty(review).unwrap_or_else(|_| "{}".to_string());
    let mut preface = String::from(
        "You are evaluating whether the latest fixes resolved the findings from `/review`. Respond with a strict JSON object containing `status` and optional `rationale`. Valid `status` values: `review_again`, `no_issue`, `continue_fix`. Do not include any additional text before or after the JSON."
    );
    if !summary.is_empty() {
        preface.push_str("\n\nOriginal findings:\n");
        preface.push_str(&summary);
    }
    preface.push_str("\n\nFull review JSON (includes file paths and line ranges):\n");
    preface.push_str(&raw_json);
    if let Some(fix) = fix_message.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        preface.push_str("\n\nLatest agent response:\n");
        preface.push_str(fix);
    }
    preface.push_str("\n\nReturn JSON: {\"status\": \"...\", \"rationale\": \"optional explanation\"}.");
    preface
}

fn build_continue_prompt(review: &code_core::protocol::ReviewOutputEvent) -> String {
    let summary = format_review_findings(review);
    let raw_json = serde_json::to_string_pretty(review).unwrap_or_else(|_| "{}".to_string());
    let mut preface = String::from(
        "The previous status check indicated more work is required on the review findings. Continue addressing the remaining issues before responding."
    );
    if !summary.is_empty() {
        preface.push_str("\n\nOutstanding findings:\n");
        preface.push_str(&summary);
    }
    preface.push_str("\n\nFull review JSON (includes file paths and line ranges):\n");
    preface.push_str(&raw_json);
    preface
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
    prompt_text: String,
) -> anyhow::Result<TurnResult> {
    let mut error_seen = false;

    let submit_id = conversation
        .submit(Op::UserInput {
            items: vec![InputItem::Text { text: prompt_text }],
        })
        .await?;

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                let _ = conversation.submit(Op::Interrupt).await;
                return Err(anyhow::anyhow!("Interrupted"));
            }
            res = conversation.next_event() => {
                let event = res?;
                let event_id = event.id.clone();
                if matches!(event.msg, EventMsg::Error(_)) {
                    error_seen = true;
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
