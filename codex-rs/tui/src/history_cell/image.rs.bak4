use super::*;
use crate::history::state::ImageRecord;
use codex_protocol::num_format::format_with_separators;

pub(crate) struct ImageOutputCell {
    record: ImageRecord,
}

impl ImageOutputCell {
    pub(crate) fn new(record: ImageRecord) -> Self {
        Self { record }
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
}
