#![allow(dead_code)]

use std::collections::VecDeque;

use code_core::config::Config;
use ratatui::text::Line;

use crate::markdown;
use crate::render::markdown_utils::is_inside_unclosed_fence;
use crate::render::markdown_utils::strip_empty_fenced_code_blocks;

/// Newline-gated accumulator that renders markdown and commits only fully
/// completed logical lines.
pub(crate) struct MarkdownStreamCollector {
    buffer: String,
    committed_line_count: usize,
    bold_first_sentence: bool,
    // When true, insert an extra newline after the next natural newline
    // boundary to force a section separation without cutting a word mid‑line.
    pending_section_break: bool,
    // Tracks whether we've already evaluated the leading bullet prefix.
    // None => undecided, Some(true) => removed, Some(false) => left intact.
    leading_bullet_state: Option<bool>,
}


impl MarkdownStreamCollector {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            committed_line_count: 0,
            bold_first_sentence: false,
            pending_section_break: false,
            leading_bullet_state: None,
        }
    }

    pub fn new_with_bold_first() -> Self {
        Self {
            buffer: String::new(),
            committed_line_count: 0,
            bold_first_sentence: true,
            pending_section_break: false,
            leading_bullet_state: None,
        }
    }

    pub fn set_bold_first_sentence(&mut self, bold: bool) {
        self.bold_first_sentence = bold;
    }

    /// Returns the number of logical lines that have already been committed
    /// (i.e., previously returned from `commit_complete_lines`).
    pub fn committed_count(&self) -> usize {
        self.committed_line_count
    }

    pub fn has_buffered_content(&self) -> bool {
        !self.buffer.is_empty()
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
        self.committed_line_count = 0;
        // Keep bold_first_sentence setting
        self.pending_section_break = false;
        self.leading_bullet_state = None;
    }

    /// Replace the buffered content and mark that the first `committed_count`
    /// logical lines are already committed.
    pub fn replace_with_and_mark_committed(&mut self, s: &str, committed_count: usize) {
        self.buffer.clear();
        self.buffer.push_str(s);
        self.committed_line_count = committed_count;
        // A full replace cancels any pending break; the new content can include
        // its own spacing.
        self.pending_section_break = false;
        self.leading_bullet_state = None;
        self.strip_leading_bullet_if_first_line();
    }

    pub fn push_delta(&mut self, delta: &str) {
        self.buffer.push_str(delta);
        self.strip_leading_bullet_if_first_line();
        // If we were asked to insert a section break but the buffer didn't end
        // with a newline at the time, defer adding the extra newline until we
        // naturally hit a newline boundary via streaming. This prevents cutting
        // a word mid‑line (observed as missing syllables like "Summarizing" → "Summizing").
        if self.pending_section_break && self.buffer.ends_with('\n') {
            // Ensure exactly one extra blank line (i.e., double newline total)
            if !self.buffer.ends_with("\n\n") {
                self.buffer.push('\n');
            }
            self.pending_section_break = false;
        }
    }

    /// Insert a paragraph/section separator if one is not already present at the
    /// end of the buffer. Ensures the next content starts after a blank line.
    pub fn insert_section_break(&mut self) {
        if self.buffer.is_empty() {
            return;
        }
        // If we're mid-line, insert a newline immediately so upcoming content
        // (e.g., a bold section title) starts on its own line. Then request an
        // extra blank line at the next natural newline boundary to produce a
        // clean paragraph separation without risking mid-word truncation.
        if !self.buffer.ends_with('\n') {
            self.buffer.push('\n');
            self.pending_section_break = true; // will add the second newline later
            return;
        }
        // Already at a boundary: ensure exactly one blank line (double newline)
        if !self.buffer.ends_with("\n\n") {
            self.buffer.push('\n');
        }
        self.pending_section_break = false;
    }

    fn strip_leading_bullet_if_first_line(&mut self) {
        if self.leading_bullet_state.is_some() || self.committed_line_count > 0 {
            return;
        }
        if self.buffer.is_empty() {
            return;
        }

        let mut chars = self.buffer.char_indices();
        let Some((_, first)) = chars.next() else {
            return;
        };
        if first != '-' {
            // Mark as processed so we do not keep re-checking on each delta.
            self.leading_bullet_state = Some(false);
            return;
        }

        match chars.next() {
            Some((second_idx, second_char)) => {
                if matches!(second_char, ' ' | '\t') {
                    let drain_end = second_idx + second_char.len_utf8();
                    self.buffer.drain(..drain_end);
                    self.leading_bullet_state = Some(true);
                } else if matches!(second_char, '\n' | '\r') {
                    self.buffer.drain(..second_idx);
                    self.leading_bullet_state = Some(true);
                } else if second_char.is_whitespace() {
                    let drain_end = second_idx + second_char.len_utf8();
                    self.buffer.drain(..drain_end);
                    self.leading_bullet_state = Some(true);
                } else {
                    self.leading_bullet_state = Some(false);
                }
            }
            None => {
                // Only '-' received so far; wait for more context.
            }
        }
    }

    /// Render the full buffer and return only the newly completed logical lines
    /// since the last commit. When the buffer does not end with a newline, the
    /// final rendered line is considered incomplete and is not emitted.
    pub fn commit_complete_lines(&mut self, config: &Config) -> Vec<Line<'static>> {
        // In non-test builds, unwrap an outer ```markdown fence during commit as well,
        // so fence markers never appear in streamed history.
        let source = unwrap_markdown_language_fence_if_enabled(self.buffer.clone());
        let source = strip_empty_fenced_code_blocks(&source);

        let mut rendered: Vec<Line<'static>> = Vec::new();
        if self.bold_first_sentence {
            markdown::append_markdown_with_bold_first(&source, &mut rendered, config);
        } else {
            markdown::append_markdown(&source, &mut rendered, config);
        }

        let mut complete_line_count = rendered.len();
        if complete_line_count > 0 {
            let last = &rendered[complete_line_count - 1];
            // Do not drop a trailing blank when it is part of a code block; that
            // would cause the blank to be emitted later next to a previously
            // committed plain blank separator, producing a visible double-gap
            // (one painted, one unpainted).
            let is_blank = crate::render::line_utils::is_blank_line_spaces_only(last);
            let is_code_bg = crate::render::line_utils::is_code_block_painted(last);
            if is_blank && !is_code_bg {
                complete_line_count -= 1;
            }
        }
        // Heuristic: if the buffer ends with a double newline and the last non-blank
        // rendered line looks like a list bullet with inline content (e.g., "- item"),
        // defer committing that line. Subsequent context (e.g., another list item)
        // can cause the renderer to split the bullet marker and text into separate
        // logical lines ("- " then "item"), which would otherwise duplicate content.
        if self.buffer.ends_with("\n\n") && complete_line_count > 0 {
            let last = &rendered[complete_line_count - 1];
            let mut text = String::new();
            for s in &last.spans {
                text.push_str(&s.content);
            }
            if text.starts_with("- ") && text.trim() != "-" {
                complete_line_count = complete_line_count.saturating_sub(1);
            }
        }
        if !self.buffer.ends_with('\n') {
            complete_line_count = complete_line_count.saturating_sub(1);
            // If we're inside an unclosed fenced code block, also drop the
            // last rendered line to avoid committing a partial code line.
            if is_inside_unclosed_fence(&source) {
                complete_line_count = complete_line_count.saturating_sub(1);
            }
            // If the next (incomplete) line appears to begin a list item,
            // also defer the previous completed line because the renderer may
            // retroactively treat it as part of the list (e.g., ordered list item 1).
            if let Some(last_nl) = source.rfind('\n') {
                let tail = &source[last_nl + 1..];
                if starts_with_list_marker(tail) {
                    complete_line_count = complete_line_count.saturating_sub(1);
                }
            }
        }

        // Conservatively withhold trailing list-like lines (unordered or ordered)
        // because streaming mid-item can cause the renderer to later split or
        // restructure them (e.g., duplicating content or separating the marker).
        // Only defers lines at the end of the out slice so previously committed
        // lines remain stable.
        if complete_line_count > self.committed_line_count {
            let mut safe_count = complete_line_count;
            while safe_count > self.committed_line_count {
                let l = &rendered[safe_count - 1];
                let mut text = String::new();
                for s in &l.spans {
                    text.push_str(&s.content);
                }
                let listish = is_potentially_volatile_list_line(&text);
                if listish {
                    safe_count -= 1;
                    continue;
                }
                break;
            }
            complete_line_count = safe_count;
        }

        if self.committed_line_count >= complete_line_count {
            return Vec::new();
        }

        let out_slice = &rendered[self.committed_line_count..complete_line_count];
        // Strong correctness: while a fenced code block is open (no closing fence yet),
        // do not emit any new lines from inside it. Wait until the fence closes to emit
        // the entire block together. This avoids stray backticks and misformatted content.
        if is_inside_unclosed_fence(&source) {
            return Vec::new();
        }

        // Additional conservative hold-back: if exactly one short, plain word
        // line would be emitted, defer it. This avoids committing a lone word
        // that might become the first ordered-list item once the next delta
        // arrives (e.g., next line starts with "2 " or "2. ").
        if out_slice.len() == 1 {
            let mut s = String::new();
            for sp in &out_slice[0].spans {
                s.push_str(&sp.content);
            }
            if is_short_plain_word(&s) {
                return Vec::new();
            }
        }

        let out = out_slice.to_vec();
        self.committed_line_count = complete_line_count;
        out
    }

    /// Soft-commit: emit newly rendered lines since the last commit, allowing the
    /// trailing (incomplete) line when appropriate to improve perceived latency.
    ///
    /// - If `relax_code_holdback` is true and we're inside an unclosed fence,
    ///   allow committing but drop the very last partial line to avoid jitter.
    /// - If `relax_list_holdback` is true, only withhold truly bare list markers
    ///   at the tail (e.g., "-", "- ", "*", "* ", or "1.").
    pub fn commit_soft_lines(
        &mut self,
        config: &Config,
        relax_list_holdback: bool,
        relax_code_holdback: bool,
    ) -> Vec<Line<'static>> {
        let source = unwrap_markdown_language_fence_if_enabled(self.buffer.clone());
        let source = strip_empty_fenced_code_blocks(&source);

        let mut rendered: Vec<Line<'static>> = Vec::new();
        if self.bold_first_sentence {
            markdown::append_markdown_with_bold_first(&source, &mut rendered, config);
        } else {
            markdown::append_markdown(&source, &mut rendered, config);
        }
        if self.committed_line_count >= rendered.len() {
            return Vec::new();
        }

        let in_open_fence = is_inside_unclosed_fence(&source);
        if in_open_fence && !relax_code_holdback {
            return Vec::new();
        }
        let mut end = rendered.len();
        if in_open_fence && relax_code_holdback {
            end = end.saturating_sub(1); // avoid partial last code line
        }

        if relax_list_holdback && end > self.committed_line_count {
            let last = &rendered[end - 1];
            let mut s = String::new();
            for sp in &last.spans {
                s.push_str(&sp.content);
            }
            if is_bare_list_marker(&s) {
                end = end.saturating_sub(1);
            }
        }

        if end <= self.committed_line_count {
            return Vec::new();
        }
        let out = rendered[self.committed_line_count..end].to_vec();
        self.committed_line_count = end;
        out
    }

    /// Finalize the stream: emit all remaining lines beyond the last commit.
    /// If the buffer does not end with a newline, a temporary one is appended
    /// for rendering. Optionally unwraps ```markdown language fences in
    /// non-test builds.
    pub fn finalize_and_drain(&mut self, config: &Config) -> Vec<Line<'static>> {
        let mut source: String = self.buffer.clone();
        if !source.ends_with('\n') {
            source.push('\n');
        }
        let source = unwrap_markdown_language_fence_if_enabled(source);
        let source = strip_empty_fenced_code_blocks(&source);

        let mut rendered: Vec<Line<'static>> = Vec::new();
        if self.bold_first_sentence {
            markdown::append_markdown_with_bold_first(&source, &mut rendered, config);
        } else {
            markdown::append_markdown(&source, &mut rendered, config);
        }

        let out = if self.committed_line_count >= rendered.len() {
            Vec::new()
        } else {
            rendered[self.committed_line_count..].to_vec()
        };

        // Reset collector state for next stream.
        self.clear();
        out
    }

    /// Return the full source that would be rendered at finalize time without mutating state.
    pub fn full_render_source_preview(&self) -> String {
        let mut source: String = self.buffer.clone();
        if !source.ends_with('\n') {
            source.push('\n');
        }
        let source = unwrap_markdown_language_fence_if_enabled(source);
        let source = strip_empty_fenced_code_blocks(&source);
        source
    }

    /// Returns true if the internal buffer currently ends with a newline.
    pub fn ends_with_newline(&self) -> bool {
        self.buffer.ends_with('\n')
    }

    /// Render a preview of the current buffer into lines without mutating
    /// internal counters. Unlike `finalize_and_drain`, this does not append a
    /// synthetic trailing newline, so the preview reflects what a soft-commit
    /// would render at this moment.
    pub fn render_preview_lines(&self, config: &Config) -> Vec<Line<'static>> {
        let source = unwrap_markdown_language_fence_if_enabled(self.buffer.clone());
        let source = strip_empty_fenced_code_blocks(&source);
        let mut rendered: Vec<Line<'static>> = Vec::new();
        if self.bold_first_sentence {
            markdown::append_markdown_with_bold_first(&source, &mut rendered, config);
        } else {
            markdown::append_markdown(&source, &mut rendered, config);
        }
        rendered
    }
}

#[inline]
fn is_potentially_volatile_list_line(text: &str) -> bool {
    let t = text.trim_end();
    if t == "-" || t == "*" || t == "- " || t == "* " {
        return true;
    }
    if t.starts_with("- ") || t.starts_with("* ") {
        return true;
    }
    // ordered list like "1. " or "23. "
    let mut it = t.chars().peekable();
    let mut saw_digit = false;
    while let Some(&ch) = it.peek() {
        if ch.is_ascii_digit() {
            saw_digit = true;
            it.next();
            continue;
        }
        break;
    }
    if saw_digit && it.peek() == Some(&'.') {
        // consume '.'
        it.next();
        if it.peek() == Some(&' ') {
            return true;
        }
    }
    false
}

#[inline]
fn is_bare_list_marker(text: &str) -> bool {
    let t = text.trim();
    if t == "-" || t == "-" || t == "*" || t == "*" {
        return true;
    }
    if t == "-" || t == "- " || t == "*" || t == "* " {
        return true;
    }
    // ordered like "1." possibly followed by a single space
    let mut it = t.chars().peekable();
    let mut saw_digit = false;
    while let Some(&ch) = it.peek() {
        if ch.is_ascii_digit() {
            saw_digit = true;
            it.next();
        } else {
            break;
        }
    }
    if !saw_digit {
        return false;
    }
    if it.peek() == Some(&'.') {
        it.next();
        return it.peek().is_none() || it.peek() == Some(&' ');
    }
    false
}

#[inline]
fn starts_with_list_marker(text: &str) -> bool {
    let t = text.trim_start();
    if t.starts_with("- ") || t.starts_with("* ") || t.starts_with("-\t") || t.starts_with("*\t") {
        return true;
    }
    // ordered list marker like "1 ", "1. ", "23 ", "23. "
    let mut it = t.chars().peekable();
    let mut saw_digit = false;
    while let Some(&ch) = it.peek() {
        if ch.is_ascii_digit() {
            saw_digit = true;
            it.next();
        } else {
            break;
        }
    }
    if !saw_digit {
        return false;
    }
    match it.peek() {
        Some('.') => {
            it.next();
            matches!(it.peek(), Some(' '))
        }
        Some(' ') => true,
        _ => false,
    }
}

#[inline]
fn is_short_plain_word(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() || t.len() > 5 {
        return false;
    }
    t.chars().all(|c| c.is_alphanumeric())
}

/// fence helpers are provided by `crate::render::markdown_utils`
fn unwrap_markdown_language_fence_if_enabled(s: String) -> String {
    // Best-effort unwrap of a single outer fenced markdown block.
    // Recognizes common forms like ```markdown, ```md (any case), optional
    // surrounding whitespace, and flexible trailing newlines/CRLF.
    // If the block is not recognized, return the input unchanged.
    let lines = s.lines().collect::<Vec<_>>();
    if lines.len() < 2 {
        return s;
    }

    // Identify opening fence and language.
    let open = lines.first().map(|l| l.trim_start()).unwrap_or("");
    if !open.starts_with("```") {
        return s;
    }
    let lang = open.trim_start_matches("```").trim();
    let is_markdown_lang = lang.eq_ignore_ascii_case("markdown") || lang.eq_ignore_ascii_case("md");
    if !is_markdown_lang {
        return s;
    }

    // Find the last non-empty line and ensure it is a closing fence.
    let mut last_idx = lines.len() - 1;
    while last_idx > 0 && lines[last_idx].trim().is_empty() {
        last_idx -= 1;
    }
    if lines[last_idx].trim() != "```" {
        return s;
    }

    // Reconstruct the inner content between the fences.
    let mut out = String::new();
    for l in lines.iter().take(last_idx).skip(1) {
        out.push_str(l);
        out.push('\n');
    }
    out
}

pub(crate) struct StepResult {
    pub history: Vec<Line<'static>>, // lines to insert into history this step
}

/// Streams already-rendered rows into history while computing the newest K
/// rows to show in a live overlay.
pub(crate) struct AnimatedLineStreamer {
    queue: VecDeque<Line<'static>>,
}

impl AnimatedLineStreamer {
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
        }
    }

    pub fn clear(&mut self) {
        self.queue.clear();
    }

    pub fn enqueue(&mut self, lines: Vec<Line<'static>>) {
        for l in lines {
            self.queue.push_back(l);
        }
    }

    pub fn step(&mut self) -> StepResult {
        let mut history = Vec::new();
        // Move exactly one per tick to animate gradual insertion.
        let burst = if self.queue.is_empty() { 0 } else { 1 };
        for _ in 0..burst {
            if let Some(l) = self.queue.pop_front() {
                history.push(l);
            }
        }

        StepResult { history }
    }

    pub fn drain_all(&mut self) -> StepResult {
        let mut history = Vec::new();
        while let Some(l) = self.queue.pop_front() {
            history.push(l);
        }
        StepResult { history }
    }

    pub fn is_idle(&self) -> bool {
        self.queue.is_empty()
    }
}
