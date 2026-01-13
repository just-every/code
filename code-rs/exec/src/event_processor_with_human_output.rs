use code_common::elapsed::format_duration;
use code_common::elapsed::format_elapsed;
use code_core::config::Config;
use code_core::plan_tool::UpdatePlanArgs;
use code_core::protocol::AgentMessageDeltaEvent;
use code_core::protocol::AgentMessageEvent;
use code_core::protocol::AgentReasoningDeltaEvent;
use code_core::protocol::AgentReasoningRawContentDeltaEvent;
use code_core::protocol::AgentReasoningRawContentEvent;
use code_core::protocol::BackgroundEventEvent;
use code_core::protocol::ErrorEvent;
use code_core::protocol::Event;
use code_core::protocol::EventMsg;
use code_core::protocol::ExecCommandBeginEvent;
use code_core::protocol::ExecCommandEndEvent;
use code_core::protocol::FileChange;
use code_core::protocol::McpInvocation;
use code_core::protocol::McpToolCallBeginEvent;
use code_core::protocol::McpToolCallEndEvent;
use code_core::protocol::PatchApplyBeginEvent;
use code_core::protocol::PatchApplyEndEvent;
use code_core::protocol::SessionConfiguredEvent;
use code_core::protocol::TaskCompleteEvent;
use code_protocol::protocol::TurnAbortReason;
use code_core::protocol::TurnDiffEvent;
use code_core::protocol::WebSearchBeginEvent;
use code_core::protocol::WebSearchCompleteEvent;
use code_protocol::num_format::format_with_separators;
use owo_colors::OwoColorize;
use owo_colors::Style;
use shlex::try_join;
use std::collections::HashMap;
use std::collections::HashSet;
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use crate::event_processor::CodexStatus;
use crate::event_processor::EventProcessor;
use crate::event_processor::handle_last_message;
use code_common::create_config_summary_entries;

/// This should be configurable. When used in CI, users may not want to impose
/// a limit so they can see the full transcript.
const MAX_OUTPUT_LINES_FOR_EXEC_TOOL_CALL: usize = 20;
pub(crate) struct EventProcessorWithHumanOutput {
    call_id_to_command: HashMap<String, ExecCommandBegin>,
    call_id_to_patch: HashMap<String, PatchApplyBegin>,

    // To ensure that --color=never is respected, ANSI escapes _must_ be added
    // using .style() with one of these fields. If you need a new style, add a
    // new field here.
    bold: Style,
    italic: Style,
    dimmed: Style,

    magenta: Style,
    red: Style,
    green: Style,
    cyan: Style,

    /// Whether to include `AgentReasoning` events in the output.
    show_agent_reasoning: bool,
    show_raw_agent_reasoning: bool,
    answer_started: bool,
    reasoning_started: bool,
    reasoning_streams_started: HashSet<String>,
    raw_reasoning_started: bool,
    last_message_path: Option<PathBuf>,
    last_turn_diff: Option<String>,

    /// If true, stop after the first TaskComplete event (default exec mode).
    /// Auto Drive sessions keep running across multiple turns, so they leave
    /// this false and handle shutdown themselves.
    stop_on_task_complete: bool,
}

impl EventProcessorWithHumanOutput {
    pub(crate) fn create_with_ansi(
        with_ansi: bool,
        config: &Config,
        last_message_path: Option<PathBuf>,
        stop_on_task_complete: bool,
    ) -> Self {
        let call_id_to_command = HashMap::new();
        let call_id_to_patch = HashMap::new();

        if with_ansi {
            Self {
                call_id_to_command,
                call_id_to_patch,
                bold: Style::new().bold(),
                italic: Style::new().italic(),
                dimmed: Style::new().dimmed(),
                magenta: Style::new().magenta(),
                red: Style::new().red(),
                green: Style::new().green(),
                cyan: Style::new().cyan(),
                show_agent_reasoning: !config.hide_agent_reasoning,
                show_raw_agent_reasoning: config.show_raw_agent_reasoning,
                answer_started: false,
                reasoning_started: false,
                reasoning_streams_started: HashSet::new(),
                raw_reasoning_started: false,
                last_message_path,
                last_turn_diff: None,
                stop_on_task_complete,
            }
        } else {
            Self {
                call_id_to_command,
                call_id_to_patch,
                bold: Style::new(),
                italic: Style::new(),
                dimmed: Style::new(),
                magenta: Style::new(),
                red: Style::new(),
                green: Style::new(),
                cyan: Style::new(),
                show_agent_reasoning: !config.hide_agent_reasoning,
                show_raw_agent_reasoning: config.show_raw_agent_reasoning,
                answer_started: false,
                reasoning_started: false,
                reasoning_streams_started: HashSet::new(),
                raw_reasoning_started: false,
                last_message_path,
                last_turn_diff: None,
                stop_on_task_complete,
            }
        }
    }

}

struct ExecCommandBegin {
    command: Vec<String>,
}

struct PatchApplyBegin {
    start_time: Instant,
    auto_approved: bool,
}

// Timestamped println helper. The timestamp is styled with self.dimmed.
#[macro_export]
macro_rules! ts_println {
    ($self:ident, $($arg:tt)*) => {{
        let now = chrono::Utc::now();
        let formatted = now.format("[%Y-%m-%dT%H:%M:%S]");
        print!("{} ", formatted.style($self.dimmed));
        println!($($arg)*);
    }};
}

impl EventProcessor for EventProcessorWithHumanOutput {
    /// Print a concise summary of the effective configuration that will be used
    /// for the session. This mirrors the information shown in the TUI welcome
    /// screen.
    fn print_config_summary(&mut self, config: &Config, prompt: &str) {
        const VERSION: &str = env!("CARGO_PKG_VERSION");
        ts_println!(
            self,
            "OpenAI Codex v{} (research preview)",
            VERSION
        );

        // Show which binary is being used to execute sub-agents so users can
        // confirm path mismatches when testing new builds.
        let exe_path = std::env::current_exe()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "<unknown>".to_string());
        ts_println!(self, "binary: {}", exe_path);

        println!("--------");

        let entries = create_config_summary_entries(config);

        for (key, value) in entries {
            println!("{} {}", format!("{key}:").style(self.bold), value);
        }

        println!("--------");

        // Echo the prompt that will be sent to the agent so it is visible in the
        // transcript/logs before any events come in. Note the prompt may have been
        // read from stdin, so it may not be visible in the terminal otherwise.
        ts_println!(
            self,
            "{}\n{}",
            "User instructions:".style(self.bold).style(self.cyan),
            prompt
        );
    }

    fn process_event(&mut self, event: Event) -> CodexStatus {
        let Event { id, msg, .. } = event;
        match msg {
            EventMsg::Error(ErrorEvent { message }) => {
                let prefix = "ERROR:".style(self.red);
                ts_println!(self, "{prefix} {message}");
            }
            EventMsg::BackgroundEvent(BackgroundEventEvent { message }) => {
                ts_println!(self, "{}", message.style(self.dimmed));
            }
            EventMsg::EnvironmentContextFull(_)
            | EventMsg::EnvironmentContextDelta(_)
            | EventMsg::BrowserSnapshot(_)
            | EventMsg::ListCustomPromptsResponse(_)
            | EventMsg::ListSkillsResponse(_)
            | EventMsg::ViewImageToolCall(_) => {
                // Environment context events are consumed by the TUI; the CLI runner
                // does not surface them alongside the human-readable transcript.
            }
            EventMsg::TaskStarted => {
                // Reset per-turn diff cache so we only print new diffs once.
                self.last_turn_diff = None;
                self.reasoning_started = false;
                self.reasoning_streams_started.clear();
            }
            EventMsg::TaskComplete(TaskCompleteEvent { last_agent_message }) => {
                if let Some(output_file) = self.last_message_path.as_deref() {
                    handle_last_message(last_agent_message.as_deref(), output_file);
                }
                if self.stop_on_task_complete {
                    return CodexStatus::InitiateShutdown;
                }
                return CodexStatus::Running;
            }
            EventMsg::TokenCount(ev) => {
                if let Some(usage_info) = ev.info {
                    ts_println!(
                        self,
                        "tokens used: {}",
                        format_with_separators(usage_info.total_token_usage.blended_total())
                    );
                }
            }
            EventMsg::AgentMessageDelta(AgentMessageDeltaEvent { delta }) => {
                if !self.answer_started {
                    ts_println!(self, "{}\n", "codex".style(self.italic).style(self.magenta));
                    self.answer_started = true;
                }
                print!("{delta}");
                #[expect(clippy::expect_used)]
                std::io::stdout().flush().expect("could not flush stdout");
            }
            EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent { delta }) => {
                if !self.show_agent_reasoning {
                    return CodexStatus::Running;
                }
                if !self.reasoning_started {
                    ts_println!(
                        self,
                        "{}\n",
                        "thinking".style(self.italic).style(self.magenta),
                    );
                    self.reasoning_started = true;
                }
                self.reasoning_streams_started.insert(id.clone());
                print!("{delta}");
                #[expect(clippy::expect_used)]
                std::io::stdout().flush().expect("could not flush stdout");
            }
            EventMsg::AgentReasoningSectionBreak(_) => {
                if !self.show_agent_reasoning {
                    return CodexStatus::Running;
                }
                println!();
                #[expect(clippy::expect_used)]
                std::io::stdout().flush().expect("could not flush stdout");
            }
            EventMsg::AgentReasoningRawContent(AgentReasoningRawContentEvent { text }) => {
                if !self.show_raw_agent_reasoning {
                    return CodexStatus::Running;
                }
                if !self.raw_reasoning_started {
                    print!("{text}");
                    #[expect(clippy::expect_used)]
                    std::io::stdout().flush().expect("could not flush stdout");
                } else {
                    println!();
                    self.raw_reasoning_started = false;
                }
            }
            EventMsg::AgentReasoningRawContentDelta(AgentReasoningRawContentDeltaEvent {
                delta,
            }) => {
                if !self.show_raw_agent_reasoning {
                    return CodexStatus::Running;
                }
                if !self.raw_reasoning_started {
                    self.raw_reasoning_started = true;
                }
                print!("{delta}");
                #[expect(clippy::expect_used)]
                std::io::stdout().flush().expect("could not flush stdout");
            }
            EventMsg::AgentMessage(AgentMessageEvent { message }) => {
                // if answer_started is false, this means we haven't received any
                // delta. Thus, we need to print the message as a new answer.
                if !self.answer_started {
                    ts_println!(
                        self,
                        "{}\n{}",
                        "codex".style(self.italic).style(self.magenta),
                        message,
                    );
                } else {
                    println!();
                    self.answer_started = false;
                }
            }
            EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
                call_id,
                command,
                cwd,
                parsed_cmd: _,
            }) => {
                self.call_id_to_command.insert(
                    call_id,
                    ExecCommandBegin {
                        command: command.clone(),
                    },
                );
                ts_println!(
                    self,
                    "{} {} in {}",
                    "exec".style(self.magenta),
                    escape_command(&command).style(self.bold),
                    cwd.to_string_lossy(),
                );
            }
            EventMsg::ExecCommandOutputDelta(_) => {}
            EventMsg::ExecCommandEnd(ExecCommandEndEvent { call_id, stdout, stderr, duration, exit_code }) => {
                let exec_command = self.call_id_to_command.remove(&call_id);
                let (duration, call) = if let Some(ExecCommandBegin { command, .. }) = exec_command
                {
                    (
                        format!(" in {}", format_duration(duration)),
                        format!("{}", escape_command(&command).style(self.bold)),
                    )
                } else {
                    ("".to_string(), format!("exec('{call_id}')"))
                };

                let truncated_stdout = stdout
                    .lines()
                    .take(MAX_OUTPUT_LINES_FOR_EXEC_TOOL_CALL)
                    .collect::<Vec<_>>()
                    .join("\n");
                let truncated_stderr = stderr
                    .lines()
                    .take(MAX_OUTPUT_LINES_FOR_EXEC_TOOL_CALL)
                    .collect::<Vec<_>>()
                    .join("\n");
                match exit_code {
                    0 => {
                        let title = format!("{call} succeeded{duration}:");
                        ts_println!(self, "{}", title.style(self.green));
                        if !truncated_stdout.is_empty() {
                            println!("{}", truncated_stdout.style(self.dimmed));
                        }
                    }
                    _ => {
                        let title = format!("{call} exited {exit_code}{duration}:");
                        ts_println!(self, "{}", title.style(self.red));
                        if !truncated_stdout.is_empty() {
                            println!("{}", truncated_stdout.style(self.dimmed));
                            println!();
                        }
                        println!("ERROR");
                        if !truncated_stderr.is_empty() {
                            println!("{}", truncated_stderr);
                        }
                    }
                }
            }
            EventMsg::McpToolCallBegin(McpToolCallBeginEvent {
                call_id: _,
                invocation,
            }) => {
                ts_println!(
                    self,
                    "{} {}",
                    "tool".style(self.magenta),
                    format_mcp_invocation(&invocation).style(self.bold),
                );
            }
            EventMsg::McpToolCallEnd(tool_call_end_event) => {
                let is_success = tool_call_end_event.is_success();
                let McpToolCallEndEvent {
                    call_id: _,
                    result,
                    invocation,
                    duration,
                } = tool_call_end_event;

                let duration = format!(" in {}", format_duration(duration));

                let status_str = if is_success { "success" } else { "failed" };
                let title_style = if is_success { self.green } else { self.red };
                let title = format!(
                    "{} {status_str}{duration}:",
                    format_mcp_invocation(&invocation)
                );

                ts_println!(self, "{}", title.style(title_style));

                if let Ok(res) = result {
                    let val: serde_json::Value = res.into();
                    let pretty =
                        serde_json::to_string_pretty(&val).unwrap_or_else(|_| val.to_string());

                    for line in pretty.lines().take(MAX_OUTPUT_LINES_FOR_EXEC_TOOL_CALL) {
                        println!("{}", line.style(self.dimmed));
                    }
                }
            }
            EventMsg::WebSearchBegin(WebSearchBeginEvent { call_id: _, .. }) => {}
            EventMsg::WebSearchComplete(WebSearchCompleteEvent { call_id: _, query }) => {
                if let Some(query) = query {
                    ts_println!(self, "🌐 Search: {query}");
                }
            }
            EventMsg::PatchApplyBegin(PatchApplyBeginEvent {
                call_id,
                auto_approved,
                changes,
            }) => {
                // Store metadata so we can calculate duration later when we
                // receive the corresponding PatchApplyEnd event.
                self.call_id_to_patch.insert(
                    call_id,
                    PatchApplyBegin {
                        start_time: Instant::now(),
                        auto_approved,
                    },
                );

                ts_println!(
                    self,
                    "{} auto_approved={}:",
                    "apply_patch".style(self.magenta),
                    auto_approved,
                );

                // Pretty-print the patch summary with colored diff markers so
                // it's easy to scan in the terminal output.
                for (path, change) in changes.iter() {
                    match change {
                        FileChange::Add { content } => {
                            let header = format!(
                                "{} {}",
                                format_file_change(change),
                                path.to_string_lossy()
                            );
                            println!("{}", header.style(self.magenta));
                            for line in content.lines() {
                                println!("{}", line.style(self.green));
                            }
                        }
                        FileChange::Delete => {
                            let header = format!(
                                "{} {}",
                                format_file_change(change),
                                path.to_string_lossy()
                            );
                            println!("{}", header.style(self.magenta));
                        }
                        FileChange::Update {
                            unified_diff,
                            move_path,
                            ..
                        } => {
                            let header = if let Some(dest) = move_path {
                                format!(
                                    "{} {} -> {}",
                                    format_file_change(change),
                                    path.to_string_lossy(),
                                    dest.to_string_lossy()
                                )
                            } else {
                                format!("{} {}", format_file_change(change), path.to_string_lossy())
                            };
                            println!("{}", header.style(self.magenta));

                            // Colorize diff lines. We keep file header lines
                            // (--- / +++) without extra coloring so they are
                            // still readable.
                            for diff_line in unified_diff.lines() {
                                if diff_line.starts_with('+') && !diff_line.starts_with("+++") {
                                    println!("{}", diff_line.style(self.green));
                                } else if diff_line.starts_with('-')
                                    && !diff_line.starts_with("---")
                                {
                                    println!("{}", diff_line.style(self.red));
                                } else {
                                    println!("{diff_line}");
                                }
                            }
                        }
                    }
                }
            }
            EventMsg::PatchApplyEnd(PatchApplyEndEvent {
                call_id,
                stdout,
                stderr,
                success,
                ..
            }) => {
                let patch_begin = self.call_id_to_patch.remove(&call_id);

                // Compute duration and summary label similar to exec commands.
                let (duration, label) = if let Some(PatchApplyBegin {
                    start_time,
                    auto_approved,
                }) = patch_begin
                {
                    (
                        format!(" in {}", format_elapsed(start_time)),
                        format!("apply_patch(auto_approved={auto_approved})"),
                    )
                } else {
                    (String::new(), format!("apply_patch('{call_id}')"))
                };

                let (exit_code, output, title_style) = if success {
                    (0, stdout, self.green)
                } else {
                    (1, stderr, self.red)
                };

                let title = format!("{label} exited {exit_code}{duration}:");
                ts_println!(self, "{}", title.style(title_style));
                for line in output.lines() {
                    println!("{}", line.style(self.dimmed));
                }
            }
            EventMsg::TurnDiff(TurnDiffEvent { unified_diff }) => {
                if self.last_turn_diff.as_deref() == Some(&unified_diff) {
                    // Suppress duplicate turn diffs; they are sometimes streamed multiple times.
                    return CodexStatus::Running;
                }
                self.last_turn_diff = Some(unified_diff.clone());
                ts_println!(self, "{}", "turn diff:".style(self.magenta));
                println!("{unified_diff}");
            }
            EventMsg::ExecApprovalRequest(_) => {
                // Should we exit?
            }
            EventMsg::ApplyPatchApprovalRequest(_) => {
                // Should we exit?
            }
            EventMsg::AgentReasoning(agent_reasoning_event) => {
                if self.show_agent_reasoning {
                    if self.reasoning_streams_started.remove(&id) {
                        println!();
                        if self.reasoning_streams_started.is_empty() {
                            self.reasoning_started = false;
                        }
                    } else if !self.reasoning_started {
                        ts_println!(
                            self,
                            "{}\n{}",
                            "codex".style(self.italic).style(self.magenta),
                            agent_reasoning_event.text,
                        );
                    } else {
                        println!();
                        ts_println!(
                            self,
                            "{}\n{}",
                            "codex".style(self.italic).style(self.magenta),
                            agent_reasoning_event.text,
                        );
                    }
                }
            }
            EventMsg::SessionConfigured(session_configured_event) => {
                let SessionConfiguredEvent { session_id: conversation_id, model, .. } = session_configured_event;

                ts_println!(
                    self,
                    "{} {}",
                    "codex session".style(self.magenta).style(self.bold),
                    conversation_id.to_string().style(self.dimmed)
                );

                ts_println!(self, "model: {}", model);
                println!();
            }
            EventMsg::PlanUpdate(plan_update_event) => {
                let UpdatePlanArgs { name, plan } = plan_update_event;
                ts_println!(self, "name: {name:?}");
                ts_println!(self, "plan: {plan:?}");
            }
            EventMsg::GetHistoryEntryResponse(_) => {
                // Currently ignored in exec output.
            }
            EventMsg::ReplayHistory(_) => {
                // Replay is a TUI concern; ignore in headless output
            }
            EventMsg::BrowserScreenshotUpdate(_) => {
                // Currently ignored in exec output.
            }
            EventMsg::AgentStatusUpdate(_) => {
                // Currently ignored in exec output.
            }
            EventMsg::CustomToolCallBegin(event) => {
                ts_println!(
                    self,
                    "{} {}",
                    "tool".style(self.magenta),
                    event.tool_name.style(self.bold),
                );
                if let Some(params) = &event.parameters {
                    if let Ok(formatted) = serde_json::to_string_pretty(params) {
                        for line in formatted.lines() {
                            println!("{}", line.style(self.dimmed));
                        }
                    }
                }
            }
            EventMsg::CustomToolCallEnd(event) => {
                let status = if event.result.is_ok() {
                    "success".style(self.green)
                } else {
                    "failed".style(self.red)
                };
                ts_println!(
                    self,
                    "{} {} {}",
                    "tool".style(self.magenta),
                    event.tool_name.style(self.bold),
                    status,
                );
                match &event.result {
                    Ok(content) => {
                        if !content.is_empty() {
                            for line in content.lines() {
                                println!("{}", line.style(self.dimmed));
                            }
                        }
                    }
                    Err(err) => {
                        if !err.is_empty() {
                            for line in err.lines() {
                                println!("{}", line.style(self.red));
                            }
                        }
                    }
                }
            }
            EventMsg::TurnAborted(abort_reason) => match abort_reason.reason {
                TurnAbortReason::Interrupted => {
                    ts_println!(self, "task interrupted");
                }
                TurnAbortReason::Replaced => {
                    ts_println!(self, "task aborted: replaced by a new task");
                }
                TurnAbortReason::ReviewEnded => {
                    ts_println!(self, "task aborted: review ended");
                }
            },
            EventMsg::ShutdownComplete => return CodexStatus::Shutdown,
            EventMsg::ConversationPath(_) => {}
            EventMsg::UserMessage(_) => {}
            EventMsg::EnteredReviewMode(request) => {
                ts_println!(
                    self,
                    "{} {}",
                    "review".style(self.magenta),
                    "started".style(self.bold),
                );
                if let Some(scope) = request
                    .metadata
                    .as_ref()
                    .and_then(|m| m.scope.as_ref())
                {
                    println!("{} {}", "scope:".style(self.dimmed), scope.style(self.dimmed));
                }
                if !request.user_facing_hint.trim().is_empty() {
                    println!("{}", request.user_facing_hint.trim().style(self.dimmed));
                }
            }
            EventMsg::ExitedReviewMode(event) => {
                ts_println!(
                    self,
                    "{} {}",
                    "review".style(self.magenta),
                    "finished".style(self.bold),
                );
                if let Some(snapshot) = &event.snapshot {
                    if let Some(branch) = &snapshot.branch {
                        println!("{} {}", "branch:".style(self.dimmed), branch);
                    }
                    if let Some(path) = &snapshot.worktree_path {
                        println!("{} {}", "worktree:".style(self.dimmed), path.display());
                    }
                    if let Some(commit) = &snapshot.snapshot_commit {
                        let short = commit.chars().take(12).collect::<String>();
                        println!("{} {}", "snapshot:".style(self.dimmed), short);
                    }
                }
                match &event.review_output {
                    Some(output) => {
                        if !output.overall_explanation.trim().is_empty() {
                            println!("{}", output.overall_explanation.trim());
                        }

                        if output.findings.is_empty() {
                            println!("{}", "no findings reported".style(self.dimmed));
                        } else {
                            for (idx, finding) in output.findings.iter().enumerate() {
                                println!(
                                    "{} {}",
                                    format!("#{}", idx + 1).style(self.bold),
                                    finding.title.trim(),
                                );
                                if !finding.body.trim().is_empty() {
                                    for line in finding.body.lines() {
                                        println!("  {}", line.trim());
                                    }
                                }
                            }
                        }

                        if output.overall_confidence_score > 0.0 {
                            println!(
                                "confidence: {:.1}",
                                output.overall_confidence_score,
                            );
                        }
                    }
                    None => println!("{}", "review ended without results".style(self.dimmed)),
                }
            }
            EventMsg::CompactionCheckpointWarning(_) => {}
        }
        CodexStatus::Running
    }
}

fn escape_command(command: &[String]) -> String {
    try_join(command.iter().map(String::as_str)).unwrap_or_else(|_| command.join(" "))
}

fn format_file_change(change: &FileChange) -> &'static str {
    match change {
        FileChange::Add { .. } => "A",
        FileChange::Delete { .. } => "D",
        FileChange::Update {
            move_path: Some(_), ..
        } => "R",
        FileChange::Update {
            move_path: None, ..
        } => "M",
    }
}

fn format_mcp_invocation(invocation: &McpInvocation) -> String {
    // Build fully-qualified tool name: server.tool
    let fq_tool_name = format!("{}.{}", invocation.server, invocation.tool);

    // Format arguments as compact JSON so they fit on one line.
    let args_str = invocation
        .arguments
        .as_ref()
        .map(|v: &serde_json::Value| serde_json::to_string(v).unwrap_or_else(|_| v.to_string()))
        .unwrap_or_default();

    if args_str.is_empty() {
        format!("{fq_tool_name}()")
    } else {
        format!("{fq_tool_name}({args_str})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use code_core::protocol::AgentReasoningEvent;

    #[test]
    fn reasoning_streams_do_not_reset_mid_stream() {
        let mut proc = EventProcessorWithHumanOutput {
            call_id_to_command: HashMap::new(),
            call_id_to_patch: HashMap::new(),
            bold: Style::new(),
            italic: Style::new(),
            dimmed: Style::new(),
            magenta: Style::new(),
            red: Style::new(),
            green: Style::new(),
            cyan: Style::new(),
            show_agent_reasoning: true,
            show_raw_agent_reasoning: false,
            answer_started: false,
            reasoning_started: false,
            reasoning_streams_started: HashSet::new(),
            raw_reasoning_started: false,
            last_message_path: None,
            last_turn_diff: None,
            stop_on_task_complete: false,
        };

        // Two separate reasoning streams interleave deltas before either one finalizes.
        proc.process_event(Event {
            id: "stream-a".to_string(),
            event_seq: 0,
            msg: EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent {
                delta: "A".to_string(),
            }),
            order: None,
        });
        proc.process_event(Event {
            id: "stream-b".to_string(),
            event_seq: 1,
            msg: EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent {
                delta: "B".to_string(),
            }),
            order: None,
        });
        assert!(proc.reasoning_started);
        assert_eq!(proc.reasoning_streams_started.len(), 2);

        // Finalizing one stream should not reset the global reasoning state while
        // another stream is still active.
        proc.process_event(Event {
            id: "stream-a".to_string(),
            event_seq: 2,
            msg: EventMsg::AgentReasoning(AgentReasoningEvent {
                text: "A".to_string(),
            }),
            order: None,
        });
        assert!(proc.reasoning_started);
        assert_eq!(proc.reasoning_streams_started.len(), 1);
        assert!(proc.reasoning_streams_started.contains("stream-b"));

        proc.process_event(Event {
            id: "stream-b".to_string(),
            event_seq: 3,
            msg: EventMsg::AgentReasoning(AgentReasoningEvent {
                text: "B".to_string(),
            }),
            order: None,
        });
        assert!(!proc.reasoning_started);
        assert!(proc.reasoning_streams_started.is_empty());
    }
}
