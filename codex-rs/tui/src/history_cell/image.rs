use super::*;

#[derive(Clone)]
pub(crate) struct ImageCellState {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl ImageCellState {
    pub(crate) fn from_dynamic(image: DynamicImage) -> Self {
        let width = image.width();
        let height = image.height();
        let rgba = image.to_rgba8().into_raw();
        Self { width, height, rgba }
    }
}

pub(crate) struct ImageOutputCell {
    state: ImageCellState,
}

impl ImageOutputCell {
    pub(crate) fn new(image: DynamicImage) -> Self {
        Self {
            state: ImageCellState::from_dynamic(image),
        }
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
        let description = format!(
            "tool result (image {}x{} RGBA, {} bytes)",
            self.state.width,
            self.state.height,
            self.state.rgba.len()
        );
        vec![
            Line::from(description),
            Line::from(""),
        ]
    }
}
