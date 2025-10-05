use crossterm::event::{KeyCode, KeyEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;

use super::bottom_pane_view::BottomPaneView;
use super::BottomPane;

#[derive(Clone, Debug)]
pub(crate) struct AgentsOverviewView {
    agents: Vec<(String, bool /*enabled*/ , bool /*installed*/ , String /*command*/ )>,
    commands: Vec<String>,
    selected: usize,
    is_complete: bool,
    app_event_tx: AppEventSender,
}

impl AgentsOverviewView {
    pub fn new(
        agents: Vec<(String, bool, bool, String)>,
        commands: Vec<String>,
        selected_index: usize,
        app_event_tx: AppEventSender,
    ) -> Self {
        let mut view = Self { agents, commands, selected: 0, is_complete: false, app_event_tx };
        let total = view.total_rows();
        if total == 0 {
            view.selected = 0;
        } else {
            view.selected = selected_index.min(total.saturating_sub(1));
        }
        view
    }

    fn total_rows(&self) -> usize { self.agents.len().saturating_add(self.commands.len()).saturating_add(1) /* Add new… */ }

    fn build_lines(&self) -> Vec<Line<'static>> {
        let mut lines: Vec<Line<'static>> = Vec::new();

        // Agents section
        lines.push(Line::from(Span::styled("Agents", Style::default().add_modifier(Modifier::BOLD))));
        let max_name_len = self
            .agents
            .iter()
            .map(|(name, _, _, _)| name.len())
            .max()
            .unwrap_or(0);
        for (i, (name, enabled, installed, _cmd)) in self.agents.iter().enumerate() {
            let sel = i == self.selected;
            let (status_text, status_color) = if !*enabled {
                ("disabled", crate::colors::error())
            } else if !*installed {
                ("not installed", crate::colors::warning())
            } else {
                ("enabled", crate::colors::success())
            };
            let dot_style = Style::default().fg(status_color);
            let name_style = if sel { Style::default().fg(crate::colors::primary()).add_modifier(Modifier::BOLD) } else { Style::default() };
            let mut spans = vec![
                Span::styled(if sel { "› " } else { "  " }, if sel { Style::default().fg(crate::colors::primary()) } else { Style::default() }),
                Span::styled(
                    format!("{name:<width$}", name = name, width = max_name_len),
                    name_style,
                ),
                Span::raw("  "),
                Span::styled("•", dot_style),
                Span::raw(" "),
                Span::styled(status_text.to_string(), Style::default().fg(status_color)),
            ];
            if sel {
                spans.push(Span::raw("  "));
                let hint = if !*installed {
                    "(press Enter to install)"
                } else {
                    "(press Enter to configure)"
                };
                spans.push(Span::styled(hint, Style::default().fg(crate::colors::text_dim())));
            }
            lines.push(Line::from(spans));
        }

        // Spacer between sections (always a single blank row)
        lines.push(Line::from(""));

        // Commands section
        lines.push(Line::from(Span::styled("Commands", Style::default().add_modifier(Modifier::BOLD))));
        for (j, cmd) in self.commands.iter().enumerate() {
            let idx = self.agents.len() + j;
            let sel = idx == self.selected;
            let name_style = if sel { Style::default().fg(crate::colors::primary()).add_modifier(Modifier::BOLD) } else { Style::default() };
            let mut spans = vec![
                Span::styled(if sel { "› " } else { "  " }, if sel { Style::default().fg(crate::colors::primary()) } else { Style::default() }),
                Span::styled(format!("/{}", cmd), name_style),
            ];
            if sel {
                spans.push(Span::raw("  "));
                spans.push(Span::styled("(press Enter to configure)", Style::default().fg(crate::colors::text_dim())));
            }
            lines.push(Line::from(spans));
        }

        // Add new… row
        let add_idx = self.agents.len() + self.commands.len();
        let add_sel = add_idx == self.selected;
        let mut add_spans = vec![
            Span::styled(if add_sel { "› " } else { "  " }, if add_sel { Style::default().fg(crate::colors::primary()) } else { Style::default() }),
            Span::styled("Add new…", if add_sel { Style::default().fg(crate::colors::primary()).add_modifier(Modifier::BOLD) } else { Style::default() }),
        ];
        if add_sel {
            add_spans.push(Span::raw("  "));
            add_spans.push(Span::styled("(press Enter to add)", Style::default().fg(crate::colors::text_dim())));
        }
        lines.push(Line::from(add_spans));

        // Footer with key hints (prefixed by a single blank line)
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("↑↓", Style::default().fg(crate::colors::function())),
            Span::styled(" Navigate  ", Style::default().fg(crate::colors::text_dim())),
            Span::styled("Enter", Style::default().fg(crate::colors::success())),
            Span::styled(" Configure  ", Style::default().fg(crate::colors::text_dim())),
            Span::styled("Esc", Style::default().fg(crate::colors::error())),
            Span::styled(" Close", Style::default().fg(crate::colors::text_dim())),
        ]));

        lines
    }
}

impl<'a> BottomPaneView<'a> for AgentsOverviewView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        match key_event {
            KeyEvent { code: KeyCode::Esc, .. } => { self.is_complete = true; }
            KeyEvent { code: KeyCode::Up, .. } => {
                if self.total_rows() == 0 { return; }
                if self.selected == 0 { self.selected = self.total_rows() - 1; } else { self.selected -= 1; }
                self.app_event_tx.send(AppEvent::AgentsOverviewSelectionChanged { index: self.selected });
            }
            KeyEvent { code: KeyCode::Down, .. } => {
                if self.total_rows() == 0 { return; }
                self.selected = (self.selected + 1) % self.total_rows();
                self.app_event_tx.send(AppEvent::AgentsOverviewSelectionChanged { index: self.selected });
            }
            KeyEvent { code: KeyCode::Enter, .. } => {
                let idx = self.selected;
                if idx < self.agents.len() {
                    // Open Agent editor
                    let (name, _en, installed, _cmd) = self.agents[idx].clone();
                    if !installed {
                        self.app_event_tx.send(AppEvent::RequestAgentInstall { name, selected_index: idx });
                        self.is_complete = true;
                    } else {
                        self.app_event_tx.send(AppEvent::ShowAgentEditor { name });
                        self.is_complete = true;
                    }
                } else {
                    // Commands region: specific name or Add new…
                    let cmd_idx = idx - self.agents.len();
                    if cmd_idx < self.commands.len() {
                        if let Some(name) = self.commands.get(cmd_idx) {
                            self.app_event_tx.send(AppEvent::ShowSubagentEditorForName { name: name.clone() });
                            self.is_complete = true;
                        }
                    } else {
                        self.app_event_tx.send(AppEvent::ShowSubagentEditorNew);
                        self.is_complete = true;
                    }
                }
            }
            _ => {}
        }
    }

    fn is_complete(&self) -> bool { self.is_complete }

    fn desired_height(&self, _width: u16) -> u16 {
        let lines = self.build_lines();
        lines.len().saturating_add(2) as u16
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(crate::colors::border()))
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .title(" Agents ")
            .title_alignment(Alignment::Center);
        let inner = block.inner(area);
        block.render(area, buf);

        let lines = self.build_lines();
        Paragraph::new(lines)
            .alignment(Alignment::Left)
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .render(Rect { x: inner.x.saturating_add(1), y: inner.y, width: inner.width.saturating_sub(2), height: inner.height }, buf);
    }
}
