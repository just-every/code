use chrono::DateTime;
use chrono::Utc;
use codex_cloud_tasks_client::TaskStatus;
use codex_cloud_tasks_client::TaskSummary;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::bottom_pane_view::BottomPaneView;
use crate::bottom_pane::scroll_state::ScrollState;
use crate::bottom_pane::selection_popup_common::{render_rows, GenericDisplayRow};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

const MAX_VISIBLE_ROWS: usize = 8;

pub(crate) struct CloudTasksView {
    tasks: Vec<TaskSummary>,
    rows: Vec<GenericDisplayRow>,
    state: ScrollState,
    env_label: String,
    env_filter: Option<String>,
    footer_hint: String,
    app_event_tx: AppEventSender,
    complete: bool,
}

impl CloudTasksView {
    pub(crate) fn new(
        tasks: Vec<TaskSummary>,
        env_label: Option<String>,
        env_filter: Option<String>,
        app_event_tx: AppEventSender,
    ) -> Self {
        let mut state = ScrollState::new();
        if !tasks.is_empty() {
            state.selected_idx = Some(0);
        }
        let rows = build_rows(&tasks);
        let mut view = Self {
            tasks,
            rows,
            state,
            env_label: env_label.unwrap_or_else(|| "All environments".to_string()),
            env_filter,
            footer_hint: "↑↓ select · Enter actions · r refresh · n new · e environments · Esc close".to_string(),
            app_event_tx,
            complete: false,
        };
        view.state.clamp_selection(view.tasks.len());
        view
    }

    fn selected_task_id(&self) -> Option<&str> {
        self.state
            .selected_idx
            .and_then(|idx| self.tasks.get(idx))
            .map(|task| task.id.0.as_str())
    }

    fn max_rows(&self) -> usize {
        MAX_VISIBLE_ROWS.min(self.rows.len().max(1))
    }
}

impl BottomPaneView<'_> for CloudTasksView {
    fn handle_key_event(&mut self, _pane: &mut super::BottomPane<'_>, key: crossterm::event::KeyEvent) {
        use crossterm::event::KeyCode;
        match key.code {
            KeyCode::Up => {
                self.state.move_up_wrap(self.tasks.len());
                self.state.ensure_visible(self.tasks.len(), self.max_rows());
            }
            KeyCode::Down => {
                self.state.move_down_wrap(self.tasks.len());
                self.state.ensure_visible(self.tasks.len(), self.max_rows());
            }
            KeyCode::Enter => {
                if let Some(task_id) = self.selected_task_id() {
                    self.app_event_tx
                        .send(AppEvent::ShowCloudTaskActions { task_id: task_id.to_string() });
                }
            }
            KeyCode::Esc => {
                self.complete = true;
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                self.app_event_tx.send(AppEvent::FetchCloudTasks {
                    environment: self.env_filter.clone(),
                });
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                self.app_event_tx.send(AppEvent::OpenCloudTaskCreate);
            }
            KeyCode::Char('e') | KeyCode::Char('E') => {
                self.app_event_tx.send(AppEvent::FetchCloudEnvironments);
            }
            _ => {}
        }
    }

    fn is_complete(&self) -> bool {
        self.complete
    }

    fn on_ctrl_c(&mut self, _pane: &mut super::BottomPane<'_>) -> super::CancellationEvent {
        self.complete = true;
        super::CancellationEvent::Handled
    }

    fn desired_height(&self, _width: u16) -> u16 {
        let rows = self.max_rows() as u16;
        // borders + env header + spacer + rows + footer
        rows.saturating_add(5)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        let title = format!(" Cloud tasks — {} ", self.env_label);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(crate::colors::border()))
            .title(title);
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height == 0 { return; }

        // Header line (environment info)
        let header_area = Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: 1,
        };
        let header_spans = vec![
            Span::styled("▌ ", Style::default().fg(crate::colors::text_dim())),
            Span::styled(
                format!("Environment: {}", self.env_label),
                Style::default().fg(crate::colors::text()),
            ),
        ];
        Paragraph::new(Line::from(header_spans)).render(header_area, buf);

        // Footer hint
        let footer_area = Rect {
            x: inner.x,
            y: inner.y.saturating_add(inner.height.saturating_sub(1)),
            width: inner.width,
            height: 1,
        };
        Paragraph::new(Line::from(vec![Span::styled(
            self.footer_hint.clone(),
            Style::default().fg(crate::colors::text_dim()),
        )]))
        .render(footer_area, buf);

        if inner.height <= 2 {
            return;
        }

        let list_area = Rect {
            x: inner.x,
            y: inner.y.saturating_add(1),
            width: inner.width,
            height: inner.height.saturating_sub(2),
        };

        render_rows(
            list_area,
            buf,
            &self.rows,
            &self.state,
            self.max_rows(),
            false,
        );
    }
}

fn build_rows(tasks: &[TaskSummary]) -> Vec<GenericDisplayRow> {
    if tasks.is_empty() {
        return vec![GenericDisplayRow {
            name: "No cloud tasks available".to_string(),
            match_indices: None,
            is_current: false,
            description: Some("Use `n` to create a new task or refresh with `r`.".to_string()),
            name_color: None,
        }];
    }

    tasks
        .iter()
        .map(|task| {
            let (status_label, status_color) = format_status(&task.status);
            let attempts = task.attempt_total.unwrap_or(1);
            let attempt_label = if attempts > 1 {
                format!(" · best-of {attempts}")
            } else {
                String::new()
            };
            let mut name = format!("{status_label} · {}", task.title);
            name.push_str(&attempt_label);

            let env_display = task
                .environment_label
                .as_ref()
                .or(task.environment_id.as_ref())
                .map(|env| format!("env: {env}"));
            let diff_summary = format_diff_summary(&task.summary);
            let updated = format_updated(task.updated_at);

            let mut description_parts = vec![diff_summary, updated];
            if let Some(env) = env_display {
                description_parts.push(env);
            }
            let description = description_parts.join(" · ");

            GenericDisplayRow {
                name,
                match_indices: None,
                is_current: matches!(task.status, TaskStatus::Applied),
                description: Some(description),
                name_color: status_color,
            }
        })
        .collect()
}

fn format_status(status: &TaskStatus) -> (&'static str, Option<ratatui::style::Color>) {
    match status {
        TaskStatus::Ready => ("READY", Some(crate::colors::success_green())),
        TaskStatus::Pending => ("PENDING", Some(crate::colors::text_mid())),
        TaskStatus::Applied => ("APPLIED", Some(crate::colors::primary())),
        TaskStatus::Error => ("ERROR", Some(crate::colors::error())),
    }
}

fn format_diff_summary(summary: &codex_cloud_tasks_client::DiffSummary) -> String {
    format!(
        "Δ {} files · +{} / -{}",
        summary.files_changed,
        summary.lines_added,
        summary.lines_removed
    )
}

fn format_updated(updated_at: DateTime<Utc>) -> String {
    let now = Utc::now();
    let delta = now.signed_duration_since(updated_at);
    if delta.num_minutes() < 1 {
        "updated just now".to_string()
    } else if delta.num_minutes() < 60 {
        format!("updated {}m ago", delta.num_minutes())
    } else if delta.num_hours() < 24 {
        format!("updated {}h ago", delta.num_hours())
    } else {
        format!("updated {}d ago", delta.num_days())
    }
}
