use std::fs;
use std::path::PathBuf;

use code_core::config::find_code_home;
use code_core::protocol::Op;
use code_protocol::custom_prompts::CustomPrompt;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::prelude::Widget;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::colors;
use crate::slash_command::built_in_slash_commands;

use super::form_text_field::{FormTextField, InputFilter};
use super::settings_panel::{render_panel, PanelFrameStyle};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Focus {
    List,
    Name,
    Body,
    Save,
}

pub(crate) struct PromptsSettingsView {
    prompts: Vec<CustomPrompt>,
    selected: usize,
    focus: Focus,
    name_field: FormTextField,
    body_field: FormTextField,
    status: Option<(String, Style)>,
    app_event_tx: AppEventSender,
    is_complete: bool,
}

impl PromptsSettingsView {
    pub fn new(prompts: Vec<CustomPrompt>, app_event_tx: AppEventSender) -> Self {
        let mut name_field = FormTextField::new_single_line();
        name_field.set_filter(InputFilter::Id);
        let body_field = FormTextField::new_multi_line();
        let mut view = Self {
            prompts,
            selected: 0,
            focus: Focus::List,
            name_field,
            body_field,
            status: None,
            app_event_tx,
            is_complete: false,
        };
        view.load_selected_into_form();
        view
    }

    pub fn handle_key_event_direct(&mut self, key: KeyEvent) -> bool {
        if self.is_complete {
            return true;
        }
        match key {
            KeyEvent { code: KeyCode::Esc, .. } => {
                self.is_complete = true;
                return true;
            }
            KeyEvent { code: KeyCode::Tab, .. } => {
                self.cycle_focus(true);
                return true;
            }
            KeyEvent { code: KeyCode::BackTab, .. } => {
                self.cycle_focus(false);
                return true;
            }
            KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. } => {
                match self.focus {
                    Focus::List => {
                        self.load_selected_into_form();
                    }
                    Focus::Save => {
                        self.save_current();
                    }
                    _ => {
                        // fall through to field handling
                    }
                }
            }
            KeyEvent { code: KeyCode::Char('n'), modifiers, .. } if modifiers.contains(KeyModifiers::CONTROL) => {
                self.start_new_prompt();
                return true;
            }
            _ => {}
        }

        match self.focus {
            Focus::List => self.handle_list_key(key),
            Focus::Name => { self.name_field.handle_key(key); true }
            Focus::Body => { self.body_field.handle_key(key); true }
            Focus::Save => false,
        }
    }

    pub fn is_complete(&self) -> bool { self.is_complete }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 { return; }
        render_panel(
            area,
            buf,
            "Prompts",
            PanelFrameStyle::overlay(),
            |inner, buf| self.render_body(inner, buf),
        );
    }

    fn render_body(&self, area: Rect, buf: &mut Buffer) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(area);

        self.render_list(chunks[0], buf);
        self.render_form(chunks[1], buf);
    }

    fn render_list(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 { return; }
        let mut lines: Vec<Line> = Vec::new();
        for (idx, p) in self.prompts.iter().enumerate() {
            let preview = p.content.lines().next().unwrap_or("").trim();
            let name_span = Span::styled(
                format!("/{}", p.name),
                Style::default().fg(colors::primary()).add_modifier(Modifier::BOLD),
            );
            let preview_span = Span::styled(
                format!("  {}", preview),
                Style::default().fg(colors::text_dim()),
            );
            let mut spans = vec![name_span];
            if !preview.is_empty() { spans.push(preview_span); }
            let mut line = Line::from(spans);
            if idx == self.selected && matches!(self.focus, Focus::List) {
                line = line.style(Style::default().add_modifier(Modifier::REVERSED));
            }
            lines.push(line);
        }
        if lines.is_empty() {
            lines.push(Line::from("No prompts yet. Press Ctrl+N to create."));
        }

        let list = Paragraph::new(lines)
            .alignment(Alignment::Left)
            .block(Block::default().borders(Borders::ALL).title("Custom Prompts"));
        list.render(area, buf);
    }

    fn render_form(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 { return; }
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(5),
                Constraint::Length(2),
                Constraint::Length(1),
            ])
            .split(area);

        // Name field
        let name_title = if matches!(self.focus, Focus::Name) { "Name (slug) • Enter to save" } else { "Name (slug)" };
        let name_block = Block::default().borders(Borders::ALL).title(name_title);
        let inner = name_block.inner(vertical[0]);
        name_block.render(vertical[0], buf);
        self.name_field.render(inner, buf, matches!(self.focus, Focus::Name));

        // Body field
        let body_title = if matches!(self.focus, Focus::Body) { "Content (multiline)" } else { "Content" };
        let body_block = Block::default().borders(Borders::ALL).title(body_title);
        let inner_body = body_block.inner(vertical[1]);
        body_block.render(vertical[1], buf);
        self.body_field.render(inner_body, buf, matches!(self.focus, Focus::Body));

        // Buttons
        let buttons_area = vertical[2];
        let save_label = if matches!(self.focus, Focus::Save) { "[Save]" } else { "Save" };
        let help = "Tab cycle • Ctrl+N new • Enter Save";
        let line = Line::from(vec![
            Span::styled(save_label, Style::default().fg(colors::success()).add_modifier(Modifier::BOLD)),
            Span::raw("    "),
            Span::styled(help, Style::default().fg(colors::text_dim())),
        ]);
        Paragraph::new(line).render(buttons_area, buf);

        // Status
        if let Some((msg, style)) = &self.status {
            Paragraph::new(Line::from(Span::styled(msg.clone(), *style)))
                .alignment(Alignment::Left)
                .render(vertical[3], buf);
        }
    }

    fn handle_list_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Up => {
                if self.selected > 0 { self.selected -= 1; }
                return true;
            }
            KeyCode::Down => {
                if self.selected + 1 < self.prompts.len() { self.selected += 1; }
                return true;
            }
            KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.start_new_prompt();
                return true;
            }
            _ => {}
        }
        false
    }

    fn start_new_prompt(&mut self) {
        self.selected = self.prompts.len();
        self.name_field.set_text("");
        self.body_field.set_text("");
        self.focus = Focus::Name;
        self.status = Some(("New prompt".to_string(), Style::default().fg(colors::info())));
    }

    fn load_selected_into_form(&mut self) {
        if let Some(p) = self.prompts.get(self.selected) {
            self.name_field.set_text(&p.name);
            self.body_field.set_text(&p.content);
            self.focus = Focus::Name;
        }
    }

    fn cycle_focus(&mut self, forward: bool) {
        let order = [Focus::List, Focus::Name, Focus::Body, Focus::Save];
        let mut idx = order.iter().position(|f| *f == self.focus).unwrap_or(0);
        if forward {
            idx = (idx + 1) % order.len();
        } else {
            idx = idx.checked_sub(1).unwrap_or(order.len() - 1);
        }
        self.focus = order[idx];
    }

    fn validate(&self, name: &str) -> Result<(), String> {
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

        let builtin: Vec<String> = built_in_slash_commands()
            .into_iter()
            .map(|(n, _)| n.to_ascii_lowercase())
            .collect();
        if builtin.contains(&slug.to_ascii_lowercase()) {
            return Err("Name conflicts with a built-in slash command".to_string());
        }

        let dup = self
            .prompts
            .iter()
            .enumerate()
            .any(|(idx, p)| idx != self.selected && p.name.eq_ignore_ascii_case(slug));
        if dup {
            return Err("A prompt with this name already exists".to_string());
        }
        Ok(())
    }

    fn save_current(&mut self) {
        let name = self.name_field.text().trim().to_string();
        let body = self.body_field.text().to_string();
        match self.validate(&name) {
            Ok(()) => {}
            Err(msg) => {
                self.status = Some((msg, Style::default().fg(colors::error())));
                return;
            }
        }

        let code_home = match find_code_home() {
            Ok(path) => path,
            Err(e) => {
                self.status = Some((format!("CODE_HOME unavailable: {e}"), Style::default().fg(colors::error())));
                return;
            }
        };
        let mut dir = code_home;
        dir.push("prompts");
        if let Err(e) = fs::create_dir_all(&dir) {
            self.status = Some((format!("Failed to create prompts dir: {e}"), Style::default().fg(colors::error())));
            return;
        }
        let mut path = PathBuf::from(&dir);
        path.push(format!("{name}.md"));
        if let Err(e) = fs::write(&path, &body) {
            self.status = Some((format!("Failed to save: {e}"), Style::default().fg(colors::error())));
            return;
        }

        // Update local list
        let mut updated = self.prompts.clone();
        let new_entry = CustomPrompt { name: name.clone(), path, content: body.clone(), description: None, argument_hint: None };
        if self.selected < updated.len() {
            updated[self.selected] = new_entry;
        } else {
            updated.push(new_entry);
            self.selected = updated.len() - 1;
        }
        self.prompts = updated;
        self.status = Some(("Saved.".to_string(), Style::default().fg(colors::success())));

        // Trigger reload so composer autocomplete picks it up.
        self.app_event_tx.send(AppEvent::CodexOp(Op::ListCustomPrompts));
    }
}
