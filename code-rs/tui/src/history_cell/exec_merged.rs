use ratatui::prelude::{Buffer, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Padding, Paragraph, Widget, Wrap};

use crate::history::compat::{
    ExecAction,
    ExecRecord,
    ExecStatus,
    HistoryId,
    MergedExecRecord,
};
use crate::util::buffer::fill_rect;

use super::core::{ExecKind, HistoryCell, HistoryCellType};
use super::exec::ExecCell;
use super::exec_helpers::coalesce_read_ranges_in_lines_local;
use super::formatting::trim_empty_lines;

// ==================== MergedExecCell ====================
// Represents multiple completed exec results merged into one cell while preserving
// the bordered, dimmed output styling for each command's stdout/stderr preview.

struct MergedExecSegment {
    record: ExecRecord,
}

impl MergedExecSegment {
    fn new(record: ExecRecord) -> Self {
        Self { record }
    }

    fn exec_parts(&self) -> (Vec<Line<'static>>, Vec<Line<'static>>, Option<Line<'static>>) {
        let exec_cell = ExecCell::from_record(self.record.clone());
        exec_cell.exec_render_parts()
    }

    fn lines(&self) -> (Vec<Line<'static>>, Vec<Line<'static>>) {
        let (pre, mut out, status_line) = self.exec_parts();
        if let Some(status) = status_line {
            out.push(status);
        }
        (pre, out)
    }
}

pub(crate) struct MergedExecCell {
    segments: Vec<MergedExecSegment>,
    kind: ExecKind,
    history_id: HistoryId,
}

impl MergedExecCell {
    pub(crate) fn rebuild_with_theme(&self) {}

    pub(crate) fn set_history_id(&mut self, id: HistoryId) {
        self.history_id = id;
    }

    pub(crate) fn to_record(&self) -> MergedExecRecord {
        MergedExecRecord {
            id: self.history_id,
            action: self.kind.into(),
            segments: self
                .segments
                .iter()
                .map(|segment| segment.record.clone())
                .collect(),
        }
    }

    pub(crate) fn from_records(
        history_id: HistoryId,
        action: ExecAction,
        segments: Vec<ExecRecord>,
    ) -> Self {
        Self {
            segments: segments.into_iter().map(MergedExecSegment::new).collect(),
            kind: action.into(),
            history_id,
        }
    }

    pub(crate) fn from_state(record: MergedExecRecord) -> Self {
        let history_id = record.id;
        let kind: ExecKind = record.action.into();
        let segments = record
            .segments
            .into_iter()
            .map(MergedExecSegment::new)
            .collect();
        Self {
            segments,
            kind,
            history_id,
        }
    }

    fn aggregated_read_preamble_lines(&self) -> Option<Vec<Line<'static>>> {
        if self.kind != ExecKind::Read {
            return None;
        }
        use ratatui::text::Span;

        fn parse_read_line(line: &Line<'_>) -> Option<(String, u32, u32)> {
            if line.spans.is_empty() {
                return None;
            }
            let first = line.spans[0].content.as_ref();
            if !(first == "└ " || first == "  ") {
                return None;
            }
            let rest: String = line
                .spans
                .iter()
                .skip(1)
                .map(|s| s.content.as_ref())
                .collect();
            if let Some(idx) = rest.rfind(" (lines ") {
                let fname = rest[..idx].to_string();
                let tail = &rest[idx + 1..];
                if tail.starts_with("(lines ") && tail.ends_with(")") {
                    let inner = &tail[7..tail.len().saturating_sub(1)];
                    if let Some((a, b)) = inner.split_once(" to ") {
                        if let (Ok(s), Ok(e)) = (a.trim().parse::<u32>(), b.trim().parse::<u32>()) {
                            return Some((fname, s, e));
                        }
                    }
                }
            }
            None
        }

        fn is_search_like(line: &Line<'_>) -> bool {
            let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            let t = text.trim();
            t.contains(" (in ")
                || t.rsplit_once(" in ")
                    .map(|(_, rhs)| rhs.trim_end().ends_with('/'))
                    .unwrap_or(false)
        }

        let mut kept: Vec<Line<'static>> = Vec::new();
        for (seg_idx, segment) in self.segments.iter().enumerate() {
            let (pre_raw, _, _) = segment.exec_parts();
            let mut pre = trim_empty_lines(pre_raw);
            if !pre.is_empty() {
                pre.remove(0);
            }
            for line in pre.into_iter() {
                if is_search_like(&line) {
                    continue;
                }
                let keep = parse_read_line(&line).is_some() || seg_idx == 0;
                if keep {
                    kept.push(line);
                }
            }
        }

        if kept.is_empty() {
            return Some(kept);
        }

        if let Some(first) = kept.first_mut() {
            let flat: String = first.spans.iter().map(|s| s.content.as_ref()).collect();
            let has_connector = flat.trim_start().starts_with("└ ");
            if !has_connector {
                first.spans.insert(
                    0,
                    Span::styled("└ ", Style::default().fg(crate::colors::text_dim())),
                );
            }
        }
        for line in kept.iter_mut().skip(1) {
            if let Some(span0) = line.spans.get_mut(0) {
                if span0.content.as_ref() == "└ " {
                    span0.content = "  ".into();
                    span0.style = span0.style.add_modifier(Modifier::DIM);
                }
            }
        }

        coalesce_read_ranges_in_lines_local(&mut kept);
        Some(kept)
    }
}

impl HistoryCell for MergedExecCell {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn kind(&self) -> HistoryCellType {
        HistoryCellType::Exec {
            kind: self.kind,
            status: ExecStatus::Success,
        }
    }
    fn desired_height(&self, width: u16) -> u16 {
        let header_rows = if self.kind == ExecKind::Run { 0 } else { 1 };
        let pre_wrap_width = width;
        let out_wrap_width = width.saturating_sub(2);
        let mut total: u16 = header_rows;

        if let Some(agg_pre) = self.aggregated_read_preamble_lines() {
            let pre_rows: u16 = Paragraph::new(Text::from(agg_pre))
                .wrap(Wrap { trim: false })
                .line_count(pre_wrap_width)
                .try_into()
                .unwrap_or(0);
            total = total.saturating_add(pre_rows);
            for segment in &self.segments {
                let (_, out_raw) = segment.lines();
                let out = trim_empty_lines(out_raw);
                let out_rows: u16 = Paragraph::new(Text::from(out))
                    .wrap(Wrap { trim: false })
                    .line_count(out_wrap_width)
                    .try_into()
                    .unwrap_or(0);
                total = total.saturating_add(out_rows);
            }
            return total;
        }

        let mut added_corner = false;
        for segment in &self.segments {
            let (pre_raw, out_raw) = segment.lines();
            let mut pre = trim_empty_lines(pre_raw);
            if self.kind != ExecKind::Run && !pre.is_empty() {
                pre.remove(0);
            }
            if self.kind != ExecKind::Run {
                if let Some(first) = pre.first_mut() {
                    let flat: String = first.spans.iter().map(|s| s.content.as_ref()).collect();
                    let has_corner = flat.trim_start().starts_with("└ ");
                    let has_spaced_corner = flat.trim_start().starts_with("  └ ");
                    if !added_corner {
                        if !(has_corner || has_spaced_corner) {
                            first.spans.insert(
                                0,
                                Span::styled("└ ", Style::default().fg(crate::colors::text_dim())),
                            );
                        }
                        added_corner = true;
                    } else if let Some(sp0) = first.spans.get_mut(0) {
                        if sp0.content.as_ref() == "└ " {
                            sp0.content = "  ".into();
                            sp0.style = sp0.style.add_modifier(Modifier::DIM);
                        }
                    }
                }
            }
            let out = trim_empty_lines(out_raw);
            let pre_rows: u16 = Paragraph::new(Text::from(pre))
                .wrap(Wrap { trim: false })
                .line_count(pre_wrap_width)
                .try_into()
                .unwrap_or(0);
            let out_rows: u16 = Paragraph::new(Text::from(out))
                .wrap(Wrap { trim: false })
                .line_count(out_wrap_width)
                .try_into()
                .unwrap_or(0);
            total = total.saturating_add(pre_rows).saturating_add(out_rows);
        }

        total
    }
    fn display_lines(&self) -> Vec<Line<'static>> {
        let mut out: Vec<Line<'static>> = Vec::new();
        for (i, segment) in self.segments.iter().enumerate() {
            let (pre_raw, out_raw) = segment.lines();
            if i > 0 {
                out.push(Line::from(""));
            }
            out.extend(trim_empty_lines(pre_raw));
            out.extend(trim_empty_lines(out_raw));
        }
        out
    }
    fn has_custom_render(&self) -> bool {
        true
    }
    fn custom_render_with_skip(&self, area: Rect, buf: &mut Buffer, mut skip_rows: u16) {
        let bg = Style::default()
            .bg(crate::colors::background())
            .fg(crate::colors::text());
        fill_rect(buf, area, Some(' '), bg);

        // Build one header line based on exec kind
        let header_line = match self.kind {
            ExecKind::Read => Some(Line::styled(
                "Read",
                Style::default().fg(crate::colors::text()),
            )),
            ExecKind::Search => Some(Line::styled(
                "Search",
                Style::default().fg(crate::colors::text_dim()),
            )),
            ExecKind::List => Some(Line::styled(
                "List",
                Style::default().fg(crate::colors::text()),
            )),
            ExecKind::Run => None,
        };

        let mut cur_y = area.y;
        let end_y = area.y.saturating_add(area.height);

        // Render or skip header line
        if let Some(header_line) = header_line {
            if skip_rows == 0 {
                if cur_y < end_y {
                    let txt = Text::from(vec![header_line]);
                    Paragraph::new(txt)
                        .block(Block::default().style(bg))
                        .wrap(Wrap { trim: false })
                        .render(
                            Rect {
                                x: area.x,
                                y: cur_y,
                                width: area.width,
                                height: 1,
                            },
                            buf,
                        );
                    cur_y = cur_y.saturating_add(1);
                }
            } else {
                skip_rows = skip_rows.saturating_sub(1);
            }
        }

        let mut added_corner: bool = false;
        let mut ensure_prefix = |lines: &mut Vec<Line<'static>>| {
            if self.kind == ExecKind::Run {
                return;
            }
            if let Some(first) = lines.first_mut() {
                let flat: String = first.spans.iter().map(|s| s.content.as_ref()).collect();
                let has_corner = flat.trim_start().starts_with("└ ");
                let has_spaced_corner = flat.trim_start().starts_with("  └ ");
                if !added_corner {
                    if !(has_corner || has_spaced_corner) {
                        first.spans.insert(
                            0,
                            Span::styled("└ ", Style::default().fg(crate::colors::text_dim())),
                        );
                    }
                    added_corner = true;
                } else {
                    // For subsequent segments, replace any leading corner with two spaces
                    if let Some(sp0) = first.spans.get_mut(0) {
                        if sp0.content.as_ref() == "└ " {
                            sp0.content = "  ".into();
                            sp0.style = sp0.style.add_modifier(Modifier::DIM);
                        }
                    }
                }
            }
        };

        // Special aggregated rendering for Read: collapse file ranges
        if self.kind == ExecKind::Read {
            if let Some(agg_pre) = self.aggregated_read_preamble_lines() {
                let pre_text = Text::from(agg_pre);
                let pre_wrap_width = area.width;
                let pre_total: u16 = Paragraph::new(pre_text.clone())
                    .wrap(Wrap { trim: false })
                    .line_count(pre_wrap_width)
                    .try_into()
                    .unwrap_or(0);
                if cur_y < end_y {
                    let pre_skip = skip_rows.min(pre_total);
                    let pre_remaining = pre_total.saturating_sub(pre_skip);
                    let pre_height = pre_remaining.min(end_y.saturating_sub(cur_y));
                    if pre_height > 0 {
                        Paragraph::new(pre_text)
                            .block(Block::default().style(bg))
                            .wrap(Wrap { trim: false })
                            .scroll((pre_skip, 0))
                            .style(bg)
                            .render(
                                Rect {
                                    x: area.x,
                                    y: cur_y,
                                    width: area.width,
                                    height: pre_height,
                                },
                                buf,
                            );
                        cur_y = cur_y.saturating_add(pre_height);
                    }
                    skip_rows = skip_rows.saturating_sub(pre_skip);
                }

                let out_wrap_width = area.width.saturating_sub(2);
                for segment in &self.segments {
                    if cur_y >= end_y {
                        break;
                    }
                    let (_, out_raw) = segment.lines();
                    let out = trim_empty_lines(out_raw);
                    let out_text = Text::from(out.clone());
                    let out_total: u16 = Paragraph::new(out_text.clone())
                        .wrap(Wrap { trim: false })
                        .line_count(out_wrap_width)
                        .try_into()
                        .unwrap_or(0);
                    let out_skip = skip_rows.min(out_total);
                    let out_remaining = out_total.saturating_sub(out_skip);
                    let out_height = out_remaining.min(end_y.saturating_sub(cur_y));
                    if out_height > 0 {
                        let out_area = Rect {
                            x: area.x,
                            y: cur_y,
                            width: area.width,
                            height: out_height,
                        };
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
                        Paragraph::new(out_text)
                            .block(block)
                            .wrap(Wrap { trim: false })
                            .scroll((out_skip, 0))
                            .style(
                                Style::default()
                                    .bg(crate::colors::background())
                                    .fg(crate::colors::text_dim()),
                            )
                            .render(out_area, buf);
                        cur_y = cur_y.saturating_add(out_height);
                    }
                    skip_rows = skip_rows.saturating_sub(out_skip);
                }
                return;
            }

            // Fallback: each segment retains its own preamble and output
        }

        for segment in &self.segments {
            if cur_y >= end_y {
                break;
            }
            let (pre_raw, out_raw) = segment.lines();
            let mut pre = trim_empty_lines(pre_raw);
            if self.kind != ExecKind::Run && !pre.is_empty() {
                pre.remove(0);
            }
            ensure_prefix(&mut pre);

            let out = trim_empty_lines(out_raw);
            let out_text = Text::from(out.clone());

            let pre_text = Text::from(pre.clone());
            let pre_wrap_width = area.width;
            let out_wrap_width = area.width.saturating_sub(2);
            let pre_total: u16 = Paragraph::new(pre_text.clone())
                .wrap(Wrap { trim: false })
                .line_count(pre_wrap_width)
                .try_into()
                .unwrap_or(0);
            let out_total: u16 = Paragraph::new(out_text.clone())
                .wrap(Wrap { trim: false })
                .line_count(out_wrap_width)
                .try_into()
                .unwrap_or(0);

            // Apply skip to pre, then out
            let pre_skip = skip_rows.min(pre_total);
            let out_skip = skip_rows.saturating_sub(pre_total).min(out_total);

            // Render pre
            let pre_remaining = pre_total.saturating_sub(pre_skip);
            let pre_height = pre_remaining.min(end_y.saturating_sub(cur_y));
            if pre_height > 0 {
                Paragraph::new(pre_text)
                    .block(Block::default().style(bg))
                    .wrap(Wrap { trim: false })
                    .scroll((pre_skip, 0))
                    .style(bg)
                    .render(
                        Rect {
                            x: area.x,
                            y: cur_y,
                            width: area.width,
                            height: pre_height,
                        },
                        buf,
                    );
                cur_y = cur_y.saturating_add(pre_height);
            }

            if cur_y >= end_y {
                break;
            }
            // Render out as bordered, dim block
            let out_remaining = out_total.saturating_sub(out_skip);
            let out_height = out_remaining.min(end_y.saturating_sub(cur_y));
            if out_height > 0 {
                let out_area = Rect {
                    x: area.x,
                    y: cur_y,
                    width: area.width,
                    height: out_height,
                };
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
                Paragraph::new(out_text)
                    .block(block)
                    .wrap(Wrap { trim: false })
                    .scroll((out_skip, 0))
                    .style(
                        Style::default()
                            .bg(crate::colors::background())
                            .fg(crate::colors::text_dim()),
                    )
                    .render(out_area, buf);
                cur_y = cur_y.saturating_add(out_height);
            }

            // Consume skip rows used in this segment
            let consumed = pre_total + out_total;
            skip_rows = skip_rows.saturating_sub(consumed);
        }
    }
}

pub(crate) fn merged_exec_lines_from_record(record: &MergedExecRecord) -> Vec<Line<'static>> {
    MergedExecCell::from_state(record.clone()).display_lines()
}
