use super::*;
use crate::history::{AssistantMessageState, AssistantStreamState};
use code_core::history::state::MAX_ASSISTANT_STREAM_RETAINED_BYTES;
use code_core::config::Config;
use code_core::config_types::UriBasedFileOpener;
use ratatui::style::Style;
use ratatui::text::Line;
use std::path::Path;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

// ==================== StreamingContentCell ====================
// Renders in-progress assistant answers backed by `AssistantStreamState`.

pub(crate) struct StreamingContentCell {
    pub(crate) id: Option<String>,
    state: AssistantStreamState,
    file_opener: UriBasedFileOpener,
    cwd: PathBuf,
}

impl HistoryCell for StreamingContentCell {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn kind(&self) -> HistoryCellType {
        HistoryCellType::Assistant
    }

    fn display_lines(&self) -> Vec<Line<'static>> {
        stream_lines_from_state_with_context(
            &self.state,
            self.file_opener,
            &self.cwd,
            self.state.in_progress,
        )
    }
}

impl StreamingContentCell {
    pub(crate) fn from_state(
        state: AssistantStreamState,
        file_opener: UriBasedFileOpener,
        cwd: PathBuf,
    ) -> Self {
        Self {
            id: Some(state.stream_id.clone()),
            state,
            file_opener,
            cwd,
        }
    }

    pub(crate) fn set_state(&mut self, state: AssistantStreamState) {
        self.state = state;
        self.id = Some(self.state.stream_id.clone());
    }

    pub(crate) fn state(&self) -> &AssistantStreamState {
        &self.state
    }

    pub(crate) fn state_mut(&mut self) -> &mut AssistantStreamState {
        &mut self.state
    }

    pub(crate) fn update_context(
        &mut self,
        file_opener: UriBasedFileOpener,
        cwd: &Path,
    ) {
        self.file_opener = file_opener;
        self.cwd = cwd.to_path_buf();
    }
}

pub(crate) fn stream_lines_from_state(
    state: &AssistantStreamState,
    cfg: &code_core::config::Config,
    show_ellipsis: bool,
) -> Vec<Line<'static>> {
    stream_lines_from_state_with_context(state, cfg.file_opener, &cfg.cwd, show_ellipsis)
}

/// Render streaming assistant content directly from the recorded
/// [`AssistantStreamState`], embedding file-opener context so hyperlinks remain
/// resolvable. Downstream caching keys off `HistoryId`, width, theme epoch, and
/// reasoning visibility, so cells no longer maintain per-width layout caches.
pub(crate) fn stream_lines_from_state_with_context(
    state: &AssistantStreamState,
    file_opener: UriBasedFileOpener,
    cwd: &Path,
    show_ellipsis: bool,
) -> Vec<Line<'static>> {
    let mut markdown = state.preview_markdown.clone();
    if state.truncated_prefix_bytes > 0 {
        let note = format!(
            "… clipped {} from the start of assistant response (showing last {}).\n\n",
            format_bytes(state.truncated_prefix_bytes),
            format_bytes(MAX_ASSISTANT_STREAM_RETAINED_BYTES),
        );
        markdown = format!("{note}{markdown}");
    }

    let message_state = AssistantMessageState {
        id: state.id,
        stream_id: Some(state.stream_id.clone()),
        markdown,
        citations: state.citations.clone(),
        metadata: state.metadata.clone(),
        token_usage: state
            .metadata
            .as_ref()
            .and_then(|meta| meta.token_usage.clone()),
        mid_turn: false,
        created_at: state.last_updated_at,
    };

    let mut rendered: Vec<Line<'static>> = Vec::new();
    // Insert a sentinel so downstream styling mirrors assistant message rendering.
    rendered.push(Line::from("stream"));
    crate::markdown::append_markdown_with_opener_and_cwd_and_bold(
        &message_state.markdown,
        &mut rendered,
        file_opener,
        cwd,
        true,
    );

    let bright = crate::colors::text_bright();
    for line in rendered.iter_mut().skip(1) {
        line.style = line.style.patch(Style::default().fg(bright));
    }

    let mut lines: Vec<Line<'static>> = rendered.into_iter().skip(1).collect();
    if show_ellipsis {
        lines.push(ellipsis_line());
    }
    lines
}

fn ellipsis_line() -> Line<'static> {
    const FRAMES: [&str; 5] = ["...", "·..", ".·.", "..·", "..."];
    let idx = (SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        / 200) as usize
        % FRAMES.len();
    Line::styled(
        FRAMES[idx].to_string(),
        Style::default().fg(crate::colors::text_dim()),
    )
}

fn format_bytes(bytes: usize) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let bytes_f = bytes as f64;
    if bytes >= GIB as usize {
        format!("{:.1} GiB", bytes_f / GIB)
    } else if bytes >= MIB as usize {
        format!("{:.1} MiB", bytes_f / MIB)
    } else if bytes >= KIB as usize {
        format!("{:.1} KiB", bytes_f / KIB)
    } else {
        format!("{bytes} B")
    }
}

pub(crate) fn new_streaming_content(
    state: AssistantStreamState,
    cfg: &Config,
) -> StreamingContentCell {
    StreamingContentCell::from_state(state, cfg.file_opener, cfg.cwd.clone())
}
