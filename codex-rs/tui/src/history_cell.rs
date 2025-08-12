use crate::diff_render::create_diff_summary;
use crate::exec_command::relativize_to_home;
use crate::exec_command::strip_bash_lc_and_escape;
use crate::slash_command::SlashCommand;
use crate::text_block::TextBlock;
use crate::text_formatting::format_and_truncate_tool_result;
use base64::Engine;
use codex_ansi_escape::ansi_escape_line;
use codex_common::create_config_summary_entries;
use codex_common::elapsed::format_duration;
use codex_core::config::Config;
use codex_core::config_types::ReasoningEffort;
use codex_core::parse_command::ParsedCommand;
use codex_core::plan_tool::PlanItemArg;
use codex_core::plan_tool::StepStatus;
use codex_core::plan_tool::UpdatePlanArgs;
use codex_core::protocol::FileChange;
use codex_core::protocol::McpInvocation;
use codex_core::protocol::SandboxPolicy;
use codex_core::protocol::SessionConfiguredEvent;
use codex_core::protocol::TokenUsage;
use codex_login::get_auth_file;
use codex_login::try_read_auth_json;
use image::DynamicImage;
use image::ImageReader;
use mcp_types::EmbeddedResourceResource;
use mcp_types::ResourceLink;
use ratatui::prelude::*;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::widgets::Paragraph;
use ratatui::widgets::WidgetRef;
use ratatui::widgets::Wrap;
use std::collections::HashMap;
use std::io::Cursor;
use std::path::PathBuf;
use std::time::Duration;
use tracing::error;

#[derive(Clone)]
pub(crate) struct CommandOutput {
    pub(crate) exit_code: i32,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

pub(crate) struct ExecCell {
    pub(crate) command: Vec<String>,
    pub(crate) parsed: Vec<ParsedCommand>,
    pub(crate) output: Option<CommandOutput>,
}


pub(crate) enum PatchEventType {
    ApprovalRequest,
    ApplyBegin { auto_approved: bool },
}

fn span_to_static(span: &Span) -> Span<'static> {
    Span {
        style: span.style,
        content: std::borrow::Cow::Owned(span.content.clone().into_owned()),
    }
}

fn line_to_static(line: &Line) -> Line<'static> {
    Line {
        style: line.style,
        alignment: line.alignment,
        spans: line.spans.iter().map(span_to_static).collect(),
    }
}

/// Represents an event to display in the conversation history. Returns its
/// `Vec<Line<'static>>` representation to make it easier to display in a
/// scrollable list.
pub(crate) enum HistoryCell {
    /// Animated welcome that shows particle animation
    AnimatedWelcome { 
        start_time: std::time::Instant,
        completed: std::cell::Cell<bool>,
        fade_start: std::cell::Cell<Option<std::time::Instant>>,
        faded_out: std::cell::Cell<bool>,
    },
    
    /// Welcome message.
    WelcomeMessage { view: TextBlock },

    /// Message from the user.
    UserPrompt { view: TextBlock },

    // AgentMessage and AgentReasoning variants were unused and have been removed.
    /// Exec command - can be either active or completed
    Exec(ExecCell),

    /// An MCP tool call that has not finished yet.
    ActiveMcpToolCall { view: TextBlock },

    /// Completed MCP tool call where we show the result serialized as JSON.
    CompletedMcpToolCall { view: TextBlock },

    /// Completed MCP tool call where the result is an image.
    /// Admittedly, [mcp_types::CallToolResult] can have multiple content types,
    /// which could be a mix of text and images, so we need to tighten this up.
    // NOTE: For image output we keep the *original* image around and lazily
    // compute a resized copy that fits the available cell width.  Caching the
    // resized version avoids doing the potentially expensive rescale twice
    // because the scroll-view first calls `height()` for layouting and then
    // `render_window()` for painting.
    CompletedMcpToolCallWithImageOutput { _image: DynamicImage },

    /// Background event.
    BackgroundEvent { view: TextBlock },

    /// Output from the `/diff` command.
    GitDiffOutput { view: TextBlock },
    
    /// Output from the `/reasoning` command.
    ReasoningOutput { view: TextBlock },

    /// Output from the `/status` command.
    StatusOutput { view: TextBlock },

    /// Output from the `/prompts` command.
    PromptsOutput { view: TextBlock },

    /// Error event from the backend.
    ErrorEvent { view: TextBlock },

    /// Info describing the newly-initialized session.
    SessionInfo { view: TextBlock },

    /// A pending code patch that is awaiting user approval. Mirrors the
    /// behaviour of `ActiveExecCommand` so the user sees *what* patch the
    /// model wants to apply before being prompted to approve or deny it.
    PendingPatch { view: TextBlock },

    /// A human‑friendly rendering of the model's current plan and step
    /// statuses provided via the `update_plan` tool.
    PlanUpdate { view: TextBlock },

    /// Result of applying a patch (success or failure) with optional output.
    PatchApplyResult { view: TextBlock },
}

const TOOL_CALL_MAX_LINES: usize = 5;

fn shlex_join_safe(command: &[String]) -> String {
    match shlex::try_join(command.iter().map(|s| s.as_str())) {
        Ok(cmd) => cmd,
        Err(_) => command.join(" "),
    }
}

fn output_lines(
    output: Option<&CommandOutput>,
    only_err: bool,
    include_angle_pipe: bool,
) -> Vec<Line<'static>> {
    let CommandOutput {
        exit_code,
        stdout,
        stderr,
    } = match output {
        Some(output) if only_err && output.exit_code == 0 => return vec![],
        Some(output) => output,
        None => return vec![],
    };

    let src = if *exit_code == 0 { stdout } else { stderr };

    let mut lines: Vec<Line<'static>> = Vec::new();
    let lines_iter = src.lines();
    let truncate_at = 2 * TOOL_CALL_MAX_LINES;
    let src_lines: Vec<_> = lines_iter.collect();
    let line_count = src_lines.len();
    let needs_truncation = line_count > truncate_at;

    let _display_lines = if needs_truncation {
        let start_lines: Vec<_> = src_lines.iter().take(TOOL_CALL_MAX_LINES).collect();
        let end_lines: Vec<_> = src_lines
            .iter()
            .skip(line_count.saturating_sub(TOOL_CALL_MAX_LINES))
            .collect();

        for (idx, raw) in start_lines.iter().enumerate() {
            let mut line = ansi_escape_line(raw);
            let prefix = if idx == 0 && include_angle_pipe {
                "  ⎿ "
            } else {
                "    "
            };
            line.spans.insert(0, prefix.into());
            line.spans.iter_mut().for_each(|span| {
                span.style = span.style.add_modifier(Modifier::DIM);
            });
            lines.push(line);
        }

        let mut more = Line::from(format!(
            "... {} lines truncated ...",
            line_count - TOOL_CALL_MAX_LINES * 2
        ));
        more.spans.insert(0, "    ".into());
        more.spans.iter_mut().for_each(|span| {
            span.style = span.style.add_modifier(Modifier::DIM);
        });
        lines.push(more);

        for raw in end_lines {
            let mut line = ansi_escape_line(raw);
            line.spans.insert(0, "    ".into());
            line.spans.iter_mut().for_each(|span| {
                span.style = span.style.add_modifier(Modifier::DIM);
            });
            lines.push(line);
        }
    } else {
        for (idx, raw) in src_lines.iter().enumerate() {
            let mut line = ansi_escape_line(raw);
            let prefix = if idx == 0 && include_angle_pipe {
                "  ⎿ "
            } else {
                "    "
            };
            line.spans.insert(0, prefix.into());
            line.spans.iter_mut().for_each(|span| {
                span.style = span.style.add_modifier(Modifier::DIM);
            });
            lines.push(line);
        }
    };

    lines
}

fn title_case(s: &str) -> String {
    if s.is_empty() {
        return String::new();
    }
    let mut chars = s.chars();
    let first = match chars.next() {
        Some(c) => c,
        None => return String::new(),
    };
    let rest: String = chars.as_str().to_ascii_lowercase();
    first.to_uppercase().collect::<String>() + &rest
}

fn pretty_provider_name(id: &str) -> String {
    if id.eq_ignore_ascii_case("openai") {
        "OpenAI".to_string()
    } else {
        title_case(id)
    }
}

impl HistoryCell {
    /// Return a cloned, plain representation of the cell's lines suitable for
    /// one‑shot insertion into the terminal scrollback. Image cells are
    /// represented with a simple placeholder for now.
    pub(crate) fn plain_lines(&self) -> Vec<Line<'static>> {
        match self {
            HistoryCell::AnimatedWelcome { .. } => {
                // For plain lines, just show a simple welcome message
                vec![
                    Line::from(""),
                    Line::from("Welcome to Coder"),
                    Line::from(""),
                ]
            }
            HistoryCell::WelcomeMessage { view }
            | HistoryCell::UserPrompt { view }
            | HistoryCell::BackgroundEvent { view }
            | HistoryCell::GitDiffOutput { view }
            | HistoryCell::ReasoningOutput { view }
            | HistoryCell::StatusOutput { view }
            | HistoryCell::PromptsOutput { view }
            | HistoryCell::ErrorEvent { view }
            | HistoryCell::SessionInfo { view }
            | HistoryCell::CompletedMcpToolCall { view }
            | HistoryCell::PendingPatch { view }
            | HistoryCell::PlanUpdate { view }
            | HistoryCell::PatchApplyResult { view }
            | HistoryCell::ActiveMcpToolCall { view, .. } => {
                view.lines.iter().map(line_to_static).collect()
            }
            HistoryCell::Exec(ExecCell { command, parsed, output }) => {
                HistoryCell::exec_command_lines(command, parsed, output.as_ref())
            }
            HistoryCell::CompletedMcpToolCallWithImageOutput { .. } => vec![
                Line::from("tool result (image output omitted)"),
                Line::from(""),
            ],
        }
    }

    pub(crate) fn desired_height(&self, width: u16) -> u16 {
        match self {
            HistoryCell::AnimatedWelcome { faded_out, .. } => {
                // If faded out, take no space
                if faded_out.get() {
                    0u16
                } else {
                    // Fixed height for animation area
                    18u16  // 16 for animation + 2 for borders
                }
            }
            HistoryCell::BackgroundEvent { view: _ } => {
                // For background events (LLM responses), use proper word wrapping
                let processed_lines = self.get_processed_lines(width);
                processed_lines.len() as u16
            }
            _ => {
                Paragraph::new(Text::from(self.plain_lines()))
                    .wrap(Wrap { trim: false })
                    .line_count(width)
                    .try_into()
                    .unwrap_or(0)
            }
        }
    }

    pub(crate) fn new_session_info(
        config: &Config,
        event: SessionConfiguredEvent,
        is_first_event: bool,
    ) -> Self {
        let SessionConfiguredEvent {
            model,
            session_id: _,
            history_log_id: _,
            history_entry_count: _,
        } = event;
        if is_first_event {
            // Since we now have a status bar, just show helpful commands in history
            let lines: Vec<Line<'static>> = vec![
                Line::from("".dim()),
                Line::from(" Popular commands:".dim()),
                Line::from(format!(" /browser <url> - {}", SlashCommand::Browser.description()).dim()),
                Line::from(format!(" /plan - {}", SlashCommand::Plan.description()).dim()),
                Line::from(format!(" /solve - {}", SlashCommand::Solve.description()).dim()),
                Line::from(format!(" /code - {}", SlashCommand::Code.description()).dim()),
                Line::from(format!(" /reasoning - {}", SlashCommand::Reasoning.description()).dim()),
                Line::from("".dim()),
            ];
            HistoryCell::WelcomeMessage {
                view: TextBlock::new(lines),
            }
        } else if config.model == model {
            HistoryCell::SessionInfo {
                view: TextBlock::new(Vec::new()),
            }
        } else {
            let lines = vec![
                Line::from("model changed:".magenta().bold()),
                Line::from(format!("requested: {}", config.model)),
                Line::from(format!("used: {model}")),
                Line::from(""),
            ];
            HistoryCell::SessionInfo {
                view: TextBlock::new(lines),
            }
        }
    }

    pub(crate) fn new_user_prompt(message: String) -> Self {
        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.push(Line::from(Span::styled("user", Style::default().fg(crate::colors::primary()).add_modifier(Modifier::BOLD))));
        lines.extend(message.lines().map(|l| Line::from(l.to_string())));
        lines.push(Line::from(""));

        HistoryCell::UserPrompt {
            view: TextBlock::new(lines),
        }
    }

    pub(crate) fn new_active_exec_command(command: Vec<String>, parsed: Vec<ParsedCommand>) -> Self {
        HistoryCell::new_exec_cell(command, parsed, None)
    }

    pub(crate) fn new_completed_exec_command(command: Vec<String>, parsed: Vec<ParsedCommand>, output: CommandOutput) -> Self {
        HistoryCell::new_exec_cell(command, parsed, Some(output))
    }

    fn new_exec_cell(command: Vec<String>, parsed: Vec<ParsedCommand>, output: Option<CommandOutput>) -> Self {
        HistoryCell::Exec(ExecCell { command, parsed, output })
    }

    fn exec_command_lines(command: &[String], parsed: &[ParsedCommand], output: Option<&CommandOutput>) -> Vec<Line<'static>> {
        match parsed.is_empty() {
            true => HistoryCell::new_exec_command_generic(command, output),
            false => HistoryCell::new_parsed_command(parsed, output),
        }
    }

    fn new_parsed_command(parsed_commands: &[ParsedCommand], output: Option<&CommandOutput>) -> Vec<Line<'static>> {
        use crate::colors::light_blue;
        
        let mut lines: Vec<Line> = vec![Line::from("⚙︎ Working")];

        for (i, parsed) in parsed_commands.iter().enumerate() {
            let str = match parsed {
                ParsedCommand::Read { name, .. } => format!("📖 {name}"),
                ParsedCommand::ListFiles { cmd, path } => match path {
                    Some(p) => format!("📂 {p}"),
                    None => format!("📂 {}", shlex_join_safe(cmd)),
                },
                ParsedCommand::Search { query, path, cmd } => match (query, path) {
                    (Some(q), Some(p)) => format!("🔎 {q} in {p}"),
                    (Some(q), None) => format!("🔎 {q}"),
                    (None, Some(p)) => format!("🔎 {p}"),
                    (None, None) => format!("🔎 {}", shlex_join_safe(cmd)),
                },
                ParsedCommand::Format { .. } => "✨ Formatting".to_string(),
                ParsedCommand::Test { cmd } => format!("🧪 {}", shlex_join_safe(cmd)),
                ParsedCommand::Lint { cmd, .. } => format!("🧹 {}", shlex_join_safe(cmd)),
                ParsedCommand::Unknown { cmd } => format!("⌨️ {}", shlex_join_safe(cmd)),
            };

            let prefix = if i == 0 { "  L " } else { "    " };
            lines.push(Line::from(vec![
                Span::styled(prefix, Style::default().add_modifier(Modifier::DIM)),
                Span::styled(str, Style::default().fg(light_blue())),
            ]));
        }

        lines.extend(output_lines(output, true, false));
        lines.push(Line::from(""));

        lines
    }

    fn new_exec_command_generic(command: &[String], output: Option<&CommandOutput>) -> Vec<Line<'static>> {
        let command_escaped = strip_bash_lc_and_escape(command);
        let mut lines: Vec<Line<'static>> = Vec::new();
        
        let mut cmd_lines = command_escaped.lines();
        if let Some(first) = cmd_lines.next() {
            lines.push(Line::from(vec![
                "⚡ Ran command ".magenta(),
                first.to_string().into(),
            ]));
        } else {
            lines.push(Line::from("⚡ Ran command".magenta()));
        }
        for cont in cmd_lines {
            lines.push(Line::from(cont.to_string()));
        }

        lines.extend(output_lines(output, false, true));
        lines.push(Line::from(""));

        lines
    }

    pub(crate) fn new_active_mcp_tool_call(invocation: McpInvocation) -> Self {
        let title_line = Line::from(vec!["tool".magenta(), " running...".dim()]);
        let lines: Vec<Line> = vec![
            title_line,
            format_mcp_invocation(invocation.clone()),
            Line::from(""),
        ];

        HistoryCell::ActiveMcpToolCall {
            view: TextBlock::new(lines),
        }
    }

    /// If the first content is an image, return a new cell with the image.
    /// TODO(rgwood-dd): Handle images properly even if they're not the first result.
    fn try_new_completed_mcp_tool_call_with_image_output(
        result: &Result<mcp_types::CallToolResult, String>,
    ) -> Option<Self> {
        match result {
            Ok(mcp_types::CallToolResult { content, .. }) => {
                if let Some(mcp_types::ContentBlock::ImageContent(image)) = content.first() {
                    let raw_data =
                        match base64::engine::general_purpose::STANDARD.decode(&image.data) {
                            Ok(data) => data,
                            Err(e) => {
                                error!("Failed to decode image data: {e}");
                                return None;
                            }
                        };
                    let reader = match ImageReader::new(Cursor::new(raw_data)).with_guessed_format()
                    {
                        Ok(reader) => reader,
                        Err(e) => {
                            error!("Failed to guess image format: {e}");
                            return None;
                        }
                    };

                    let image = match reader.decode() {
                        Ok(image) => image,
                        Err(e) => {
                            error!("Image decoding failed: {e}");
                            return None;
                        }
                    };

                    Some(HistoryCell::CompletedMcpToolCallWithImageOutput { _image: image })
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub(crate) fn new_completed_mcp_tool_call(
        num_cols: u16,
        invocation: McpInvocation,
        duration: Duration,
        success: bool,
        result: Result<mcp_types::CallToolResult, String>,
    ) -> Self {
        if let Some(cell) = Self::try_new_completed_mcp_tool_call_with_image_output(&result) {
            return cell;
        }

        let duration = format_duration(duration);
        let status_str = if success { "success" } else { "failed" };
        let title_line = Line::from(vec![
            "tool".magenta(),
            " ".into(),
            if success {
                status_str.green()
            } else {
                status_str.red()
            },
            format!(", duration: {duration}").gray(),
        ]);

        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.push(title_line);
        lines.push(format_mcp_invocation(invocation));

        match result {
            Ok(mcp_types::CallToolResult { content, .. }) => {
                if !content.is_empty() {
                    lines.push(Line::from(""));

                    for tool_call_result in content {
                        let line_text = match tool_call_result {
                            mcp_types::ContentBlock::TextContent(text) => {
                                format_and_truncate_tool_result(
                                    &text.text,
                                    TOOL_CALL_MAX_LINES,
                                    num_cols as usize,
                                )
                            }
                            mcp_types::ContentBlock::ImageContent(_) => {
                                // TODO show images even if they're not the first result, will require a refactor of `CompletedMcpToolCall`
                                "<image content>".to_string()
                            }
                            mcp_types::ContentBlock::AudioContent(_) => {
                                "<audio content>".to_string()
                            }
                            mcp_types::ContentBlock::EmbeddedResource(resource) => {
                                let uri = match resource.resource {
                                    EmbeddedResourceResource::TextResourceContents(text) => {
                                        text.uri
                                    }
                                    EmbeddedResourceResource::BlobResourceContents(blob) => {
                                        blob.uri
                                    }
                                };
                                format!("embedded resource: {uri}")
                            }
                            mcp_types::ContentBlock::ResourceLink(ResourceLink { uri, .. }) => {
                                format!("link: {uri}")
                            }
                        };
                        lines.push(Line::styled(line_text, Style::default().fg(Color::Gray)));
                    }
                }

                lines.push(Line::from(""));
            }
            Err(e) => {
                lines.push(Line::from(vec![
                    Span::styled(
                        "Error: ",
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(e),
                ]));
            }
        };

        HistoryCell::CompletedMcpToolCall {
            view: TextBlock::new(lines),
        }
    }
    // allow dead code for now. maybe we'll use it again.
    #[allow(dead_code)]
    pub(crate) fn new_background_event(message: String) -> Self {
        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.push(Line::from("event".dim()));
        lines.extend(message.lines().map(|line| ansi_escape_line(line).dim()));
        lines.push(Line::from(""));
        HistoryCell::BackgroundEvent {
            view: TextBlock::new(lines),
        }
    }

    pub(crate) fn new_diff_output(message: String) -> Self {
        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.push(Line::from("/diff".magenta()));

        if message.trim().is_empty() {
            lines.push(Line::from("No changes detected.".italic()));
        } else {
            lines.extend(message.lines().map(ansi_escape_line));
        }

        lines.push(Line::from(""));
        HistoryCell::GitDiffOutput {
            view: TextBlock::new(lines),
        }
    }

    pub(crate) fn new_reasoning_output(effort: ReasoningEffort) -> Self {
        use ratatui::style::Stylize;
        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.push(Line::from("/reasoning".magenta()));
        lines.push(Line::from(vec![
            "Reasoning effort changed to: ".into(),
            format!("{}", effort).bold().into(),
        ]));
        lines.push(Line::from(""));
        HistoryCell::ReasoningOutput {
            view: TextBlock::new(lines),
        }
    }

    pub(crate) fn new_status_output(config: &Config, usage: &TokenUsage) -> Self {
        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.push(Line::from("/status".magenta()));

        let config_entries = create_config_summary_entries(config);
        let lookup = |k: &str| -> String {
            config_entries
                .iter()
                .find(|(key, _)| *key == k)
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        };

        // 📂 Workspace
        lines.push(Line::from(vec!["📂 ".into(), "Workspace".bold()]));
        // Path (home-relative, e.g., ~/code/project)
        let cwd_str = match relativize_to_home(&config.cwd) {
            Some(rel) if !rel.as_os_str().is_empty() => format!("~/{}", rel.display()),
            Some(_) => "~".to_string(),
            None => config.cwd.display().to_string(),
        };
        lines.push(Line::from(vec!["  • Path: ".into(), cwd_str.into()]));
        // Approval mode (as-is)
        lines.push(Line::from(vec![
            "  • Approval Mode: ".into(),
            lookup("approval").into(),
        ]));
        // Sandbox (simplified name only)
        let sandbox_name = match &config.sandbox_policy {
            SandboxPolicy::DangerFullAccess => "danger-full-access",
            SandboxPolicy::ReadOnly => "read-only",
            SandboxPolicy::WorkspaceWrite { .. } => "workspace-write",
        };
        lines.push(Line::from(vec![
            "  • Sandbox: ".into(),
            sandbox_name.into(),
        ]));

        lines.push(Line::from(""));

        // 👤 Account (only if ChatGPT tokens exist), shown under the first block
        let auth_file = get_auth_file(&config.codex_home);
        if let Ok(auth) = try_read_auth_json(&auth_file) {
            if let Some(tokens) = auth.tokens.clone() {
                lines.push(Line::from(vec!["👤 ".into(), "Account".bold()]));
                lines.push(Line::from("  • Signed in with ChatGPT"));

                let info = tokens.id_token;
                if let Some(email) = &info.email {
                    lines.push(Line::from(vec!["  • Login: ".into(), email.clone().into()]));
                }

                match auth.openai_api_key.as_deref() {
                    Some(key) if !key.is_empty() => {
                        lines.push(Line::from(
                            "  • Using API key. Run codex login to use ChatGPT plan",
                        ));
                    }
                    _ => {
                        let plan_text = info
                            .get_chatgpt_plan_type()
                            .map(|s| title_case(&s))
                            .unwrap_or_else(|| "Unknown".to_string());
                        lines.push(Line::from(vec!["  • Plan: ".into(), plan_text.into()]));
                    }
                }

                lines.push(Line::from(""));
            }
        }

        // 🧠 Model
        lines.push(Line::from(vec!["🧠 ".into(), "Model".bold()]));
        lines.push(Line::from(vec![
            "  • Name: ".into(),
            config.model.clone().into(),
        ]));
        let provider_disp = pretty_provider_name(&config.model_provider_id);
        lines.push(Line::from(vec![
            "  • Provider: ".into(),
            provider_disp.into(),
        ]));
        // Only show Reasoning fields if present in config summary
        let reff = lookup("reasoning effort");
        if !reff.is_empty() {
            lines.push(Line::from(vec![
                "  • Reasoning Effort: ".into(),
                title_case(&reff).into(),
            ]));
        }
        let rsum = lookup("reasoning summaries");
        if !rsum.is_empty() {
            lines.push(Line::from(vec![
                "  • Reasoning Summaries: ".into(),
                title_case(&rsum).into(),
            ]));
        }

        lines.push(Line::from(""));

        // 📊 Token Usage
        lines.push(Line::from(vec!["📊 ".into(), "Token Usage".bold()]));
        // Input: <input> [+ <cached> cached]
        let mut input_line_spans: Vec<Span<'static>> = vec![
            "  • Input: ".into(),
            usage.non_cached_input().to_string().into(),
        ];
        if let Some(cached) = usage.cached_input_tokens {
            if cached > 0 {
                input_line_spans.push(format!(" (+ {cached} cached)").into());
            }
        }
        lines.push(Line::from(input_line_spans));
        // Output: <output>
        lines.push(Line::from(vec![
            "  • Output: ".into(),
            usage.output_tokens.to_string().into(),
        ]));
        // Total: <total>
        lines.push(Line::from(vec![
            "  • Total: ".into(),
            usage.blended_total().to_string().into(),
        ]));

        lines.push(Line::from(""));
        HistoryCell::StatusOutput {
            view: TextBlock::new(lines),
        }
    }

    pub(crate) fn new_prompts_output() -> Self {
        let lines: Vec<Line<'static>> = vec![
            Line::from("/prompts".magenta()),
            Line::from(""),
            Line::from(" 1. Explain this codebase"),
            Line::from(" 2. Summarize recent commits"),
            Line::from(" 3. Implement {feature}"),
            Line::from(" 4. Find and fix a bug in @filename"),
            Line::from(" 5. Write tests for @filename"),
            Line::from(" 6. Improve documentation in @filename"),
            Line::from(""),
        ];
        HistoryCell::PromptsOutput {
            view: TextBlock::new(lines),
        }
    }

    pub(crate) fn new_error_event(message: String) -> Self {
        let lines: Vec<Line<'static>> =
            vec![vec!["🖐 ".red().bold(), message.into()].into(), "".into()];
        HistoryCell::ErrorEvent {
            view: TextBlock::new(lines),
        }
    }

    /// Render a user‑friendly plan update styled like a checkbox todo list.
    pub(crate) fn new_plan_update(update: UpdatePlanArgs) -> Self {
        let UpdatePlanArgs { explanation, plan } = update;

        let mut lines: Vec<Line<'static>> = Vec::new();
        // Header with progress summary
        let total = plan.len();
        let completed = plan
            .iter()
            .filter(|p| matches!(p.status, StepStatus::Completed))
            .count();

        let width: usize = 10;
        let filled = if total > 0 {
            (completed * width + total / 2) / total
        } else {
            0
        };
        let empty = width.saturating_sub(filled);

        let mut header: Vec<Span> = Vec::new();
        header.push(Span::raw("📋"));
        header.push(Span::styled(
            " Updated",
            Style::default().add_modifier(Modifier::BOLD).magenta(),
        ));
        header.push(Span::raw(" to do list ["));
        if filled > 0 {
            header.push(Span::styled(
                "█".repeat(filled),
                Style::default().fg(Color::Green),
            ));
        }
        if empty > 0 {
            header.push(Span::styled(
                "░".repeat(empty),
                Style::default().fg(Color::Gray),
            ));
        }
        header.push(Span::raw("] "));
        header.push(Span::raw(format!("{completed}/{total}")));
        lines.push(Line::from(header));

        // Optional explanation/note from the model
        if let Some(expl) = explanation.and_then(|s| {
            let t = s.trim().to_string();
            if t.is_empty() { None } else { Some(t) }
        }) {
            lines.push(Line::from("note".gray().italic()));
            for l in expl.lines() {
                lines.push(Line::from(l.to_string()).gray());
            }
        }

        // Steps styled as checkbox items
        if plan.is_empty() {
            lines.push(Line::from("(no steps provided)".gray().italic()));
        } else {
            for (idx, PlanItemArg { step, status }) in plan.into_iter().enumerate() {
                let (box_span, text_span) = match status {
                    StepStatus::Completed => (
                        Span::styled("✔", Style::default().fg(Color::Green)),
                        Span::styled(
                            step,
                            Style::default()
                                .fg(Color::Gray)
                                .add_modifier(Modifier::CROSSED_OUT | Modifier::DIM),
                        ),
                    ),
                    StepStatus::InProgress => (
                        Span::raw("□"),
                        Span::styled(
                            step,
                            Style::default()
                                .fg(Color::Blue)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ),
                    StepStatus::Pending => (
                        Span::raw("□"),
                        Span::styled(
                            step,
                            Style::default().fg(Color::Gray).add_modifier(Modifier::DIM),
                        ),
                    ),
                };
                let prefix = if idx == 0 {
                    Span::raw("  ⎿ ")
                } else {
                    Span::raw("    ")
                };
                lines.push(Line::from(vec![
                    prefix,
                    box_span,
                    Span::raw(" "),
                    text_span,
                ]));
            }
        }

        lines.push(Line::from(""));

        HistoryCell::PlanUpdate {
            view: TextBlock::new(lines),
        }
    }

    /// Create a new `PendingPatch` cell that lists the file‑level summary of
    /// a proposed patch. The summary lines should already be formatted (e.g.
    /// "A path/to/file.rs").
    pub(crate) fn new_patch_event(
        event_type: PatchEventType,
        changes: HashMap<PathBuf, FileChange>,
    ) -> Self {
        let title = match event_type {
            PatchEventType::ApprovalRequest => "proposed patch",
            PatchEventType::ApplyBegin {
                auto_approved: true,
            } => "✏️ Applying patch",
            PatchEventType::ApplyBegin {
                auto_approved: false,
            } => {
                let lines: Vec<Line<'static>> = vec![
                    Line::from("✏️ Applying patch".magenta().bold()),
                    Line::from(""),
                ];
                return Self::PendingPatch {
                    view: TextBlock::new(lines),
                };
            }
        };

        let summary_lines = create_diff_summary(title, changes);

        let mut lines: Vec<Line<'static>> = Vec::new();

        for line in summary_lines {
            lines.push(line);
        }

        lines.push(Line::from(""));

        HistoryCell::PendingPatch {
            view: TextBlock::new(lines),
        }
    }

    pub(crate) fn new_patch_apply_failure(stderr: String) -> Self {
        let mut lines: Vec<Line<'static>> = Vec::new();

        // Failure title
        lines.push(Line::from("✘ Failed to apply patch".magenta().bold()));

        if !stderr.trim().is_empty() {
            let mut iter = stderr.lines();
            for (i, raw) in iter.by_ref().take(TOOL_CALL_MAX_LINES).enumerate() {
                let prefix = if i == 0 { "  ⎿ " } else { "    " };
                let s = format!("{prefix}{raw}");
                lines.push(ansi_escape_line(&s).dim());
            }
            let remaining = iter.count();
            if remaining > 0 {
                lines.push(Line::from(""));
                lines.push(Line::from(format!("... +{remaining} lines")).dim());
            }
        }

        lines.push(Line::from(""));

        HistoryCell::PatchApplyResult {
            view: TextBlock::new(lines),
        }
    }

    /// Create a simple text line cell for streaming messages
    pub(crate) fn new_text_line(line: Line<'static>) -> Self {
        HistoryCell::BackgroundEvent {
            view: TextBlock::new(vec![line]),
        }
    }
    
    /// Create a streaming content cell for live model output
    pub(crate) fn new_streaming_content(lines: Vec<Line<'static>>) -> Self {
        HistoryCell::BackgroundEvent {
            view: TextBlock::new(lines),
        }
    }

    /// Get processed lines with proper word wrapping and markdown support
    pub(crate) fn get_processed_lines(&self, width: u16) -> Vec<Line<'static>> {
        match self {
            HistoryCell::BackgroundEvent { view } => {
                // Convert the TextBlock lines back to text for processing
                let text = view.lines.iter()
                    .map(|line| {
                        line.spans.iter()
                            .map(|span| span.content.as_ref())
                            .collect::<String>()
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                
                // Process with markdown support and word wrapping
                crate::text_processing::process_markdown_text(&text, width)
            }
            _ => self.plain_lines(),
        }
    }
}

impl WidgetRef for &HistoryCell {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        match self {
            HistoryCell::AnimatedWelcome { start_time, completed, fade_start, faded_out } => {
                let fade_duration = std::time::Duration::from_millis(800); // 0.8 seconds fade
                
                // Check if we're in fade-out phase
                if let Some(fade_time) = fade_start.get() {
                    let fade_elapsed = fade_time.elapsed();
                    if fade_elapsed < fade_duration && !faded_out.get() {
                        // Fade-out animation
                        let fade_progress = fade_elapsed.as_secs_f32() / fade_duration.as_secs_f32();
                        let alpha = 1.0 - fade_progress; // From 1.0 to 0.0
                        
                        crate::glitch_animation::render_intro_animation_with_alpha(
                            area,
                            buf,
                            1.0, // Full animation progress (static state)
                            alpha
                        );
                    } else {
                        // Fade-out complete - mark as faded out
                        faded_out.set(true);
                        // Don't render anything (invisible)
                    }
                } else {
                    // Normal animation phase
                    let elapsed = start_time.elapsed();
                    let animation_duration = std::time::Duration::from_secs(2);  // 2 seconds total
                    
                    if elapsed < animation_duration && !completed.get() {
                        // Calculate animation progress
                        let progress = elapsed.as_secs_f32() / animation_duration.as_secs_f32();
                        
                        // Render the animation (randomly chooses between neon and bracket build)
                        crate::glitch_animation::render_intro_animation(
                            area,
                            buf,
                            progress
                        );
                        
                        // Request redraw for animation
                        // Note: We can't send events from here directly, but the ChatWidget
                        // will check for animation cells and request redraws
                    } else {
                        // Animation complete - mark it and render final static state
                        completed.set(true);
                        
                        // Render the final static state
                        crate::glitch_animation::render_intro_animation(
                            area,
                            buf,
                            1.0  // Full progress = static final state
                        );
                    }
                }
            }
            HistoryCell::BackgroundEvent { .. } => {
                // Use processed lines with markdown support and proper word wrapping
                let processed_lines = self.get_processed_lines(area.width);
                Paragraph::new(Text::from(processed_lines))
                    .style(Style::default()
                        .fg(crate::colors::text())
                        .bg(crate::colors::background()))
                    .render(area, buf);
            }
            _ => {
                // Apply theme background and text color to the paragraph
                Paragraph::new(Text::from(self.plain_lines()))
                    .wrap(Wrap { trim: false })
                    .style(Style::default()
                        .fg(crate::colors::text())
                        .bg(crate::colors::background()))
                    .render(area, buf);
            }
        }
    }
}


fn format_mcp_invocation<'a>(invocation: McpInvocation) -> Line<'a> {
    let args_str = invocation
        .arguments
        .as_ref()
        .map(|v| {
            // Use compact form to keep things short but readable.
            serde_json::to_string(v).unwrap_or_else(|_| v.to_string())
        })
        .unwrap_or_default();

    let invocation_spans = vec![
        Span::styled(invocation.server.clone(), Style::default().fg(Color::Blue)),
        Span::raw("."),
        Span::styled(invocation.tool.clone(), Style::default().fg(Color::Blue)),
        Span::raw("("),
        Span::styled(args_str, Style::default().fg(Color::Gray)),
        Span::raw(")"),
    ];
    Line::from(invocation_spans)
}
