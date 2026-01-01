use super::*;
use crate::history::state::ImageRecord;
use code_protocol::num_format::format_with_separators;
use ::image::ImageReader;
use ::image::image_dimensions;
use ratatui::widgets::{Paragraph, Wrap};
use ratatui_image::{Image, Resize};
use ratatui_image::picker::Picker;
use ratatui_image::FilterType;
use std::cell::RefCell;
use std::path::Path;
use std::path::PathBuf;
use std::rc::Rc;

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

    fn image_rows_for_width(&self, width: u16) -> Option<u16> {
        const MIN_IMAGE_ROWS: usize = 4;
        const MAX_IMAGE_ROWS: usize = 60;
        let path = self.image_path()?;
        if width == 0 || !path.exists() {
            return None;
        }
        let picker = self.ensure_picker();
        let (cell_w, cell_h) = picker.font_size();
        if cell_w == 0 || cell_h == 0 {
            return Some(MIN_IMAGE_ROWS as u16);
        }
        let (img_w, img_h) = match image_dimensions(path) {
            Ok((w, h)) if w > 0 && h > 0 => (w, h),
            _ => return Some(MIN_IMAGE_ROWS as u16),
        };
        let cols = width as u32;
        let rows_by_w = (cols * cell_w as u32 * img_h) as f64
            / (img_w * cell_h as u32) as f64;
        let rows = rows_by_w.ceil().max(1.0) as usize;
        Some(rows.clamp(MIN_IMAGE_ROWS, MAX_IMAGE_ROWS) as u16)
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

    fn render_text_only(&self, area: Rect, buf: &mut Buffer, skip_rows: u16) {
        let cell_bg = crate::colors::background();
        let bg_style = Style::default().bg(cell_bg).fg(crate::colors::text());
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
        let record = &self.record;
        let mut descriptors = vec![format!("{}x{} px", record.width, record.height)];
        if let Some(mime) = &record.mime_type {
            descriptors.push(mime.clone());
        }
        if let Some(byte_len) = record.byte_len {
            descriptors.push(format!(
                "{} bytes",
                format_with_separators(u64::from(byte_len))
            ));
        }
        let summary = format!("tool result ({})", descriptors.join(", "));

        let mut lines = vec![Line::from(summary)];
        if let Some(alt) = record.alt_text.as_ref() {
            if !alt.is_empty() {
                lines.push(Line::from(format!("alt: {alt}")));
            }
        }
        if let Some(path) = record.source_path.as_ref() {
            lines.push(Line::from(format!("source: {}", path.display())));
        }
        if let Some(hash) = record.sha256.as_ref() {
            let short = if hash.len() > 12 {
                format!("{}…", &hash[..12])
            } else {
                hash.clone()
            };
            lines.push(Line::from(format!("sha256: {short}")));
        }
        lines.push(Line::from(""));
        lines
    }

    fn desired_height(&self, width: u16) -> u16 {
        let image_rows = self.image_rows_for_width(width).unwrap_or(0);
        let lines = self.display_lines_trimmed();
        let text_height = Paragraph::new(Text::from(lines))
            .wrap(Wrap { trim: false })
            .line_count(width) as u16;
        image_rows.saturating_add(text_height)
    }

    fn has_custom_render(&self) -> bool {
        self.image_path().is_some()
    }

    fn custom_render_with_skip(&self, area: Rect, buf: &mut Buffer, skip_rows: u16) {
        let Some(path) = self.image_path() else {
            self.render_text_only(area, buf, skip_rows);
            return;
        };
        let Some(image_rows) = self.image_rows_for_width(area.width) else {
            self.render_text_only(area, buf, skip_rows);
            return;
        };

        let cell_bg = crate::colors::background();
        let bg_style = Style::default().bg(cell_bg).fg(crate::colors::text());
        fill_rect(buf, area, Some(' '), bg_style);

        let mut image_visible_rows = 0u16;
        if skip_rows < image_rows {
            image_visible_rows = (image_rows - skip_rows).min(area.height);
            let offscreen = match self.render_image_buffer(path, area.width, image_rows) {
                Ok(buffer) => buffer,
                Err(_) => {
                    self.render_text_only(area, buf, skip_rows);
                    return;
                }
            };
            let src_start = skip_rows;
            for row in 0..image_visible_rows {
                let dest_row = area.y + row;
                let src_row = src_start + row;
                for col in 0..area.width {
                    let dest_col = area.x + col;
                    let Some(src_cell) = offscreen.cell((col, src_row)) else {
                        continue;
                    };
                    if let Some(dest_cell) = buf.cell_mut((dest_col, dest_row)) {
                        *dest_cell = src_cell.clone();
                    }
                }
            }
        }

        if area.height > image_visible_rows {
            let text_area = Rect {
                x: area.x,
                y: area.y + image_visible_rows,
                width: area.width,
                height: area.height - image_visible_rows,
            };
            let text_scroll = skip_rows.saturating_sub(image_rows);
            let lines = self.display_lines_trimmed();
            let text = Text::from(lines);
            Paragraph::new(text)
                .wrap(Wrap { trim: false })
                .scroll((text_scroll, 0))
                .style(Style::default().bg(cell_bg))
                .render(text_area, buf);
        }
    }
}
