use super::card_style::{
    ansi16_inverse_color,
    browser_card_style,
    fill_card_background,
    hint_text_style,
    primary_text_style,
    rows_to_lines,
    secondary_text_style,
    title_text_style,
    truncate_with_ellipsis,
    CardRow,
    CardSegment,
    CardStyle,
    CARD_ACCENT_WIDTH,
};
use super::*;
use crate::colors;
use crate::history::state::ImageRecord;
use crate::theme::{palette_mode, PaletteMode};
use code_protocol::num_format::format_with_separators;
use ::image::ImageReader;
use ::image::image_dimensions;
use ratatui::widgets::{Paragraph, Wrap};
use ratatui_image::{Image, Resize};
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::FilterType;
use std::cell::RefCell;
use std::path::Path;
use std::path::PathBuf;
use std::rc::Rc;
use unicode_width::UnicodeWidthChar;

const BORDER_TOP: &str = "╭─";
const BORDER_BODY: &str = "│";
const BORDER_BOTTOM: &str = "╰─";

const DEFAULT_TEXT_INDENT: usize = 2;
const TEXT_RIGHT_PADDING: usize = 2;
const IMAGE_GAP: usize = 2;
const IMAGE_LEFT_PAD: usize = 1;
const IMAGE_MIN_WIDTH: usize = 18;
const IMAGE_MAX_WIDTH: usize = 64;
const MIN_TEXT_WIDTH: usize = 28;
const MIN_IMAGE_ROWS: usize = 4;
const MAX_IMAGE_ROWS: usize = 60;
const HINT_TEXT: &str = "Image output";

struct ImagePreviewLayout {
    start_row: usize,
    height_rows: usize,
    width_cols: usize,
    indent_cols: usize,
}

pub(crate) struct ImageOutputCell {
    record: ImageRecord,
    cached_picker: Rc<RefCell<Option<ratatui_image::picker::Picker>>>,
    cached_image_protocol:
        Rc<RefCell<Option<(PathBuf, ratatui::layout::Rect, ratatui_image::protocol::Protocol)>>>,
}

impl ImageOutputCell {
    pub(crate) fn new(record: ImageRecord) -> Self {
        Self {
            record,
            cached_picker: Rc::new(RefCell::new(None)),
            cached_image_protocol: Rc::new(RefCell::new(None)),
        }
    }

    pub(crate) fn from_record(record: ImageRecord) -> Self {
        Self::new(record)
    }

    pub(crate) fn ensure_picker_initialized(
        &self,
        picker: Option<Picker>,
        font_size: (u16, u16),
    ) {
        let mut slot = self.cached_picker.borrow_mut();
        let needs_init = match slot.as_ref() {
            None => true,
            Some(existing) => {
                if let Some(provided) = picker.as_ref() {
                    existing.font_size() != provided.font_size()
                        || existing.protocol_type() != provided.protocol_type()
                } else {
                    existing.font_size() != font_size
                }
            }
        };
        if needs_init {
            *slot = Some(picker.unwrap_or_else(|| Picker::from_fontsize(font_size)));
            self.cached_image_protocol.borrow_mut().take();
        }
    }

    pub(crate) fn record(&self) -> &ImageRecord {
        &self.record
    }

    pub(crate) fn record_mut(&mut self) -> &mut ImageRecord {
        &mut self.record
    }

    fn image_path(&self) -> Option<&Path> {
        self.record.source_path.as_deref()
    }

    fn ensure_picker(&self) -> Picker {
        let mut picker_ref = self.cached_picker.borrow_mut();
        if picker_ref.is_none() {
            *picker_ref = Some(Picker::from_fontsize((8, 16)));
        }
        picker_ref.as_ref().unwrap().clone()
    }

    fn accent_style(style: &CardStyle) -> Style {
        if palette_mode() == PaletteMode::Ansi16 {
            return Style::default().fg(ansi16_inverse_color());
        }
        let dim = colors::mix_toward(style.accent_fg, style.text_secondary, 0.85);
        Style::default().fg(dim)
    }

    fn display_label(&self) -> String {
        if let Some(alt) = self
            .record
            .alt_text
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            return alt.to_string();
        }
        if let Some(path) = self.record.source_path.as_ref() {
            if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                if !name.trim().is_empty() {
                    return name.to_string();
                }
            }
            return path.display().to_string();
        }
        "Image".to_string()
    }

    fn header_title_text(&self) -> String {
        let label = self.display_label();
        if label.eq_ignore_ascii_case("image") {
            "Image".to_string()
        } else {
            format!("Image: {label}")
        }
    }

    fn metadata_lines(&self) -> Vec<String> {
        let record = &self.record;
        let width = record.width;
        let height = record.height;
        let mut lines = Vec::new();
        lines.push(format!("Dimensions: {width}x{height} px"));
        if let Some(mime) = record
            .mime_type
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            lines.push(format!("Type: {mime}"));
        }
        if let Some(byte_len) = record.byte_len {
            let size = format_with_separators(u64::from(byte_len));
            lines.push(format!("Size: {size} bytes"));
        }
        if let Some(alt) = record
            .alt_text
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            lines.push(format!("Alt: {alt}"));
        }
        if let Some(path) = record.source_path.as_ref() {
            lines.push(format!("Source: {}", path.display()));
        }
        if let Some(hash) = record.sha256.as_ref() {
            let short = if hash.len() > 12 {
                format!("{}…", &hash[..12])
            } else {
                hash.clone()
            };
            lines.push(format!("SHA256: {short}"));
        }
        lines
    }

    fn top_border_row(&self, body_width: usize, style: &CardStyle) -> CardRow {
        let mut segments = Vec::new();
        if body_width == 0 {
            return CardRow::new(
                BORDER_TOP.to_string(),
                Self::accent_style(style),
                segments,
                None,
            );
        }

        let title_style = if palette_mode() == PaletteMode::Ansi16 {
            Style::default().fg(ansi16_inverse_color())
        } else {
            title_text_style(style)
        };

        segments.push(CardSegment::new(" ".to_string(), title_style));
        let remaining = body_width.saturating_sub(1);
        let text = truncate_with_ellipsis(self.header_title_text().as_str(), remaining);
        if !text.is_empty() {
            segments.push(CardSegment::new(text, title_style));
        }
        CardRow::new(BORDER_TOP.to_string(), Self::accent_style(style), segments, None)
    }

    fn blank_border_row(&self, body_width: usize, style: &CardStyle) -> CardRow {
        CardRow::new(
            BORDER_BODY.to_string(),
            Self::accent_style(style),
            vec![CardSegment::new(" ".repeat(body_width), Style::default())],
            None,
        )
    }

    fn body_text_row(
        &self,
        text: impl Into<String>,
        body_width: usize,
        style: &CardStyle,
        text_style: Style,
        indent_cols: usize,
        right_padding_cols: usize,
    ) -> CardRow {
        if body_width == 0 {
            return CardRow::new(BORDER_BODY.to_string(), Self::accent_style(style), Vec::new(), None);
        }
        let indent = indent_cols.min(body_width.saturating_sub(1));
        let available = body_width.saturating_sub(indent);
        let mut segments = Vec::new();
        if indent > 0 {
            segments.push(CardSegment::new(" ".repeat(indent), Style::default()));
        }
        let text: String = text.into();
        if available == 0 {
            return CardRow::new(BORDER_BODY.to_string(), Self::accent_style(style), segments, None);
        }
        let usable_width = available.saturating_sub(right_padding_cols);
        let display = if usable_width == 0 {
            String::new()
        } else {
            truncate_with_ellipsis(text.as_str(), usable_width)
        };
        segments.push(CardSegment::new(display, text_style));
        if right_padding_cols > 0 && available > 0 {
            let pad = right_padding_cols.min(available);
            segments.push(CardSegment::new(" ".repeat(pad), Style::default()));
        }
        CardRow::new(BORDER_BODY.to_string(), Self::accent_style(style), segments, None)
    }

    fn bottom_border_row(&self, body_width: usize, style: &CardStyle) -> CardRow {
        let text = truncate_with_ellipsis(HINT_TEXT, body_width);
        let hint_style = if palette_mode() == PaletteMode::Ansi16 {
            Style::default().fg(ansi16_inverse_color())
        } else {
            hint_text_style(style)
        };
        let segment = CardSegment::new(text, hint_style);
        CardRow::new(
            BORDER_BOTTOM.to_string(),
            Self::accent_style(style),
            vec![segment],
            None,
        )
    }

    fn build_card_rows(
        &self,
        width: u16,
        style: &CardStyle,
    ) -> (Vec<CardRow>, Option<ImagePreviewLayout>) {
        if width == 0 {
            return (Vec::new(), None);
        }

        let accent_width = CARD_ACCENT_WIDTH.min(width as usize);
        let body_width = width
            .saturating_sub(accent_width as u16)
            .saturating_sub(1) as usize;
        if body_width == 0 {
            return (Vec::new(), None);
        }

        let mut rows: Vec<CardRow> = Vec::new();
        rows.push(self.top_border_row(body_width, style));
        rows.push(self.blank_border_row(body_width, style));

        let mut image_layout = self.compute_image_layout(body_width);
        let indent_cols = image_layout
            .as_ref()
            .map(|layout| layout.indent_cols)
            .unwrap_or(DEFAULT_TEXT_INDENT);
        let indent_cols = indent_cols.min(body_width.saturating_sub(1));
        let right_padding = TEXT_RIGHT_PADDING.min(body_width);

        let content_start = rows.len();

        rows.push(self.body_text_row(
            "Details",
            body_width,
            style,
            primary_text_style(style),
            indent_cols,
            right_padding,
        ));

        let details = self.metadata_lines();
        if details.is_empty() {
            for wrapped in wrap_card_lines(
                "No image metadata available",
                body_width,
                indent_cols,
                right_padding,
            ) {
                rows.push(self.body_text_row(
                    wrapped,
                    body_width,
                    style,
                    secondary_text_style(style),
                    indent_cols,
                    right_padding,
                ));
            }
        } else {
            for detail in details {
                for wrapped in wrap_card_lines(
                    detail.as_str(),
                    body_width,
                    indent_cols,
                    right_padding,
                ) {
                    rows.push(self.body_text_row(
                        wrapped,
                        body_width,
                        style,
                        secondary_text_style(style),
                        indent_cols,
                        right_padding,
                    ));
                }
            }
        }

        if let Some(layout) = image_layout.as_mut() {
            layout.start_row = content_start;
            let existing = rows.len().saturating_sub(content_start);
            if existing < layout.height_rows {
                let missing = layout.height_rows - existing;
                for _ in 0..missing {
                    rows.push(self.body_text_row(
                        "",
                        body_width,
                        style,
                        Style::default(),
                        indent_cols,
                        right_padding,
                    ));
                }
            }
        }

        rows.push(self.blank_border_row(body_width, style));
        rows.push(self.bottom_border_row(body_width, style));

        (rows, image_layout)
    }

    fn compute_image_layout(&self, body_width: usize) -> Option<ImagePreviewLayout> {
        if self.image_path().is_none() {
            return None;
        }

        if body_width
            < IMAGE_LEFT_PAD + IMAGE_MIN_WIDTH + IMAGE_GAP + MIN_TEXT_WIDTH + TEXT_RIGHT_PADDING
        {
            return None;
        }

        let max_image = body_width
            .saturating_sub(IMAGE_LEFT_PAD + MIN_TEXT_WIDTH + IMAGE_GAP + TEXT_RIGHT_PADDING);
        if max_image < IMAGE_MIN_WIDTH {
            return None;
        }

        let mut image_cols = max_image;
        if image_cols > IMAGE_MAX_WIDTH {
            image_cols = IMAGE_MAX_WIDTH;
        }
        if image_cols < IMAGE_MIN_WIDTH {
            image_cols = IMAGE_MIN_WIDTH;
        }

        let rows = self.compute_image_rows(image_cols)?;
        Some(ImagePreviewLayout {
            start_row: 0,
            height_rows: rows,
            width_cols: image_cols,
            indent_cols: IMAGE_LEFT_PAD + image_cols + IMAGE_GAP,
        })
    }

    fn compute_image_rows(&self, image_cols: usize) -> Option<usize> {
        if image_cols == 0 {
            return None;
        }
        let path = self.image_path()?;
        let picker = self.ensure_picker();
        let (cell_w, cell_h) = picker.font_size();
        if cell_w == 0 || cell_h == 0 {
            return Some(MIN_IMAGE_ROWS);
        }

        let (img_w, img_h) = match image_dimensions(path) {
            Ok((w, h)) if w > 0 && h > 0 => (w, h),
            _ => return Some(MIN_IMAGE_ROWS),
        };

        let cols = image_cols as u32;
        let rows_by_w = (cols * cell_w as u32 * img_h) as f64
            / (img_w * cell_h as u32) as f64;
        let rows = rows_by_w.ceil().max(1.0) as usize;
        Some(rows.clamp(MIN_IMAGE_ROWS, MAX_IMAGE_ROWS))
    }

    fn render_card(&self, area: Rect, buf: &mut Buffer, skip_rows: u16) {
        if area.width <= 2 || area.height == 0 {
            return;
        }

        let style = browser_card_style();
        let draw_width = area.width - 2;
        let render_area = Rect {
            width: draw_width,
            ..area
        };

        fill_card_background(buf, render_area, &style);
        let (rows, preview_layout) = self.build_card_rows(render_area.width, &style);
        if rows.is_empty() {
            self.render_plain_summary(area, buf, skip_rows);
            return;
        }
        let lines = rows_to_lines(&rows, &style, render_area.width);
        let text = Text::from(lines);

        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .scroll((skip_rows, 0))
            .render(render_area, buf);

        if let Some(layout) = preview_layout.as_ref() {
            if let Some(path) = self.image_path() {
                self.render_image_preview(render_area, buf, skip_rows, layout, path);
            }
        }

        let clear_start = area.x + draw_width;
        let clear_end = area.x + area.width;
        for x in clear_start..clear_end {
            for row in 0..area.height {
                let cell = &mut buf[(x, area.y + row)];
                cell.set_symbol(" ");
                cell.set_bg(crate::colors::background());
            }
        }
    }

    fn render_image_preview(
        &self,
        area: Rect,
        buf: &mut Buffer,
        skip_rows: u16,
        layout: &ImagePreviewLayout,
        path: &Path,
    ) {
        let accent_width = CARD_ACCENT_WIDTH.min(area.width as usize) as u16;
        if accent_width >= area.width {
            return;
        }

        let viewport_top = skip_rows as usize;
        let viewport_bottom = viewport_top + area.height as usize;
        let shot_top = layout.start_row;
        let shot_bottom = layout.start_row + layout.height_rows;

        if shot_bottom <= viewport_top || shot_top >= viewport_bottom {
            return;
        }

        let visible_top = shot_top.max(viewport_top);
        let visible_bottom = shot_bottom.min(viewport_bottom);
        if visible_bottom <= visible_top {
            return;
        }

        let body_width = area.width.saturating_sub(accent_width);
        if body_width == 0 {
            return;
        }

        let left_pad = IMAGE_LEFT_PAD.min(body_width as usize) as u16;
        if body_width <= left_pad {
            return;
        }

        let usable_width = body_width.saturating_sub(left_pad);
        let image_width = layout.width_cols.min(usable_width as usize) as u16;
        if image_width == 0 {
            return;
        }


        let rows_to_copy = (visible_bottom - visible_top) as u16;
        if rows_to_copy == 0 {
            return;
        }

        let dest_x = area.x + accent_width + left_pad;
        let dest_y = area.y + (visible_top - viewport_top) as u16;
        let placeholder_area = Rect {
            x: dest_x,
            y: dest_y,
            width: image_width,
            height: rows_to_copy,
        };

        if !path.exists() {
            self.render_image_placeholder(path, placeholder_area, buf);
            return;
        }

        let full_height = layout.height_rows as u16;
        if full_height == 0 {
            return;
        }

        let picker = self.ensure_picker();
        let supports_partial_render = matches!(picker.protocol_type(), ProtocolType::Halfblocks);
        let is_partially_visible = visible_top != shot_top || visible_bottom != shot_bottom;
        if is_partially_visible && !supports_partial_render {
            // Graphical terminal protocols (kitty/sixel/iterm2) render with escape sequences that
            // can't be safely truncated mid-image without risking cursor movement/overdraw.
            // Prefer a placeholder over broken scroll/clipping.
            self.render_image_placeholder(path, placeholder_area, buf);
            return;
        }

        // For full-image rendering, draw directly into the final buffer so protocols that rely on
        // escape sequences behave correctly.
        if !is_partially_visible {
            let protocol_target = Rect::new(0, 0, image_width, full_height);
            if self.ensure_protocol(path, protocol_target, &picker).is_err() {
                self.render_image_placeholder(path, placeholder_area, buf);
                return;
            }
            let dest_target = Rect::new(dest_x, dest_y, image_width, full_height);
            if let Some((_, _, protocol)) = self.cached_image_protocol.borrow_mut().as_mut() {
                let image = Image::new(protocol);
                image.render(dest_target, buf);
            } else {
                self.render_image_placeholder(path, placeholder_area, buf);
            }
            return;
        }

        if !supports_partial_render {
            self.render_image_placeholder(path, placeholder_area, buf);
            return;
        }

        let offscreen = match self.render_image_buffer(path, image_width, full_height) {
            Ok(buffer) => buffer,
            Err(_) => {
                self.render_image_placeholder(path, placeholder_area, buf);
                return;
            }
        };

        let src_start_row = (visible_top - shot_top) as u16;
        let area_bottom = area.y + area.height;
        let area_right = area.x + area.width;

        for row in 0..rows_to_copy {
            let dest_row = dest_y + row;
            if dest_row >= area_bottom {
                break;
            }
            let src_row = src_start_row + row;
            for col in 0..image_width {
                let dest_col = dest_x + col;
                if dest_col >= area_right {
                    break;
                }
                let Some(src_cell) = offscreen.cell((col, src_row)) else { continue; };
                if let Some(dest_cell) = buf.cell_mut((dest_col, dest_row)) {
                    *dest_cell = src_cell.clone();
                }
            }
        }
    }

    fn render_image_placeholder(&self, path: &Path, area: Rect, buf: &mut Buffer) {
        use ratatui::style::{Modifier, Style};
        use ratatui::widgets::{Block, Borders};

        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("image");
        let placeholder_text = format!("Image:\n{filename}");

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(colors::info()))
            .title("Image");
        let inner = block.inner(area);
        block.render(area, buf);
        Paragraph::new(placeholder_text)
            .style(
                Style::default()
                    .fg(colors::text_dim())
                    .add_modifier(Modifier::ITALIC),
            )
            .wrap(Wrap { trim: true })
            .render(inner, buf);
    }

    fn render_image_buffer(&self, path: &Path, width: u16, height: u16) -> Result<Buffer, ()> {
        if width == 0 || height == 0 {
            return Err(());
        }
        let picker = self.ensure_picker();
        let target = Rect::new(0, 0, width, height);
        self.ensure_protocol(path, target, &picker)?;

        let mut buffer = Buffer::empty(target);
        if let Some((_, _, protocol)) = self.cached_image_protocol.borrow_mut().as_mut() {
            let image = Image::new(protocol);
            image.render(target, &mut buffer);
            Ok(buffer)
        } else {
            Err(())
        }
    }

    fn ensure_protocol(&self, path: &Path, target: Rect, picker: &Picker) -> Result<(), ()> {
        let mut cache = self.cached_image_protocol.borrow_mut();
        let needs_recreate = match cache.as_ref() {
            Some((cached_path, cached_rect, _)) => cached_path != path || *cached_rect != target,
            None => true,
        };
        if needs_recreate {
            let dyn_img = match ImageReader::open(path) {
                Ok(reader) => reader.decode().map_err(|_| ())?,
                Err(_) => return Err(()),
            };
            let protocol = picker
                .new_protocol(dyn_img, target, Resize::Fit(Some(FilterType::Lanczos3)))
                .map_err(|_| ())?;
            *cache = Some((path.to_path_buf(), target, protocol));
        }
        Ok(())
    }

    fn render_plain_summary(&self, area: Rect, buf: &mut Buffer, skip_rows: u16) {
        let cell_bg = colors::background();
        let bg_style = Style::default().bg(cell_bg).fg(colors::text());
        fill_rect(buf, area, Some(' '), bg_style);
        let lines = self.display_lines_trimmed();
        let text = Text::from(lines);
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .scroll((skip_rows, 0))
            .style(Style::default().bg(cell_bg))
            .render(area, buf);
    }
}

impl HistoryCell for ImageOutputCell {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn kind(&self) -> HistoryCellType {
        HistoryCellType::Image
    }

    fn display_lines(&self) -> Vec<Line<'static>> {
        let label = self.display_label();
        let summary = if label.eq_ignore_ascii_case("image") {
            "Image output".to_string()
        } else {
            format!("Image output: {label}")
        };

        let mut lines = vec![Line::from(summary)];
        for line in self.metadata_lines() {
            lines.push(Line::from(line));
        }
        lines.push(Line::from(""));
        lines
    }

    fn desired_height(&self, width: u16) -> u16 {
        let style = browser_card_style();
        let trimmed_width = width.saturating_sub(2);
        if trimmed_width == 0 {
            return 0;
        }
        let (rows, _) = self.build_card_rows(trimmed_width, &style);
        rows.len().max(1) as u16
    }

    fn has_custom_render(&self) -> bool {
        true
    }

    fn custom_render_with_skip(&self, area: Rect, buf: &mut Buffer, skip_rows: u16) {
        self.render_card(area, buf, skip_rows);
    }
}

fn wrap_card_lines(text: &str, body_width: usize, indent_cols: usize, right_padding: usize) -> Vec<String> {
    let available = body_width
        .saturating_sub(indent_cols)
        .saturating_sub(right_padding);
    if available == 0 {
        return vec![String::new()];
    }
    wrap_line_to_width(text, available)
}

fn wrap_line_to_width(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![String::new()];
    }
    if text.trim().is_empty() {
        return vec![String::new()];
    }

    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_width = 0;

    for word in text.split_whitespace() {
        let mut word_parts = if string_display_width(word) > width {
            split_long_card_word(word, width)
        } else {
            vec![word.to_string()]
        };

        for part in word_parts.drain(..) {
            let part_width = string_display_width(part.as_str());
            if current.is_empty() {
                current.push_str(part.as_str());
                current_width = part_width;
            } else if current_width + 1 + part_width > width {
                lines.push(current);
                current = part.clone();
                current_width = part_width;
            } else {
                current.push(' ');
                current.push_str(part.as_str());
                current_width += 1 + part_width;
            }
        }
    }

    if !current.is_empty() {
        lines.push(current);
    }

    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn split_long_card_word(word: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![String::new()];
    }

    let mut parts = Vec::new();
    let mut current = String::new();
    let mut current_width = 0;

    for ch in word.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(1);
        if current_width + ch_width > width && !current.is_empty() {
            parts.push(current);
            current = String::new();
            current_width = 0;
        }
        current.push(ch);
        current_width += ch_width;
    }

    if !current.is_empty() {
        parts.push(current);
    }

    if parts.is_empty() {
        parts.push(String::new());
    }
    parts
}

fn string_display_width(text: &str) -> usize {
    text
        .chars()
        .map(|ch| UnicodeWidthChar::width(ch).unwrap_or(0))
        .sum()
}
