use super::*;
use crate::history::state::{RateLimitLegendEntry, RateLimitsRecord, TextTone};
use code_common::elapsed::format_duration;
use code_core::protocol::RateLimitSnapshotEvent;
use ratatui::style::Color;
use time::{
    format_description::FormatItem,
    macros::format_description,
    Duration as TimeDuration,
    OffsetDateTime,
};

pub(crate) struct RateLimitsCell {
    record: RateLimitsRecord,
}

impl RateLimitsCell {
    pub(crate) fn from_record(record: RateLimitsRecord) -> Self {
        Self { record }
    }

    pub(crate) fn record(&self) -> &RateLimitsRecord {
        &self.record
    }

    pub(crate) fn record_mut(&mut self) -> &mut RateLimitsRecord {
        &mut self.record
    }
}

impl HistoryCell for RateLimitsCell {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn kind(&self) -> HistoryCellType {
        HistoryCellType::Notice
    }

    fn display_lines(&self) -> Vec<Line<'static>> {
        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.push(
            Line::styled(
                "Rate limits update",
                Style::default()
                    .fg(crate::colors::warning())
                    .add_modifier(Modifier::BOLD),
            ),
        );

        let snapshot = &self.record.snapshot;
        lines.extend(snapshot_summary_lines(snapshot));

        if !self.record.legend.is_empty() {
            lines.push(Line::default());
            lines.push(Line::styled(
                "Warnings",
                Style::default()
                    .fg(crate::colors::warning())
                    .add_modifier(Modifier::BOLD),
            ));
            for entry in &self.record.legend {
                lines.extend(legend_lines(entry));
            }
        }

        lines
    }

    fn desired_height(&self, width: u16) -> u16 {
        Paragraph::new(Text::from(self.display_lines_trimmed()))
            .wrap(Wrap { trim: false })
            .line_count(width)
            .try_into()
            .unwrap_or(0)
    }
}

fn snapshot_summary_lines(snapshot: &RateLimitSnapshotEvent) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    let hourly_line = format!(
        "└ Hourly usage: {percent:.0}% of {window}",
        percent = snapshot.primary_used_percent,
        window = format_window_minutes(snapshot.primary_window_minutes),
    );
    lines.push(Line::from(vec![Span::styled(
        hourly_line,
        Style::default().fg(crate::colors::text()),
    )]));

    if let Some(seconds) = snapshot.primary_reset_after_seconds {
        lines.push(Line::from(vec![Span::styled(
            format_reset_line(seconds),
            Style::default().fg(crate::colors::text_dim()),
        )]));
    }

    let weekly_line = format!(
        "└ Weekly usage: {percent:.0}% of {window}",
        percent = snapshot.secondary_used_percent,
        window = format_window_minutes(snapshot.secondary_window_minutes),
    );
    lines.push(Line::from(vec![Span::styled(
        weekly_line,
        Style::default().fg(crate::colors::text()),
    )]));

    if let Some(seconds) = snapshot.secondary_reset_after_seconds {
        lines.push(Line::from(vec![Span::styled(
            format_reset_line(seconds),
            Style::default().fg(crate::colors::text_dim()),
        )]));
    }

    lines
}

const TIME_OF_DAY_FORMAT: &[FormatItem<'static>] =
    format_description!("[hour repr:12 padding:none]:[minute][period case:lower]");
const DAY_TIME_FORMAT: &[FormatItem<'static>] = format_description!(
    "[weekday repr:long] [hour repr:12 padding:none]:[minute][period case:lower]"
);

fn format_window_minutes(minutes: u64) -> String {
    if minutes < 60 {
        return format!("{minutes} min window");
    }

    let hours = (minutes as f64 / 60.0).round().max(1.0);
    if hours < 24.0 {
        let hours = hours as u64;
        let unit = if hours == 1 { "hour" } else { "hours" };
        return format!("{hours} {unit} window");
    }

    let days = (minutes as f64 / 1_440.0).round().max(1.0) as u64;
    if days % 7 == 0 {
        let weeks = days / 7;
        let unit = if weeks == 1 { "week" } else { "weeks" };
        return format!("{weeks} {unit} window");
    }

    let unit = if days == 1 { "day" } else { "days" };
    format!("{days} {unit} window")
}

fn format_reset_line(seconds: u64) -> String {
    let reset_duration = std::time::Duration::from_secs(seconds);
    let reset = format_duration(reset_duration);
    let timestamp = format_reset_timestamp(seconds)
        .map(|formatted| format!(" @ {formatted}"))
        .unwrap_or_default();
    format!("   • resets in {reset}{timestamp}")
}

fn format_reset_timestamp(seconds: u64) -> Option<String> {
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    let seconds_i64 = i64::try_from(seconds).ok()?;
    let reset_at = now + TimeDuration::seconds(seconds_i64);
    let same_day = now.date() == reset_at.date();
    let format = if same_day {
        TIME_OF_DAY_FORMAT
    } else {
        DAY_TIME_FORMAT
    };
    reset_at.format(format).ok()
}

fn legend_lines(entry: &RateLimitLegendEntry) -> Vec<Line<'static>> {
    let tone_style = Style::default().fg(color_for_tone(entry.tone));
    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        Span::styled("  • ", tone_style),
        Span::styled(entry.label.clone(), tone_style.add_modifier(Modifier::BOLD)),
    ]));

    if !entry.description.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            format!("    {}", entry.description),
            Style::default().fg(crate::colors::text()),
        )]));
    }

    lines
}

fn color_for_tone(tone: TextTone) -> Color {
    match tone {
        TextTone::Default => crate::colors::text(),
        TextTone::Dim => crate::colors::text_dim(),
        TextTone::Primary => crate::colors::primary(),
        TextTone::Success => crate::colors::success(),
        TextTone::Warning => crate::colors::warning(),
        TextTone::Error => crate::colors::error(),
        TextTone::Info => crate::colors::info(),
        TextTone::Accent(index) => super::text::color_for_tone(TextTone::Accent(index), &crate::theme::current_theme()),
    }
}
