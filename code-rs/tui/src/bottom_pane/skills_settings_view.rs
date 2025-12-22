use std::fs;
use code_core::config::find_code_home;
use code_core::protocol::Op;
use code_protocol::skills::{Skill, SkillScope};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::prelude::Widget;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::colors;

use super::form_text_field::{FormTextField, InputFilter};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Focus {
    List,
    Name,
    Body,
    Save,
    Delete,
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    List,
    Edit,
}

pub(crate) struct SkillsSettingsView {
    skills: Vec<Skill>,
    selected: usize,
    focus: Focus,
    name_field: FormTextField,
    body_field: FormTextField,
    status: Option<(String, Style)>,
    app_event_tx: AppEventSender,
    is_complete: bool,
    mode: Mode,
}

impl SkillsSettingsView {
    pub fn new(skills: Vec<Skill>, app_event_tx: AppEventSender) -> Self {
        let mut name_field = FormTextField::new_single_line();
        name_field.set_filter(InputFilter::Id);
        let body_field = FormTextField::new_multi_line();
        Self {
            skills,
            selected: 0,
            focus: Focus::List,
            name_field,
            body_field,
            status: None,
            app_event_tx,
            is_complete: false,
            mode: Mode::List,
        }
    }

    pub fn handle_key_event_direct(&mut self, key: KeyEvent) -> bool {
        if self.is_complete {
            return true;
        }
        match self.mode {
            Mode::List => match key {
                KeyEvent { code: KeyCode::Esc, .. } => {
                    self.is_complete = true;
                    true
                }
                KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. } => {
                    self.enter_editor();
                    true
                }
                KeyEvent { code: KeyCode::Char('n'), modifiers, .. }
                    if modifiers.contains(KeyModifiers::CONTROL) =>
                {
                    self.start_new_skill();
                    true
                }
                other => self.handle_list_key(other),
            },
            Mode::Edit => match key {
                KeyEvent { code: KeyCode::Esc, .. } => {
                    self.mode = Mode::List;
                    self.focus = Focus::List;
                    self.status = None;
                    true
                }
                KeyEvent { code: KeyCode::Tab, .. } => {
                    self.cycle_focus(true);
                    true
                }
                KeyEvent { code: KeyCode::BackTab, .. } => {
                    self.cycle_focus(false);
                    true
                }
                KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. } => {
                    match self.focus {
                        Focus::Save => self.save_current(),
                        Focus::Delete => self.delete_current(),
                        Focus::Cancel => {
                            self.mode = Mode::List;
                            self.focus = Focus::List;
                            self.status = None;
                        }
                        _ => {}
                    }
                    true
                }
                KeyEvent { code: KeyCode::Char('n'), modifiers, .. }
                    if modifiers.contains(KeyModifiers::CONTROL) =>
                {
                    self.start_new_skill();
                    true
                }
                _ => match self.focus {
                    Focus::Name => {
                        self.name_field.handle_key(key);
                        true
                    }
                    Focus::Body => {
                        self.body_field.handle_key(key);
                        true
                    }
                    Focus::Save | Focus::Delete | Focus::Cancel => false,
                    Focus::List => self.handle_list_key(key),
                },
            },
        }
    }

    pub fn is_complete(&self) -> bool {
        self.is_complete
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        self.render_body(area, buf);
    }

    fn render_body(&self, area: Rect, buf: &mut Buffer) {
        match self.mode {
            Mode::List => self.render_list(area, buf),
            Mode::Edit => self.render_form(area, buf),
        }
    }

    fn render_list(&self, area: Rect, buf: &mut Buffer) {
        let mut lines: Vec<Line> = Vec::new();
        for (idx, skill) in self.skills.iter().enumerate() {
            let arrow = if idx == self.selected { ">" } else { " " };
            let name_style = if idx == self.selected {
                Style::default().fg(colors::primary()).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(colors::text())
            };
            let scope_text = match skill.scope {
                SkillScope::Repo => " [repo]",
                SkillScope::User => " [user]",
                SkillScope::System => " [system]",
            };
            let name_span = Span::styled(format!("{arrow} {name}", name = skill.name), name_style);
            let scope_span = Span::styled(scope_text, Style::default().fg(colors::text_dim()));
            let desc_span = Span::styled(
                format!("  {desc}", desc = skill.description),
                Style::default().fg(colors::text_dim()),
            );
            lines.push(Line::from(vec![name_span, scope_span, desc_span]));
        }
        if lines.is_empty() {
            lines.push(Line::from("No skills yet. Press Ctrl+N to create."));
        }

        let add_arrow = if self.selected == self.skills.len() { ">" } else { " " };
        let add_style = if self.selected == self.skills.len() {
            Style::default().fg(colors::primary()).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(colors::success()).add_modifier(Modifier::BOLD)
        };
        lines.push(Line::from(vec![Span::styled(
            format!("{add_arrow} Add new..."),
            add_style,
        )]));

        let title = Paragraph::new(vec![Line::from(Span::styled(
            "Skills are reusable instruction bundles stored as SKILL.md files. Edit the frontmatter to update name and description.",
            Style::default().fg(colors::text_dim()),
        ))])
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true })
        .style(Style::default().bg(colors::background()));

        let list = Paragraph::new(lines)
            .alignment(Alignment::Left)
            .style(Style::default().bg(colors::background()));

        let outer = Block::default()
            .borders(Borders::ALL)
            .style(Style::default().bg(colors::background()));
        let inner = outer.inner(area);
        outer.render(area, buf);
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(inner);

        title.render(chunks[0], buf);
        list.render(chunks[1], buf);
    }

    fn render_form(&self, area: Rect, buf: &mut Buffer) {
        let outer = Block::default()
            .borders(Borders::ALL)
            .title("Skill")
            .style(Style::default().bg(colors::background()));
        let inner = outer.inner(area);
        outer.render(area, buf);

        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(6),
                Constraint::Length(2),
                Constraint::Length(1),
            ])
            .split(inner);

        let name_label = Span::styled("Name", Style::default().fg(colors::text_dim()));
        let name_line = Line::from(vec![name_label]);
        Paragraph::new(name_line).render(vertical[0], buf);

        self.name_field
            .render(vertical[0], buf, matches!(self.focus, Focus::Name));
        self.body_field
            .render(vertical[1], buf, matches!(self.focus, Focus::Body));

        let save_label = if self.focus == Focus::Save { "Save" } else { "Save" };
        let delete_label = if self.focus == Focus::Delete { "Delete" } else { "Delete" };
        let cancel_label = if self.focus == Focus::Cancel { "Cancel" } else { "Cancel" };

        let btn_span = |label: &str, focus: Focus, color: Style| {
            if self.focus == focus {
                Span::styled(label.to_string(), color.bg(colors::primary()).fg(colors::background()))
            } else {
                Span::styled(label.to_string(), color)
            }
        };
        let line = Line::from(vec![
            btn_span(save_label, Focus::Save, Style::default().fg(colors::success()).add_modifier(Modifier::BOLD)),
            Span::raw("   "),
            btn_span(delete_label, Focus::Delete, Style::default().fg(colors::error()).add_modifier(Modifier::BOLD)),
            Span::raw("   "),
            btn_span(cancel_label, Focus::Cancel, Style::default().fg(colors::text_dim()).add_modifier(Modifier::BOLD)),
            Span::raw("    Tab cycle - Enter activates"),
        ]);
        Paragraph::new(line).render(vertical[2], buf);

        if let Some((msg, style)) = &self.status {
            Paragraph::new(Line::from(Span::styled(msg.clone(), *style)))
                .alignment(Alignment::Left)
                .render(vertical[3], buf);
        }
    }

    fn handle_list_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Up => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
                return true;
            }
            KeyCode::Down => {
                let max = self.skills.len();
                if self.selected < max {
                    self.selected += 1;
                }
                return true;
            }
            KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.start_new_skill();
                return true;
            }
            _ => {}
        }
        false
    }

    fn start_new_skill(&mut self) {
        self.selected = self.skills.len();
        self.name_field.set_text("");
        self.body_field.set_text("---\nname: Example Skill\ndescription: Describe this skill\n---\n");
        self.focus = Focus::Name;
        self.status = Some(("New skill".to_string(), Style::default().fg(colors::info())));
        self.mode = Mode::Edit;
    }

    fn load_selected_into_form(&mut self) {
        if let Some(skill) = self.skills.get(self.selected) {
            self.name_field.set_text(&skill_slug(skill));
            self.body_field.set_text(&skill.content);
            self.focus = Focus::Name;
        }
    }

    fn enter_editor(&mut self) {
        if self.selected >= self.skills.len() {
            self.start_new_skill();
        } else {
            self.load_selected_into_form();
            self.mode = Mode::Edit;
        }
    }

    fn cycle_focus(&mut self, forward: bool) {
        let order = [Focus::List, Focus::Name, Focus::Body, Focus::Save, Focus::Delete, Focus::Cancel];
        let mut idx = order.iter().position(|f| *f == self.focus).unwrap_or(0);
        if forward {
            idx = (idx + 1) % order.len();
        } else {
            idx = idx.checked_sub(1).unwrap_or(order.len() - 1);
        }
        self.focus = order[idx];
    }

    fn validate_name(&self, name: &str) -> Result<(), String> {
        let slug = name.trim();
        if slug.is_empty() {
            return Err("Name is required".to_string());
        }
        if !slug
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        {
            return Err("Name must use letters, numbers, '-', '_' or '.'".to_string());
        }

        let dup = self
            .skills
            .iter()
            .enumerate()
            .any(|(idx, skill)| idx != self.selected && skill_slug(skill).eq_ignore_ascii_case(slug));
        if dup {
            return Err("A skill with this name already exists".to_string());
        }
        Ok(())
    }

    fn validate_frontmatter(&self, body: &str) -> Result<(), String> {
        let Some(frontmatter) = extract_frontmatter(body) else {
            return Err("SKILL.md must start with YAML frontmatter".to_string());
        };
        if frontmatter_value(&frontmatter, "name").is_none() {
            return Err("Frontmatter must include name".to_string());
        }
        if frontmatter_value(&frontmatter, "description").is_none() {
            return Err("Frontmatter must include description".to_string());
        }
        Ok(())
    }

    fn save_current(&mut self) {
        if let Some(skill) = self.skills.get(self.selected) {
            if skill.scope != SkillScope::User {
                self.status = Some((
                    "Only user skills can be saved".to_string(),
                    Style::default().fg(colors::error()),
                ));
                return;
            }
        }

        let name = self.name_field.text().trim().to_string();
        let body = self.body_field.text().to_string();
        if let Err(msg) = self.validate_name(&name) {
            self.status = Some((msg, Style::default().fg(colors::error())));
            return;
        }
        if let Err(msg) = self.validate_frontmatter(&body) {
            self.status = Some((msg, Style::default().fg(colors::error())));
            return;
        }

        let code_home = match find_code_home() {
            Ok(path) => path,
            Err(err) => {
                self.status = Some((
                    format!("CODE_HOME unavailable: {err}"),
                    Style::default().fg(colors::error()),
                ));
                return;
            }
        };
        let mut dir = code_home;
        dir.push("skills");
        dir.push(&name);
        if let Err(err) = fs::create_dir_all(&dir) {
            self.status = Some((
                format!("Failed to create skill dir: {err}"),
                Style::default().fg(colors::error()),
            ));
            return;
        }
        let mut path = dir;
        path.push("SKILL.md");
        if let Err(err) = fs::write(&path, &body) {
            self.status = Some((
                format!("Failed to save: {err}"),
                Style::default().fg(colors::error()),
            ));
            return;
        }

        let description = frontmatter_value(&body, "description")
            .unwrap_or_else(|| "No description".to_string());
        let display_name = frontmatter_value(&body, "name").unwrap_or_else(|| name.clone());

        let mut updated = self.skills.clone();
        let new_entry = Skill {
            name: display_name,
            path,
            description,
            scope: SkillScope::User,
            content: body.clone(),
        };
        if self.selected < updated.len() {
            updated[self.selected] = new_entry;
        } else {
            updated.push(new_entry);
            self.selected = updated.len() - 1;
        }
        self.skills = updated;
        self.status = Some(("Saved.".to_string(), Style::default().fg(colors::success())));

        self.app_event_tx.send(AppEvent::CodexOp(Op::ListSkills));
    }

    fn delete_current(&mut self) {
        if self.selected >= self.skills.len() {
            self.status = Some(("Nothing to delete".to_string(), Style::default().fg(colors::warning())));
            self.mode = Mode::List;
            self.focus = Focus::List;
            return;
        }
        let skill = self.skills[self.selected].clone();
        if skill.scope != SkillScope::User {
            self.status = Some((
                "Only user skills can be deleted".to_string(),
                Style::default().fg(colors::error()),
            ));
            return;
        }

        if let Err(err) = fs::remove_file(&skill.path) {
            if err.kind() != std::io::ErrorKind::NotFound {
                self.status = Some((
                    format!("Delete failed: {err}"),
                    Style::default().fg(colors::error()),
                ));
                return;
            }
        }

        if let Some(parent) = skill.path.parent() {
            let _ = fs::remove_dir(parent);
        }

        if self.selected < self.skills.len() {
            self.skills.remove(self.selected);
            if self.selected >= self.skills.len() && !self.skills.is_empty() {
                self.selected = self.skills.len() - 1;
            }
        }

        self.mode = Mode::List;
        self.focus = Focus::List;
        self.status = Some(("Deleted.".to_string(), Style::default().fg(colors::success())));

        self.app_event_tx.send(AppEvent::CodexOp(Op::ListSkills));
    }
}

fn skill_slug(skill: &Skill) -> String {
    skill
        .path
        .parent()
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| skill.name.clone())
}

fn extract_frontmatter(body: &str) -> Option<String> {
    let mut lines = body.lines();
    if lines.next()? != "---" {
        return None;
    }
    let mut frontmatter = String::new();
    for line in lines {
        if line.trim() == "---" {
            return Some(frontmatter);
        }
        frontmatter.push_str(line);
        frontmatter.push('\n');
    }
    None
}

fn frontmatter_value(body: &str, key: &str) -> Option<String> {
    let frontmatter = extract_frontmatter(body)?;
    for line in frontmatter.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(&format!("{key}:")) {
            let value = rest.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}
