use std::io::Cursor;
use std::time::{Duration, SystemTime};

use base64::Engine;
use code_common::elapsed::format_duration;
use code_core::config::Config;
use code_core::protocol::McpInvocation;
use mcp_types::{EmbeddedResourceResource, ResourceLink};
use ratatui::prelude::{Buffer, Rect};
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Padding, Paragraph, Widget, Wrap};
use sha2::{Digest, Sha256};
use tracing::error;

use super::core::{HistoryCell, HistoryCellType, ToolCellStatus};
use super::formatting::{
    build_preview_lines,
    clean_wait_command,
    line_to_plain_text,
    lines_to_plain_text,
    pretty_provider_name,
    select_preview_from_lines,
    select_preview_from_plain_text,
    trim_empty_lines,
};
use super::image::ImageOutputCell;
use super::tool::{RunningToolCallCell, ToolCallCell};

use crate::history::compat::{
    ArgumentValue,
    HistoryId,
    ImageRecord,
    RunningToolState,
    ToolArgument,
    ToolCallState,
    ToolResultPreview,
    ToolStatus as HistoryToolStatus,
};
use ::image::ImageReader;
use crate::util::buffer::fill_rect;

#[allow(dead_code)]
pub(crate) fn new_active_mcp_tool_call(invocation: McpInvocation) -> ToolCallCell {
    let invocation_line = format_mcp_invocation(invocation);
    let invocation_text = line_to_plain_text(&invocation_line);
    let state = ToolCallState {
        id: HistoryId::ZERO,
        call_id: None,
        status: HistoryToolStatus::Running,
        title: "Working".to_string(),
        duration: None,
        arguments: vec![ToolArgument {
            name: "invocation".to_string(),
            value: ArgumentValue::Text(invocation_text),
        }],
        result_preview: None,
        error_message: None,
    };
    ToolCallCell::new(state)
}

#[allow(dead_code)]
pub(crate) fn new_active_custom_tool_call(tool_name: String, args: Option<String>) -> ToolCallCell {
    let invocation_str = if let Some(args) = args {
        format!("{}({})", tool_name, args)
    } else {
        format!("{}()", tool_name)
    };
    let state = ToolCallState {
        id: HistoryId::ZERO,
        call_id: None,
        status: HistoryToolStatus::Running,
        title: "Working".to_string(),
        duration: None,
        arguments: vec![ToolArgument {
            name: "invocation".to_string(),
            value: ArgumentValue::Text(invocation_str),
        }],
        result_preview: None,
        error_message: None,
    };
    ToolCallCell::new(state)
}

// Friendly present-participle titles for running browser tools
fn browser_running_title(tool_name: &str) -> &'static str {
    match tool_name {
        "browser_click" => "Clicking...",
        "browser_type" => "Typing...",
        "browser_key" => "Sending key...",
        "browser_javascript" => "Running JavaScript...",
        "browser_scroll" => "Scrolling...",
        "browser_open" => "Opening...",
        "browser_close" => "Closing...",
        "browser_status" => "Checking status...",
        "browser_history" => "Navigating...",
        "browser_inspect" => "Inspecting...",
        "browser_console" => "Reading console...",
        "browser_move" => "Moving...",
        _ => "Working...",
    }
}

fn argument_value_from_json(value: &serde_json::Value) -> ArgumentValue {
    match value {
        serde_json::Value::String(s) => ArgumentValue::Text(s.clone()),
        serde_json::Value::Number(n) => ArgumentValue::Text(n.to_string()),
        serde_json::Value::Bool(b) => ArgumentValue::Text(b.to_string()),
        serde_json::Value::Null => ArgumentValue::Text("null".to_string()),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            ArgumentValue::Json(value.clone())
        }
    }
}

fn arguments_from_json(value: &serde_json::Value) -> Vec<ToolArgument> {
    arguments_from_json_excluding(value, &[])
}

fn arguments_from_json_excluding(
    value: &serde_json::Value,
    exclude: &[&str],
) -> Vec<ToolArgument> {
    match value {
        serde_json::Value::Object(map) => map
            .iter()
            .filter(|(key, _)| !exclude.contains(&key.as_str()))
            .map(|(key, val)| ToolArgument {
                name: key.clone(),
                value: argument_value_from_json(val),
            })
            .collect(),
        serde_json::Value::Array(items) => vec![ToolArgument {
            name: "items".to_string(),
            value: ArgumentValue::Json(serde_json::Value::Array(items.clone())),
        }],
        other => vec![ToolArgument {
            name: "args".to_string(),
            value: argument_value_from_json(other),
        }],
    }
}

pub(crate) fn new_running_browser_tool_call(
    tool_name: String,
    args: Option<String>,
) -> RunningToolCallCell {
    // Parse args JSON and use compact humanized form when possible
    let mut arguments: Vec<ToolArgument> = Vec::new();
    if let Some(args_str) = args {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&args_str) {
            if let Some(lines) = format_browser_args_humanized(&tool_name, &json) {
                let summary = lines_to_plain_text(&lines);
                if !summary.is_empty() {
                    arguments.push(ToolArgument {
                        name: "summary".to_string(),
                        value: ArgumentValue::Text(summary),
                    });
                }
            }
            let mut kv_args = arguments_from_json(&json);
            arguments.append(&mut kv_args);
        }
    }
    let state = RunningToolState {
        id: HistoryId::ZERO,
        call_id: None,
        title: browser_running_title(&tool_name).to_string(),
        started_at: SystemTime::now(),
        arguments,
        wait_has_target: false,
        wait_has_call_id: false,
        wait_cap_ms: None,
    };
    RunningToolCallCell::new(state)
}

fn custom_tool_running_title(tool_name: &str) -> String {
    if tool_name == "wait" {
        return "Waiting".to_string();
    }
    if tool_name.starts_with("agent_") || tool_name == "agent" {
        // Reuse agent title and append ellipsis
        format!("{}...", agent_tool_title(tool_name, None))
    } else if tool_name.starts_with("browser_") {
        browser_running_title(tool_name).to_string()
    } else {
        // TitleCase from snake_case and append ellipsis
        let pretty = tool_name
            .split('_')
            .filter(|s| !s.is_empty())
            .map(|s| {
                let mut chars = s.chars();
                match chars.next() {
                    Some(f) => format!("{}{}", f.to_uppercase(), chars.as_str()),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        format!("{}...", pretty)
    }
}

pub(crate) fn new_running_custom_tool_call(
    tool_name: String,
    args: Option<String>,
) -> RunningToolCallCell {
    // Parse args JSON and format as structured key/value arguments
    let mut arguments: Vec<ToolArgument> = Vec::new();
    let mut wait_has_target = false;
    let mut wait_has_call_id = false;
    let mut wait_cap_ms = None;
    if let Some(args_str) = args {
        match serde_json::from_str::<serde_json::Value>(&args_str) {
            Ok(json) => {
                if tool_name == "wait" {
                    wait_cap_ms = json.get("timeout_ms").and_then(|v| v.as_u64());
                    if let Some(for_what) = json.get("for").and_then(|v| v.as_str()) {
                        let cleaned = clean_wait_command(for_what);
                        arguments.push(ToolArgument {
                            name: "for".to_string(),
                            value: ArgumentValue::Text(cleaned),
                        });
                        wait_has_target = true;
                    }
                    if let Some(cid) = json.get("call_id").and_then(|v| v.as_str()) {
                        arguments.push(ToolArgument {
                            name: "call_id".to_string(),
                            value: ArgumentValue::Text(cid.to_string()),
                        });
                        wait_has_call_id = true;
                    }
                    let mut remaining = json.clone();
                    if let serde_json::Value::Object(ref mut map) = remaining {
                        map.remove("for");
                        map.remove("call_id");
                        map.remove("timeout_ms");
                    }
                    let mut others = arguments_from_json(&remaining);
                    arguments.append(&mut others);
                } else {
                    let mut kv_args = arguments_from_json(&json);
                    arguments.append(&mut kv_args);
                }
            }
            Err(_) => {
                arguments.push(ToolArgument {
                    name: "args".to_string(),
                    value: ArgumentValue::Text(args_str.clone()),
                });
            }
        }
    }
    let state = RunningToolState {
        id: HistoryId::ZERO,
        call_id: None,
        title: custom_tool_running_title(&tool_name),
        started_at: SystemTime::now(),
        arguments,
        wait_has_target,
        wait_has_call_id,
        wait_cap_ms,
    };
    RunningToolCallCell::new(state)
}

pub(crate) fn new_running_mcp_tool_call(invocation: McpInvocation) -> RunningToolCallCell {
    // Represent as provider.tool(...) on one dim line beneath a generic running header with timer
    let line = format_mcp_invocation(invocation);
    let invocation_text = line_to_plain_text(&line);
    let state = RunningToolState {
        id: HistoryId::ZERO,
        call_id: None,
        title: "Working...".to_string(),
        started_at: SystemTime::now(),
        arguments: vec![ToolArgument {
            name: "invocation".to_string(),
            value: ArgumentValue::Text(invocation_text),
        }],
        wait_has_target: false,
        wait_has_call_id: false,
        wait_cap_ms: None,
    };
    RunningToolCallCell::new(state)
}

pub(crate) fn new_completed_custom_tool_call(
    tool_name: String,
    args: Option<String>,
    duration: Duration,
    success: bool,
    result: String,
) -> ToolCallCell {
    // Special rendering for browser_* tools
    if tool_name.starts_with("browser_") {
        return new_completed_browser_tool_call(tool_name, args, duration, success, result);
    }
    // Special rendering for agent tools
    if tool_name.starts_with("agent_") || tool_name == "agent" {
        return new_completed_agent_tool_call(tool_name, args, duration, success, result);
    }
    let status = if success {
        HistoryToolStatus::Success
    } else {
        HistoryToolStatus::Failed
    };
    let status_title = if success { "Complete" } else { "Error" };
    let invocation_str = if let Some(args) = args.clone() {
        format!("{}({})", tool_name, args)
    } else {
        format!("{}()", tool_name)
    };

    let mut arguments = vec![ToolArgument {
        name: "invocation".to_string(),
        value: ArgumentValue::Text(invocation_str),
    }];

    if let Some(args_str) = args {
        match serde_json::from_str::<serde_json::Value>(&args_str) {
            Ok(json) => {
                let mut parsed = arguments_from_json(&json);
                arguments.append(&mut parsed);
            }
            Err(_) => {
                if !args_str.is_empty() {
                    arguments.push(ToolArgument {
                        name: "args".to_string(),
                        value: ArgumentValue::Text(args_str),
                    });
                }
            }
        }
    }

    let result_preview = if result.is_empty() {
        None
    } else {
        let preview_lines = build_preview_lines(&result, true);
        let preview_strings = preview_lines
            .iter()
            .map(line_to_plain_text)
            .collect::<Vec<_>>();
        Some(ToolResultPreview {
            lines: preview_strings,
            truncated: false,
        })
    };

    let state = ToolCallState {
        id: HistoryId::ZERO,
        call_id: None,
        status,
        title: status_title.to_string(),
        duration: Some(duration),
        arguments,
        result_preview,
        error_message: None,
    };
    ToolCallCell::new(state)
}

/// Completed web_fetch tool call with markdown rendering of the `markdown` field.
// Web fetch preview sizing: show 10 lines at the start and 5 at the end.
const WEB_FETCH_HEAD_LINES: usize = 10;
const WEB_FETCH_TAIL_LINES: usize = 5;

pub(crate) fn new_completed_web_fetch_tool_call(
    cfg: &Config,
    args: Option<String>,
    duration: Duration,
    success: bool,
    result: String,
) -> WebFetchToolCell {
    let duration = format_duration(duration);
    let status_str = if success { "Complete" } else { "Error" };
    let title_line = if success {
        Line::from(vec![
            Span::styled(status_str, Style::default().fg(crate::colors::success())),
            format!(", duration: {duration}").dim(),
        ])
    } else {
        Line::from(vec![
            Span::styled(status_str, Style::default().fg(crate::colors::error())),
            format!(", duration: {duration}").dim(),
        ])
    };

    let invocation_str = if let Some(args) = args {
        format!("{}({})", "web_fetch", args)
    } else {
        format!("{}()", "web_fetch")
    };

    // Header/preamble (no border)
    let mut pre_lines: Vec<Line<'static>> = Vec::new();
    pre_lines.push(title_line);
    pre_lines.push(Line::styled(
        invocation_str,
        Style::default()
            .fg(crate::colors::text_dim())
            .add_modifier(Modifier::ITALIC),
    ));

    // Try to parse JSON and extract the markdown field
    let mut appended_markdown = false;
    let mut body_lines: Vec<Line<'static>> = Vec::new();
    if !result.is_empty() {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&result) {
            if let Some(md) = value.get("markdown").and_then(|v| v.as_str()) {
                // Build a smarter sectioned preview from the raw markdown.
                let mut sect = build_web_fetch_sectioned_preview(md, cfg);
                dim_webfetch_emphasis_and_links(&mut sect);
                body_lines.extend(sect);
                appended_markdown = true;
            }
        }
    }

    // Fallback: compact preview if JSON parse failed or no markdown present
    if !appended_markdown && !result.is_empty() {
        // Fallback to plain text/JSON preview with ANSI preserved.
        let mut pv =
            select_preview_from_plain_text(&result, WEB_FETCH_HEAD_LINES, WEB_FETCH_TAIL_LINES);
        dim_webfetch_emphasis_and_links(&mut pv);
        body_lines.extend(pv);
    }

    // Spacer below header and below body to match exec styling
    pre_lines.push(Line::from(""));
    if !body_lines.is_empty() {
        body_lines.push(Line::from(""));
    }

    WebFetchToolCell {
        pre_lines,
        body_lines,
        state: if success {
            ToolCellStatus::Success
        } else {
            ToolCellStatus::Failed
        },
    }
}

// ==================== WebFetchToolCell ====================

pub(crate) struct WebFetchToolCell {
    pre_lines: Vec<Line<'static>>,  // header/invocation
    body_lines: Vec<Line<'static>>, // bordered, dim preview
    state: ToolCellStatus,
}

impl HistoryCell for WebFetchToolCell {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn kind(&self) -> HistoryCellType {
        HistoryCellType::Tool { status: self.state }
    }
    fn display_lines(&self) -> Vec<Line<'static>> {
        // Fallback textual representation used only for measurement outside custom render
        let mut v = Vec::new();
        v.extend(self.pre_lines.clone());
        v.extend(self.body_lines.clone());
        v
    }
    fn has_custom_render(&self) -> bool {
        true
    }
    fn desired_height(&self, width: u16) -> u16 {
        let pre_text = Text::from(trim_empty_lines(self.pre_lines.clone()));
        let body_text = Text::from(trim_empty_lines(self.body_lines.clone()));
        let pre_total: u16 = Paragraph::new(pre_text)
            .wrap(Wrap { trim: false })
            .line_count(width)
            .try_into()
            .unwrap_or(0);
        let body_total: u16 = Paragraph::new(body_text)
            .wrap(Wrap { trim: false })
            .line_count(width.saturating_sub(2))
            .try_into()
            .unwrap_or(0);
        pre_total.saturating_add(body_total)
    }
    fn custom_render_with_skip(&self, area: Rect, buf: &mut Buffer, skip_rows: u16) {
        // Measure with the same widths we will render with.
        let pre_text = Text::from(trim_empty_lines(self.pre_lines.clone()));
        let body_text = Text::from(trim_empty_lines(self.body_lines.clone()));
        let pre_wrap_width = area.width;
        let body_wrap_width = area.width.saturating_sub(2);
        let pre_total: u16 = Paragraph::new(pre_text.clone())
            .wrap(Wrap { trim: false })
            .line_count(pre_wrap_width)
            .try_into()
            .unwrap_or(0);
        let body_total: u16 = Paragraph::new(body_text.clone())
            .wrap(Wrap { trim: false })
            .line_count(body_wrap_width)
            .try_into()
            .unwrap_or(0);

        let pre_skip = skip_rows.min(pre_total);
        let body_skip = skip_rows.saturating_sub(pre_total).min(body_total);

        let pre_remaining = pre_total.saturating_sub(pre_skip);
        let pre_height = pre_remaining.min(area.height);
        let body_available = area.height.saturating_sub(pre_height);
        let body_remaining = body_total.saturating_sub(body_skip);
        let body_height = body_available.min(body_remaining);

        // Render preamble
        if pre_height > 0 {
            let pre_area = Rect {
                x: area.x,
                y: area.y,
                width: area.width,
                height: pre_height,
            };
            let bg_style = Style::default()
                .bg(crate::colors::background())
                .fg(crate::colors::text());
            fill_rect(buf, pre_area, Some(' '), bg_style);
            let pre_block =
                Block::default().style(Style::default().bg(crate::colors::background()));
            Paragraph::new(pre_text)
                .block(pre_block)
                .wrap(Wrap { trim: false })
                .scroll((pre_skip, 0))
                .style(Style::default().bg(crate::colors::background()))
                .render(pre_area, buf);
        }

        // Render body with left border + dim text
        if body_height > 0 {
            let body_area = Rect {
                x: area.x,
                y: area.y.saturating_add(pre_height),
                width: area.width,
                height: body_height,
            };
            let bg_style = Style::default()
                .bg(crate::colors::background())
                .fg(crate::colors::text_dim());
            fill_rect(buf, body_area, Some(' '), bg_style);
            let block = Block::default()
                .borders(Borders::LEFT)
                .border_style(
                    Style::default()
                        .fg(crate::colors::border_dim())
                        .bg(crate::colors::background()),
                )
                .style(Style::default().bg(crate::colors::background()))
                .padding(Padding {
                    left: 1,
                    right: 0,
                    top: 0,
                    bottom: 0,
                });
            Paragraph::new(body_text)
                .block(block)
                .wrap(Wrap { trim: false })
                .scroll((body_skip, 0))
                .style(
                    Style::default()
                        .bg(crate::colors::background())
                        .fg(crate::colors::text_dim()),
                )
                .render(body_area, buf);
        }
    }
}

// Build sectioned preview for web_fetch markdown:
// - First 2 non-empty lines
// - Up to 5 sections: a heading line (starts with #) plus the next 4 lines
// - Last 2 non-empty lines
// Ellipses (⋮) are inserted between groups. All content is rendered as markdown.
fn build_web_fetch_sectioned_preview(md: &str, cfg: &Config) -> Vec<Line<'static>> {
    let lines: Vec<&str> = md.lines().collect();

    // Collect first 1 and last 1 non-empty lines (by raw markdown lines)
    let first_non_empty: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| if l.trim().is_empty() { None } else { Some(i) })
        .take(1)
        .collect();
    let last_non_empty_rev: Vec<usize> = lines
        .iter()
        .enumerate()
        .rev()
        .filter_map(|(i, l)| if l.trim().is_empty() { None } else { Some(i) })
        .take(1)
        .collect();
    let mut last_non_empty = last_non_empty_rev.clone();
    last_non_empty.reverse();

    // Find up to 5 heading indices outside code fences
    let mut in_code = false;
    let mut section_heads: Vec<usize> = Vec::new();
    let mut i = 0;
    while i < lines.len() && section_heads.len() < 5 {
        let l = lines[i];
        let trimmed = l.trim_start();
        // Toggle code fence state
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_code = !in_code;
            i += 1;
            continue;
        }
        if !in_code {
            // Heading: 1-6 leading # followed by a space
            let mut level = 0usize;
            for ch in trimmed.chars() {
                if ch == '#' {
                    level += 1;
                } else {
                    break;
                }
            }
            if level >= 1 && level <= 6 {
                if trimmed.chars().nth(level).map_or(false, |c| c == ' ') {
                    section_heads.push(i);
                }
            }
        }
        i += 1;
    }

    // Helper to render a slice of raw markdown lines
    let render_slice = |start: usize, end_excl: usize, out: &mut Vec<Line<'static>>| {
        if start >= end_excl || start >= lines.len() {
            return;
        }
        let end = end_excl.min(lines.len());
        let segment = lines[start..end].join("\n");
        let mut seg_lines: Vec<Line<'static>> = Vec::new();
        crate::markdown::append_markdown(&segment, &mut seg_lines, cfg);
        // Trim leading/trailing empties per segment to keep things tight
        out.extend(trim_empty_lines(seg_lines));
    };

    let mut out: Vec<Line<'static>> = Vec::new();

    // First 2 lines
    if !first_non_empty.is_empty() {
        let start = first_non_empty[0];
        let end = first_non_empty
            .last()
            .copied()
            .unwrap_or(start)
            .saturating_add(1);
        render_slice(start, end, &mut out);
    }

    // Sections
    if !section_heads.is_empty() {
        if !out.is_empty() {
            out.push(Line::from("⋮".dim()));
        }
        for (idx, &h) in section_heads.iter().enumerate() {
            // heading + next 4 lines (total up to 5)
            let end = (h + 5).min(lines.len());
            render_slice(h, end, &mut out);
            if idx + 1 < section_heads.len() {
                out.push(Line::from("⋮".dim()));
            }
        }
    }

    // Last 2 lines
    if !last_non_empty.is_empty() {
        // Avoid duplicating lines if they overlap with earlier content
        let last_start = *last_non_empty.first().unwrap_or(&0);
        if !out.is_empty() {
            out.push(Line::from("⋮".dim()));
        }
        let last_end = last_non_empty
            .last()
            .copied()
            .unwrap_or(last_start)
            .saturating_add(1);
        render_slice(last_start, last_end, &mut out);
    }

    if out.is_empty() {
        // Fallback: if nothing matched, show head/tail preview
        let mut all_md_lines: Vec<Line<'static>> = Vec::new();
        crate::markdown::append_markdown(md, &mut all_md_lines, cfg);
        return select_preview_from_lines(
            &all_md_lines,
            WEB_FETCH_HEAD_LINES,
            WEB_FETCH_TAIL_LINES,
        );
    }

    out
}

// Post-process rendered markdown lines to dim emphasis, lists, and links for web_fetch only.
fn dim_webfetch_emphasis_and_links(lines: &mut Vec<Line<'static>>) {
    let text_dim = crate::colors::text_dim();
    let code_bg = crate::colors::code_block_bg();
    // Recompute the link color logic used by the markdown renderer to detect link spans
    let link_fg = crate::colors::mix_toward(crate::colors::text(), crate::colors::primary(), 0.35);
    for line in lines.iter_mut() {
        // Heuristic list detection on the plain text form
        let s: String = line.spans.iter().map(|sp| sp.content.as_ref()).collect();
        let t = s.trim_start();
        let is_list = t.starts_with('-')
            || t.starts_with('*')
            || t.starts_with('+')
            || t.starts_with('•')
            || t.starts_with('·')
            || t.starts_with('⋅')
            || t.chars().take_while(|c| c.is_ascii_digit()).count() > 0
                && (t.chars().skip_while(|c| c.is_ascii_digit()).next() == Some('.')
                    || t.chars().skip_while(|c| c.is_ascii_digit()).next() == Some(')'));

        for sp in line.spans.iter_mut() {
            // Skip code block spans (have a solid code background)
            if sp.style.bg == Some(code_bg) {
                continue;
            }
            let style = &mut sp.style;
            let is_bold = style.add_modifier.contains(Modifier::BOLD);
            let is_under = style.add_modifier.contains(Modifier::UNDERLINED);
            let is_link_colored = style.fg == Some(link_fg);
            if is_list || is_bold || is_under || is_link_colored {
                style.fg = Some(text_dim);
            }
        }
    }
}

// Map `browser_*` tool names to friendly titles
fn browser_tool_title(tool_name: &str) -> &'static str {
    match tool_name {
        "browser_click" => "Browser Click",
        "browser_type" => "Browser Type",
        "browser_key" => "Browser Key",
        "browser_javascript" => "Browser JavaScript",
        "browser_scroll" => "Browser Scroll",
        "browser_open" => "Browser Open",
        "browser_close" => "Browser Close",
        "browser_fetch" => "Browser Fetch",
        "browser_status" => "Browser Status",
        "browser_history" => "Browser History",
        "browser_inspect" => "Browser Inspect",
        "browser_console" => "Browser Console",
        "browser_cdp" => "Browser CDP",
        "browser_move" => "Browser Move",
        _ => "Browser Tool",
    }
}

// Attempt a compact, humanized one-line summary for browser tools.
// Returns Some(lines) when a concise form is available for the given tool, else None.
fn format_browser_args_humanized(
    tool_name: &str,
    args: &serde_json::Value,
) -> Option<Vec<Line<'static>>> {
    use serde_json::Value;
    let text = |s: String| Span::styled(s, Style::default().fg(crate::colors::text()));

    // Helper: format coordinate pair as integers (pixels)
    let fmt_xy = |x: f64, y: f64| -> String {
        let xi = x.round() as i64;
        let yi = y.round() as i64;
        format!("({xi}, {yi})")
    };

    match (tool_name, args) {
        ("browser_click", Value::Object(map)) => {
            // Expect optional `type`, and x/y for absolute. Only compact when both x and y provided.
            let ty = map
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("click")
                .to_lowercase();
            let (x, y) = match (
                map.get("x").and_then(|v| v.as_f64()),
                map.get("y").and_then(|v| v.as_f64()),
            ) {
                (Some(x), Some(y)) => (x, y),
                _ => return None,
            };
            let msg = format!("└ {ty} at {}", fmt_xy(x, y));
            Some(vec![Line::from(text(msg))])
        }
        ("browser_fetch", Value::Object(map)) => {
            if let Some(url) = map.get("url").and_then(|v| v.as_str()) {
                let msg = format!("└ fetch {}", url);
                Some(vec![Line::from(text(msg))])
            } else {
                None
            }
        }
        ("browser_move", Value::Object(map)) => {
            // Prefer absolute x/y → "to (x, y)"; otherwise relative dx/dy → "by (dx, dy)".
            if let (Some(x), Some(y)) = (
                map.get("x").and_then(|v| v.as_f64()),
                map.get("y").and_then(|v| v.as_f64()),
            ) {
                let msg = format!("└ to {}", fmt_xy(x, y));
                return Some(vec![Line::from(text(msg))]);
            }
            if let (Some(dx), Some(dy)) = (
                map.get("dx").and_then(|v| v.as_f64()),
                map.get("dy").and_then(|v| v.as_f64()),
            ) {
                let msg = format!("└ by {}", fmt_xy(dx, dy));
                return Some(vec![Line::from(text(msg))]);
            }
            None
        }
        _ => None,
    }
}

fn new_completed_browser_tool_call(
    tool_name: String,
    args: Option<String>,
    duration: Duration,
    success: bool,
    result: String,
) -> ToolCallCell {
    let status = if success {
        HistoryToolStatus::Success
    } else {
        HistoryToolStatus::Failed
    };
    let mut arguments: Vec<ToolArgument> = Vec::new();
    if let Some(args_str) = args {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&args_str) {
            if let Some(lines) = format_browser_args_humanized(&tool_name, &json) {
                let summary = lines_to_plain_text(&lines);
                if !summary.is_empty() {
                    arguments.push(ToolArgument {
                        name: "summary".to_string(),
                        value: ArgumentValue::Text(summary),
                    });
                }
            }
            let mut kv = arguments_from_json(&json);
            arguments.append(&mut kv);
        } else if !args_str.is_empty() {
            arguments.push(ToolArgument {
                name: "args".to_string(),
                value: ArgumentValue::Text(args_str),
            });
        }
    }

    let result_preview = if result.is_empty() {
        None
    } else {
        let preview_lines = build_preview_lines(&result, true);
        let preview_strings = preview_lines
            .iter()
            .map(line_to_plain_text)
            .collect::<Vec<_>>();
        Some(ToolResultPreview {
            lines: preview_strings,
            truncated: false,
        })
    };

    let state = ToolCallState {
        id: HistoryId::ZERO,
        call_id: None,
        status,
        title: browser_tool_title(&tool_name).to_string(),
        duration: Some(duration),
        arguments,
        result_preview,
        error_message: None,
    };
    ToolCallCell::new(state)
}

// Map `agent_*` tool names to friendly titles
fn agent_tool_title(tool_name: &str, action: Option<&str>) -> String {
    let key = action.unwrap_or_else(|| match tool_name {
        "agent_run" => "create",
        "agent_wait" => "wait",
        "agent_result" => "result",
        "agent_cancel" => "cancel",
        "agent_check" => "status",
        "agent_list" => "list",
        "agent" => "create",
        other => other,
    });

    match key {
        "create" | "agent" => "Agent Run".to_string(),
        "wait" | "agent_wait" => "Agent Wait".to_string(),
        "result" | "agent_result" => "Agent Result".to_string(),
        "cancel" | "agent_cancel" => "Agent Cancel".to_string(),
        "status" | "agent_check" | "agent_status" => "Agent Status".to_string(),
        "list" | "agent_list" => "Agent List".to_string(),
        other => {
            if let Some(rest) = other.strip_prefix("agent_") {
                let title = rest
                    .split('_')
                    .filter(|s| !s.is_empty())
                    .map(|s| {
                        let mut chars = s.chars();
                        match chars.next() {
                            Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                            None => String::new(),
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                format!("Agent {}", title)
            } else {
                "Agent Tool".to_string()
            }
        }
    }
}

fn new_completed_agent_tool_call(
    tool_name: String,
    args: Option<String>,
    duration: Duration,
    success: bool,
    result: String,
) -> ToolCallCell {
    let status = if success {
        HistoryToolStatus::Success
    } else {
        HistoryToolStatus::Failed
    };
    let mut arguments: Vec<ToolArgument> = Vec::new();
    let mut action: Option<String> = None;
    if let Some(args_str) = args {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&args_str) {
            if let Some(act) = json.get("action").and_then(|v| v.as_str()) {
                action = Some(act.to_string());
            }
            let mut kv = arguments_from_json(&json);
            arguments.append(&mut kv);
        } else if !args_str.is_empty() {
            arguments.push(ToolArgument {
                name: "args".to_string(),
                value: ArgumentValue::Text(args_str),
            });
        }
    }

    let result_preview = if result.is_empty() {
        None
    } else {
        let preview_lines = build_preview_lines(&result, true);
        let preview_strings = preview_lines
            .iter()
            .map(line_to_plain_text)
            .collect::<Vec<_>>();
        Some(ToolResultPreview {
            lines: preview_strings,
            truncated: false,
        })
    };

    let state = ToolCallState {
        id: HistoryId::ZERO,
        call_id: None,
        status,
        title: agent_tool_title(&tool_name, action.as_deref()),
        duration: Some(duration),
        arguments,
        result_preview,
        error_message: None,
    };
    ToolCallCell::new(state)
}

// Try to create an image cell if the MCP result contains an image
fn try_new_completed_mcp_tool_call_with_image_output(
    result: &Result<mcp_types::CallToolResult, String>,
) -> Option<ImageOutputCell> {
    match result {
        Ok(mcp_types::CallToolResult { content, .. }) => {
            if let Some(mcp_types::ContentBlock::ImageContent(image_block)) = content.first() {
                let raw_data = match base64::engine::general_purpose::STANDARD
                    .decode(&image_block.data)
                {
                    Ok(data) => data,
                    Err(e) => {
                        error!("Failed to decode image data: {e}");
                        return None;
                    }
                };
                let reader = match ImageReader::new(Cursor::new(&raw_data)).with_guessed_format() {
                    Ok(reader) => reader,
                    Err(e) => {
                        error!("Failed to guess image format: {e}");
                        return None;
                    }
                };

                let decoded = match reader.decode() {
                    Ok(image) => image,
                    Err(e) => {
                        error!("Image decoding failed: {e}");
                        return None;
                    }
                };

                let width = decoded.width().min(u16::MAX as u32) as u16;
                let height = decoded.height().min(u16::MAX as u32) as u16;
                let sha_hex = format!("{:x}", Sha256::digest(&raw_data));
                let byte_len = raw_data.len().min(u32::MAX as usize) as u32;

                let record = ImageRecord {
                    id: HistoryId::ZERO,
                    source_path: None,
                    alt_text: None,
                    width,
                    height,
                    sha256: Some(sha_hex),
                    mime_type: Some(image_block.mime_type.clone()),
                    byte_len: Some(byte_len),
                };

                Some(ImageOutputCell::from_record(record))
            } else {
                None
            }
        }
        _ => None,
    }
}

pub(crate) fn new_completed_mcp_tool_call(
    _num_cols: usize,
    invocation: McpInvocation,
    duration: Duration,
    success: bool,
    result: Result<mcp_types::CallToolResult, String>,
) -> Box<dyn HistoryCell> {
    if let Some(cell) = try_new_completed_mcp_tool_call_with_image_output(&result) {
        return Box::new(cell);
    }

    let status = if success {
        HistoryToolStatus::Success
    } else {
        HistoryToolStatus::Failed
    };

    let invocation_line = format_mcp_invocation(invocation);
    let invocation_text = line_to_plain_text(&invocation_line);
    let arguments = vec![ToolArgument {
        name: "invocation".to_string(),
        value: ArgumentValue::Text(invocation_text),
    }];

    let mut preview_lines: Vec<String> = Vec::new();
    let mut error_message: Option<String> = None;

    match result {
        Ok(mcp_types::CallToolResult { content, .. }) => {
            for tool_call_result in content {
                match tool_call_result {
                    mcp_types::ContentBlock::TextContent(text) => {
                        let preview = build_preview_lines(&text.text, true);
                        for line in preview {
                            preview_lines.push(line_to_plain_text(&line));
                        }
                        preview_lines.push(String::new());
                    }
                    mcp_types::ContentBlock::ImageContent(_) => {
                        preview_lines.push("<image content>".to_string());
                    }
                    mcp_types::ContentBlock::AudioContent(_) => {
                        preview_lines.push("<audio content>".to_string());
                    }
                    mcp_types::ContentBlock::EmbeddedResource(resource) => {
                        let uri = match resource.resource {
                            EmbeddedResourceResource::TextResourceContents(text) => text.uri,
                            EmbeddedResourceResource::BlobResourceContents(blob) => blob.uri,
                        };
                        preview_lines.push(format!("embedded resource: {uri}"));
                    }
                    mcp_types::ContentBlock::ResourceLink(ResourceLink { uri, .. }) => {
                        preview_lines.push(format!("link: {uri}"));
                    }
                }
            }
            if preview_lines.last().map(|s| !s.is_empty()).unwrap_or(false) {
                preview_lines.push(String::new());
            }
        }
        Err(e) => {
            error_message = Some(format!("Error: {e}"));
        }
    }

    let result_preview = if preview_lines.is_empty() {
        None
    } else {
        Some(ToolResultPreview {
            lines: preview_lines,
            truncated: false,
        })
    };

    let state = ToolCallState {
        id: HistoryId::ZERO,
        call_id: None,
        status,
        title: if success { "Complete" } else { "Error" }.to_string(),
        duration: Some(duration),
        arguments,
        result_preview,
        error_message,
    };

    Box::new(ToolCallCell::new(state))
}

fn format_mcp_invocation(invocation: McpInvocation) -> Line<'static> {
    let provider_name = pretty_provider_name(&invocation.server);
    let invocation_str = if let Some(args) = invocation.arguments {
        format!("{}.{}({})", provider_name, invocation.tool, args)
    } else {
        format!("{}.{}()", provider_name, invocation.tool)
    };

    Line::styled(
        invocation_str,
        Style::default()
            .fg(crate::colors::text_dim())
            .add_modifier(Modifier::ITALIC),
    )
}
