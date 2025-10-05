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
use super::form_text_field::FormTextField;
use super::BottomPane;

#[derive(Debug)]
struct AgentEditorLayout {
    lines: Vec<Line<'static>>,
    ro_offset: u16,
    wr_offset: u16,
    instr_offset: u16,
    ro_height: u16,
    wr_height: u16,
    instr_height: u16,
}

#[derive(Debug)]
pub(crate) struct AgentEditorView {
    name: String,
    enabled: bool,
    params_ro: FormTextField,
    params_wr: FormTextField,
    instr: FormTextField,
    field: usize, // 0 toggle, 1 ro, 2 wr, 3 instr, 4 save, 5 cancel
    complete: bool,
    app_event_tx: AppEventSender,
    installed: bool,
    install_hint: String,
}

impl AgentEditorView {
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

        let mut v = Self {
            name,
            enabled,
            params_ro: FormTextField::new_multi_line(),
            params_wr: FormTextField::new_multi_line(),
            instr: FormTextField::new_multi_line(),
            field: 0,
            complete: false,
            app_event_tx,
            installed: command_exists(&command),
            install_hint: String::new(),
        };

        if let Some(ro) = args_read_only { v.params_ro.set_text(&ro.join(" ")); }
        if let Some(wr) = args_write { v.params_wr.set_text(&wr.join(" ")); }
        if let Some(s) = instructions { v.instr.set_text(&s); v.instr.move_cursor_to_start(); }

        // OS-specific short hint
        #[cfg(target_os = "macos")]
        {
            let brew_formula = macos_brew_formula_for_command(&command);
            v.install_hint = format!("'{command}' not found. On macOS, try Homebrew (brew install {brew_formula}) or consult the agent's docs.");
        }
        #[cfg(target_os = "linux")]
        { v.install_hint = format!("'{}' not found. On Linux, install via your package manager or consult the agent's docs.", command); }
        #[cfg(target_os = "windows")]
        { v.install_hint = format!("'{}' not found. On Windows, install the CLI from the vendor site and ensure it’s on PATH.", command); }

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

        let top_block: u16 = 3; // blank, title, blank
        let enabled_block: u16 = 2; // toggle row + spacer
        let instr_desc_lines: u16 = 1; // description row after box
        let spacer_before_buttons: u16 = 1;
        let buttons_block: u16 = 1;
        let footer_lines_default: u16 = 0;

        let base_fixed_top = top_block
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

        let mut lines: Vec<Line<'static>> = Vec::new();
        let mut cursor: u16 = 0;
        let desc_style = Style::default().fg(crate::colors::text_dim());

        // Title, spacer
        lines.push(Line::from(Span::styled(
            format!("Agents » Edit Agent » {}", self.name),
            Style::default().add_modifier(Modifier::BOLD),
        )));
        cursor = cursor.saturating_add(1);
        lines.push(Line::from(""));
        cursor = cursor.saturating_add(1);

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
        cursor = cursor.saturating_add(1);
        lines.push(Line::from(""));
        cursor = cursor.saturating_add(1);

        // Read-only params box
        let ro_offset = cursor;
        for _ in 0..ro_box_h {
            lines.push(Line::from(""));
            cursor = cursor.saturating_add(1);
        }
        lines.push(Line::from(""));
        cursor = cursor.saturating_add(1);

        // Write params box
        let wr_offset = cursor;
        for _ in 0..wr_box_h {
            lines.push(Line::from(""));
            cursor = cursor.saturating_add(1);
        }
        lines.push(Line::from(""));
        cursor = cursor.saturating_add(1);

        // Instructions box
        let instr_offset = cursor;
        for _ in 0..instr_box_h {
            lines.push(Line::from(""));
            cursor = cursor.saturating_add(1);
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
        let save_style = sel(4).fg(crate::colors::success());
        let cancel_style = sel(5).fg(crate::colors::text());
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
        cursor = lines.len() as u16;

        // No footer hints in the editor form

        debug_assert_eq!(cursor as usize, lines.len());

        AgentEditorLayout {
            lines,
            ro_offset,
            wr_offset,
            instr_offset,
            ro_height: ro_box_h,
            wr_height: wr_box_h,
            instr_height: instr_box_h,
        }
    }
}

impl<'a> BottomPaneView<'a> for AgentEditorView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        if !self.installed {
            match key_event {
                KeyEvent { code: KeyCode::Esc, .. } | KeyEvent { code: KeyCode::Enter, .. } => { self.complete = true; }
                _ => {}
            }
            return;
        }
        match key_event {
            KeyEvent { code: KeyCode::Esc, .. } => { self.complete = true; self.app_event_tx.send(AppEvent::ShowAgentsOverview); },
            KeyEvent { code: KeyCode::Up, .. } => { if self.field > 0 { self.field -= 1; } },
            KeyEvent { code: KeyCode::Down, .. } => { self.field = (self.field + 1).min(5); },
            KeyEvent { code: KeyCode::Left, .. } if self.field == 0 => { self.enabled = true; },
            KeyEvent { code: KeyCode::Right, .. } if self.field == 0 => { self.enabled = false; },
            KeyEvent { code: KeyCode::Left, .. } if self.field == 5 => { self.field = 4; },
            KeyEvent { code: KeyCode::Right, .. } if self.field == 4 => { self.field = 5; },
            KeyEvent { code: KeyCode::Char(' '), .. } if self.field == 0 => { self.enabled = !self.enabled; },
            ev @ KeyEvent { .. } if self.field == 1 => { let _ = self.params_ro.handle_key(ev); },
            ev @ KeyEvent { .. } if self.field == 2 => { let _ = self.params_wr.handle_key(ev); },
            ev @ KeyEvent { .. } if self.field == 3 => { let _ = self.instr.handle_key(ev); },
            KeyEvent { code: KeyCode::Enter, .. } if self.field == 4 => {
                // Save: split params by whitespace; empty -> None
                let ro = self.params_ro.text().split_whitespace().map(|s| s.to_string()).collect::<Vec<_>>();
                let wr = self.params_wr.text().split_whitespace().map(|s| s.to_string()).collect::<Vec<_>>();
                let ro_opt = if ro.is_empty() { None } else { Some(ro) };
                let wr_opt = if wr.is_empty() { None } else { Some(wr) };
                let instr_opt = { let t = self.instr.text().trim().to_string(); if t.is_empty() { None } else { Some(t) } };
                self.app_event_tx.send(AppEvent::UpdateAgentConfig {
                    name: self.name.clone(),
                    enabled: self.enabled,
                    args_read_only: ro_opt,
                    args_write: wr_opt,
                    instructions: instr_opt,
                });
                self.complete = true;
                self.app_event_tx.send(AppEvent::ShowAgentsOverview);
            }
            KeyEvent { code: KeyCode::Enter, .. } if self.field == 5 => { self.complete = true; self.app_event_tx.send(AppEvent::ShowAgentsOverview); }
            _ => {}
        }
    }

    fn is_complete(&self) -> bool { self.complete }

    fn desired_height(&self, width: u16) -> u16 {
        let content_width = width.saturating_sub(4).max(1);
        let layout = self.layout(content_width, None);
        layout.lines.len().saturating_add(1) as u16
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

        if !self.installed {
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
        let AgentEditorLayout { lines, ro_offset, wr_offset, instr_offset, ro_height, wr_height, instr_height } = layout;

        Paragraph::new(lines)
            .alignment(Alignment::Left)
            .wrap(ratatui::widgets::Wrap { trim: false })
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .render(content, buf);

        // Draw input boxes at the same y offsets we reserved above
        let ro_rect = Rect { x: content.x, y: content.y.saturating_add(ro_offset), width: content.width, height: ro_height };
        let ro_rect = ro_rect.intersection(*buf.area());
        let ro_block = Block::default()
            .borders(Borders::ALL)
            .title(Line::from(" Read-only Params "))
            .border_style(if self.field == 1 { Style::default().fg(crate::colors::primary()).add_modifier(Modifier::BOLD) } else { Style::default().fg(crate::colors::border()) });
        if ro_rect.width > 0 && ro_rect.height > 0 {
            let ro_inner_rect = ro_block.inner(ro_rect);
            let ro_inner = ro_inner_rect.inner(Margin::new(1, 0));
            ro_block.render(ro_rect, buf);
            Self::clear_rect(buf, ro_inner_rect);
            self.params_ro.render(ro_inner, buf, self.field == 1);
        }

        // WR params box (3 rows)
        let wr_rect = Rect { x: content.x, y: content.y.saturating_add(wr_offset), width: content.width, height: wr_height };
        let wr_rect = wr_rect.intersection(*buf.area());
        let wr_block = Block::default()
            .borders(Borders::ALL)
            .title(Line::from(" Write Params "))
            .border_style(if self.field == 2 { Style::default().fg(crate::colors::primary()).add_modifier(Modifier::BOLD) } else { Style::default().fg(crate::colors::border()) });
        if wr_rect.width > 0 && wr_rect.height > 0 {
            let wr_inner_rect = wr_block.inner(wr_rect);
            let wr_inner = wr_inner_rect.inner(Margin::new(1, 0));
            wr_block.render(wr_rect, buf);
            Self::clear_rect(buf, wr_inner_rect);
            self.params_wr.render(wr_inner, buf, self.field == 2);
        }

        // Instructions (multi-line; height consistent with reserved space above)
        let instr_rect = Rect { x: content.x, y: content.y.saturating_add(instr_offset), width: content.width, height: instr_height };
        let instr_rect = instr_rect.intersection(*buf.area());
        let instr_block = Block::default()
            .borders(Borders::ALL)
            .title(Line::from(" Instructions "))
            .border_style(if self.field == 3 { Style::default().fg(crate::colors::primary()).add_modifier(Modifier::BOLD) } else { Style::default().fg(crate::colors::border()) });
        if instr_rect.width > 0 && instr_rect.height > 0 {
            let instr_inner_rect = instr_block.inner(instr_rect);
            let instr_inner = instr_inner_rect.inner(Margin::new(1, 0));
            instr_block.render(instr_rect, buf);
            Self::clear_rect(buf, instr_inner_rect);
            self.instr.render(instr_inner, buf, self.field == 3);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{buffer::Buffer, layout::Rect};
    use std::sync::mpsc::channel;

    #[test]
    fn write_params_margin_stays_clear() {
        let (high_tx, _high_rx) = channel();
        let (bulk_tx, _bulk_rx) = channel();
        let sender = AppEventSender::new_dual(high_tx, bulk_tx);
        let view = AgentEditorView::new(
            "claude".into(),
            true,
            None,
            Some(vec!["--dangerously-skip-permissions".into()]),
            None,
            "claude".into(),
            sender,
        );

        let area = Rect { x: 0, y: 0, width: 80, height: 24 };
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);

        let preferred_layout = view.layout(area.width.saturating_sub(4), None);
        assert_eq!(view.desired_height(area.width), preferred_layout.lines.len().saturating_add(2) as u16);

        let spacer_idx = (preferred_layout.instr_offset + preferred_layout.instr_height) as usize;
        assert!(preferred_layout.lines[spacer_idx].spans.iter().all(|span| span.content.trim().is_empty()));
        let buttons_idx = spacer_idx + 1;
        let buttons_line = &preferred_layout.lines[buttons_idx];
        let buttons_text: String = buttons_line
            .spans
            .iter()
            .map(|span| span.content.clone().into_owned())
            .collect();
        assert!(buttons_text.contains("[ Save ]"));

        let mut found = false;
        for y in area.y..area.y + area.height {
            let mut row = String::with_capacity(area.width as usize);
            for x in area.x..area.x + area.width {
                row.push_str(buf[(x, y)].symbol());
            }
            if let Some(idx) = row.find("--dangerously-skip-permissions") {
                assert!(idx > 0, "text unexpectedly starts at column 0");
                let prev = row[..idx].chars().last();
                assert_eq!(prev, Some(' '));
                found = true;
                break;
            }
        }
        assert!(found, "write params text not found in rendered buffer");
    }
}
