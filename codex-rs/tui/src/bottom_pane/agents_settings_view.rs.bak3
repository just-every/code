use crossterm::event::{KeyCode, KeyEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect, Margin};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;

use super::bottom_pane_view::BottomPaneView;
// list_selection_view no longer used (overview replaces the list)
use super::BottomPane;
use super::form_text_field::FormTextField;

// Removed legacy AgentsSettingsView list. Overview replaces it.

#[derive(Debug)]
pub struct SubagentEditorView {
    name_field: FormTextField,
    read_only: bool,
    selected_agent_indices: Vec<usize>,
    agent_cursor: usize,
    orch_field: FormTextField,
    available_agents: Vec<String>,
    is_new: bool,
    field: usize, // 0 name, 1 mode, 2 agents, 3 orch, 4 save, 5 cancel
    is_complete: bool,
    app_event_tx: AppEventSender,
    confirm_delete: bool,
}

impl SubagentEditorView {
    fn build_with(
        available_agents: Vec<String>,
        existing: Vec<codex_core::config_types::SubagentCommandConfig>,
        app_event_tx: AppEventSender,
        name: &str,
    ) -> Self {
        let mut me = Self {
            name_field: FormTextField::new_single_line(),
            read_only: if name.is_empty() { false } else { codex_core::slash_commands::default_read_only_for(name) },
            selected_agent_indices: Vec::new(),
            agent_cursor: 0,
            orch_field: FormTextField::new_multi_line(),
            available_agents,
            is_new: name.is_empty(),
            field: 0,
            is_complete: false,
            app_event_tx,
            confirm_delete: false,
        };
        // Always seed the name field with the provided name
        if !name.is_empty() { me.name_field.set_text(name); }
        // Restrict ID field to [A-Za-z0-9_-]
        me.name_field.set_filter(super::form_text_field::InputFilter::Id);
        // Seed from existing config if present
        if let Some(cfg) = existing.iter().find(|c| c.name.eq_ignore_ascii_case(name)) {
            me.name_field.set_text(&cfg.name);
            me.read_only = cfg.read_only;
            me.orch_field.set_text(&cfg.orchestrator_instructions.clone().unwrap_or_default());
            let set: std::collections::HashSet<String> = cfg.agents.iter().cloned().collect();
            for (idx, a) in me.available_agents.iter().enumerate() {
                if set.contains(a) { me.selected_agent_indices.push(idx); }
            }
        } else {
            // No user config yet; provide sensible defaults from core for built-ins
            if !name.is_empty() {
                me.read_only = codex_core::slash_commands::default_read_only_for(name);
                if let Some(instr) = codex_core::slash_commands::default_instructions_for(name) {
                    me.orch_field.set_text(&instr);
                    // Start cursor at the top so the first lines are visible.
                    me.orch_field.move_cursor_to_start();
                }
            }
            // Default selection: when no explicit config exists, preselect all available agents.
            if me.selected_agent_indices.is_empty() {
                me.selected_agent_indices = (0..me.available_agents.len()).collect();
            }
        }
        me
    }

    pub fn new_with_data(
        name: String,
        available_agents: Vec<String>,
        existing: Vec<codex_core::config_types::SubagentCommandConfig>,
        is_new: bool,
        app_event_tx: AppEventSender,
    ) -> Self {
        let mut s = Self::build_with(available_agents, existing, app_event_tx, &name);
        s.is_new = is_new;
        s
    }

    fn toggle_agent_at(&mut self, idx: usize) {
        if let Some(pos) = self.selected_agent_indices.iter().position(|i| *i == idx) {
            self.selected_agent_indices.remove(pos);
        } else {
            self.selected_agent_indices.push(idx);
        }
    }

    fn save(&mut self) {
        let agents: Vec<String> = if self.selected_agent_indices.is_empty() {
            Vec::new()
        } else {
            self.selected_agent_indices.iter().filter_map(|i| self.available_agents.get(*i).cloned()).collect()
        };
        let cfg = codex_core::config_types::SubagentCommandConfig {
            name: self.name_field.text().to_string(),
            read_only: self.read_only,
            agents,
            orchestrator_instructions: {
                let t = self.orch_field.text().trim().to_string();
                if t.is_empty() { None } else { Some(t) }
            },
            agent_instructions: None,
        };
        // Persist to disk asynchronously to avoid blocking the TUI runtime
        if let Ok(home) = codex_core::config::find_codex_home() {
            let cfg_clone = cfg.clone();
            tokio::spawn(async move {
                let _ = codex_core::config_edit::upsert_subagent_command(&home, &cfg_clone).await;
            });
        }
        // Update in-memory config
        self.app_event_tx.send(AppEvent::UpdateSubagentCommand(cfg));
    }
}

impl<'a> BottomPaneView<'a> for SubagentEditorView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        let show_delete = !self.is_new && !matches!(self.name_field.text().to_ascii_lowercase().as_str(), "plan" | "solve" | "code");
        let last_btn_idx = if show_delete { 6 } else { 5 };
        match key_event {
            KeyEvent { code: KeyCode::Esc, .. } => {
                // Return to Agents overview on first Esc
                self.is_complete = true;
                self.app_event_tx.send(AppEvent::ShowAgentsOverview);
            }
            KeyEvent { code: KeyCode::Tab, .. } => { self.field = (self.field + 1).min(last_btn_idx); }
            KeyEvent { code: KeyCode::BackTab, .. } => { if self.field > 0 { self.field -= 1; } }
            KeyEvent { code: KeyCode::Up, modifiers, .. } => {
                if self.field == 3 {
                    // In text: Up scrolls/moves unless at very start, then move to previous input
                    let at_start = self.orch_field.cursor_is_at_start();
                    let _ = self.orch_field.handle_key(KeyEvent { code: KeyCode::Up, modifiers, ..key_event });
                    if at_start { if self.field > 0 { self.field -= 1; } }
                } else if self.field > 0 { self.field -= 1; }
            }
            KeyEvent { code: KeyCode::Down, modifiers, .. } => {
                if self.field == 3 {
                    // In text: Down scrolls/moves unless at end, then move to next input
                    let at_end = self.orch_field.cursor_is_at_end();
                    let _ = self.orch_field.handle_key(KeyEvent { code: KeyCode::Down, modifiers, ..key_event });
                    if at_end { self.field = (self.field + 1).min(5); }
                } else { self.field = (self.field + 1).min(5); }
            }
            KeyEvent { code: KeyCode::Left, .. } if self.field == 1 => { self.read_only = !self.read_only; }
            KeyEvent { code: KeyCode::Right, .. } if self.field == 1 => { self.read_only = !self.read_only; }
            KeyEvent { code: KeyCode::Enter, .. } if self.field == 1 => { self.read_only = !self.read_only; }
            KeyEvent { code: KeyCode::Left, .. } if self.field == 2 => { if self.agent_cursor > 0 { self.agent_cursor -= 1; } }
            KeyEvent { code: KeyCode::Right, .. } if self.field == 2 => { if self.agent_cursor + 1 < self.available_agents.len() { self.agent_cursor += 1; } }
            KeyEvent { code: KeyCode::Char(' '), .. } if self.field == 2 => {
                let idx = self.agent_cursor.min(self.available_agents.len().saturating_sub(1));
                self.toggle_agent_at(idx);
            }
            KeyEvent { code: KeyCode::Enter, .. } if self.field == 2 => {
                let idx = self.agent_cursor.min(self.available_agents.len().saturating_sub(1));
                self.toggle_agent_at(idx);
            }
            // Left/Right between Save / Delete / Cancel
            KeyEvent { code: KeyCode::Left, .. } if self.field >= 5 && show_delete => { if self.field > 4 { self.field -= 1; } }
            KeyEvent { code: KeyCode::Right, .. } if self.field >= 4 && show_delete => { if self.field < 6 { self.field += 1; } }
            KeyEvent { code: KeyCode::Left, .. } if !show_delete && self.field == 5 => { self.field = 4; }
            KeyEvent { code: KeyCode::Right, .. } if !show_delete && self.field == 4 => { self.field = 5; }
            // Delegate input to focused text fields (handles Shift‑chars, Enter/newline, undo, etc.)
            ev @ KeyEvent { .. } if self.field == 0 => { let _ = self.name_field.handle_key(ev); }
            ev @ KeyEvent { .. } if self.field == 3 => { let _ = self.orch_field.handle_key(ev); }
            KeyEvent { code: KeyCode::Enter, .. } if self.field == 4 && !self.confirm_delete => { self.save(); self.is_complete = true; }
            KeyEvent { code: KeyCode::Enter, .. } if self.field == 5 && show_delete && !self.confirm_delete => { self.confirm_delete = true; }
            KeyEvent { code: KeyCode::Enter, .. } if self.field == 6 && !self.confirm_delete => {
                // Cancel → return to Agents overview
                self.is_complete = true;
                self.app_event_tx.send(AppEvent::ShowAgentsOverview);
            }
            KeyEvent { code: KeyCode::Enter, .. } if self.field == 5 && !show_delete && !self.confirm_delete => {
                // Cancel in 2-button layout
                self.is_complete = true;
                self.app_event_tx.send(AppEvent::ShowAgentsOverview);
            }
            // Confirm phase: 4 = Confirm, 5 = Back (when confirm_delete is true)
            KeyEvent { code: KeyCode::Enter, .. } if self.confirm_delete && self.field == 4 => {
                // Delete from disk and in-memory, then close
                let id = self.name_field.text().to_string();
                if !id.trim().is_empty() {
                    if let Ok(home) = codex_core::config::find_codex_home() {
                        let idc = id.clone();
                        tokio::spawn(async move { let _ = codex_core::config_edit::delete_subagent_command(&home, &idc).await; });
                    }
                    self.app_event_tx.send(AppEvent::DeleteSubagentCommand(id));
                }
                self.is_complete = true;
                self.app_event_tx.send(AppEvent::ShowAgentsOverview);
            }
            KeyEvent { code: KeyCode::Enter, .. } if self.confirm_delete && self.field == 5 => { self.confirm_delete = false; }
            _ => {}
        }
    }

    fn is_complete(&self) -> bool { self.is_complete }

    fn handle_paste(&mut self, text: String) -> super::bottom_pane_view::ConditionalUpdate {
        match self.field { 0 => self.name_field.handle_paste(text), 3 => self.orch_field.handle_paste(text), _ => {} }
        super::bottom_pane_view::ConditionalUpdate::NeedsRedraw
    }

    fn desired_height(&self, width: u16) -> u16 {
        // Compute content width consistent with render: inner = width-2; content = inner-1
        let inner_w = width.saturating_sub(2);
        let content_w = inner_w.saturating_sub(1).max(10) as usize;
        // Static rows (with spacing and title):
        // top(1) + title(1) + spacer(1) + name box(3) + spacer(1) + mode(1) + spacer(1)
        // + agents(1) + spacer(1) + orch box(dynamic) + spacer(1) + buttons(1) + bottom(1)
        let name_box_h: u16 = 3;
        // Orchestrator inner width accounts for borders (2) and left/right padding (2)
        let orch_inner_w = (content_w as u16).saturating_sub(4);
        let desired_orch_inner = self.orch_field.desired_height(orch_inner_w).max(1);
        let orch_box_h = desired_orch_inner.min(8).saturating_add(2).max(3);
        let base_rows: u16 = 1  // title
            + 1  // spacer after title
            + name_box_h
            + 1  // spacer
            + 1  // mode row
            + 1  // spacer before agents
            + 1  // agents row
            + 1; // spacer before instructions box
        let rows_after_orch: u16 = 1  // spacer after instructions box
            + 1; // buttons row
        let total_rows = base_rows + orch_box_h + rows_after_orch;
        total_rows.saturating_add(2).clamp(8, 50)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(crate::colors::border()))
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .title(" Configure Agent Command ")
            .title_alignment(Alignment::Center);
        let inner = block.inner(area);
        block.render(area, buf);
        // Compute the content rect once and reuse for layout and reserved lines
        let content_rect = Rect { x: inner.x.saturating_add(1), y: inner.y, width: inner.width.saturating_sub(1), height: inner.height };

        let mut lines: Vec<Line<'static>> = Vec::new();
        let sel = |idx: usize| if self.field == idx { Style::default().bg(crate::colors::selection()).add_modifier(Modifier::BOLD) } else { Style::default() };
        let label = |idx: usize| if self.field == idx { Style::default().fg(crate::colors::primary()).add_modifier(Modifier::BOLD) } else { Style::default() };

        // Bold title
        lines.push(Line::from(Span::styled("Agents » Edit Command", Style::default().add_modifier(Modifier::BOLD))));
        // Spacer after title
        lines.push(Line::from(""));
        // Reserve a box area for Name (we draw the bordered box with a title after)
        let name_box_h: u16 = 3;
        for _ in 0..name_box_h { lines.push(Line::from("")); }
        // Spacer between inputs
        lines.push(Line::from(""));
        // Mode row: checkbox style (left padding to align with boxed inputs)
        {
            let mut spans: Vec<Span> = Vec::new();
            spans.push(Span::raw(" "));
            spans.push(Span::styled("Mode:", label(1)));
            spans.push(Span::raw("  "));
            // [x] read-only
            let ro = if self.read_only { "[x]" } else { "[ ]" };
            spans.push(Span::styled(format!("{} read-only", ro), sel(1)));
            spans.push(Span::raw("  "));
            // [x] write (inverse of read_only)
            let wr = if self.read_only { "[ ]" } else { "[x]" };
            spans.push(Span::styled(format!("{} write", wr), sel(1)));
            lines.push(Line::from(spans));
        }

        // Agents selection with cursor highlight
        let mut spans: Vec<Span> = Vec::new();
        for (idx, a) in self.available_agents.iter().enumerate() {
            let checked = if self.selected_agent_indices.contains(&idx) { "[x]" } else { "[ ]" };
            let mut style = sel(2);
            if self.field == 2 && idx == self.agent_cursor { style = style.fg(crate::colors::primary()).add_modifier(Modifier::BOLD); }
            spans.push(Span::styled(format!("{} {}", checked, a), style));
            spans.push(Span::raw("  "));
        }
        // Spacer between inputs
        lines.push(Line::from(""));
        // Agents on the same line as label (left padding to align with boxed inputs)
        {
            let mut line_spans: Vec<Span> = Vec::new();
            line_spans.push(Span::raw(" "));
            line_spans.push(Span::styled("Agents:", label(2)));
            line_spans.push(Span::raw("  "));
            line_spans.extend(spans);
            lines.push(Line::from(line_spans));
        }

        // Spacer between inputs
        lines.push(Line::from(""));
        // Reserve rows for the instructions box (height = inner + borders)
        let orch_inner_w = content_rect.width.saturating_sub(4);
        let desired_orch_inner = self.orch_field.desired_height(orch_inner_w).max(1);
        let orch_box_h_reserved = desired_orch_inner.min(8).saturating_add(2).max(3);
        for _ in 0..orch_box_h_reserved { lines.push(Line::from("")); }
        // Spacer between inputs
        lines.push(Line::from(""));

        // Buttons row
        let show_delete = !self.is_new && !matches!(self.name_field.text().to_ascii_lowercase().as_str(), "plan" | "solve" | "code");
        if self.confirm_delete {
            let confirm_style = sel(4).fg(crate::colors::error()).add_modifier(Modifier::BOLD);
            let back_style = sel(5).fg(crate::colors::text());
            let mut btn_spans: Vec<Span> = Vec::new();
            btn_spans.push(Span::styled("[ Confirm Delete ]", confirm_style));
            btn_spans.push(Span::raw("  "));
            btn_spans.push(Span::styled("[ Back ]", back_style));
            lines.push(Line::from(btn_spans));
        } else if show_delete {
            let save_style = sel(4).fg(crate::colors::success());
            let delete_style = sel(5).fg(crate::colors::error());
            let cancel_style = sel(6).fg(crate::colors::text());
            let mut btn_spans: Vec<Span> = Vec::new();
            btn_spans.push(Span::styled("[ Save ]", save_style));
            btn_spans.push(Span::raw("  "));
            btn_spans.push(Span::styled("[ Delete ]", delete_style));
            btn_spans.push(Span::raw("  "));
            btn_spans.push(Span::styled("[ Cancel ]", cancel_style));
            lines.push(Line::from(btn_spans));
        } else {
            let save_style = sel(4).fg(crate::colors::success());
            let cancel_style = sel(5).fg(crate::colors::text());
            let mut btn_spans: Vec<Span> = Vec::new();
            btn_spans.push(Span::styled("[ Save ]", save_style));
            btn_spans.push(Span::raw("  "));
            btn_spans.push(Span::styled("[ Cancel ]", cancel_style));
            lines.push(Line::from(btn_spans));
        }
        // Remove any trailing blank lines so buttons hug the bottom frame
        while lines
            .last()
            .map(|line| line.spans.iter().all(|s| s.content.trim().is_empty()))
            .unwrap_or(false)
        {
            lines.pop();
        }

        let paragraph = Paragraph::new(lines)
            .alignment(Alignment::Left)
            .wrap(ratatui::widgets::Wrap { trim: false })
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()));
        paragraph.render(content_rect, buf);

        // Draw text fields over the paragraph using the same content rect
        let content_w = content_rect.width;
        let mut y = content_rect.y;

        // Skip title + spacer
        y = y.saturating_add(2);
        // Row: Name box with title; height fixed (3)
        let name_box_rect = Rect { x: content_rect.x, y, width: content_w, height: name_box_h };
        let name_border = if self.field == 0 { Style::default().fg(crate::colors::primary()).add_modifier(Modifier::BOLD) } else { Style::default().fg(crate::colors::border()) };
        let name_block = Block::default()
            .borders(Borders::ALL)
            .border_style(name_border)
            .title(Line::from(" ID "));
        let name_inner = name_block.inner(name_box_rect);
        let name_padded = name_inner.inner(Margin::new(1, 0));
        name_block.render(name_box_rect, buf);
        self.name_field.render(name_padded, buf, self.field == 0);

        // After name box + spacer + mode row + spacer + agents row + spacer
        y = y.saturating_add(name_box_h);
        y = y.saturating_add(1); // spacer
        y = y.saturating_add(1); // mode row
        y = y.saturating_add(1); // spacer
        y = y.saturating_add(1); // agents row
        y = y.saturating_add(1); // spacer
        // Orchestrator box: height = inner content + 2 borders, with title as label
        // Use the same clamped height for the actual box we render
        let orch_box_h = orch_box_h_reserved;
        let orch_inner_h = orch_box_h.saturating_sub(2);
        let orch_box_rect = Rect { x: content_rect.x, y, width: content_w, height: orch_box_h };
        let orch_border = if self.field == 3 { Style::default().fg(crate::colors::primary()).add_modifier(Modifier::BOLD) } else { Style::default().fg(crate::colors::border()) };
        let orch_block = Block::default()
            .borders(Borders::ALL)
            .border_style(orch_border)
            .title(Line::from(" Instructions "));
        if orch_box_h >= 2 {
            let orch_inner = orch_block.inner(orch_box_rect);
            let orch_padded = orch_inner.inner(Margin::new(1, 0));
            orch_block.render(orch_box_rect, buf);
            // Render the text field only if there is inner height
            if orch_inner_h > 0 {
                self.orch_field.render(orch_padded, buf, self.field == 3);
            }
        }
    }
}

impl SubagentEditorView {}

// (handle_paste implemented in BottomPaneView impl below)
