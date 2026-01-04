use crate::history::compat::HistoryRecord;
use code_core::config::Config;
use ratatui::text::Line;

use super::assistant::AssistantMarkdownCell;
use super::background::BackgroundEventCell;
use super::context::ContextCell;
use super::diff::DiffCell;
use super::exec::ExecCell;
use super::exec_merged::MergedExecCell;
use super::explore::{explore_lines_from_record, ExploreAggregationCell};
use super::image::ImageOutputCell;
use super::loading::LoadingCell;
use super::patch::PatchSummaryCell;
use super::plan_update::PlanUpdateCell;
use super::plain::PlainHistoryCell;
use super::rate_limits::RateLimitsCell;
use super::reasoning::CollapsibleReasoningCell;
use super::stream::StreamingContentCell;
use super::tool::{RunningToolCallCell, ToolCallCell};
use super::upgrade::UpgradeNoticeCell;
use super::wait_status::WaitStatusCell;
use super::HistoryCell;

pub(crate) fn cell_from_record(record: &HistoryRecord, cfg: &Config) -> Box<dyn HistoryCell> {
    match record {
        HistoryRecord::PlainMessage(state) => Box::new(PlainHistoryCell::from_state(state.clone())),
        HistoryRecord::WaitStatus(state) => Box::new(WaitStatusCell::from_state(state.clone())),
        HistoryRecord::Loading(state) => Box::new(LoadingCell::from_state(state.clone())),
        HistoryRecord::RunningTool(state) => {
            Box::new(RunningToolCallCell::from_state(state.clone()))
        }
        HistoryRecord::ToolCall(state) => Box::new(ToolCallCell::from_state(state.clone())),
        HistoryRecord::PlanUpdate(state) => Box::new(PlanUpdateCell::from_state(state.clone())),
        HistoryRecord::UpgradeNotice(state) => Box::new(UpgradeNoticeCell::from_state(state.clone())),
        HistoryRecord::Reasoning(state) => Box::new(CollapsibleReasoningCell::from_state(state.clone())),
        HistoryRecord::Exec(state) => Box::new(ExecCell::from_record(state.clone())),
        HistoryRecord::MergedExec(state) => Box::new(MergedExecCell::from_state(state.clone())),
        HistoryRecord::AssistantStream(state) => {
            Box::new(StreamingContentCell::from_state(
                state.clone(),
                cfg.file_opener,
                cfg.cwd.clone(),
            ))
        }
        HistoryRecord::AssistantMessage(state) => {
            Box::new(AssistantMarkdownCell::from_state(state.clone(), cfg))
        }
        HistoryRecord::Diff(state) => Box::new(DiffCell::from_record(state.clone())),
        HistoryRecord::Image(state) => Box::new(ImageOutputCell::from_record(state.clone())),
        HistoryRecord::Explore(state) => {
            Box::new(ExploreAggregationCell::from_record(state.clone()))
        }
        HistoryRecord::RateLimits(state) => Box::new(RateLimitsCell::from_record(state.clone())),
        HistoryRecord::Patch(state) => Box::new(PatchSummaryCell::from_record(state.clone())),
        HistoryRecord::BackgroundEvent(state) => Box::new(BackgroundEventCell::new(state.clone())),
        HistoryRecord::Notice(state) => Box::new(PlainHistoryCell::from_notice_record(state.clone())),
        HistoryRecord::Context(state) => Box::new(ContextCell::new(state.clone())),
    }
}

pub(crate) fn lines_from_record(record: &HistoryRecord, cfg: &Config) -> Vec<Line<'static>> {
    match record {
        HistoryRecord::Explore(state) => return explore_lines_from_record(state),
        _ => {}
    }
    cell_from_record(record, cfg).display_lines_trimmed()
}

pub(crate) fn record_from_cell(cell: &dyn HistoryCell) -> Option<HistoryRecord> {
    if let Some(plain) = cell.as_any().downcast_ref::<PlainHistoryCell>() {
        return Some(HistoryRecord::PlainMessage(plain.state().clone()));
    }
    if let Some(wait) = cell.as_any().downcast_ref::<WaitStatusCell>() {
        return Some(HistoryRecord::WaitStatus(wait.state().clone()));
    }
    if let Some(loading) = cell.as_any().downcast_ref::<LoadingCell>() {
        return Some(HistoryRecord::Loading(loading.state().clone()));
    }
    if let Some(background) = cell
        .as_any()
        .downcast_ref::<BackgroundEventCell>()
    {
        return Some(HistoryRecord::BackgroundEvent(background.state().clone()));
    }
    if let Some(context) = cell.as_any().downcast_ref::<ContextCell>() {
        return Some(HistoryRecord::Context(context.record().clone()));
    }
    if let Some(merged) = cell.as_any().downcast_ref::<MergedExecCell>() {
        return Some(HistoryRecord::MergedExec(merged.to_record()));
    }
    if let Some(explore) = cell
        .as_any()
        .downcast_ref::<ExploreAggregationCell>()
    {
        return Some(HistoryRecord::Explore(explore.record().clone()));
    }
    if let Some(tool_call) = cell.as_any().downcast_ref::<ToolCallCell>() {
        return Some(HistoryRecord::ToolCall(tool_call.state().clone()));
    }
    if let Some(running_tool) = cell
        .as_any()
        .downcast_ref::<RunningToolCallCell>()
    {
        return Some(HistoryRecord::RunningTool(running_tool.state().clone()));
    }
    if let Some(plan) = cell.as_any().downcast_ref::<PlanUpdateCell>() {
        return Some(HistoryRecord::PlanUpdate(plan.state().clone()));
    }
    if let Some(upgrade) = cell.as_any().downcast_ref::<UpgradeNoticeCell>() {
        return Some(HistoryRecord::UpgradeNotice(upgrade.state().clone()));
    }
    if let Some(reasoning) = cell
        .as_any()
        .downcast_ref::<CollapsibleReasoningCell>()
    {
        return Some(HistoryRecord::Reasoning(reasoning.reasoning_state()));
    }
    if let Some(exec) = cell.as_any().downcast_ref::<ExecCell>() {
        return Some(HistoryRecord::Exec(exec.record.clone()));
    }
    if let Some(stream) = cell
        .as_any()
        .downcast_ref::<StreamingContentCell>()
    {
        return Some(HistoryRecord::AssistantStream(stream.state().clone()));
    }
    if let Some(assistant) = cell
        .as_any()
        .downcast_ref::<AssistantMarkdownCell>()
    {
        return Some(HistoryRecord::AssistantMessage(assistant.state().clone()));
    }
    if let Some(diff) = cell.as_any().downcast_ref::<DiffCell>() {
        return Some(HistoryRecord::Diff(diff.record().clone()));
    }
    if let Some(image) = cell.as_any().downcast_ref::<ImageOutputCell>() {
        return Some(HistoryRecord::Image(image.record().clone()));
    }
    if let Some(patch) = cell.as_any().downcast_ref::<PatchSummaryCell>() {
        return Some(HistoryRecord::Patch(patch.record().clone()));
    }
    if let Some(rate_limits) = cell
        .as_any()
        .downcast_ref::<RateLimitsCell>()
    {
        return Some(HistoryRecord::RateLimits(rate_limits.record().clone()));
    }
    None
}
