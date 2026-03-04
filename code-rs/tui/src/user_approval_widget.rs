//! A modal widget that prompts the user to approve or deny an action
//! requested by the agent.
//!
//! This is a (very) rough port of
//! `src/components/chat/terminal-chat-command-review.tsx` from the TypeScript
//! UI to Rust using [`ratatui`]. The goal is feature‑parity for the keyboard
//! driven workflow – a fully‑fledged visual match is not required.

use std::path::Path;
use std::path::PathBuf;
use code_core::protocol::Op;
use code_core::protocol::ReviewDecision;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::prelude::*;
use ratatui::text::{Line, Span};
use ratatui::widgets::Block;
use ratatui::widgets::BorderType;
use ratatui::widgets::Borders;
use ratatui::widgets::Paragraph;
use ratatui::widgets::WidgetRef;
use ratatui::widgets::Wrap;
use shlex::split as shlex_split;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::chatwidget::BackgroundOrderTicket;
use crate::exec_command::strip_bash_lc_and_escape;
use code_core::protocol::ApprovedCommandMatchKind;

/// Request coming from the agent that needs user approval.
pub(crate) enum ApprovalRequest {
    Exec {
        id: String,
        command: Vec<String>,
        reason: Option<String>,
    },
    ApplyPatch {
        id: String,
        reason: Option<String>,
        grant_root: Option<PathBuf>,
    },
    TerminalCommand {
        id: u64,
        command: String,
    },
}

#[derive(Clone)]
struct SelectOption {
    label: String,
    description: String,
    hotkey: KeyCode,
    action: SelectAction,
}

#[derive(Clone)]
enum SelectAction {
    ApproveOnce,
    ApproveForSession {
        command: Vec<String>,
        match_kind: ApprovedCommandMatchKind,
        persist: bool,
        semantic_prefix: Option<Vec<String>>,
    },
    Abort,
}

/// A modal prompting the user to approve or deny the pending request.
pub(crate) struct UserApprovalWidget<'a> {
    approval_request: ApprovalRequest,
    app_event_tx: AppEventSender,
    before_ticket: BackgroundOrderTicket,
    confirmation_prompt: Paragraph<'a>,
    select_options: Vec<SelectOption>,

    /// Currently selected index in *select* mode.
    selected_option: usize,

    /// Set to `true` once a decision has been sent – the parent view can then
    /// remove this widget from its queue.
    done: bool,
}

impl UserApprovalWidget<'_> {
    pub(crate) fn new(
        approval_request: ApprovalRequest,
        before_ticket: BackgroundOrderTicket,
        app_event_tx: AppEventSender,
    ) -> Self {
        let confirmation_prompt = match &approval_request {
            ApprovalRequest::Exec {
                command, reason, ..
            } => {
                let cmd = strip_bash_lc_and_escape(command);
                // Present a single-line summary without cwd: "codex wants to run: <cmd>"
                let mut cmd_span: Span = cmd.clone().into();
                cmd_span.style = cmd_span.style.add_modifier(Modifier::DIM);
                let mut contents: Vec<Line> = vec![
                    Line::from(""), // extra spacing above the prompt
                    Line::from(vec![
                        "? ".fg(crate::colors::info()),
                        "Code wants to run ".bold(),
                        cmd_span,
                    ]),
                    Line::from(""),
                ];
                if let Some(reason) = reason {
                    contents.push(Line::from(reason.clone().italic()));
                    contents.push(Line::from(""));
                }
                Paragraph::new(contents).wrap(Wrap { trim: false })
            }
            ApprovalRequest::ApplyPatch {
                reason, grant_root, ..
            } => {
                let mut contents: Vec<Line> = vec![];

                if let Some(r) = reason {
                    contents.push(Line::from(r.clone().italic()));
                    contents.push(Line::from(""));
                }

                if let Some(root) = grant_root {
                    contents.push(Line::from(format!(
                        "This will grant write access to {} for the remainder of this session.",
                        root.display()
                    )));
                    contents.push(Line::from(""));
                }

                Paragraph::new(contents).wrap(Wrap { trim: false })
            }
            ApprovalRequest::TerminalCommand { command, .. } => {
                let mut cmd_span: Span = format!("$ {command}").into();
                cmd_span.style = cmd_span.style.add_modifier(Modifier::DIM);
                let contents = vec![
                    Line::from(""),
                    Line::from(vec![
                        "? ".fg(crate::colors::info()),
                        "Run shell command ".bold(),
                        cmd_span,
                        " now?".into(),
                    ]),
                    Line::from(""),
                ];
                Paragraph::new(contents).wrap(Wrap { trim: false })
            }
        };

        let select_options = match &approval_request {
            ApprovalRequest::Exec { command, .. } => build_exec_select_options(command),
            ApprovalRequest::ApplyPatch { .. } => build_patch_select_options(),
            ApprovalRequest::TerminalCommand { .. } => build_terminal_select_options(),
        };

        Self {
            approval_request,
            app_event_tx,
            before_ticket,
            confirmation_prompt,
            select_options,
            selected_option: 0,
            done: false,
        }
    }

    fn get_confirmation_prompt_height(&self, width: u16) -> u16 {
        // Should cache this for last value of width.
        self.confirmation_prompt.line_count(width) as u16
    }

    /// Process a `KeyEvent` coming from crossterm. Always consumes the event
    /// while the modal is visible.
    /// Process a key event originating from crossterm. As the modal fully
    /// captures input while visible, we don’t need to report whether the event
    /// was consumed—callers can assume it always is.
    pub(crate) fn handle_key_event(&mut self, key: KeyEvent) {
        // Prevent duplicate decisions if the key auto‑repeats while the modal
        // is being torn down.
        if self.done {
            return;
        }
        // Accept both Press and Repeat to accommodate Windows terminals that
        // may emit an initial Repeat for some keys (e.g. Enter) when keyboard
        // enhancement flags are enabled.
        if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            self.handle_select_key(key);
        }
    }

    /// Normalize a key for comparison.
    /// - For `KeyCode::Char`, converts to lowercase for case-insensitive matching.
    /// - Other key codes are returned unchanged.
    fn normalize_keycode(code: KeyCode) -> KeyCode {
        match code {
            KeyCode::Char(c) => KeyCode::Char(c.to_ascii_lowercase()),
            other => other,
        }
    }

    /// Handle Ctrl-C pressed by the user while the modal is visible.
    /// Behaves like pressing Escape: abort the request and close the modal.
    pub(crate) fn on_ctrl_c(&mut self) {
        self.send_decision(ReviewDecision::Abort);
    }

    fn handle_select_key(&mut self, key_event: KeyEvent) {
        let len = self.select_options.len();
        if len == 0 {
            return;
        }
        match key_event.code {
            KeyCode::Up | KeyCode::Left => {
                self.selected_option = if self.selected_option == 0 {
                    len - 1
                } else {
                    self.selected_option - 1
                };
            }
            KeyCode::Down | KeyCode::Right => {
                self.selected_option = (self.selected_option + 1) % len;
            }
            KeyCode::Enter => {
                if let Some(option) = self.select_options.get(self.selected_option).cloned() {
                    self.perform_action(option.action);
                }
            }
            KeyCode::Esc => {
                self.perform_action(SelectAction::Abort);
            }
            other => {
                let normalized = Self::normalize_keycode(other);
                if let Some((idx, option)) = self
                    .select_options
                    .iter()
                    .enumerate()
                    .find(|(_, opt)| Self::normalize_keycode(opt.hotkey) == normalized)
                {
                    self.selected_option = idx;
                    self.perform_action(option.action.clone());
                }
            }
        }
    }

    fn send_decision(&mut self, decision: ReviewDecision) {
        self.send_decision_with_feedback(decision, String::new())
    }

    fn send_decision_with_feedback(&mut self, decision: ReviewDecision, feedback: String) {
        if let ApprovalRequest::TerminalCommand { id, .. } = &self.approval_request {
            let approved = matches!(decision, ReviewDecision::Approved | ReviewDecision::ApprovedForSession);
            self.app_event_tx
                .send(AppEvent::TerminalApprovalDecision { id: *id, approved });
            self.done = true;
            return;
        }

        // Emit a background event instead of an assistant message.
        let message = match &self.approval_request {
            ApprovalRequest::Exec { command, .. } => {
                let cmd = strip_bash_lc_and_escape(command);
                match decision {
                    ReviewDecision::Approved => format!("approved: run {} (this time)", cmd),
                    ReviewDecision::ApprovedForSession => format!("approved: run {} (every time this session)", cmd),
                    ReviewDecision::Denied => format!("not approved: run {}", cmd),
                    ReviewDecision::Abort => format!("canceled: run {}", cmd),
                }
            }
            ApprovalRequest::ApplyPatch { .. } => {
                format!("patch approval decision: {:?}", decision)
            }
            ApprovalRequest::TerminalCommand { .. } => unreachable!("terminal approvals handled earlier"),
        };
        let message = if feedback.trim().is_empty() {
            message
        } else {
            // Append feedback, preserving line breaks
            format!("{}\nfeedback:\n{}", message, feedback)
        };
        // Insert above the upcoming command begin so the decision reads first.
        self.app_event_tx
            .send_background_before_next_output_with_ticket(&self.before_ticket, message);

        // If the user aborted an exec approval, immediately cancel any running task
        // so the UI reflects their intent (clear spinner/status) without waiting
        // for backend cleanup. Core still receives the Abort below.
        match (&self.approval_request, decision) {
            (ApprovalRequest::Exec { .. }, ReviewDecision::Abort) => {
                self.app_event_tx.send(AppEvent::CancelRunningTask);
            }
            (ApprovalRequest::Exec { .. }, ReviewDecision::Denied) => {
                self.app_event_tx.send(AppEvent::MarkTaskIdle);
            }
            (ApprovalRequest::ApplyPatch { .. }, ReviewDecision::Abort) => {
                self.app_event_tx.send(AppEvent::CancelRunningTask);
            }
            (ApprovalRequest::ApplyPatch { .. }, ReviewDecision::Denied) => {
                self.app_event_tx.send(AppEvent::MarkTaskIdle);
            }
            _ => {}
        }

        let op = match &self.approval_request {
            ApprovalRequest::Exec { id, .. } => Op::ExecApproval {
                id: id.clone(),
                turn_id: None,
                decision,
            },
            ApprovalRequest::ApplyPatch { id, .. } => Op::PatchApproval {
                id: id.clone(),
                decision,
            },
            ApprovalRequest::TerminalCommand { .. } => unreachable!("terminal approvals handled earlier"),
        };

        self.app_event_tx.send(AppEvent::CodexOp(op));
        self.done = true;
    }

    fn perform_action(&mut self, action: SelectAction) {
        match action {
            SelectAction::ApproveOnce => {
                self.send_decision(ReviewDecision::Approved);
            }
            SelectAction::ApproveForSession {
                command,
                match_kind,
                persist,
                semantic_prefix,
            } => {
                self.app_event_tx.send(AppEvent::RegisterApprovedCommand {
                    command: command.clone(),
                    match_kind: match_kind.clone(),
                    persist,
                    semantic_prefix: semantic_prefix.clone(),
                });
                self.send_decision(ReviewDecision::ApprovedForSession);
            }
            SelectAction::Abort => {
                self.send_decision(ReviewDecision::Abort);
            }
        }
    }

    /// Returns `true` once the user has made a decision and the widget no
    /// longer needs to be displayed.
    pub(crate) fn is_complete(&self) -> bool {
        self.done
    }

    pub(crate) fn desired_height(&self, width: u16) -> u16 {
        let prompt = self.get_confirmation_prompt_height(width);
        let option_lines = (self.select_options.len() as u16).saturating_mul(2);
        prompt + option_lines + 2
    }
}

impl WidgetRef for &UserApprovalWidget<'_> {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let prompt_height = self.get_confirmation_prompt_height(area.width);
        let [prompt_chunk, options_chunk] = Layout::vertical([
            Constraint::Length(prompt_height),
            Constraint::Min(0),
        ])
        .areas(area);

        self.confirmation_prompt.clone().render(prompt_chunk, buf);

        let mut lines: Vec<Line> = Vec::new();
        for (idx, option) in self.select_options.iter().enumerate() {
            let selected = idx == self.selected_option;
            let indicator = if selected { "› " } else { "  " };
            let line_style = if selected {
                Style::default()
                    .fg(crate::colors::primary())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let label = format!("{}{}{}", indicator, option.label, hotkey_suffix(option.hotkey));
            lines.push(Line::from(Span::styled(label, line_style)));

            let desc_style = Style::default()
                .fg(crate::colors::text_dim())
                .add_modifier(Modifier::ITALIC);
            lines.push(Line::from(Span::styled(
                format!("    {}", option.description),
                desc_style,
            )));
            lines.push(Line::from(""));
        }
        if !lines.is_empty() {
            lines.pop();
        }

        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .render(options_chunk.inner(Margin::new(1, 0)), buf);

        Block::bordered()
            .border_type(BorderType::QuadrantOutside)
            .border_style(Style::default().fg(crate::colors::light_blue()))
            .borders(Borders::LEFT)
            .render_ref(Rect::new(0, options_chunk.y, 1, options_chunk.height), buf);
    }
}

fn build_exec_select_options(command: &[String]) -> Vec<SelectOption> {
    let mut options = Vec::new();

    options.push(SelectOption {
        label: "Yes".to_string(),
        description: "Approve and run the command".to_string(),
        hotkey: KeyCode::Char('y'),
        action: SelectAction::ApproveOnce,
    });

    let full_display = strip_bash_lc_and_escape(command);
    options.push(SelectOption {
        label: format!("Always allow '{full_display}' for this project"),
        description: "Approve this exact command automatically next time".to_string(),
        hotkey: KeyCode::Char('a'),
        action: SelectAction::ApproveForSession {
            command: command.to_vec(),
            match_kind: ApprovedCommandMatchKind::Exact,
            persist: true,
            semantic_prefix: None,
        },
    });

    let normalized_tokens = normalized_command_tokens(command);
    if let Some(tokens) = normalized_tokens.as_ref() {
        if let Some(prefix) = prefix_candidate(tokens) {
            let prefix_display = strip_bash_lc_and_escape(&prefix);
            let prefix_with_wildcard = format!("{prefix_display} *");
        options.push(SelectOption {
            label: format!("Always allow '{prefix_with_wildcard}' for this project"),
            description: "Approve any command starting with this prefix".to_string(),
            hotkey: KeyCode::Char('p'),
            action: SelectAction::ApproveForSession {
                command: prefix.clone(),
                match_kind: ApprovedCommandMatchKind::Prefix,
                persist: true,
                semantic_prefix: Some(prefix),
            },
        });
    }
    }

    options.push(SelectOption {
        label: "No, provide feedback".to_string(),
        description: "Do not run the command; provide feedback".to_string(),
        hotkey: KeyCode::Char('n'),
        action: SelectAction::Abort,
    });

    options
}

fn build_patch_select_options() -> Vec<SelectOption> {
    vec![
        SelectOption {
            label: "Yes".to_string(),
            description: "Approve and apply the changes".to_string(),
            hotkey: KeyCode::Char('y'),
            action: SelectAction::ApproveOnce,
        },
        SelectOption {
            label: "No, provide feedback".to_string(),
            description: "Do not apply the changes; provide feedback".to_string(),
            hotkey: KeyCode::Char('n'),
            action: SelectAction::Abort,
        },
    ]
}

fn build_terminal_select_options() -> Vec<SelectOption> {
    vec![
        SelectOption {
            label: "Yes".to_string(),
            description: "Approve and run the command".to_string(),
            hotkey: KeyCode::Char('y'),
            action: SelectAction::ApproveOnce,
        },
        SelectOption {
            label: "No".to_string(),
            description: "Dismiss without running the command".to_string(),
            hotkey: KeyCode::Char('n'),
            action: SelectAction::Abort,
        },
    ]
}

fn normalized_command_tokens(command: &[String]) -> Option<Vec<String>> {
    if command.is_empty() {
        return None;
    }

    if command.len() == 3 && is_shell_wrapper(&command[0], &command[1]) {
        if let Some(script_tokens) = shlex_split(&command[2]) {
            return Some(script_tokens);
        }
        return Some(vec![command[2].clone()]);
    }

    Some(command.to_vec())
}

fn prefix_candidate(tokens: &[String]) -> Option<Vec<String>> {
    if tokens.len() < 2 {
        return None;
    }

    let mut prefix: Vec<String> = Vec::with_capacity(tokens.len());
    for (idx, token) in tokens.iter().enumerate() {
        if idx == 0 {
            prefix.push(token.clone());
            continue;
        }

        if token.starts_with('-')
            || token.contains('/')
            || token.contains('.')
            || token.contains('\\')
        {
            break;
        }

        prefix.push(token.clone());
        if prefix.len() == 3 {
            break;
        }
    }

    if prefix.len() >= 2 && prefix.len() < tokens.len() {
        Some(prefix)
    } else {
        None
    }
}

fn is_shell_wrapper(shell: &str, flag: &str) -> bool {
    let file_name = Path::new(shell)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(shell)
        .to_ascii_lowercase();
    matches!(file_name.as_str(), "bash" | "sh" | "zsh") && matches!(flag, "-lc" | "-c")
}

fn hotkey_suffix(key: KeyCode) -> String {
    match key {
        KeyCode::Char(c) => format!(" ({})", c.to_ascii_lowercase()),
        _ => String::new(),
    }
}
