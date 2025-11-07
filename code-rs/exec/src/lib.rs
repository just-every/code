mod cli;
mod event_processor;
mod event_processor_with_human_output;
mod event_processor_with_json_output;

pub use cli::Cli;
use code_core::AuthManager;
use code_core::BUILT_IN_OSS_MODEL_PROVIDER_ID;
use code_core::ConversationManager;
use code_core::NewConversation;
use code_core::config::set_default_originator;
use code_core::config::Config;
use code_core::config::ConfigOverrides;
use code_core::git_info::get_git_repo_root;
use code_core::protocol::AskForApproval;
use code_core::protocol::Event;
use code_core::protocol::EventMsg;
use code_core::protocol::InputItem;
use code_core::protocol::Op;
use code_core::protocol::TaskCompleteEvent;
use code_protocol::protocol::SessionSource;
use code_ollama::DEFAULT_OSS_MODEL;
use code_protocol::config_types::SandboxMode;
use event_processor_with_human_output::EventProcessorWithHumanOutput;
use event_processor_with_json_output::EventProcessorWithJsonOutput;
use serde_json::Value;
use std::io::IsTerminal;
use std::io::Read;
use std::path::PathBuf;
use supports_color::Stream;
use tracing::debug;
use tracing::error;
use tracing::info;
use tracing_subscriber::EnvFilter;

use crate::cli::Command as ExecCommand;
use crate::event_processor::CodexStatus;
use crate::event_processor::EventProcessor;
use code_core::find_conversation_path_by_id_str;

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

    let config = Config::load_with_cli_overrides(cli_kv_overrides, overrides)?;
    let mut event_processor: Box<dyn EventProcessor> = if json_mode {
        Box::new(EventProcessorWithJsonOutput::new(last_message_file.clone()))
    } else {
        Box::new(EventProcessorWithHumanOutput::create_with_ansi(
            stdout_with_ansi,
            &config,
            last_message_file.clone(),
        ))
    };

    if oss {
        code_ollama::ensure_oss_ready(&config)
            .await
            .map_err(|e| anyhow::anyhow!("OSS setup failed: {e}"))?;
    }

    // Print the effective configuration and prompt so users can see what Codex
    // is using.
    event_processor.print_config_summary(&config, &prompt);

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
    event_processor.print_config_summary(&config, &prompt);
    info!("Codex initialized with event: {session_configured:?}");

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
    let items: Vec<InputItem> = vec![InputItem::Text { text: prompt }];
    // Fallback for older core protocol: send only user input items.
    let initial_prompt_task_id = conversation
        .submit(Op::UserInput { items })
        .await?;
    info!("Sent prompt with event ID: {initial_prompt_task_id}");

    // Run the loop until the task is complete.
    // Track whether a fatal error was reported by the server so we can
    // exit with a non-zero status for automation-friendly signaling.
    let mut error_seen = false;
    while let Some(event) = rx.recv().await {
        if matches!(event.msg, EventMsg::Error(_)) {
            error_seen = true;
        }
        let shutdown: CodexStatus = event_processor.process_event(event);
        match shutdown {
            CodexStatus::Running => continue,
            CodexStatus::InitiateShutdown => {
                conversation.submit(Op::Shutdown).await?;
            }
            CodexStatus::Shutdown => {
                break;
            }
        }
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
    if args.last {
        match code_core::RolloutRecorder::list_conversations(&config.code_home, 1, None, &[]).await {
            Ok(page) => Ok(page.items.first().map(|it| it.path.clone())),
            Err(e) => {
                error!("Error listing conversations: {e}");
                Ok(None)
            }
        }
    } else if let Some(id_str) = args.session_id.as_deref() {
        let path = find_conversation_path_by_id_str(&config.code_home, id_str).await?;
        Ok(path)
    } else {
        Ok(None)
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
