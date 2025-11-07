use code_core::config_types::{validation_tool_category, ValidationCategory};
use code_core::protocol::ValidationGroup;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};
use std::cell::Cell;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::colors;

use super::bottom_pane_view::BottomPaneView;
use super::scroll_state::ScrollState;
use super::BottomPane;

#[derive(Clone, Debug)]
pub(crate) struct ToolStatus {
    pub name: &'static str,
    pub description: &'static str,
    pub installed: bool,
    pub install_hint: String,
    pub category: ValidationCategory,
}

#[derive(Clone, Debug)]
pub(crate) struct GroupStatus {
    pub group: ValidationGroup,
    pub name: &'static str,
}

#[derive(Clone, Debug)]
pub(crate) struct ToolRow {
    pub status: ToolStatus,
    pub enabled: bool,
    pub group_enabled: bool,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum SelectionKind {
    Group(usize),
    Tool(usize),
}

enum RowData {
    Header { group_idx: usize },
    Spacer,
    Tool { idx: usize },
}

const DEFAULT_VISIBLE_ROWS: usize = 8;

pub(crate) struct ValidationSettingsView {
    groups: Vec<(GroupStatus, bool)>,
    tools: Vec<ToolRow>,
    app_event_tx: AppEventSender,
    state: ScrollState,
    is_complete: bool,
    tool_name_width: usize,
    viewport_rows: Cell<usize>,
    pending_notice: Option<String>,
}

impl ValidationSettingsView {
    pub fn new(
        groups: Vec<(GroupStatus, bool)>,
        tools: Vec<ToolRow>,
        app_event_tx: AppEventSender,
    ) -> Self {
        let mut state = ScrollState::new();
        state.selected_idx = Some(0);
        let tool_name_width = tools.iter().map(|row| row.status.name.len()).max().unwrap_or(0);
        Self {
            groups,
            tools,
            app_event_tx,
            state,
            is_complete: false,
            tool_name_width,
            viewport_rows: Cell::new(0),
            pending_notice: None,
        }
    }

    fn toggle_group(&mut self, idx: usize) {
        if idx >= self.groups.len() {
            return;
        }
        let group = self.groups[idx].0.group;
        let new_value;
        {
            let (_, enabled) = &mut self.groups[idx];
            new_value = !*enabled;
            *enabled = new_value;
        }
        self.apply_group_to_tools(group, new_value);
        self.app_event_tx
            .send(AppEvent::UpdateValidationGroup { group, enable: new_value });
    }

    fn toggle_tool(&mut self, idx: usize) {
        if let Some(row) = self.tools.get_mut(idx) {
            if !row.status.installed {
                return;
            }
            row.enabled = !row.enabled;
            self.app_event_tx.send(AppEvent::UpdateValidationTool {
                name: row.status.name.to_string(),
                enable: row.enabled,
            });
        }
    }

    fn apply_group_to_tools(&mut self, group: ValidationGroup, enabled: bool) {
        for tool in &mut self.tools {
            if group_for_category(tool.status.category) == group {
                tool.group_enabled = enabled;
            }
        }
    }

    fn visible_budget(&self, total: usize) -> usize {
        if total == 0 {
            return 1;
        }
        let hint = self.viewport_rows.get();
        let target = if hint == 0 { DEFAULT_VISIBLE_ROWS } else { hint };
        target.clamp(1, total)
    }

    fn build_rows(&self) -> (Vec<RowData>, Vec<usize>, Vec<SelectionKind>) {
        let mut rows = Vec::new();
        let mut selection_rows = Vec::new();
        let mut selection_kinds = Vec::new();

        for (group_idx, (status, enabled)) in self.groups.iter().enumerate() {
            rows.push(RowData::Header { group_idx });
            selection_rows.push(rows.len() - 1);
            selection_kinds.push(SelectionKind::Group(group_idx));

            for (idx, row) in self.tools.iter().enumerate() {
                if group_for_category(row.status.category) == status.group {
                    rows.push(RowData::Tool { idx });
                    if *enabled {
                        selection_rows.push(rows.len() - 1);
                        selection_kinds.push(SelectionKind::Tool(idx));
                    }
                }
            }

            if group_idx + 1 < self.groups.len() {
                rows.push(RowData::Spacer);
            }
        }

        (rows, selection_rows, selection_kinds)
    }

    fn activate_selection(&mut self, pane: Option<&mut BottomPane<'_>>, selection: SelectionKind) {
        match selection {
            SelectionKind::Group(idx) => self.toggle_group(idx),
            SelectionKind::Tool(idx) => {
                if let Some(tool) = self.tools.get(idx) {
                    if !tool.status.installed {
                        let command = tool.status.install_hint.trim().to_string();
                        let tool_name = tool.status.name.to_string();
                        if command.is_empty() {
                            self.flash_notice(
                                pane,
                                format!(
                                    "No install command available for {}",
                                    tool_name
                                ),
                            );
                        } else {
                            self.flash_notice(
                                pane,
                                format!(
                                    "Opening terminal to install {}",
                                    tool_name
                                ),
                            );
                            self.is_complete = true;
                            self.app_event_tx.send(AppEvent::RequestValidationToolInstall {
                                name: tool_name,
                                command,
                            });
                        }
                    } else {
                        self.toggle_tool(idx);
                    }
                }
            }
        }
    }

    fn flash_notice(&mut self, pane: Option<&mut BottomPane<'_>>, text: String) {
        if let Some(pane) = pane {
            pane.flash_footer_notice(text.clone());
        }
        self.pending_notice = Some(text);
        self.app_event_tx.send(AppEvent::RequestRedraw);
    }

    fn handle_key_event_internal(&mut self, mut pane: Option<&mut BottomPane<'_>>, key_event: KeyEvent) {
        let (_, _, selection_kinds) = self.build_rows();
        let mut total = selection_kinds.len();
        if total == 0 {
            if matches!(key_event.code, KeyCode::Esc) {
                self.is_complete = true;
            }
            return;
        }

        if self.state.selected_idx.is_none() {
            self.state.selected_idx = Some(0);
        }
        self.state.clamp_selection(total);
        self.state.scroll_top = self.state.scroll_top.min(total.saturating_sub(1));
        let visible_budget = self.visible_budget(total);
        self.state.ensure_visible(total, visible_budget);

        let current_kind = self
            .state
            .selected_idx
            .and_then(|sel| selection_kinds.get(sel))
            .copied();

        match key_event {
            KeyEvent { code: KeyCode::Up, .. } => {
                self.state.move_up_wrap(total);
            }
            KeyEvent { code: KeyCode::Down, .. } => {
                self.state.move_down_wrap(total);
            }
            KeyEvent { code: KeyCode::Left, .. } | KeyEvent { code: KeyCode::Right, .. } => {
                if let Some(kind) = current_kind {
                    match kind {
                        SelectionKind::Group(idx) => self.toggle_group(idx),
                        SelectionKind::Tool(idx) => {
                            if let Some(tool) = self.tools.get(idx) {
                                if tool.status.installed {
                                    self.toggle_tool(idx);
                                }
                            }
                        }
                    }
                }
            }
            KeyEvent { code: KeyCode::Char(' '), .. } => {
                if let Some(kind) = current_kind {
                    self.activate_selection(pane.take(), kind);
                }
            }
            KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. } => {
                if let Some(kind) = current_kind {
                    self.activate_selection(pane.take(), kind);
                }
            }
            KeyEvent { code: KeyCode::Esc, .. } => {
                self.is_complete = true;
            }
            _ => {}
        }

        let (_, _, selection_kinds) = self.build_rows();
        total = selection_kinds.len();
        if total == 0 {
            self.state.selected_idx = None;
            self.state.scroll_top = 0;
        } else {
            self.state.clamp_selection(total);
            self.state.scroll_top = self.state.scroll_top.min(total.saturating_sub(1));
            let visible_budget = self.visible_budget(total);
            self.state.ensure_visible(total, visible_budget);
        }
    }

    pub fn handle_key_event_direct(&mut self, key_event: KeyEvent) {
        self.handle_key_event_internal(None, key_event);
    }

    pub fn is_view_complete(&self) -> bool {
        self.is_complete
    }

    fn render_header_lines(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(Span::styled(
                "Toggle validation groups and installed tools.",
                Style::default().fg(colors::text_dim()),
            )),
            Line::from(Span::styled(
                "Use ↑↓ to navigate · Enter/Space toggle · Esc close",
                Style::default().fg(colors::text_dim()),
            )),
            Line::from(""),
        ]
    }

    fn render_footer_lines(&self) -> Vec<Line<'static>> {
        let base = Line::from(vec![
            Span::styled("↑↓", Style::default().fg(colors::function())),
            Span::styled(" Navigate  ", Style::default().fg(colors::text_dim())),
            Span::styled("Enter", Style::default().fg(colors::success())),
            Span::styled(" Toggle  ", Style::default().fg(colors::text_dim())),
            Span::styled("Space", Style::default().fg(colors::success())),
            Span::styled(" Toggle  ", Style::default().fg(colors::text_dim())),
            Span::styled("Esc", Style::default().fg(colors::error())),
            Span::styled(" Close", Style::default().fg(colors::text_dim())),
        ]);

        let mut lines = vec![base];
        if let Some(notice) = &self.pending_notice {
            lines.push(Line::from(vec![Span::styled(
                notice.clone(),
                Style::default().fg(colors::warning()),
            )]));
        }
        lines
    }

    fn render_row(&self, row: &RowData, selected: bool) -> Line<'static> {
        let arrow = if selected { "› " } else { "  " };
        let arrow_style = if selected {
            Style::default().fg(colors::primary())
        } else {
            Style::default().fg(colors::text_dim())
        };
        match row {
            RowData::Header { group_idx } => {
                let Some((status, enabled)) = self.groups.get(*group_idx) else {
                    return Line::from("");
                };
                let description = match status.group {
                    ValidationGroup::Functional => "Compile & structural checks",
                    ValidationGroup::Stylistic => "Formatting and style linting",
                };
                let label_style = if selected {
                    Style::default().fg(colors::primary()).add_modifier(Modifier::BOLD)
                } else if *enabled {
                    Style::default().fg(colors::text()).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(colors::text_dim()).add_modifier(Modifier::BOLD)
                };
                let status_span = if *enabled {
                    Span::styled("enabled", Style::default().fg(colors::success()))
                } else {
                    Span::styled("disabled", Style::default().fg(colors::text_dim()))
                };
                let mut spans = vec![
                    Span::styled(arrow, arrow_style),
                    Span::styled(status.name, label_style),
                    Span::raw("  "),
                    status_span,
                    Span::raw("  "),
                    Span::styled(description, Style::default().fg(colors::text_dim())),
                ];
                if selected {
                    let hint = if *enabled { "(press Enter to disable)" } else { "(press Enter to enable)" };
                    spans.push(Span::raw("  "));
                    spans.push(Span::styled(hint, Style::default().fg(colors::text_dim())));
                }
                Line::from(spans)
            }
            RowData::Spacer => Line::from(""),
            RowData::Tool { idx } => {
                let row = &self.tools[*idx];
                let width = self.tool_name_width.max(row.status.name.len());
                let base_style = if row.group_enabled {
                    if selected {
                        Style::default().fg(colors::primary()).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(colors::text())
                    }
                } else {
                    Style::default().fg(colors::text_dim())
                };
                let name_span = Span::styled(
                    format!("{name:<width$}", name = row.status.name, width = width),
                    base_style,
                );
                let mut spans = vec![Span::styled(arrow, arrow_style), Span::raw("  "), name_span];
                spans.push(Span::raw("  "));
                if row.group_enabled {
                    let (status_label, status_style) = if !row.status.installed {
                        ("missing", Style::default().fg(colors::warning()).add_modifier(Modifier::BOLD))
                    } else if row.enabled {
                        ("enabled", Style::default().fg(colors::success()))
                    } else {
                        ("disabled", Style::default().fg(colors::warning()))
                    };
                    spans.push(Span::styled(status_label, status_style));
                    spans.push(Span::raw("  "));
                    spans.push(Span::styled(row.status.description, Style::default().fg(colors::text_dim())));
                    if selected {
                        let hint = if !row.status.installed {
                            "(press Enter to install)"
                        } else {
                            "(press Enter to toggle)"
                        };
                        spans.push(Span::raw("  "));
                        spans.push(Span::styled(hint, Style::default().fg(colors::text_dim())));
                    }
                } else {
                    spans.push(Span::styled(
                        row.status.description,
                        Style::default().fg(colors::text_dim()),
                    ));
                }
                Line::from(spans)
            }
        }
    }
}

impl<'a> BottomPaneView<'a> for ValidationSettingsView {
    fn handle_key_event(&mut self, pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        self.handle_key_event_internal(Some(pane), key_event);
    }

    fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn desired_height(&self, _width: u16) -> u16 {
        let base = 6; // header + footer + padding
        let rows = (self.groups.len() + self.tools.len() + 2) as u16; // section headers and spacing
        base + rows.min(18)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        Clear.render(area, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(colors::border()))
            .style(Style::default().bg(colors::background()).fg(colors::text()))
            .title(" Validation Settings ")
            .title_alignment(Alignment::Center);
        let inner = block.inner(area);
        block.render(area, buf);

        let header_lines = self.render_header_lines();
        let footer_lines = self.render_footer_lines();

        let available_height = inner.height as usize;
        let header_height = header_lines.len().min(available_height) as usize;
        let footer_height = if available_height > header_height {
            1 + footer_lines.len()
        } else {
            0
        };
        let list_height = available_height.saturating_sub(header_height + footer_height);
        let visible_slots = list_height.max(1);
        self.viewport_rows.set(visible_slots);

        let (rows, selection_rows, _) = self.build_rows();
        let selection_count = selection_rows.len();
        let selected_idx = self.state.selected_idx.unwrap_or(0).min(selection_count.saturating_sub(1));
        let selected_row_index = selection_rows.get(selected_idx).copied().unwrap_or(0);

        let mut start_row = selection_rows
            .get(self.state.scroll_top.min(selection_count.saturating_sub(1)))
            .copied()
            .unwrap_or(0);
        while start_row > 0 {
            match rows[start_row - 1] {
                RowData::Header { .. } => start_row -= 1,
                RowData::Spacer => start_row -= 1,
                _ => break,
            }
        }

        let mut visible_lines: Vec<Line> = Vec::new();
        visible_lines.extend(header_lines.iter().cloned());

        let mut remaining = visible_slots;
        let mut row_index = start_row;
        while remaining > 0 && row_index < rows.len() {
            let is_selected = row_index == selected_row_index;
            let line = self.render_row(&rows[row_index], is_selected);
            visible_lines.push(line);
            remaining = remaining.saturating_sub(1);
            row_index += 1;
        }

        if footer_height > 0 {
            visible_lines.push(Line::from(""));
            visible_lines.extend(footer_lines.into_iter());
        }

        Paragraph::new(visible_lines)
            .alignment(Alignment::Left)
            .style(Style::default().bg(colors::background()).fg(colors::text()))
            .render(inner, buf);
    }
}

fn group_for_category(category: ValidationCategory) -> ValidationGroup {
    match category {
        ValidationCategory::Functional => ValidationGroup::Functional,
        ValidationCategory::Stylistic => ValidationGroup::Stylistic,
    }
}

pub(crate) fn detect_tools() -> Vec<ToolStatus> {
    vec![
        ToolStatus {
            name: "actionlint",
            description: "Lint GitHub workflows for syntax and logic issues.",
            installed: has("actionlint"),
            install_hint: actionlint_hint(),
            category: validation_tool_category("actionlint"),
        },
        ToolStatus {
            name: "shellcheck",
            description: "Analyze shell scripts for bugs and common pitfalls.",
            installed: has("shellcheck"),
            install_hint: shellcheck_hint(),
            category: validation_tool_category("shellcheck"),
        },
        ToolStatus {
            name: "markdownlint",
            description: "Lint Markdown content for style and formatting problems.",
            installed: has("markdownlint"),
            install_hint: markdownlint_hint(),
            category: validation_tool_category("markdownlint"),
        },
        ToolStatus {
            name: "hadolint",
            description: "Lint Dockerfiles for best practices and mistakes.",
            installed: has("hadolint"),
            install_hint: hadolint_hint(),
            category: validation_tool_category("hadolint"),
        },
        ToolStatus {
            name: "yamllint",
            description: "Validate YAML files for syntax issues.",
            installed: has("yamllint"),
            install_hint: yamllint_hint(),
            category: validation_tool_category("yamllint"),
        },
        ToolStatus {
            name: "cargo-check",
            description: "Run `cargo check` to catch Rust compilation errors quickly.",
            installed: has("cargo"),
            install_hint: cargo_check_hint(),
            category: validation_tool_category("cargo-check"),
        },
        ToolStatus {
            name: "tsc",
            description: "Type-check TypeScript projects with `tsc --noEmit`.",
            installed: has("tsc"),
            install_hint: tsc_hint(),
            category: validation_tool_category("tsc"),
        },
        ToolStatus {
            name: "eslint",
            description: "Lint JavaScript/TypeScript with ESLint (no warnings allowed).",
            installed: has("eslint"),
            install_hint: eslint_hint(),
            category: validation_tool_category("eslint"),
        },
        ToolStatus {
            name: "mypy",
            description: "Static type-check Python files using mypy.",
            installed: has("mypy"),
            install_hint: mypy_hint(),
            category: validation_tool_category("mypy"),
        },
        ToolStatus {
            name: "pyright",
            description: "Run Pyright for fast Python type analysis.",
            installed: has("pyright"),
            install_hint: pyright_hint(),
            category: validation_tool_category("pyright"),
        },
        ToolStatus {
            name: "phpstan",
            description: "Analyze PHP code with phpstan using project rules.",
            installed: has("phpstan"),
            install_hint: phpstan_hint(),
            category: validation_tool_category("phpstan"),
        },
        ToolStatus {
            name: "psalm",
            description: "Run Psalm to detect PHP runtime issues.",
            installed: has("psalm"),
            install_hint: psalm_hint(),
            category: validation_tool_category("psalm"),
        },
        ToolStatus {
            name: "golangci-lint",
            description: "Lint Go modules with golangci-lint.",
            installed: has("golangci-lint"),
            install_hint: golangci_lint_hint(),
            category: validation_tool_category("golangci-lint"),
        },
        ToolStatus {
            name: "shfmt",
            description: "Format shell scripts consistently with shfmt.",
            installed: has("shfmt"),
            install_hint: shfmt_hint(),
            category: validation_tool_category("shfmt"),
        },
        ToolStatus {
            name: "prettier",
            description: "Format web assets (JS/TS/JSON/MD) with Prettier.",
            installed: has("prettier"),
            install_hint: prettier_hint(),
            category: validation_tool_category("prettier"),
        },
    ]
}

fn which(exe: &str) -> Option<std::path::PathBuf> {
    let name = std::ffi::OsStr::new(exe);
    let paths: Vec<std::path::PathBuf> = std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).collect())
        .unwrap_or_else(Vec::new);
    for dir in paths {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn has(cmd: &str) -> bool {
    which(cmd).is_some()
}

fn is_macos() -> bool {
    cfg!(target_os = "macos")
}

pub fn actionlint_hint() -> String {
    if is_macos() && has("brew") {
        return "brew install actionlint".to_string();
    }
    if has("brew") {
        return "brew install actionlint".to_string();
    }
    "See: https://github.com/rhysd/actionlint#installation".to_string()
}

pub fn shellcheck_hint() -> String {
    if is_macos() && has("brew") {
        return "brew install shellcheck".to_string();
    }
    if has("apt-get") {
        return "sudo apt-get update && sudo apt-get install -y shellcheck".to_string();
    }
    if has("dnf") {
        return "sudo dnf install -y ShellCheck".to_string();
    }
    if has("yum") {
        return "sudo yum install -y ShellCheck".to_string();
    }
    if has("brew") {
        return "brew install shellcheck".to_string();
    }
    "https://www.shellcheck.net/".to_string()
}

pub fn markdownlint_hint() -> String {
    if has("npm") {
        return "npm i -g markdownlint-cli2".to_string();
    }
    if is_macos() && has("brew") {
        return "brew install markdownlint-cli2".to_string();
    }
    "npm i -g markdownlint-cli2".to_string()
}

pub fn hadolint_hint() -> String {
    if is_macos() && has("brew") {
        return "brew install hadolint".to_string();
    }
    if has("apt-get") {
        return "sudo apt-get update && sudo apt-get install -y hadolint".to_string();
    }
    if has("dnf") {
        return "sudo dnf install -y hadolint".to_string();
    }
    if has("yum") {
        return "sudo yum install -y hadolint".to_string();
    }
    if has("brew") {
        return "brew install hadolint".to_string();
    }
    "https://github.com/hadolint/hadolint".to_string()
}

pub fn yamllint_hint() -> String {
    if is_macos() && has("brew") {
        return "brew install yamllint".to_string();
    }
    if has("apt-get") {
        return "sudo apt-get update && sudo apt-get install -y yamllint".to_string();
    }
    if has("dnf") {
        return "sudo dnf install -y yamllint".to_string();
    }
    if has("yum") {
        return "sudo yum install -y yamllint".to_string();
    }
    if has("brew") {
        return "brew install yamllint".to_string();
    }
    "https://yamllint.readthedocs.io/".to_string()
}

pub fn cargo_check_hint() -> String {
    if has("cargo") {
        return "cargo check --all-targets".to_string();
    }
    "Install Rust (https://rustup.rs) to enable cargo check".to_string()
}

pub fn shfmt_hint() -> String {
    if is_macos() && has("brew") {
        return "brew install shfmt".to_string();
    }
    if has("apt-get") {
        return "sudo apt-get update && sudo apt-get install -y shfmt".to_string();
    }
    if has("dnf") {
        return "sudo dnf install -y shfmt".to_string();
    }
    if has("yum") {
        return "sudo yum install -y shfmt".to_string();
    }
    if has("brew") {
        return "brew install shfmt".to_string();
    }
    "https://github.com/mvdan/sh".to_string()
}

pub fn prettier_hint() -> String {
    if has("npm") {
        return "npx --yes prettier --write <path>".to_string();
    }
    if is_macos() && has("brew") {
        return "brew install prettier".to_string();
    }
    "npm install --global prettier".to_string()
}

pub fn tsc_hint() -> String {
    if has("pnpm") {
        return "pnpm add -D typescript".to_string();
    }
    if has("yarn") {
        return "yarn add --dev typescript".to_string();
    }
    "npm install --save-dev typescript".to_string()
}

pub fn eslint_hint() -> String {
    if has("pnpm") {
        return "pnpm add -D eslint".to_string();
    }
    if has("yarn") {
        return "yarn add --dev eslint".to_string();
    }
    "npm install --save-dev eslint".to_string()
}

pub fn phpstan_hint() -> String {
    if has("composer") {
        return "composer require --dev phpstan/phpstan".to_string();
    }
    "See: https://phpstan.org/user-guide/getting-started".to_string()
}

pub fn psalm_hint() -> String {
    if has("composer") {
        return "composer require --dev vimeo/psalm".to_string();
    }
    "See: https://psalm.dev/docs/install/".to_string()
}

pub fn mypy_hint() -> String {
    if has("pipx") {
        return "pipx install mypy".to_string();
    }
    if has("pip3") {
        return "pip3 install --user mypy".to_string();
    }
    "pip install --user mypy".to_string()
}

pub fn pyright_hint() -> String {
    if has("npm") {
        return "npm install --save-dev pyright".to_string();
    }
    if has("pipx") {
        return "pipx install pyright".to_string();
    }
    "See: https://github.com/microsoft/pyright".to_string()
}

pub fn golangci_lint_hint() -> String {
    if is_macos() && has("brew") {
        return "brew install golangci-lint".to_string();
    }
    if has("go") {
        return "go install github.com/golangci/golangci-lint/cmd/golangci-lint@latest".to_string();
    }
    "https://golangci-lint.run/usage/install/".to_string()
}
