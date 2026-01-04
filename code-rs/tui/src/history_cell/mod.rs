mod agent;
mod animated;
mod assistant;
mod auto_drive;
mod background;
mod browser;
mod card_style;
mod context;
mod core;
mod diff;
mod exec;
mod exec_helpers;
mod exec_merged;
mod explore;
mod formatting;
mod frozen;
mod image;
mod loading;
mod patch;
mod plain;
mod plan_update;
mod rate_limits;
mod reasoning;
mod registry;
mod stream;
mod text;
mod tool;
mod tool_factory;
mod upgrade;
mod wait_status;
mod web_search;

pub(crate) use assistant::{
    assistant_markdown_lines,
    compute_assistant_layout,
    AssistantLayoutCache,
    AssistantMarkdownCell,
};
pub(crate) use animated::{AnimatedWelcomeCell, new_animated_welcome};
pub(crate) use background::{
    new_background_event,
    new_connecting_mcp_status,
    BackgroundEventCell,
};
pub(crate) use context::ContextCell;
pub(crate) use core::{
    CommandOutput,
    ExecKind,
    gutter_symbol_for_kind,
    HistoryCell,
    HistoryCellType,
    PatchEventType,
    PatchKind,
    ToolCellStatus,
};
pub(crate) use diff::{diff_lines_from_record, diff_record_from_string, DiffCell};
#[allow(unused_imports)]
pub(crate) use diff::{new_diff_cell_from_string, new_diff_output};
pub(crate) use exec::{
    display_lines_from_record as exec_display_lines_from_record,
    new_active_exec_command,
    new_completed_exec_command,
    ExecCell,
};
#[allow(unused_imports)]
pub(crate) use exec::ParsedExecMetadata;
pub(crate) use exec_helpers::{
    action_enum_from_parsed,
    emphasize_shell_command_name,
    exec_command_lines,
    exec_render_parts_parsed,
    exec_render_parts_parsed_with_meta,
    first_context_path,
    format_inline_script_for_display,
    insert_line_breaks_after_double_ampersand,
    normalize_shell_command_display,
    parse_read_line_annotation,
    parse_read_line_annotation_with_range,
    running_status_line,
};
pub(crate) use exec_merged::{merged_exec_lines_from_record, MergedExecCell};
pub(crate) use explore::{
    explore_lines_from_record_with_force,
    explore_lines_without_truncation,
    explore_record_push_from_parsed,
    explore_record_update_status,
    ExploreAggregationCell,
};
#[allow(unused_imports)]
pub(crate) use explore::explore_lines_from_record;
pub(crate) use formatting::{
    clean_wait_command,
    normalize_overwrite_sequences,
    output_lines,
    pretty_provider_name,
    trim_empty_lines,
};
#[allow(unused_imports)]
pub(crate) use formatting::{build_preview_lines, line_to_plain_text, lines_to_plain_text};
pub(crate) use frozen::FrozenHistoryCell;
pub(crate) use image::ImageOutputCell;
pub(crate) use loading::LoadingCell;
#[allow(unused_imports)]
pub(crate) use loading::new_loading_cell;
pub(crate) use patch::{new_patch_apply_failure, new_patch_event, PatchSummaryCell};
pub(crate) use plain::{
    new_error_event,
    new_model_output,
    new_popular_commands_notice,
    new_prompts_output,
    new_queued_user_prompt,
    new_reasoning_output,
    new_session_info,
    new_status_output,
    new_user_prompt,
    new_warning_event,
    plain_message_state_from_lines,
    plain_message_state_from_paragraphs,
    plain_role_for_kind,
    PlainHistoryCell,
};
#[allow(unused_imports)]
pub(crate) use plain::new_text_line;
pub(crate) use plan_update::{new_plan_update, PlanUpdateCell};
pub(crate) use rate_limits::RateLimitsCell;
pub(crate) use reasoning::CollapsibleReasoningCell;
pub(crate) use registry::{cell_from_record, lines_from_record, record_from_cell};
pub(crate) use stream::{new_streaming_content, stream_lines_from_state, StreamingContentCell};
pub(crate) use tool::{RunningToolCallCell, ToolCallCell};
pub(crate) use tool_factory::{
    new_completed_custom_tool_call,
    new_completed_mcp_tool_call,
    new_completed_web_fetch_tool_call,
    new_running_browser_tool_call,
    new_running_custom_tool_call,
    new_running_mcp_tool_call,
};
#[allow(unused_imports)]
pub(crate) use tool_factory::{
    new_active_custom_tool_call,
    new_active_mcp_tool_call,
    WebFetchToolCell,
};
pub(crate) use upgrade::{new_upgrade_prelude, UpgradeNoticeCell};
pub(crate) use wait_status::{new_completed_wait_tool_call, WaitStatusCell};
pub(crate) use auto_drive::{AutoDriveActionKind, AutoDriveCardCell, AutoDriveStatus};
pub(crate) use browser::BrowserSessionCell;
pub(crate) use web_search::{WebSearchSessionCell, WebSearchStatus};
pub(crate) use agent::{AgentDetail, AgentRunCell, AgentStatusKind, AgentStatusPreview, StepProgress};
pub(crate) use crate::history::state::ExploreEntryStatus;
pub(crate) use crate::insert_history::word_wrap_lines;
pub(crate) use crate::util::buffer::{fill_rect, write_line};
pub(crate) use code_common::elapsed::format_duration;
pub(crate) use crate::history::compat::{ContextRecord, ExecStatus};
pub(crate) use ratatui::prelude::Alignment;
pub(crate) use ratatui::prelude::{Buffer, Rect, Stylize};
pub(crate) use ratatui::style::{Modifier, Style};
pub(crate) use ratatui::text::{Line, Span, Text};
pub(crate) use ratatui::widgets::{Block, Borders, Padding, Paragraph, Widget, Wrap};
#[allow(unused_imports)]
pub(crate) use ratatui::widgets::WidgetRef;
pub(crate) use std::path::Path;
