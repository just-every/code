use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use code_core::config::Config;
use code_core::config::ConfigOverrides;
use code_core::config_types::ReasoningEffort;
use code_core::debug_logger::DebugLogger;
use code_core::protocol::SandboxPolicy;
use code_core::{AuthManager, ModelClient, Prompt, ResponseEvent, TextFormat};
use code_protocol::models::{ContentItem, ResponseItem};
use futures::StreamExt;
use serde::Deserialize;
use serde_json::{self, json, Value};
use tracing::debug;
use uuid::Uuid;

use crate::app_event::{
    AppEvent,
    Redacted,
    TerminalAfter,
    TerminalCommandGate,
    TerminalRunController,
    TerminalRunEvent,
};
use crate::app_event_sender::AppEventSender;

const MAX_OUTPUT_CHARS: usize = 8_000;
const MAX_STEPS: usize = 6;

enum GuidedTerminalMode {
    AgentInstall {
        agent_name: String,
        default_command: String,
        selected_index: usize,
    },
    Prompt { user_prompt: String },
    DirectCommand { command: String },
    #[cfg_attr(test, allow(dead_code))]
    Upgrade {
        initial_command: String,
        latest_version: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
struct InstallDecision {
    finish_status: String,
    message: String,
    #[serde(default)]
    command: Option<String>,
}

pub(super) fn start_agent_install_session(
    app_event_tx: AppEventSender,
    terminal_id: u64,
    agent_name: String,
    default_command: String,
    cwd: Option<String>,
    controller: TerminalRunController,
    controller_rx: Receiver<TerminalRunEvent>,
    selected_index: usize,
    debug_enabled: bool,
) {
    start_guided_terminal_session(
        app_event_tx,
        terminal_id,
        GuidedTerminalMode::AgentInstall {
            agent_name,
            default_command,
            selected_index,
        },
        cwd,
        controller,
        controller_rx,
        None,
        debug_enabled,
    );
}

pub(super) fn start_prompt_terminal_session(
    app_event_tx: AppEventSender,
    terminal_id: u64,
    user_prompt: String,
    cwd: Option<String>,
    controller: TerminalRunController,
    controller_rx: Receiver<TerminalRunEvent>,
    debug_enabled: bool,
) {
    start_guided_terminal_session(
        app_event_tx,
        terminal_id,
        GuidedTerminalMode::Prompt { user_prompt },
        cwd,
        controller,
        controller_rx,
        None,
        debug_enabled,
    );
}

pub(super) fn start_direct_terminal_session(
    app_event_tx: AppEventSender,
    terminal_id: u64,
    command: String,
    cwd: Option<String>,
    controller: TerminalRunController,
    controller_rx: Receiver<TerminalRunEvent>,
    debug_enabled: bool,
) {
    start_guided_terminal_session(
        app_event_tx,
        terminal_id,
        GuidedTerminalMode::DirectCommand { command },
        cwd,
        controller,
        controller_rx,
        None,
        debug_enabled,
    );
}

pub(super) fn start_upgrade_terminal_session(
    app_event_tx: AppEventSender,
    terminal_id: u64,
    initial_command: String,
    latest_version: Option<String>,
    cwd: Option<String>,
    controller: TerminalRunController,
    controller_rx: Receiver<TerminalRunEvent>,
    config: Config,
    debug_enabled: bool,
) {
    start_guided_terminal_session(
        app_event_tx,
        terminal_id,
        GuidedTerminalMode::Upgrade {
            initial_command,
            latest_version,
        },
        cwd,
        controller,
        controller_rx,
        Some(config),
        debug_enabled,
    );
}

fn start_guided_terminal_session(
    app_event_tx: AppEventSender,
    terminal_id: u64,
    mode: GuidedTerminalMode,
    cwd: Option<String>,
    controller: TerminalRunController,
    controller_rx: Receiver<TerminalRunEvent>,
    config: Option<Config>,
    debug_enabled: bool,
) {
    let fail_tx = app_event_tx.clone();
    if let Err(err) = std::thread::Builder::new()
        .name("guided-terminal-session".to_string())
        .spawn(move || {
        let runtime = match tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(err) => {
                let helper = match &mode {
                    GuidedTerminalMode::AgentInstall { .. } => "Install helper",
                    GuidedTerminalMode::Prompt { .. }
                    | GuidedTerminalMode::DirectCommand { .. } => "Terminal helper",
                    GuidedTerminalMode::Upgrade { .. } => "Upgrade helper",
                };
                let msg = format!("Failed to start {helper} runtime: {err}");
                app_event_tx.send(AppEvent::TerminalChunk {
                    id: terminal_id,
                    chunk: format!("{msg}\n").into_bytes(),
                    _is_stderr: true,
                });
                app_event_tx.send(AppEvent::TerminalUpdateMessage {
                    id: terminal_id,
                    message: msg,
                });
                return;
            }
        };

        let mut controller_rx = controller_rx;
        if let Err(err) = run_guided_loop(
            &runtime,
            &app_event_tx,
            terminal_id,
            &mode,
            cwd.as_deref(),
            controller,
            &mut controller_rx,
            config,
            debug_enabled,
        ) {
            let helper = match &mode {
                GuidedTerminalMode::AgentInstall { .. } => "Install helper",
                GuidedTerminalMode::Prompt { .. }
                | GuidedTerminalMode::DirectCommand { .. } => "Terminal helper",
                GuidedTerminalMode::Upgrade { .. } => "Upgrade helper",
            };
            let msg = if debug_enabled {
                format!("{helper} error: {err:#}")
            } else {
                format!("{helper} error: {err}")
            };
            app_event_tx.send(AppEvent::TerminalChunk {
                id: terminal_id,
                chunk: format!("{msg}\n").into_bytes(),
                _is_stderr: true,
            });
            app_event_tx.send(AppEvent::TerminalUpdateMessage {
                id: terminal_id,
                message: msg,
            });
        }
        })
    {
        let msg = format!("Failed to start guided terminal helper: {err}");
        fail_tx.send(AppEvent::TerminalChunk {
            id: terminal_id,
            chunk: format!("{msg}\n").into_bytes(),
            _is_stderr: true,
        });
        fail_tx.send(AppEvent::TerminalUpdateMessage { id: terminal_id, message: msg });
    }
}

fn run_guided_loop(
    runtime: &tokio::runtime::Runtime,
    app_event_tx: &AppEventSender,
    terminal_id: u64,
    mode: &GuidedTerminalMode,
    cwd: Option<&str>,
    controller: TerminalRunController,
    controller_rx: &mut Receiver<TerminalRunEvent>,
    provided_config: Option<Config>,
    debug_enabled: bool,
) -> Result<()> {
    let cfg = match provided_config {
        Some(cfg) => cfg,
        None => Config::load_with_cli_overrides(vec![], ConfigOverrides::default())
            .context("loading config")?,
    };
    let preferred_auth = if cfg.using_chatgpt_auth {
        code_protocol::mcp_protocol::AuthMode::ChatGPT
    } else {
        code_protocol::mcp_protocol::AuthMode::ApiKey
    };
    let auth_mgr = AuthManager::shared_with_mode_and_originator(
        cfg.code_home.clone(),
        preferred_auth,
        cfg.responses_originator_header.clone(),
    );
    let client = ModelClient::new(
        Arc::new(cfg.clone()),
        Some(auth_mgr),
        None,
        cfg.model_provider.clone(),
        ReasoningEffort::Low,
        cfg.model_reasoning_summary,
        cfg.model_text_verbosity,
        Uuid::new_v4(),
        Arc::new(Mutex::new(
            DebugLogger::new(debug_enabled)
                .unwrap_or_else(|_| DebugLogger::new(false).expect("debug logger")),
        )),
    );

    let platform = std::env::consts::OS;
    let sandbox = if matches!(cfg.sandbox_policy, SandboxPolicy::DangerFullAccess) {
        "full access"
    } else {
        "limited sandbox"
    };
    let cwd_text = cwd.unwrap_or("unknown");

    let (helper_label, developer_intro, initial_user, schema_name) = match mode {
        GuidedTerminalMode::AgentInstall {
            agent_name,
            default_command,
            ..
        } => (
            "Install helper",
            format!(
                "You are coordinating shell commands to install the agent named \"{agent_name}\"."
            ),
            format!(
                "Install target: {agent_name}.\nPlatform: {platform}.\nSandbox: {sandbox}.\nWorking directory: {cwd_text}.\nSuggested starting command: {default_command}.\nPlease propose the first command to run."
            ),
            "agent_install_flow",
        ),
        GuidedTerminalMode::Prompt { user_prompt } => (
            "Terminal helper",
            format!(
                "You are coordinating shell commands to satisfy the user's request:\n\"{user_prompt}\"."
            ),
            format!(
                "User request: {user_prompt}.\nPlatform: {platform}.\nSandbox: {sandbox}.\nWorking directory: {cwd_text}.\nPlease propose the first command to run."
            ),
            "guided_terminal_flow",
        ),
        GuidedTerminalMode::DirectCommand { command } => (
            "Terminal helper",
            format!(
                "You are assisting the user with shell commands. They manually executed the first command `{command}`."
            ),
            format!(
                "Initial user command: {command}.\nPlatform: {platform}.\nSandbox: {sandbox}.\nWorking directory: {cwd_text}.\nReview the provided command output and suggest any follow-up command if helpful."
            ),
            "direct_terminal_flow",
        ),
        GuidedTerminalMode::Upgrade {
            initial_command,
            latest_version,
        } => {
            let latest = latest_version
                .as_ref()
                .map(String::as_str)
                .unwrap_or("unknown");
            (
                "Upgrade helper",
                format!(
                    "You are helping the user upgrade Code to the latest available version. The preferred upgrade command is `{initial_command}`. Prioritize resolving permission prompts (including sudo password requests) and confirm the upgrade succeeds."
                ),
                format!(
                    "Upgrade target: Code (latest version: {latest}).\nPrimary command: {initial_command}.\nPlatform: {platform}.\nSandbox: {sandbox}.\nWorking directory: {cwd_text}.\nRun the command, diagnose any failures (especially permissions), and guide the user until the upgrade completes."
                ),
                "upgrade_terminal_flow",
            )
        }
    };

    if debug_enabled {
        match mode {
            GuidedTerminalMode::AgentInstall {
                agent_name,
                default_command,
                ..
            } => {
                debug!(
                    "[{}] Starting guided install session: agent={} default_command={} platform={} sandbox={} cwd={}",
                    helper_label,
                    agent_name,
                    default_command,
                    platform,
                    sandbox,
                    cwd_text,
                );
            }
            GuidedTerminalMode::Prompt { user_prompt } => {
                debug!(
                    "[{}] Starting guided terminal session: prompt={} platform={} sandbox={} cwd={}",
                    helper_label,
                    user_prompt,
                    platform,
                    sandbox,
                    cwd_text,
                );
            }
            GuidedTerminalMode::DirectCommand { command } => {
                debug!(
                    "[{}] Starting direct terminal session: command={} platform={} sandbox={} cwd={}",
                    helper_label,
                    command,
                    platform,
                    sandbox,
                    cwd_text,
                );
            }
            GuidedTerminalMode::Upgrade {
                initial_command,
                latest_version,
            } => {
                debug!(
                    "[{}] Starting upgrade session: command={} latest_version={:?} platform={} sandbox={} cwd={}",
                    helper_label,
                    initial_command,
                    latest_version,
                    platform,
                    sandbox,
                    cwd_text,
                );
            }
        }
    }

    let developer = format!(
        "{developer_intro}

    Rules:
    - `finish_status`: one of `continue`, `finish_success`, or `finish_failed`.
      * Use `continue` when another shell command is required.
      * Use `finish_success` when the task completed successfully.
      * Use `finish_failed` when the task cannot continue or needs manual intervention.
    - `message`: short status (<= 160 characters) describing what happened or what to do next.
    - `command`: exact shell command to run next. Supply a single non-interactive command when `finish_status` is `continue`; set to null otherwise. Do not repeat the user's wording—return a valid executable shell command.
    - The provided command will be executed and its output returned to you. Prefer non-destructive diagnostics (search, list, install alternative package) when handling errors.
    - Always inspect the latest command output before choosing the next action. Suggest follow-up steps (e.g. alternate packages, additional instructions) when a command fails.
    - Respect the detected platform: use Homebrew on macOS, apt/dnf/pacman on Linux, winget/choco/powershell on Windows.",
    );

    let schema = json!({
        "type": "object",
        "properties": {
            "finish_status": {
                "type": "string",
                "enum": ["continue", "finish_success", "finish_failed"],
                "description": "Use 'continue' to supply another command, 'finish_success' when installation completed, or 'finish_failed' when installation cannot proceed."
            },
            "message": { "type": "string", "minLength": 1, "maxLength": 160 },
            "command": {
                "type": ["string", "null"],
                "minLength": 1,
                "description": "Shell command to execute next. Must be null when finish_status is not 'continue'.",
            }
        },
        "required": ["finish_status", "message", "command"],
        "additionalProperties": false
    });

    let developer_msg = make_message("developer", developer);
    let mut conversation: Vec<ResponseItem> = Vec::new();
    conversation.push(make_message("user", initial_user));

    let mut steps = match mode {
        GuidedTerminalMode::DirectCommand { .. } | GuidedTerminalMode::Upgrade { .. } => 1,
        _ => 0,
    };

    let sandbox_restricted = !matches!(cfg.sandbox_policy, SandboxPolicy::DangerFullAccess);

    if let GuidedTerminalMode::DirectCommand { command } = mode {
        let wrapped = wrap_command(command);
        if wrapped.is_empty() {
            app_event_tx.send(AppEvent::TerminalChunk {
                id: terminal_id,
                chunk: b"Unable to build shell command for execution.\n".to_vec(),
                _is_stderr: true,
            });
            app_event_tx.send(AppEvent::TerminalUpdateMessage {
                id: terminal_id,
                message: "Command could not be constructed.".to_string(),
            });
            return Ok(());
        }
        app_event_tx.send(AppEvent::TerminalRunCommand {
            id: terminal_id,
            command: wrapped,
            command_display: command.clone(),
            controller: Some(controller.clone()),
        });

        let Some((output, exit_code)) = collect_command_output(controller_rx)
            .context("collecting initial command output")?
        else {
            if debug_enabled {
                debug!("[Terminal helper] Initial command cancelled by user");
            }
            return Ok(());
        };

        let truncated = tail_chars(&output, MAX_OUTPUT_CHARS);
        let summary = format!(
            "Command: {command}\nExit code: {}\nOutput (last {} chars):\n{}",
            exit_code
                .map(|c| c.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            truncated.chars().count(),
            truncated
        );
        conversation.push(make_message("user", summary));
        app_event_tx.send(AppEvent::TerminalSetAssistantMessage {
            id: terminal_id,
            message: "Analyzing output…".to_string(),
        });
    } else if let GuidedTerminalMode::Upgrade { initial_command, .. } = mode {
        if sandbox_restricted {
            let notice = format!(
                "Automatic upgrades require Full Access. Run `{initial_command}` manually in your own shell, then re-run `/update` once it finishes.\n"
            );
            app_event_tx.send(AppEvent::TerminalChunk {
                id: terminal_id,
                chunk: notice.clone().into_bytes(),
                _is_stderr: true,
            });
            app_event_tx.send(AppEvent::TerminalUpdateMessage {
                id: terminal_id,
                message: notice.trim().to_string(),
            });
            app_event_tx.send(AppEvent::TerminalSetAssistantMessage {
                id: terminal_id,
                message: "Waiting for manual upgrade…".to_string(),
            });
            conversation.push(make_message(
                "user",
                "Automatic execution blocked by sandbox. Await user's manual upgrade before continuing.".to_string(),
            ));
            app_event_tx.send(AppEvent::TerminalExit {
                id: terminal_id,
                exit_code: None,
                _duration: Duration::from_millis(0),
            });
            return Ok(());
        }

        let wrapped = wrap_command(initial_command);
        if wrapped.is_empty() {
            app_event_tx.send(AppEvent::TerminalChunk {
                id: terminal_id,
                chunk: b"Unable to build upgrade command for execution.\n".to_vec(),
                _is_stderr: true,
            });
            app_event_tx.send(AppEvent::TerminalUpdateMessage {
                id: terminal_id,
                message: "Upgrade command could not be constructed.".to_string(),
            });
            return Ok(());
        }
        app_event_tx.send(AppEvent::TerminalRunCommand {
            id: terminal_id,
            command: wrapped,
            command_display: initial_command.clone(),
            controller: Some(controller.clone()),
        });

        let Some((output, exit_code)) = collect_command_output(controller_rx)
            .context("collecting upgrade command output")?
        else {
            if debug_enabled {
                debug!("[Upgrade helper] Initial command cancelled by user");
            }
            return Ok(());
        };

        let truncated = tail_chars(&output, MAX_OUTPUT_CHARS);
        let summary = format!(
            "Command: {initial_command}\nExit code: {}\nOutput (last {} chars):\n{}",
            exit_code
                .map(|c| c.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            truncated.chars().count(),
            truncated
        );
        conversation.push(make_message("user", summary));
        app_event_tx.send(AppEvent::TerminalSetAssistantMessage {
            id: terminal_id,
            message: "Analyzing output…".to_string(),
        });
    }

    loop {
        steps += 1;
        if steps > MAX_STEPS {
            return Err(anyhow!("hit step limit without completing guided session"));
        }

        if debug_enabled {
            debug!("[{}] Requesting next command (step={})", helper_label, steps);
        }
        if steps == 1 {
                app_event_tx.send(AppEvent::TerminalSetAssistantMessage {
                    id: terminal_id,
                    message: "Starting analysis…".to_string(),
                });
        }

        let mut prompt = Prompt::default();
        prompt.input.push(developer_msg.clone());
        prompt.input.extend(conversation.clone());
        prompt.store = true;
        prompt.text_format = Some(TextFormat {
            r#type: "json_schema".to_string(),
            name: Some(schema_name.to_string()),
            strict: Some(true),
            schema: Some(schema.clone()),
        });
        prompt.set_log_tag(format!("guided_terminal/{schema_name}"));

        let raw = request_decision(runtime, &client, &prompt).context("model stream failed")?;
        let (decision, raw_value) = parse_decision(&raw)?;
        if debug_enabled {
            debug!(
                "[{}] Model decision: message={:?} command={:?} raw={}",
                helper_label,
                decision.message,
                decision.command.as_deref().unwrap_or("<none>"),
                raw_value,
            );
        }
        conversation.push(make_message("assistant", raw.clone()));

        app_event_tx.send(AppEvent::TerminalSetAssistantMessage {
            id: terminal_id,
            message: decision.message.clone(),
        });

        let finish_status = decision.finish_status.as_str();
        match finish_status {
            "continue" => {
                let suggested_raw = decision
                    .command
                    .as_deref()
                    .map(str::trim)
                    .filter(|c| !c.is_empty())
                    .ok_or_else(|| anyhow!("model response missing command for next step"))?;
                let suggested = simplify_command(suggested_raw).to_string();

                let require_confirmation = match mode {
                    GuidedTerminalMode::AgentInstall { .. } => steps > 1,
                    GuidedTerminalMode::Prompt { .. } => steps > 1,
                    GuidedTerminalMode::DirectCommand { .. } | GuidedTerminalMode::Upgrade { .. } => {
                        true
                    }
                };
                let final_command = if require_confirmation {
                    let (gate_tx, gate_rx) = channel();
                    app_event_tx.send(AppEvent::TerminalAwaitCommand {
                        id: terminal_id,
                        suggestion: suggested.clone(),
                        ack: Redacted(gate_tx),
                    });
                    match gate_rx.recv() {
                        Ok(TerminalCommandGate::Run(cmd)) => cmd,
                        Ok(TerminalCommandGate::Cancel) | Err(_) => {
                            if debug_enabled {
                                debug!("[{}] Command run cancelled by user", helper_label);
                            }
                            break;
                        }
                    }
                } else {
                    suggested
                };

                let final_command = final_command.trim().to_string();
                if final_command.is_empty() {
                    return Err(anyhow!("next command was empty after confirmation"));
                }

                app_event_tx.send(AppEvent::TerminalRunCommand {
                    id: terminal_id,
                    command: wrap_command(&final_command),
                    command_display: final_command.clone(),
                    controller: Some(controller.clone()),
                });

                let Some((output, exit_code)) = collect_command_output(controller_rx)
                    .context("collecting command output")?
                else {
                    if debug_enabled {
                        debug!("[{}] Command collection cancelled by user", helper_label);
                    }
                    break;
                };
                if debug_enabled {
                    debug!(
                        "[{}] Command finished: command={} exit_code={:?}",
                        helper_label,
                        final_command,
                        exit_code,
                    );
                }

                let truncated = tail_chars(&output, MAX_OUTPUT_CHARS);
                let summary = format!(
                    "Command: {final_command}\nExit code: {}\nOutput (last {} chars):\n{}",
                    exit_code
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| "unknown".to_string()),
                    truncated.chars().count(),
                    truncated
                );
                conversation.push(make_message("user", summary));

                app_event_tx.send(AppEvent::TerminalSetAssistantMessage {
                    id: terminal_id,
                    message: "Analyzing output…".to_string(),
                });
            }
            "finish_success" => {
                if decision
                    .command
                    .as_deref()
                    .map(str::trim)
                    .filter(|c| !c.is_empty())
                    .is_some()
                {
                    return Err(anyhow!("finish_success must set command to null"));
                }
                if let GuidedTerminalMode::AgentInstall {
                    selected_index,
                    ..
                } = mode
                {
                    app_event_tx.send(AppEvent::TerminalForceClose { id: terminal_id });
                    app_event_tx.send(AppEvent::TerminalAfter(
                        TerminalAfter::RefreshAgentsAndClose {
                            selected_index: *selected_index,
                        },
                    ));
                }
                break;
            }
            "finish_failed" => {
                if decision
                    .command
                    .as_deref()
                    .map(str::trim)
                    .filter(|c| !c.is_empty())
                    .is_some()
                {
                    return Err(anyhow!("finish_failed must set command to null"));
                }
                break;
            }
            other => {
                return Err(anyhow!("unexpected finish_status '{other}'"));
            }
        }
    }

    Ok(())
}


fn request_decision(
    runtime: &tokio::runtime::Runtime,
    client: &ModelClient,
    prompt: &Prompt,
) -> Result<String> {
    runtime.block_on(async {
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
                Err(err) => return Err(anyhow!("model stream error: {err}")),
                _ => {}
            }
        }
        Ok(out)
    })
}

fn parse_decision(raw: &str) -> Result<(InstallDecision, Value)> {
    let value: Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => {
            let Some(json_blob) = extract_first_json_object(raw) else {
                return Err(anyhow!("model response was not valid JSON"));
            };
            serde_json::from_str(&json_blob).context("parsing JSON from model output")?
        }
    };
    let decision: InstallDecision = serde_json::from_value(value.clone())
        .context("decoding install decision")?;
    Ok((decision, value))
}

fn collect_command_output(
    controller_rx: &Receiver<TerminalRunEvent>,
) -> Result<Option<(String, Option<i32>)>> {
    let mut buf: Vec<u8> = Vec::new();
    let exit_code = loop {
        match controller_rx.recv() {
            Ok(TerminalRunEvent::Chunk { data, _is_stderr: _ }) => buf.extend_from_slice(&data),
            Ok(TerminalRunEvent::Exit { exit_code, _duration: _ }) => break exit_code,
            Err(_) => return Ok(None),
        }
    };
    let text = String::from_utf8_lossy(&buf).to_string();
    Ok(Some((text, exit_code)))
}

pub(crate) fn simplify_command(raw: &str) -> &str {
    let trimmed = raw.trim();
    let lower = trimmed.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("bash -lc ") {
        let original = &trimmed[trimmed.len() - rest.len()..];
        return original.trim_matches(|c| c == '\'' || c == '"').trim();
    }
    trimmed
}

pub(crate) fn wrap_command(raw: &str) -> Vec<String> {
    let simplified = simplify_command(raw);
    if simplified.is_empty() {
        return Vec::new();
    }
    if cfg!(target_os = "windows") {
        vec![
            "powershell.exe".to_string(),
            "-NoProfile".to_string(),
            "-ExecutionPolicy".to_string(),
            "Bypass".to_string(),
            "-Command".to_string(),
            simplified.to_string(),
        ]
    } else {
        vec!["/bin/bash".to_string(), "-lc".to_string(), simplified.to_string()]
    }
}

fn tail_chars(text: &str, max_chars: usize) -> String {
    let total = text.chars().count();
    if total <= max_chars {
        return text.to_string();
    }
    let mut idx = text.len();
    let mut count = 0usize;
    for (i, _) in text.char_indices().rev() {
        count += 1;
        if count >= max_chars {
            idx = i;
            break;
        }
    }
    text[idx..].to_string()
}

fn make_message(role: &str, text: String) -> ResponseItem {
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

fn extract_first_json_object(input: &str) -> Option<String> {
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
