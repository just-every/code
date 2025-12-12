use std::path::PathBuf;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget, WidgetRef, Wrap};
use code_core::config::set_project_access_mode;
use code_core::config::set_project_trusted;
use code_core::protocol::AskForApproval;
use code_core::protocol::SandboxPolicy;
use code_protocol::config_types::SandboxMode as SandboxModeCfg;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;

use crate::colors;

use crate::onboarding::onboarding_screen::KeyboardHandler;
use crate::onboarding::onboarding_screen::StepStateProvider;

use super::onboarding_screen::StepState;
use crate::app::ChatWidgetArgs;
use std::sync::Arc;
use std::sync::Mutex;

pub(crate) struct TrustDirectoryWidget {
    pub code_home: PathBuf,
    pub cwd: PathBuf,
    pub is_git_repo: bool,
    pub selection: Option<TrustDirectorySelection>,
    pub highlighted: TrustDirectorySelection,
    pub error: Option<String>,
    pub chat_widget_args: Arc<Mutex<ChatWidgetArgs>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TrustDirectorySelection {
    Trust,
    DontTrust,
}

impl WidgetRef for &TrustDirectoryWidget {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let success_style = Style::default()
            .fg(colors::success())
            .add_modifier(Modifier::BOLD);
        let mut lines: Vec<Line> = vec![
            Line::from(vec![
                Span::styled("You are running Code in ", success_style),
                Span::styled(
                    self.cwd.to_string_lossy().to_string(),
                    Style::default().fg(colors::success()),
                ),
            ]),
            Line::from(""),
        ];

        if self.is_git_repo {
            lines.push(Line::from(
                "  Since this folder is version controlled, you may wish to allow Code",
            ));
            lines.push(Line::from(
                "  to work in this folder without asking for approval.",
            ));
        } else {
            lines.push(Line::from(
                "  Since this folder is not version controlled, we recommend requiring",
            ));
            lines.push(Line::from("  approval of all edits and commands."));
        }
        lines.push(Line::from(""));

        let create_option =
            |idx: usize, option: TrustDirectorySelection, text: &str| -> Line<'static> {
                let is_selected = self.highlighted == option;
                if is_selected {
                    Line::from(vec![
                        Span::styled(
                            format!("> {}. ", idx + 1),
                            Style::default().fg(colors::primary()),
                        ),
                        Span::styled(text.to_owned(), Style::default().fg(colors::primary())),
                    ])
                } else {
                    Line::from(format!("  {}. {}", idx + 1, text))
                }
            };

        if self.is_git_repo {
            lines.push(create_option(
                0,
                TrustDirectorySelection::Trust,
                "Yes, allow Code to work in this folder without asking for approval",
            ));
            lines.push(create_option(
                1,
                TrustDirectorySelection::DontTrust,
                "No, ask me to approve edits and commands",
            ));
        } else {
            lines.push(create_option(
                0,
                TrustDirectorySelection::Trust,
                "Allow Code to work in this folder without asking for approval",
            ));
            lines.push(create_option(
                1,
                TrustDirectorySelection::DontTrust,
                "Require approval of edits and commands",
            ));
        }
        lines.push(Line::from(""));
        if let Some(error) = &self.error {
            lines.push(Line::from(format!("  {error}")).fg(crate::colors::error()));
            lines.push(Line::from(""));
        }
        lines.push(Line::from(vec![
            Span::raw("  Press "),
            Span::styled("Enter", Style::default().fg(colors::function())),
            Span::raw(" to continue"),
        ]));

        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .render(area, buf);
    }
}

impl KeyboardHandler for TrustDirectoryWidget {
    fn handle_key_event(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.highlighted = TrustDirectorySelection::Trust;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.highlighted = TrustDirectorySelection::DontTrust;
            }
            KeyCode::Char('1') => self.handle_trust(),
            KeyCode::Char('2') => self.handle_dont_trust(),
            KeyCode::Enter => match self.highlighted {
                TrustDirectorySelection::Trust => self.handle_trust(),
                TrustDirectorySelection::DontTrust => self.handle_dont_trust(),
            },
            _ => {}
        }
    }
}

impl StepStateProvider for TrustDirectoryWidget {
    fn get_step_state(&self) -> StepState {
        match self.selection {
            Some(_) => StepState::Complete,
            None => StepState::InProgress,
        }
    }
}

impl TrustDirectoryWidget {
    fn handle_trust(&mut self) {
        if let Err(e) = set_project_trusted(&self.code_home, &self.cwd) {
            tracing::error!("Failed to set project trusted: {e:?}");
            self.error = Some(e.to_string());
            // self.error = Some("Failed to set project trusted".to_string());
        }

        // Update the in-memory chat config for this session to a more permissive
        // policy suitable for a trusted workspace (Full Access).
        if let Ok(mut args) = self.chat_widget_args.lock() {
            args.config.approval_policy = AskForApproval::Never;
            args.config.sandbox_policy = SandboxPolicy::DangerFullAccess;
        }

        // Persist the access mode explicitly so subsequent runs don't rely solely on
        // the trust fallback logic and so the UI reflects the chosen mode immediately.
        if let Err(e) = set_project_access_mode(
            &self.code_home,
            &self.cwd,
            AskForApproval::Never,
            SandboxModeCfg::DangerFullAccess,
        ) {
            tracing::warn!("Failed to persist project access mode: {e:?}");
        }

        self.selection = Some(TrustDirectorySelection::Trust);
    }

    fn handle_dont_trust(&mut self) {
        self.highlighted = TrustDirectorySelection::DontTrust;
        self.selection = Some(TrustDirectorySelection::DontTrust);
    }
}
