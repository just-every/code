//! Rendering for structured plan update history cells built from `PlanUpdateState`.

use super::*;
use crate::history::state::{HistoryId, PlanIcon, PlanProgress, PlanStep, PlanUpdateState};
use code_core::plan_tool::{PlanItemArg, StepStatus, UpdatePlanArgs};

pub(crate) struct PlanUpdateCell {
    state: PlanUpdateState,
}

impl PlanUpdateCell {
    pub(crate) fn new(state: PlanUpdateState) -> Self {
        let mut state = state;
        state.id = HistoryId::ZERO;
        Self { state }
    }

    pub(crate) fn is_complete(&self) -> bool {
        let progress = &self.state.progress;
        progress.total > 0 && progress.completed >= progress.total
    }

    #[allow(dead_code)]
    pub(crate) fn from_state(state: PlanUpdateState) -> Self {
        Self { state }
    }

    pub(crate) fn state(&self) -> &PlanUpdateState {
        &self.state
    }

    pub(crate) fn state_mut(&mut self) -> &mut PlanUpdateState {
        &mut self.state
    }

    fn header_line(&self) -> Line<'static> {
        let progress = &self.state.progress;
        let is_complete = self.is_complete();
        let header_color = if is_complete {
            crate::colors::success()
        } else {
            crate::colors::info()
        };

        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.push(Span::styled(
            self.state.name.clone(),
            Style::default()
                .fg(header_color)
                .add_modifier(Modifier::BOLD),
        ));

        let bar = progress_meter(progress, 10);
        spans.push(Span::raw(" ["));
        spans.push(Span::styled(bar.filled, Style::default().fg(crate::colors::success())));
        spans.push(Span::styled(bar.empty, Style::default().add_modifier(Modifier::DIM)));
        spans.push(Span::raw("] "));
        spans.push(Span::raw(format!("{}/{}", progress.completed, progress.total)));
        Line::from(spans)
    }

    fn step_line(&self, step: &PlanStep, is_first: bool) -> Line<'static> {
        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.push(if is_first {
            Span::raw("└ ")
        } else {
            Span::raw("  ")
        });

        match step.status {
            StepStatus::Completed => {
                spans.push(Span::styled(
                    "✔",
                    Style::default().fg(crate::colors::success()),
                ));
                spans.push(Span::raw(" "));
                spans.push(Span::styled(
                    step.description.clone(),
                    Style::default()
                        .fg(crate::colors::text_dim())
                        .add_modifier(Modifier::CROSSED_OUT),
                ));
            }
            StepStatus::InProgress => {
                spans.push(Span::raw("□"));
                spans.push(Span::raw(" "));
                spans.push(Span::styled(
                    step.description.clone(),
                    Style::default().fg(crate::colors::info()),
                ));
            }
            StepStatus::Pending => {
                spans.push(Span::raw("□"));
                spans.push(Span::raw(" "));
                spans.push(Span::styled(
                    step.description.clone(),
                    Style::default().fg(crate::colors::text_dim()),
                ));
            }
        }

        Line::from(spans)
    }
}

impl HistoryCell for PlanUpdateCell {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn kind(&self) -> HistoryCellType {
        HistoryCellType::PlanUpdate
    }

    fn display_lines(&self) -> Vec<Line<'static>> {
        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.push(self.header_line());

        if self.state.steps.is_empty() {
            lines.push(Line::from("(no steps provided)".dim().italic()));
        } else {
            for (index, step) in self.state.steps.iter().enumerate() {
                lines.push(self.step_line(step, index == 0));
            }
        }

        lines
    }

    fn gutter_symbol(&self) -> Option<&'static str> {
        Some(icon_symbol(&self.state.icon))
    }
}

struct ProgressMeter {
    filled: String,
    empty: String,
}

fn progress_meter(progress: &PlanProgress, width: usize) -> ProgressMeter {
    if progress.total == 0 {
        return ProgressMeter {
            filled: String::new(),
            empty: "░".repeat(width),
        };
    }
    let filled_units = (progress.completed * width + progress.total / 2) / progress.total;
    let empty_units = width.saturating_sub(filled_units);
    ProgressMeter {
        filled: "█".repeat(filled_units),
        empty: "░".repeat(empty_units),
    }
}

fn icon_symbol(icon: &PlanIcon) -> &'static str {
    match icon {
        PlanIcon::LightBulb => "💡",
        PlanIcon::Rocket => "🚀",
        PlanIcon::Clipboard => "📋",
        PlanIcon::Custom(kind) => match kind.as_str() {
            "progress-empty" => "○",
            "progress-start" => "◔",
            "progress-mid" => "◑",
            "progress-late" => "◕",
            "progress-complete" => "●",
            _ => "•",
        },
    }
}

fn plan_progress_icon(total: usize, completed: usize) -> PlanIcon {
    if total == 0 || completed == 0 {
        PlanIcon::Custom("progress-empty".to_string())
    } else if completed >= total {
        PlanIcon::Custom("progress-complete".to_string())
    } else if completed.saturating_mul(3) <= total {
        PlanIcon::Custom("progress-start".to_string())
    } else if completed.saturating_mul(3) < total.saturating_mul(2) {
        PlanIcon::Custom("progress-mid".to_string())
    } else {
        PlanIcon::Custom("progress-late".to_string())
    }
}

pub(crate) fn new_plan_update(update: UpdatePlanArgs) -> PlanUpdateCell {
    let UpdatePlanArgs { name, plan } = update;

    let total = plan.len();
    let completed = plan
        .iter()
        .filter(|p| matches!(p.status, StepStatus::Completed))
        .count();
    let icon = plan_progress_icon(total, completed);
    let progress = PlanProgress { completed, total };

    let name = name
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("Plan")
        .to_string();

    let steps: Vec<PlanStep> = plan
        .into_iter()
        .map(|PlanItemArg { step, status }| PlanStep {
            description: step,
            status,
        })
        .collect();

    let state = PlanUpdateState {
        id: HistoryId::ZERO,
        name,
        icon,
        progress,
        steps,
    };

    PlanUpdateCell::new(state)
}
