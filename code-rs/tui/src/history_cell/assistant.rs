use super::*;
use crate::history::state::AssistantMessageState;
use code_core::config::Config;
use code_core::config_types::UriBasedFileOpener;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

// ==================== AssistantMarkdownCell ====================
// Stores assistant markdown state alongside minimal rendering context (file opener + cwd).

pub(crate) struct AssistantMarkdownCell {
    state: AssistantMessageState,
    file_opener: UriBasedFileOpener,
    cwd: PathBuf,
    layout_cache: RefCell<HashMap<u16, Rc<AssistantLayoutCache>>>,
}

impl AssistantMarkdownCell {
    pub(crate) fn from_state(
        state: AssistantMessageState,
        cfg: &code_core::config::Config,
    ) -> Self {
        Self {
            state,
            file_opener: cfg.file_opener,
            cwd: cfg.cwd.clone(),
            layout_cache: RefCell::new(HashMap::new()),
        }
    }

    pub(crate) fn update_state(
        &mut self,
        state: AssistantMessageState,
        cfg: &code_core::config::Config,
    ) {
        self.state = state;
        self.file_opener = cfg.file_opener;
        self.cwd = cfg.cwd.clone();
        self.layout_cache.borrow_mut().clear();
    }

    pub(crate) fn set_mid_turn(&mut self, mid_turn: bool) {
        if self.state.mid_turn == mid_turn {
            return;
        }
        self.state.mid_turn = mid_turn;
        self.layout_cache.borrow_mut().clear();
    }

    pub(crate) fn stream_id(&self) -> Option<&str> {
        self.state.stream_id.as_deref()
    }

    pub(crate) fn markdown(&self) -> &str {
        &self.state.markdown
    }

    pub(crate) fn state(&self) -> &AssistantMessageState {
        &self.state
    }

    pub(crate) fn state_mut(&mut self) -> &mut AssistantMessageState {
        &mut self.state
    }

    pub(crate) fn ensure_layout(&self, width: u16) -> Rc<AssistantLayoutCache> {
        if width == 0 {
            let mut cache = self.layout_cache.borrow_mut();
            let entry = cache.entry(0).or_insert_with(|| {
                Rc::new(AssistantLayoutCache {
                    segs: Vec::new(),
                    seg_rows: Vec::new(),
                    total_rows_with_padding: 0,
                })
            });
            return Rc::clone(entry);
        }

        if let Some(plan) = self.layout_cache.borrow().get(&width) {
            return Rc::clone(plan);
        }

        let plan = Rc::new(compute_assistant_layout_with_context(
            &self.state,
            self.file_opener,
            &self.cwd,
            width,
        ));
        self.layout_cache
            .borrow_mut()
            .insert(width, Rc::clone(&plan));
        plan
    }

    pub(crate) fn render_with_layout(
        &self,
        plan: &AssistantLayoutCache,
        area: Rect,
        buf: &mut Buffer,
        skip_rows: u16,
    ) {
        let cell_bg = if self.state.mid_turn {
            crate::colors::assistant_mid_turn_bg()
        } else {
            crate::colors::assistant_bg()
        };
        let bg_style = Style::default().bg(cell_bg);
        fill_rect(buf, area, Some(' '), bg_style);

        if area.width == 0 || area.height == 0 {
            return;
        }

        let segs = &plan.segs;
        let seg_rows = &plan.seg_rows;
        let mut remaining_skip = skip_rows;
        let mut cur_y = area.y;
        let end_y = area.y.saturating_add(area.height);

        if remaining_skip == 0
            && cur_y < end_y
            && area.height.saturating_sub(skip_rows) > 1
        {
            cur_y = cur_y.saturating_add(1);
        }
        remaining_skip = remaining_skip.saturating_sub(1);

        for (seg_idx, seg) in segs.iter().enumerate() {
            if cur_y >= end_y {
                break;
            }
            let rows = seg_rows.get(seg_idx).copied().unwrap_or(0);
            if remaining_skip >= rows {
                remaining_skip -= rows;
                continue;
            }

            match seg {
                AssistantSeg::Text(lines) | AssistantSeg::Bullet(lines) => {
                    let total = lines.len() as u16;
                    if total == 0 {
                        continue;
                    }
                    let start = usize::from(remaining_skip);
                    let visible = total.saturating_sub(remaining_skip);
                    let avail = end_y.saturating_sub(cur_y);
                    let draw_count = visible.min(avail);
                    if draw_count == 0 {
                        remaining_skip = 0;
                        continue;
                    }
                    for line in lines.iter().skip(start).take(draw_count as usize) {
                        if cur_y >= end_y {
                            break;
                        }
                        write_line(buf, area.x, cur_y, area.width, line, bg_style);
                        cur_y = cur_y.saturating_add(1);
                    }
                    remaining_skip = 0;
                }
                AssistantSeg::Code {
                    lines,
                    lang_label,
                    max_line_width,
                } => {
                    let avail = end_y.saturating_sub(cur_y);
                    if avail == 0 {
                        break;
                    }

                    let full_height = lines.len() as u16 + 2;
                    let card_w = max_line_width
                        .saturating_add(6)
                        .min(area.width.max(6));

                    let temp_area = Rect::new(0, 0, card_w, full_height);
                    let mut temp_buf = Buffer::empty(temp_area);
                    let code_bg = if self.state.mid_turn {
                        cell_bg
                    } else {
                        crate::colors::code_block_bg()
                    };
                    let blk = Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(crate::colors::border()))
                        .style(Style::default().bg(code_bg))
                        .padding(Padding {
                            left: 2,
                            right: 2,
                            top: 0,
                            bottom: 0,
                        });
                    let blk = if let Some(lang) = lang_label {
                        blk.title(Span::styled(
                            format!(" {} ", lang),
                            Style::default().fg(crate::colors::text_dim()),
                        ))
                    } else {
                        blk
                    };
                    let inner_rect = blk.inner(temp_area);
                    blk.clone().render(temp_area, &mut temp_buf);
                    for (idx, line) in lines.iter().enumerate() {
                        let target_y = inner_rect.y.saturating_add(idx as u16);
                        if target_y >= inner_rect.y.saturating_add(inner_rect.height) {
                            break;
                        }
                        write_line(
                            &mut temp_buf,
                            inner_rect.x,
                            target_y,
                            inner_rect.width,
                            line,
                            Style::default().bg(code_bg),
                        );
                    }

                    let start_row = remaining_skip.min(full_height);
                    let draw_rows = avail.min(full_height.saturating_sub(remaining_skip));
                    if draw_rows == 0 {
                        remaining_skip = 0;
                        continue;
                    }

                    for row_offset in 0..usize::from(draw_rows) {
                        let src_y = start_row + row_offset as u16;
                        let dest_y = cur_y.saturating_add(row_offset as u16);
                        if dest_y >= end_y {
                            break;
                        }
                        for col in 0..usize::from(card_w) {
                            let dest_x = area.x + col as u16;
                            if dest_x >= area.x.saturating_add(area.width) {
                                break;
                            }
                            let cell = temp_buf[(col as u16, src_y)].clone();
                            buf[(dest_x, dest_y)] = cell;
                        }
                    }
                    cur_y = cur_y.saturating_add(draw_rows);
                    remaining_skip = 0;
                }
            }
        }

        if remaining_skip == 0
            && cur_y < end_y
            && area.height.saturating_sub(skip_rows) > 1
        {
            cur_y = cur_y.saturating_add(1);
        } else {
            remaining_skip = remaining_skip.saturating_sub(1);
        }
        let _ = (cur_y, remaining_skip);
    }
}

impl HistoryCell for AssistantMarkdownCell {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn kind(&self) -> HistoryCellType {
        HistoryCellType::Assistant
    }

    fn gutter_symbol(&self) -> Option<&'static str> {
        if self.state.mid_turn {
            None
        } else {
            super::gutter_symbol_for_kind(self.kind())
        }
    }

    fn display_lines(&self) -> Vec<Line<'static>> {
        assistant_markdown_lines_with_context(&self.state, self.file_opener, &self.cwd)
    }

    fn has_custom_render(&self) -> bool {
        true
    }

    fn desired_height(&self, width: u16) -> u16 {
        self.ensure_layout(width).total_rows()
    }

    fn custom_render_with_skip(&self, area: Rect, buf: &mut Buffer, skip_rows: u16) {
        let plan = self.ensure_layout(area.width);
        self.render_with_layout(plan.as_ref(), area, buf, skip_rows);
    }
}

// Cached layout for AssistantMarkdownCell (per width)
#[derive(Clone)]
pub(crate) struct AssistantLayoutCache {
    pub(crate) segs: Vec<AssistantSeg>,
    pub(crate) seg_rows: Vec<u16>,
    pub(crate) total_rows_with_padding: u16,
}

impl AssistantLayoutCache {
    pub(crate) fn total_rows(&self) -> u16 {
        self.total_rows_with_padding
    }
}

pub(crate) fn assistant_markdown_lines(
    state: &AssistantMessageState,
    cfg: &Config,
) -> Vec<Line<'static>> {
    assistant_markdown_lines_with_context(state, cfg.file_opener, &cfg.cwd)
}

pub(crate) fn assistant_markdown_lines_with_context(
    state: &AssistantMessageState,
    file_opener: UriBasedFileOpener,
    cwd: &Path,
) -> Vec<Line<'static>> {
    let mut out: Vec<Line<'static>> = Vec::new();
    out.push(Line::from("codex"));
    crate::markdown::append_markdown_with_opener_and_cwd_and_bold(
        &state.markdown,
        &mut out,
        file_opener,
        cwd,
        !state.mid_turn,
    );
    let fg = if state.mid_turn {
        crate::colors::text_mid()
    } else {
        crate::colors::text_bright()
    };
    for line in out.iter_mut().skip(1) {
        line.style = line.style.patch(Style::default().fg(fg));
    }
    out.into_iter().skip(1).collect()
}

pub(crate) fn compute_assistant_layout(
    state: &AssistantMessageState,
    cfg: &Config,
    width: u16,
) -> AssistantLayoutCache {
    compute_assistant_layout_with_context(state, cfg.file_opener, &cfg.cwd, width)
}

pub(crate) fn compute_assistant_layout_with_context(
    state: &AssistantMessageState,
    file_opener: UriBasedFileOpener,
    cwd: &Path,
    width: u16,
) -> AssistantLayoutCache {
    let text_wrap_width = width;
    let mut segs: Vec<AssistantSeg> = Vec::new();
    let mut text_buf: Vec<Line<'static>> = Vec::new();
    let mut iter = super::trim_empty_lines(assistant_markdown_lines_with_context(state, file_opener, cwd))
        .into_iter()
        .peekable();
    let measure_line = |line: &Line<'_>| -> u16 {
        line.spans
            .iter()
            .map(|s| unicode_width::UnicodeWidthStr::width(s.content.as_ref()))
            .sum::<usize>()
            .min(u16::MAX as usize) as u16
    };

    while let Some(line) = iter.next() {
        if crate::render::line_utils::is_code_block_painted(&line) {
            if !text_buf.is_empty() {
                let wrapped = word_wrap_lines(&text_buf, text_wrap_width);
                segs.push(AssistantSeg::Text(wrapped));
                text_buf.clear();
            }

            let mut chunk = vec![line];
            while let Some(next) = iter.peek() {
                if crate::render::line_utils::is_code_block_painted(next) {
                    chunk.push(iter.next().unwrap());
                } else {
                    break;
                }
            }

            let mut lang_label: Option<String> = None;
            let mut content_lines: Vec<Line<'static>> = Vec::new();
            for (idx, candidate) in chunk.into_iter().enumerate() {
                if idx == 0 {
                    let flat: String = candidate
                        .spans
                        .iter()
                        .map(|s| s.content.as_ref())
                        .collect();
                    if let Some(s) = flat.strip_prefix("⟦LANG:") {
                        if let Some(end) = s.find('⟧') {
                            lang_label = Some(s[..end].to_string());
                            continue;
                        }
                    }
                }
                content_lines.push(candidate);
            }

            while content_lines
                .first()
                .is_some_and(|l| crate::render::line_utils::is_blank_line_spaces_only(l))
            {
                let _ = content_lines.remove(0);
            }
            while content_lines
                .last()
                .is_some_and(|l| crate::render::line_utils::is_blank_line_spaces_only(l))
            {
                let _ = content_lines.pop();
            }

            if content_lines.is_empty() {
                continue;
            }

            let code_wrap_width = width.saturating_sub(6) as usize;
            let content_lines = wrap_code_lines(content_lines, code_wrap_width);
            if content_lines.is_empty() {
                continue;
            }

            let max_line_width = content_lines
                .iter()
                .map(|l| measure_line(l))
                .max()
                .unwrap_or(0);

            segs.push(AssistantSeg::Code {
                lines: content_lines,
                lang_label,
                max_line_width,
            });
            continue;
        }

        if text_wrap_width > 4 && is_horizontal_rule_line(&line) {
            if !text_buf.is_empty() {
                let wrapped = word_wrap_lines(&text_buf, text_wrap_width);
                segs.push(AssistantSeg::Text(wrapped));
                text_buf.clear();
            }
            let hr = Line::from(Span::styled(
                std::iter::repeat('─')
                    .take(text_wrap_width as usize)
                    .collect::<String>(),
                Style::default().fg(crate::colors::assistant_hr()),
            ));
            segs.push(AssistantSeg::Bullet(vec![hr]));
            continue;
        }

        if text_wrap_width > 4 {
            if let Some((indent_spaces, bullet_char)) = detect_bullet_prefix(&line) {
                if !text_buf.is_empty() {
                    let wrapped = word_wrap_lines(&text_buf, text_wrap_width);
                    segs.push(AssistantSeg::Text(wrapped));
                    text_buf.clear();
                }
                segs.push(AssistantSeg::Bullet(wrap_bullet_line(
                    line,
                    indent_spaces,
                    &bullet_char,
                    text_wrap_width,
                )));
                continue;
            }
        }

        text_buf.push(line);
    }

    if !text_buf.is_empty() {
        let wrapped = word_wrap_lines(&text_buf, text_wrap_width);
        segs.push(AssistantSeg::Text(wrapped));
        text_buf.clear();
    }

    let mut seg_rows: Vec<u16> = Vec::with_capacity(segs.len());
    let mut total: u16 = 0;
    for seg in &segs {
        let rows = match seg {
            AssistantSeg::Text(lines) | AssistantSeg::Bullet(lines) => lines.len() as u16,
            AssistantSeg::Code { lines, .. } => lines.len() as u16 + 2,
        };
        seg_rows.push(rows);
        total = total.saturating_add(rows);
    }
    total = total.saturating_add(2);

    AssistantLayoutCache {
        segs,
        seg_rows,
        total_rows_with_padding: total,
    }
}

#[derive(Clone, Debug)]
pub(crate) enum AssistantSeg {
    Text(Vec<Line<'static>>),
    Bullet(Vec<Line<'static>>),
    Code {
        lines: Vec<Line<'static>>,
        lang_label: Option<String>,
        max_line_width: u16,
    },
}

fn wrap_code_lines(lines: Vec<Line<'static>>, width: usize) -> Vec<Line<'static>> {
    if width == 0 {
        return lines;
    }

    let mut out = Vec::new();
    for line in lines {
        let trimmed = trim_code_line_padding(line);
        out.extend(wrap_code_line(trimmed, width));
    }
    out
}

fn wrap_code_line(line: Line<'static>, width: usize) -> Vec<Line<'static>> {
    if width == 0 {
        return vec![line];
    }

    fn flush_current_line(
        out: &mut Vec<Line<'static>>,
        current_spans: &mut Vec<Span<'static>>,
        style: Style,
        alignment: Option<Alignment>,
        current_width: &mut usize,
    ) {
        if current_spans.is_empty() {
            return;
        }
        out.push(Line {
            style,
            alignment,
            spans: std::mem::take(current_spans),
        });
        *current_width = 0;
    }

    let line_width: usize = line
        .spans
        .iter()
        .map(|s| unicode_width::UnicodeWidthStr::width(s.content.as_ref()))
        .sum();
    if line_width <= width {
        return vec![line];
    }

    let mut out: Vec<Line<'static>> = Vec::new();
    let mut current_spans: Vec<Span<'static>> = Vec::new();
    let mut current_width = 0usize;
    let style = line.style;
    let alignment = line.alignment;

    for span in line.spans {
        let span_style = span.style;
        let mut remaining = span.content.into_owned();
        while !remaining.is_empty() {
            if current_width >= width {
                flush_current_line(&mut out, &mut current_spans, style, alignment, &mut current_width);
            }

            let available = width.saturating_sub(current_width);
            if available == 0 {
                continue;
            }

            let (prefix, suffix, taken) = crate::live_wrap::take_prefix_by_width(&remaining, available);
            if taken == 0 {
                if current_width > 0 {
                    flush_current_line(&mut out, &mut current_spans, style, alignment, &mut current_width);
                }
                if let Some((idx, ch)) = remaining.char_indices().next() {
                    let len = idx + ch.len_utf8();
                    let piece = remaining[..len].to_string();
                    current_width += unicode_width::UnicodeWidthStr::width(piece.as_str());
                    current_spans.push(Span::styled(piece, span_style));
                    remaining = remaining[len..].to_string();
                } else {
                    break;
                }
            } else {
                current_width += taken;
                current_spans.push(Span::styled(prefix, span_style));
                remaining = suffix.to_string();
            }

            if current_width >= width {
                flush_current_line(&mut out, &mut current_spans, style, alignment, &mut current_width);
            }
        }
    }

    if !current_spans.is_empty() {
        out.push(Line {
            style,
            alignment,
            spans: current_spans,
        });
    } else if out.is_empty() {
        out.push(Line {
            style,
            alignment,
            spans: Vec::new(),
        });
    }

    out
}

fn trim_code_line_padding(mut line: Line<'static>) -> Line<'static> {
    let pad_style = Style::default().bg(crate::colors::code_block_bg());
    while let Some(last) = line.spans.last() {
        if last.style != pad_style {
            break;
        }
        if !last.content.chars().all(|ch| ch == ' ') {
            break;
        }
        line.spans.pop();
    }
    line
}

// Detect lines that start with a markdown bullet produced by our renderer and return (indent, bullet)
pub(crate) fn detect_bullet_prefix(
    line: &ratatui::text::Line<'_>,
) -> Option<(usize, String)> {
    let bullets = ["-", "•", "◦", "·", "∘", "⋅", "☐", "✔"];
    let spans = &line.spans;
    if spans.is_empty() {
        return None;
    }
    // First span may be leading spaces
    let mut idx = 0;
    let mut indent = 0usize;
    if let Some(s) = spans.get(0) {
        let t = s.content.as_ref();
        if !t.is_empty() && t.chars().all(|c| c == ' ') {
            indent = t.chars().count();
            idx = 1;
        }
    }
    let bullet_span = spans.get(idx)?;
    let mut bullet_text = bullet_span.content.as_ref().to_string();
    let has_following_space_span = spans
        .get(idx + 1)
        .map(|s| s.content.as_ref() == " ")
        .unwrap_or(false);
    let has_trailing_space_in_bullet = bullet_text.ends_with(' ');
    if !(has_following_space_span || has_trailing_space_in_bullet) {
        return None;
    }
    if has_trailing_space_in_bullet {
        bullet_text.pop();
    }
    if bullets.contains(&bullet_text.as_str()) {
        return Some((indent, bullet_text));
    }
    if bullet_text.len() >= 2
        && bullet_text.ends_with('.')
        && bullet_text[..bullet_text.len() - 1]
            .chars()
            .all(|c| c.is_ascii_digit())
    {
        return Some((indent, bullet_text));
    }
    let flat: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    let mut chars = flat.chars().peekable();
    let mut indent_count = 0usize;
    while matches!(chars.peek(), Some(' ')) {
        chars.next();
        indent_count += 1;
    }
    let mut token = String::new();
    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() {
            break;
        }
        token.push(ch);
        chars.next();
        if token.len() > 8 {
            break;
        }
    }
    let has_space = matches!(chars.peek(), Some(c) if c.is_whitespace());
    if has_space {
        if bullets.contains(&token.as_str())
            || (token.len() >= 2
                && token.ends_with('.')
                && token[..token.len() - 1].chars().all(|c| c.is_ascii_digit()))
        {
            return Some((indent_count, token));
        }
    }
    None
}

// Wrap a bullet line with a hanging indent so wrapped lines align under the content start.
pub(crate) fn wrap_bullet_line(
    mut line: ratatui::text::Line<'static>,
    indent_spaces: usize,
    bullet: &str,
    width: u16,
) -> Vec<ratatui::text::Line<'static>> {
    use ratatui::style::Style;
    use ratatui::text::Span;
    use unicode_width::UnicodeWidthStr as UWStr;

    let width = width.saturating_sub(1) as usize;
    let mut spans = std::mem::take(&mut line.spans);
    if spans.iter().any(|s| s.content.as_ref().contains('\u{1b}')) {
        line.spans = spans;
        return vec![line];
    }
    let mut i = 0usize;
    if i < spans.len() {
        let t = spans[i].content.as_ref();
        if t.chars().all(|c| c == ' ') {
            i += 1;
        }
    }
    let bullet_style = if i < spans.len() {
        spans[i].style
    } else {
        Style::default()
    };
    if i < spans.len() {
        let bullet_span_text = spans[i].content.as_ref().to_string();
        i += 1;
        if !bullet_span_text.ends_with(' ') && i < spans.len() && spans[i].content.as_ref() == " " {
            i += 1;
        }
    }

    use unicode_segmentation::UnicodeSegmentation;
    let rest_spans = spans.drain(i..).collect::<Vec<_>>();
    let mut clusters: Vec<(String, Style)> = Vec::new();
    for sp in &rest_spans {
        let st = sp.style;
        for g in sp.content.as_ref().graphemes(true) {
            clusters.push((g.to_string(), st));
        }
    }

    let mut leading_content_spaces: usize = 0;
    while leading_content_spaces < clusters.len()
        && (clusters[leading_content_spaces].0 == " "
            || clusters[leading_content_spaces].0 == "\u{3000}")
    {
        leading_content_spaces += 1;
    }

    let bullet_cols = UWStr::width(bullet);
    let gap_after_bullet = 1usize;
    let extra_gap = leading_content_spaces;
    let first_prefix = indent_spaces + bullet_cols + gap_after_bullet + extra_gap;
    let cont_prefix = indent_spaces + bullet_cols + gap_after_bullet + extra_gap;

    let mut out: Vec<ratatui::text::Line<'static>> = Vec::new();
    let mut pos = leading_content_spaces;
    let mut first = true;
    while pos < clusters.len() {
        let avail_cols = if first {
            width.saturating_sub(first_prefix)
        } else {
            width.saturating_sub(cont_prefix)
        } as usize;
        let avail_cols = avail_cols.max(1);

        let mut taken = 0usize;
        let mut cols = 0usize;
        let mut last_space_idx: Option<usize> = None;
        while pos + taken < clusters.len() {
            let (ref g, _) = clusters[pos + taken];
            let w = UWStr::width(g.as_str());
            if cols.saturating_add(w) > avail_cols {
                break;
            }
            cols += w;
            if g == " " || g == "\u{3000}" {
                last_space_idx = Some(pos + taken);
            }
            taken += 1;
            if cols == avail_cols {
                break;
            }
        }

        let (cut_end, next_start) = if pos + taken >= clusters.len() {
            (pos + taken, pos + taken)
        } else if let Some(space_idx) = last_space_idx {
            let mut next = space_idx;
            let mut cut = space_idx;
            while cut > pos && clusters[cut - 1].0 == " " {
                cut -= 1;
            }
            while next < clusters.len() && clusters[next].0 == " " {
                next += 1;
            }
            (cut, next)
        } else {
            (pos + taken, pos + taken)
        };

        if cut_end <= pos {
            let mut p = pos;
            while p < clusters.len() && clusters[p].0 == " " {
                p += 1;
            }
            if p == pos {
                p = pos + 1;
            }
            pos = p;
            continue;
        }

        let slice = &clusters[pos..cut_end];
        let mut seg_spans: Vec<Span<'static>> = Vec::new();
        if first {
            if indent_spaces > 0 {
                seg_spans.push(Span::raw(" ".repeat(indent_spaces)));
            }
            seg_spans.push(Span::styled(bullet.to_string(), bullet_style));
            seg_spans.push(Span::raw("  "));
        } else {
            seg_spans.push(Span::raw(" ".repeat(cont_prefix)));
        }
        let mut cur_style = None::<Style>;
        let mut buf = String::new();
        for (g, st) in slice.iter() {
            if cur_style.map(|cs| cs == *st).unwrap_or(false) {
                buf.push_str(g);
            } else {
                if !buf.is_empty() {
                    seg_spans.push(Span::styled(std::mem::take(&mut buf), cur_style.unwrap()));
                }
                cur_style = Some(*st);
                buf.push_str(g);
            }
        }
        if !buf.is_empty() {
            seg_spans.push(Span::styled(buf, cur_style.unwrap()));
        }
        out.push(ratatui::text::Line::from(seg_spans));
        pos = next_start;
        first = false;
    }

    if out.is_empty() {
        let mut seg_spans: Vec<Span<'static>> = Vec::new();
        if indent_spaces > 0 {
            seg_spans.push(Span::raw(" ".repeat(indent_spaces)));
        }
        seg_spans.push(Span::styled(bullet.to_string(), bullet_style));
        out.push(ratatui::text::Line::from(seg_spans));
    }

    out
}

pub(crate) fn is_horizontal_rule_line(line: &ratatui::text::Line<'_>) -> bool {
    let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    let t = text.trim();
    if t.is_empty() {
        return false;
    }
    let chars: Vec<char> = t.chars().collect();
    let only = |ch: char| chars.iter().all(|c| *c == ch || c.is_whitespace());
    (only('-') && chars.iter().filter(|c| **c == '-').count() >= 3)
        || (only('*') && chars.iter().filter(|c| **c == '*').count() >= 3)
        || (only('_') && chars.iter().filter(|c| **c == '_').count() >= 3)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_text(line: &Line<'_>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn wrap_code_line_moves_wide_grapheme() {
        let line = Line::from(vec![Span::raw("abc界")]);
        let wrapped = wrap_code_line(line, 4);
        let rendered: Vec<String> = wrapped.iter().map(line_text).collect();
        assert_eq!(rendered, vec!["abc", "界"]);
        for text in rendered {
            assert!(unicode_width::UnicodeWidthStr::width(text.as_str()) <= 4);
        }
    }
}
