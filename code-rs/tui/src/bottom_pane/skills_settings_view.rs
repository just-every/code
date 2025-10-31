use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::bottom_pane_view::BottomPaneView;
use crate::bottom_pane::BottomPane;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::prelude::Widget;

#[derive(Clone, Debug)]
pub(crate) struct SkillDisplay {
    pub id: String,
    pub description: Option<String>,
    pub source_label: String,
    pub allowed_tools: Vec<String>,
    pub enabled: bool,
}

enum RowKind {
    GlobalToggle,
    Skill { index: usize },
    Paths { label: &'static str, values: Vec<String> },
    Message(&'static str),
    Spacer,
}

struct Row {
    kind: RowKind,
}

pub(crate) struct SkillsSettingsView {
    skills_enabled: bool,
    skills: Vec<SkillDisplay>,
    rows: Vec<Row>,
    selectable: Vec<usize>,
    selected_idx: usize,
    app_event_tx: AppEventSender,
    is_complete: bool,
}

impl SkillsSettingsView {
    pub fn new(
        skills_enabled: bool,
        skills: Vec<SkillDisplay>,
        user_paths: Vec<String>,
        project_paths: Vec<String>,
        app_event_tx: AppEventSender,
    ) -> Self {
        let mut rows = Vec::new();
        let mut selectable = Vec::new();

        rows.push(Row { kind: RowKind::GlobalToggle });
        selectable.push(0);

        if !user_paths.is_empty() || !project_paths.is_empty() {
            rows.push(Row { kind: RowKind::Spacer });
        }
        if !user_paths.is_empty() {
            rows.push(Row {
                kind: RowKind::Paths { label: "User paths", values: user_paths },
            });
        }
        if !project_paths.is_empty() {
            rows.push(Row {
                kind: RowKind::Paths { label: "Project paths", values: project_paths },
            });
        }

        if !skills.is_empty() {
            rows.push(Row { kind: RowKind::Spacer });
        }

        for (idx, _) in skills.iter().enumerate() {
            let row_index = rows.len();
            rows.push(Row { kind: RowKind::Skill { index: idx } });
            selectable.push(row_index);
        }

        if skills.is_empty() {
            rows.push(Row {
                kind: RowKind::Message("No skills discovered yet."),
            });
        }

        Self {
            skills_enabled,
            skills,
            rows,
            selectable,
            selected_idx: 0,
            app_event_tx,
            is_complete: false,
        }
    }

    fn selected_row_index(&self) -> usize {
        self.selectable
            .get(self.selected_idx)
            .copied()
            .unwrap_or(0)
    }

    fn move_selection(&mut self, delta: isize) {
        if self.selectable.is_empty() {
            return;
        }
        let len = self.selectable.len() as isize;
        let mut idx = self.selected_idx as isize + delta;
        if idx < 0 {
            idx = len - 1;
        }
        if idx >= len {
            idx = 0;
        }
        self.selected_idx = idx as usize;
    }

    fn toggle_selected(&mut self) {
        let row_idx = self.selected_row_index();
        match self.rows.get(row_idx).map(|row| &row.kind) {
            Some(RowKind::GlobalToggle) => {
                self.skills_enabled = !self.skills_enabled;
                self.app_event_tx
                    .send(AppEvent::UpdateSkillsEnabled { enabled: self.skills_enabled });
            }
            Some(RowKind::Skill { index }) => {
                if let Some(skill) = self.skills.get_mut(*index) {
                    skill.enabled = !skill.enabled;
                    self.app_event_tx.send(AppEvent::UpdateSkillToggle {
                        skill_id: skill.id.clone(),
                        enable: skill.enabled,
                    });
                }
            }
            _ => {}
        }
    }

    pub fn handle_key_event_direct(&mut self, key_event: KeyEvent) {
        match key_event {
            KeyEvent { code: KeyCode::Up, modifiers: KeyModifiers::NONE, .. } => self.move_selection(-1),
            KeyEvent { code: KeyCode::Down, modifiers: KeyModifiers::NONE, .. } => self.move_selection(1),
            KeyEvent { code: KeyCode::Left, modifiers: KeyModifiers::NONE, .. }
            | KeyEvent { code: KeyCode::Right, modifiers: KeyModifiers::NONE, .. } => {
                self.toggle_selected();
            }
            KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. }
            | KeyEvent { code: KeyCode::Char(' '), modifiers: KeyModifiers::NONE, .. } => {
                self.toggle_selected();
            }
            KeyEvent { code: KeyCode::Esc, .. } => {
                self.is_complete = true;
            }
            _ => {}
        }
    }

    fn render_row(&self, row: &Row, row_idx: usize) -> Vec<Line<'static>> {
        let is_selected = row_idx == self.selected_row_index();
        match &row.kind {
            RowKind::GlobalToggle => {
                let label = if self.skills_enabled { "Enabled" } else { "Disabled" };
                let mut style = Style::default().fg(crate::colors::text());
                if is_selected {
                    style = style
                        .bg(crate::colors::selection())
                        .add_modifier(Modifier::BOLD);
                }
                vec![Line::from(vec![
                    Span::styled("Claude Skills", Style::default().fg(crate::colors::text_dim())),
                    Span::raw(": "),
                    Span::styled(label, style),
                ])]
            }
            RowKind::Skill { index } => {
                let skill = &self.skills[*index];
                let mut style = Style::default().fg(crate::colors::text());
                if is_selected {
                    style = style
                        .bg(crate::colors::selection())
                        .add_modifier(Modifier::BOLD);
                }
                let status = if skill.enabled { "On" } else { "Off" };
                let mut lines = Vec::new();
                lines.push(Line::from(vec![
                    Span::styled(skill.id.clone(), style),
                    Span::raw("  "),
                    Span::styled(status, Style::default().fg(if skill.enabled {
                        crate::colors::success()
                    } else {
                        crate::colors::text_dim()
                    })),
                    Span::raw("  "),
                    Span::styled(skill.source_label.clone(), Style::default().fg(crate::colors::text_dim())),
                ]));
                if let Some(desc) = &skill.description {
                    lines.push(Line::from(vec![Span::styled(
                        desc.clone(),
                        Style::default().fg(crate::colors::dim()),
                    )]));
                }
                if !skill.allowed_tools.is_empty() {
                    lines.push(Line::from(vec![Span::styled(
                        format!("Tools: {}", skill.allowed_tools.join(", ")),
                        Style::default().fg(crate::colors::text_dim()),
                    )]));
                }
                lines
            }
            RowKind::Paths { label, values } => {
                let mut lines = Vec::new();
                lines.push(Line::from(vec![Span::styled(
                    format!("{}:", label),
                    Style::default().fg(crate::colors::text_dim()).add_modifier(Modifier::BOLD),
                )]));
                if values.is_empty() {
                    lines.push(Line::from(vec![Span::styled(
                        "  (none)",
                        Style::default().fg(crate::colors::dim()),
                    )]));
                } else {
                    for value in values {
                        lines.push(Line::from(vec![Span::styled(
                            format!("  {value}"),
                            Style::default().fg(crate::colors::dim()),
                        )]));
                    }
                }
                lines
            }
            RowKind::Message(text) => vec![Line::from(vec![Span::styled(
                text.to_string(),
                Style::default().fg(crate::colors::dim()),
            )])],
            RowKind::Spacer => vec![Line::from("")],
        }
    }

    fn render_lines(&self) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        for (idx, row) in self.rows.iter().enumerate() {
            lines.extend(self.render_row(row, idx));
        }
        lines
    }

    pub fn is_view_complete(&self) -> bool {
        self.is_complete
    }
}

impl<'a> BottomPaneView<'a> for SkillsSettingsView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        self.handle_key_event_direct(key_event);
    }

    fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn desired_height(&self, _width: u16) -> u16 {
        12
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(crate::colors::border()))
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .title(" Skills ");
        let inner = block.inner(area);
        block.render(area, buf);

        let lines = self.render_lines();
        Paragraph::new(lines)
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .render(inner, buf);
    }
}
