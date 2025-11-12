use crossterm::event::{KeyCode, KeyEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
#[cfg(target_os = "macos")]
use crate::agent_install_helpers::macos_brew_formula_for_command;

use super::bottom_pane_view::BottomPaneView;
use super::form_text_field::{FormTextField, InputFilter};
use super::BottomPane;

#[derive(Debug)]
struct AgentEditorLayout {
    lines: Vec<Line<'static>>,
    name_offset: u16,
    command_offset: u16,
    ro_offset: u16,
    wr_offset: u16,
    instr_offset: u16,
    ro_height: u16,
    wr_height: u16,
    instr_height: u16,
    name_height: u16,
    command_height: u16,
}

#[derive(Debug)]
pub(crate) struct AgentEditorView {
    name: String,
    name_field: FormTextField,
    name_editable: bool,
    enabled: bool,
    command: String,
    command_field: FormTextField,
    params_ro: FormTextField,
    params_wr: FormTextField,
    instr: FormTextField,
    field: usize, // 0 toggle, 1 name, 2 command, 3 ro, 4 wr, 5 instr, 6 save, 7 cancel
    complete: bool,
    app_event_tx: AppEventSender,
    installed: bool,
    description: Option<String>,
    install_hint: String,
}

impl AgentEditorView {
    fn persist_current_agent(&self) {
        let ro = self
            .params_ro
            .text()
            .split_whitespace()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        let wr = self
            .params_wr
            .text()
            .split_whitespace()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        let ro_opt = if ro.is_empty() { None } else { Some(ro) };
        let wr_opt = if wr.is_empty() { None } else { Some(wr) };
        let instr_opt = {
            let t = self.instr.text().trim().to_string();
            if t.is_empty() { None } else { Some(t) }
        };

        let name_value = self.name_field.text().trim();
        let final_name = if name_value.is_empty() { self.name.clone() } else { name_value.to_string() };
        let command_value = self.command_field.text().trim();
        let final_command = if command_value.is_empty() { self.command.clone() } else { command_value.to_string() };
        self.app_event_tx.send(AppEvent::UpdateAgentConfig {
            name: final_name,
            enabled: self.enabled,
            args_read_only: ro_opt,
            args_write: wr_opt,
            instructions: instr_opt,
            command: final_command,
        });
    }

    fn handle_key_internal(&mut self, key_event: KeyEvent) -> bool {
        if !self.installed {
            match key_event.code {
                KeyCode::Esc | KeyCode::Enter => {
                    self.complete = true;
                    true
                }
                _ => false,
            }
        } else {
            let last_field_idx = 7;
            match key_event {
                KeyEvent { code: KeyCode::Esc, .. } => {
                    self.complete = true;
                    self.app_event_tx.send(AppEvent::ShowAgentsOverview);
                    true
                }
                KeyEvent { code: KeyCode::Tab, .. } => {
                    self.field = (self.field + 1).min(last_field_idx);
                    true
                }
                KeyEvent { code: KeyCode::BackTab, .. } => {
                    if self.field > 0 {
                        self.field -= 1;
                    }
                    true
                }
                KeyEvent { code: KeyCode::Up, .. } => {
                    if self.field > 0 {
                        self.field -= 1;
                    }
                    true
                }
                KeyEvent { code: KeyCode::Down, .. } => {
                    self.field = (self.field + 1).min(last_field_idx);
                    true
                }
                KeyEvent { code: KeyCode::Left, .. } if self.field == 0 => {
                    self.enabled = true;
                    self.persist_current_agent();
                    true
                }
                KeyEvent { code: KeyCode::Right, .. } if self.field == 0 => {
                    self.enabled = false;
                    self.persist_current_agent();
                    true
                }
                KeyEvent { code: KeyCode::Char(' '), .. } if self.field == 0 => {
                    self.enabled = !self.enabled;
                    self.persist_current_agent();
                    true
                }
                ev @ KeyEvent { .. } if self.field == 1 => {
                    if self.name_editable {
                        let _ = self.name_field.handle_key(ev);
                    }
                    true
                }
                ev @ KeyEvent { .. } if self.field == 2 => {
                    let _ = self.command_field.handle_key(ev);
                    true
                }
                ev @ KeyEvent { .. } if self.field == 3 => {
                    let _ = self.params_ro.handle_key(ev);
                    true
                }
                ev @ KeyEvent { .. } if self.field == 4 => {
                    let _ = self.params_wr.handle_key(ev);
                    true
                }
                ev @ KeyEvent { .. } if self.field == 5 => {
                    let _ = self.instr.handle_key(ev);
                    true
                }
                KeyEvent { code: KeyCode::Enter, .. } if self.field == 6 => {
                    self.persist_current_agent();
                    self.complete = true;
                    self.app_event_tx.send(AppEvent::ShowAgentsOverview);
                    true
                }
                KeyEvent { code: KeyCode::Enter, .. } if self.field == 7 => {
                    self.complete = true;
                    self.app_event_tx.send(AppEvent::ShowAgentsOverview);
                    true
                }
                _ => false,
            }
        }
    }

    pub(crate) fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        self.handle_key_internal(key_event)
    }

    fn clear_rect(buf: &mut Buffer, rect: Rect) {
        if rect.width == 0 || rect.height == 0 {
            return;
        }
        let style = Style::default()
            .bg(crate::colors::background())
            .fg(crate::colors::text());
        for y in rect.y..rect.y.saturating_add(rect.height) {
            for x in rect.x..rect.x.saturating_add(rect.width) {
                let cell = &mut buf[(x, y)];
                cell.set_symbol(" ");
                cell.set_style(style);
            }
        }
    }

    pub fn new(
        name: String,
        enabled: bool,
        args_read_only: Option<Vec<String>>,
        args_write: Option<Vec<String>>,
        instructions: Option<String>,
        description: Option<String>,
        command: String,
        app_event_tx: AppEventSender,
    ) -> Self {
        // Simple PATH check similar to the core executor’s logic
        fn command_exists(cmd: &str) -> bool {
            if cmd.contains(std::path::MAIN_SEPARATOR) || cmd.contains('/') || cmd.contains('\\') {
                return std::fs::metadata(cmd).map(|m| m.is_file()).unwrap_or(false);
            }
            #[cfg(target_os = "windows")]
            {
                if let Ok(p) = which::which(cmd) {
                    if !p.is_file() { return false; }
                    match p.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()) {
                        Some(ext) if matches!(ext.as_str(), "exe" | "com" | "cmd" | "bat") => true,
                        _ => false,
                    }
                } else { false }
            }
            #[cfg(not(target_os = "windows"))]
            {
                use std::os::unix::fs::PermissionsExt;
                let Some(path_os) = std::env::var_os("PATH") else { return false; };
                for dir in std::env::split_paths(&path_os) {
                    if dir.as_os_str().is_empty() { continue; }
                    let candidate = dir.join(cmd);
                    if let Ok(meta) = std::fs::metadata(&candidate) {
                        if meta.is_file() {
                            if meta.permissions().mode() & 0o111 != 0 { return true; }
                        }
                    }
                }
                false
            }
        }

        let trimmed_description = description
            .and_then(|desc| {
                let trimmed = desc.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            });

        let name_editable = name.is_empty();
        let mut name_field = FormTextField::new_single_line();
        name_field.set_text(&name);
        name_field.set_filter(InputFilter::Id);
        let mut command_field = FormTextField::new_single_line();
        command_field.set_text(&command);
        let command_exists_flag = !command.trim().is_empty() && command_exists(&command);
        let mut v = Self {
            name,
            name_field,
            name_editable,
            enabled,
            command: command.clone(),
            command_field,
            params_ro: FormTextField::new_multi_line(),
            params_wr: FormTextField::new_multi_line(),
            instr: FormTextField::new_multi_line(),
            field: 0,
            complete: false,
            app_event_tx,
            installed: command_exists_flag,
            description: trimmed_description,
            install_hint: String::new(),
        };

        if let Some(ro) = args_read_only { v.params_ro.set_text(&ro.join(" ")); }
        if let Some(wr) = args_write { v.params_wr.set_text(&wr.join(" ")); }
        if let Some(s) = instructions { v.instr.set_text(&s); v.instr.move_cursor_to_start(); }

        // OS-specific short hint
        if !v.command.trim().is_empty() {
            #[cfg(target_os = "macos")]
            {
                let brew_formula = macos_brew_formula_for_command(&v.command);
                v.install_hint = format!("'{}' not found. On macOS, try Homebrew (brew install {brew_formula}) or consult the agent's docs.", v.command);
            }
            #[cfg(target_os = "linux")]
            { v.install_hint = format!("'{}' not found. On Linux, install via your package manager or consult the agent's docs.", v.command); }
            #[cfg(target_os = "windows")]
            { v.install_hint = format!("'{}' not found. On Windows, install the CLI from the vendor site and ensure it’s on PATH.", v.command); }
        }

        v
    }

    fn layout(&self, content_width: u16, max_height: Option<u16>) -> AgentEditorLayout {
        let instr_inner_width = content_width.saturating_sub(4);
        let desired_instr_inner = self.instr.desired_height(instr_inner_width).min(8);
        let mut instr_box_h = desired_instr_inner.saturating_add(2);

        let ro_inner_width = content_width.saturating_sub(4);
        let desired_ro_inner = self.params_ro.desired_height(ro_inner_width).min(6);
        let ro_box_h = desired_ro_inner.saturating_add(2);
        let desired_wr_inner = self.params_wr.desired_height(ro_inner_width).min(6);
        let wr_box_h = desired_wr_inner.saturating_add(2);

        let title_block: u16 = 2; // title + blank
        let desc_style = Style::default().fg(crate::colors::text_dim());
        let desc_text = self
            .description
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string());
        let desc_block = if desc_text.is_some() { 2 } else { 0 };
        let name_box_h: u16 = 3;
        let command_box_h: u16 = 3;
        let top_block = title_block + desc_block;
        let enabled_block: u16 = 2; // toggle row + spacer
        let instr_desc_lines: u16 = 1; // description row after box
        let spacer_before_buttons: u16 = 1;
        let buttons_block: u16 = 1;
        let footer_lines_default: u16 = 0;

        let base_fixed_top = top_block
            + name_box_h
            + 1
            + command_box_h
            + 1
            + enabled_block
            + ro_box_h
            + 1 // blank after read-only box
            + wr_box_h
            + 1; // blank after write box

        let mut footer_lines = footer_lines_default;
        let mut include_gap_before_buttons = spacer_before_buttons > 0;

        if let Some(height) = max_height {
            let mut fixed_after_box = instr_desc_lines + spacer_before_buttons + buttons_block + footer_lines;
            if base_fixed_top.saturating_add(instr_box_h).saturating_add(fixed_after_box) > height {
                footer_lines = 0;
            }
            fixed_after_box = instr_desc_lines + spacer_before_buttons + buttons_block + footer_lines;
            if base_fixed_top.saturating_add(instr_box_h).saturating_add(fixed_after_box) > height {
                let min_ih: u16 = 3;
                let available_for_box = height
                    .saturating_sub(base_fixed_top)
                    .saturating_sub(fixed_after_box);
                instr_box_h = instr_box_h.min(available_for_box).max(min_ih);
            }
            fixed_after_box = instr_desc_lines + spacer_before_buttons + buttons_block + footer_lines;
            if base_fixed_top.saturating_add(instr_box_h).saturating_add(fixed_after_box) > height {
                include_gap_before_buttons = false;
            }
        }

        let sel = |idx: usize| {
            if self.field == idx {
                Style::default()
                    .bg(crate::colors::selection())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            }
        };

        let name_offset = top_block;
        let command_offset = name_offset + name_box_h + 1;
        let toggle_offset = command_offset + command_box_h + 1;
        let ro_offset = toggle_offset + enabled_block;
        let wr_offset = ro_offset + ro_box_h + 1;
        let instr_offset = wr_offset + wr_box_h + 1;
        let mut lines: Vec<Line<'static>> = Vec::new();

        // Title, spacer
        lines.push(Line::from(Span::styled(
            format!("Agents » Edit Agent » {}", self.name),
            Style::default().add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));
        if let Some(desc_line) = &desc_text {
            lines.push(Line::from(Span::styled(desc_line.clone(), desc_style)));
            lines.push(Line::from(""));
        }

        // Reserve space for Name box
        for _ in 0..name_box_h {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(""));
        // Reserve space for Command box
        for _ in 0..command_box_h {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(""));

        // Enabled toggle + spacer
        let enabled_style = if self.enabled {
            Style::default()
                .fg(crate::colors::success())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(crate::colors::text_dim())
        };
        let disabled_style = if self.enabled {
            Style::default().fg(crate::colors::text_dim())
        } else {
            Style::default()
                .fg(crate::colors::error())
                .add_modifier(Modifier::BOLD)
        };
        let label_style = if self.field == 0 {
            Style::default()
                .fg(crate::colors::primary())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(crate::colors::text())
        };
        let enabled_text = format!("[{}] Enabled", if self.enabled { 'x' } else { ' ' });
        let disabled_text = format!("[{}] Disabled", if self.enabled { ' ' } else { 'x' });
        lines.push(Line::from(vec![
            Span::styled("Status:", label_style),
            Span::raw("  "),
            Span::styled(enabled_text, enabled_style),
            Span::raw("  "),
            Span::styled(disabled_text, disabled_style),
        ]));
        lines.push(Line::from(""));

        // Read-only params box
        for _ in 0..ro_box_h {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(""));

        // Write params box
        for _ in 0..wr_box_h {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(""));

        // Instructions box
        for _ in 0..instr_box_h {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(Span::styled(
            "Optional guidance prepended to every request sent to the agent.",
            desc_style,
        )));
        lines.push(Line::from(""));

        // Buttons row
        if include_gap_before_buttons {
            lines.push(Line::from(""));
        }
        let save_style = sel(6).fg(crate::colors::success());
        let cancel_style = sel(7).fg(crate::colors::text());
        lines.push(Line::from(vec![
            Span::styled("[ Save ]", save_style),
            Span::raw("  "),
            Span::styled("[ Cancel ]", cancel_style),
        ]));

        // Trim any trailing blank rows so the button row hugs the bottom border
        while lines
            .last()
            .map(|line| line.spans.iter().all(|s| s.content.trim().is_empty()))
            .unwrap_or(false)
        {
            lines.pop();
        }
        // No footer hints in the editor form

        AgentEditorLayout {
            lines,
            name_offset,
            command_offset,
            ro_offset,
            wr_offset,
            instr_offset,
            ro_height: ro_box_h,
            wr_height: wr_box_h,
            instr_height: instr_box_h,
            name_height: name_box_h,
            command_height: command_box_h,
        }
    }
}

impl<'a> BottomPaneView<'a> for AgentEditorView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        let _ = self.handle_key_internal(key_event);
    }

    fn is_complete(&self) -> bool { self.complete }

    fn desired_height(&self, width: u16) -> u16 {
        let content_width = width.saturating_sub(4).max(1);
        let layout = self.layout(content_width, None);
        (layout.lines.len() as u16).saturating_add(2)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(crate::colors::border()))
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .title(" Configure Agent ")
            .title_alignment(Alignment::Center);
        let inner = block.inner(area);
        block.render(area, buf);

        let content = Rect { x: inner.x.saturating_add(1), y: inner.y, width: inner.width.saturating_sub(2), height: inner.height };

        if !self.installed && !self.command.trim().is_empty() {
            let mut lines: Vec<Line<'static>> = Vec::new();
            lines.push(Line::from(Span::styled("Not installed", Style::default().fg(crate::colors::warning()).add_modifier(Modifier::BOLD))));
            lines.push(Line::from(Span::styled(self.install_hint.clone(), Style::default().fg(crate::colors::text_dim()))));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![Span::styled("[ Close ]", Style::default().bg(crate::colors::selection()).add_modifier(Modifier::BOLD))]));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Enter", Style::default().fg(crate::colors::success())),
                Span::styled(" Close  ", Style::default().fg(crate::colors::text_dim())),
                Span::styled("Esc", Style::default().fg(crate::colors::error())),
                Span::styled(" Cancel", Style::default().fg(crate::colors::text_dim())),
            ]));

            Paragraph::new(lines)
                .alignment(Alignment::Left)
                .wrap(ratatui::widgets::Wrap { trim: false })
                .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
                .render(content, buf);
            return;
        }

        let layout = self.layout(content.width, Some(content.height));
        let AgentEditorLayout {
            lines,
            name_offset,
            command_offset,
            ro_offset,
            wr_offset,
            instr_offset,
            ro_height,
            wr_height,
            instr_height,
            name_height,
            command_height,
        } = layout;

        Paragraph::new(lines)
            .alignment(Alignment::Left)
            .wrap(ratatui::widgets::Wrap { trim: false })
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .render(content, buf);

        // Draw name and command boxes
        let name_rect = Rect { x: content.x, y: content.y.saturating_add(name_offset), width: content.width, height: name_height };
        let name_rect = name_rect.intersection(*buf.area());
        if name_rect.width > 0 && name_rect.height > 0 {
            let name_block = Block::default()
                .borders(Borders::ALL)
                .title(Line::from(" ID "))
                .border_style(if self.field == 1 {
                    Style::default().fg(crate::colors::primary()).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(crate::colors::border())
                });
            let name_inner = name_block.inner(name_rect);
            let name_field_inner = name_inner.inner(Margin::new(1, 0));
            name_block.render(name_rect, buf);
            Self::clear_rect(buf, name_inner);
            self.name_field
                .render(name_field_inner, buf, self.field == 1 && self.name_editable);
        }

        let command_rect = Rect { x: content.x, y: content.y.saturating_add(command_offset), width: content.width, height: command_height };
        let command_rect = command_rect.intersection(*buf.area());
        if command_rect.width > 0 && command_rect.height > 0 {
            let command_block = Block::default()
                .borders(Borders::ALL)
                .title(Line::from(" Command "))
                .border_style(if self.field == 2 {
                    Style::default().fg(crate::colors::primary()).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(crate::colors::border())
                });
            let command_inner = command_block.inner(command_rect);
            let command_field_inner = command_inner.inner(Margin::new(1, 0));
            command_block.render(command_rect, buf);
            Self::clear_rect(buf, command_inner);
            self.command_field.render(command_field_inner, buf, self.field == 2);
        }

        // Draw input boxes at the same y offsets we reserved above
        let ro_rect = Rect { x: content.x, y: content.y.saturating_add(ro_offset), width: content.width, height: ro_height };
        let ro_rect = ro_rect.intersection(*buf.area());
        let ro_block = Block::default()
            .borders(Borders::ALL)
            .title(Line::from(" Read-only Params "))
            .border_style(if self.field == 3 { Style::default().fg(crate::colors::primary()).add_modifier(Modifier::BOLD) } else { Style::default().fg(crate::colors::border()) });
        if ro_rect.width > 0 && ro_rect.height > 0 {
            let ro_inner_rect = ro_block.inner(ro_rect);
            let ro_inner = ro_inner_rect.inner(Margin::new(1, 0));
            ro_block.render(ro_rect, buf);
            Self::clear_rect(buf, ro_inner_rect);
            self.params_ro.render(ro_inner, buf, self.field == 3);
        }

        // WR params box (3 rows)
        let wr_rect = Rect { x: content.x, y: content.y.saturating_add(wr_offset), width: content.width, height: wr_height };
        let wr_rect = wr_rect.intersection(*buf.area());
        let wr_block = Block::default()
            .borders(Borders::ALL)
            .title(Line::from(" Write Params "))
            .border_style(if self.field == 4 { Style::default().fg(crate::colors::primary()).add_modifier(Modifier::BOLD) } else { Style::default().fg(crate::colors::border()) });
        if wr_rect.width > 0 && wr_rect.height > 0 {
            let wr_inner_rect = wr_block.inner(wr_rect);
            let wr_inner = wr_inner_rect.inner(Margin::new(1, 0));
            wr_block.render(wr_rect, buf);
            Self::clear_rect(buf, wr_inner_rect);
            self.params_wr.render(wr_inner, buf, self.field == 4);
        }

        // Instructions (multi-line; height consistent with reserved space above)
        let instr_rect = Rect { x: content.x, y: content.y.saturating_add(instr_offset), width: content.width, height: instr_height };
        let instr_rect = instr_rect.intersection(*buf.area());
        let instr_block = Block::default()
            .borders(Borders::ALL)
            .title(Line::from(" Instructions "))
            .border_style(if self.field == 5 { Style::default().fg(crate::colors::primary()).add_modifier(Modifier::BOLD) } else { Style::default().fg(crate::colors::border()) });
        if instr_rect.width > 0 && instr_rect.height > 0 {
            let instr_inner_rect = instr_block.inner(instr_rect);
            let instr_inner = instr_inner_rect.inner(Margin::new(1, 0));
            instr_block.render(instr_rect, buf);
            Self::clear_rect(buf, instr_inner_rect);
            self.instr.render(instr_inner, buf, self.field == 5);
        }
    }
}
