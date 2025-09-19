use codex_core::protocol::TokenUsage;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use crossterm::event::KeyModifiers;
use ratatui::buffer::Buffer;
use ratatui::layout::Constraint;
use ratatui::layout::Layout;
use ratatui::layout::Margin;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui::widgets::Borders;
use ratatui::widgets::StatefulWidgetRef;
use ratatui::widgets::WidgetRef;

use super::chat_composer_history::ChatComposerHistory;
use super::command_popup::CommandItem;
use super::command_popup::CommandPopup;
use super::file_search_popup::FileSearchPopup;
use super::paste_burst::PasteBurst;
use crate::slash_command::SlashCommand;
use codex_protocol::custom_prompts::CustomPrompt;

use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::textarea::TextArea;
use crate::bottom_pane::textarea::TextAreaState;
use crate::clipboard_paste::normalize_pasted_path;
use crate::clipboard_paste::paste_image_to_temp_png;
use crate::clipboard_paste::try_decode_base64_image_to_temp_png;
use codex_file_search::FileMatch;
use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;
use std::time::Instant;

// Dynamic placeholder rendered when the composer is empty.
/// If the pasted content exceeds this number of characters, replace it with a
/// placeholder in the UI.
const LARGE_PASTE_CHAR_THRESHOLD: usize = 1000;

/// Result returned when the user interacts with the text area.
#[derive(Debug, PartialEq)]
pub enum InputResult {
    Submitted(String),
    Command(SlashCommand),
    ScrollUp,
    ScrollDown,
    None,
}

struct TokenUsageInfo {
    total_token_usage: TokenUsage,
    last_token_usage: TokenUsage,
    model_context_window: Option<u64>,
    /// Baseline token count present in the context before the user's first
    /// message content is considered. This is used to normalize the
    /// "context left" percentage so it reflects the portion the user can
    /// influence rather than fixed prompt overhead (system prompt, tool
    /// instructions, etc.).
    ///
    /// Preferred source is `cached_input_tokens` from the first turn (when
    /// available), otherwise we fall back to 0.
    initial_prompt_tokens: u64,
}

// Format an integer with thousands separators (e.g., 125,654).
fn format_with_thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    let mut count = 0usize;
    for ch in s.chars().rev() {
        if count != 0 && count % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
        count += 1;
    }
    out.chars().rev().collect()
}

pub(crate) struct ChatComposer {
    textarea: TextArea,
    textarea_state: RefCell<TextAreaState>,
    active_popup: ActivePopup,
    app_event_tx: AppEventSender,
    history: ChatComposerHistory,
    ctrl_c_quit_hint: bool,
    #[allow(dead_code)]
    use_shift_enter_hint: bool,
    dismissed_file_popup_token: Option<String>,
    current_file_query: Option<String>,
    // Tracks a one-off Tab-triggered file search. When set, we will only
    // create/show a popup if the results are non-empty to avoid flicker.
    pending_tab_file_query: Option<String>,
    pending_pastes: Vec<(String, String)>,
    token_usage_info: Option<TokenUsageInfo>,
    has_focus: bool,
    has_chat_history: bool,
    /// Tracks whether the user has typed or pasted any content since startup.
    typed_anything: bool,
    is_task_running: bool,
    // Current status message to display when task is running
    status_message: String,
    // Animation thread for spinning icon when task is running
    animation_running: Option<Arc<AtomicBool>>,
    using_chatgpt_auth: bool,
    // Ephemeral footer notice and its expiry
    footer_notice: Option<(String, std::time::Instant)>,
    // Persistent/ephemeral access-mode indicator shown on the left
    access_mode_label: Option<String>,
    access_mode_label_expiry: Option<std::time::Instant>,
    access_mode_hint_expiry: Option<std::time::Instant>,
    // Footer hint visibility flags
    show_reasoning_hint: bool,
    show_diffs_hint: bool,
    reasoning_shown: bool,
    // Config-driven labels for footer hints
    help_hint_label: String,
    reasoning_hint_label: String,
    diffs_hint_label: String,
    // Sticky flag: after a chat ScrollUp, make the very next Down trigger
    // chat ScrollDown instead of moving within the textarea, unless another
    // key is pressed in between.
    next_down_scrolls_history: bool,
    // Detect and coalesce paste bursts for smoother UX
    paste_burst: PasteBurst,
}

/// Popup state – at most one can be visible at any time.
enum ActivePopup {
    None,
    Command(CommandPopup),
    File(FileSearchPopup),
}

impl ChatComposer {
    pub fn new(
        has_input_focus: bool,
        app_event_tx: AppEventSender,
        enhanced_keys_supported: bool,
        using_chatgpt_auth: bool,
        help_hint_label: String,
        reasoning_hint_label: String,
        diffs_hint_label: String,
    ) -> Self {
        let use_shift_enter_hint = enhanced_keys_supported;

        Self {
            textarea: TextArea::new(),
            textarea_state: RefCell::new(TextAreaState::default()),
            active_popup: ActivePopup::None,
            app_event_tx,
            history: ChatComposerHistory::new(),
            ctrl_c_quit_hint: false,
            use_shift_enter_hint,
            dismissed_file_popup_token: None,
            current_file_query: None,
            pending_tab_file_query: None,
            pending_pastes: Vec::new(),
            token_usage_info: None,
            has_focus: has_input_focus,
            has_chat_history: false,
            typed_anything: false,
            // no double‑Esc handling here; App manages Esc policy
            is_task_running: false,
            status_message: String::from("coding"),
            animation_running: None,
            using_chatgpt_auth,
            footer_notice: None,
            access_mode_label: None,
            access_mode_label_expiry: None,
            access_mode_hint_expiry: None,
            show_reasoning_hint: false,
            show_diffs_hint: false,
            reasoning_shown: false,
            help_hint_label,
            reasoning_hint_label,
            diffs_hint_label,
            next_down_scrolls_history: false,
            paste_burst: PasteBurst::default(),
        }
    }

    /// Returns true if the input starts with a slash command and the cursor
    /// is positioned within the command head (i.e., before the first
    /// whitespace on the first line). Used to decide whether to keep the
    /// slash-command popup active and to suppress file completion.
    fn is_cursor_in_slash_command_head(&self) -> bool {
        let text = self.textarea.text();
        if text.is_empty() { return false; }
        let cursor = self.textarea.cursor();
        let first_line_end = text.find('\n').unwrap_or(text.len());
        let first_line = &text[..first_line_end];
        if !first_line.starts_with('/') { return false; }
        let head_end = first_line
            .char_indices()
            .find(|(_, c)| c.is_whitespace())
            .map(|(i, _)| i)
            .unwrap_or(first_line_end);
        cursor <= head_end
    }

    pub fn set_has_chat_history(&mut self, has_history: bool) {
        self.has_chat_history = has_history;
    }

    pub fn set_task_running(&mut self, running: bool) {
        self.is_task_running = running;

        if running {
            // Start animation thread if not already running
            if self.animation_running.is_none() {
                let animation_flag = Arc::new(AtomicBool::new(true));
                let animation_flag_clone = Arc::clone(&animation_flag);
                let app_event_tx_clone = self.app_event_tx.clone();

                // Drive redraws at the spinner's native cadence with a
                // phase‑aligned, monotonic scheduler to minimize drift and
                // reduce perceived frame skipping under load. We purposely
                // avoid very small intervals to keep CPU impact low.
                thread::spawn(move || {
                    use std::time::Instant;
                    // Default to ~120ms if spinner state is not yet initialized
                    let default_ms: u64 = 120;
                    // Clamp to a sane floor so we never busy loop if a custom spinner
                    // has an extremely small interval configured.
                    let min_ms: u64 = 60; // ~16 FPS upper bound for this thread

                    // Determine the target period. If the user changes the spinner
                    // while running, we'll still get correct visual output because
                    // frames are time‑based at render; this cadence simply requests
                    // redraws.
                    let period_ms = crate::spinner::current_spinner()
                        .interval_ms
                        .max(min_ms)
                        .max(1);
                    let period = Duration::from_millis(period_ms); // fallback uses default below if needed

                    let mut next = Instant::now()
                        .checked_add(if period_ms == 0 { Duration::from_millis(default_ms) } else { period })
                        .unwrap_or_else(Instant::now);

                    while animation_flag_clone.load(Ordering::Relaxed) {
                        let now = Instant::now();
                        if now < next {
                            let sleep_dur = next - now;
                            thread::sleep(sleep_dur);
                        } else {
                            // If we're late (system busy), request a redraw immediately.
                            app_event_tx_clone.send(crate::app_event::AppEvent::RequestRedraw);
                            // Step the schedule forward by whole periods to avoid
                            // bursty catch‑up redraws.
                            let mut target = next;
                            while target <= now {
                                if let Some(t) = target.checked_add(period) { target = t; } else { break; }
                            }
                            next = target;
                        }
                    }
                });

                self.animation_running = Some(animation_flag);
            }
        } else {
            // Stop animation thread
            if let Some(animation_flag) = self.animation_running.take() {
                animation_flag.store(false, Ordering::Relaxed);
            }
        }
    }

    pub fn update_status_message(&mut self, message: String) {
        self.status_message = Self::map_status_message(&message);
    }

    pub fn flash_footer_notice(&mut self, text: String) {
        let expiry = std::time::Instant::now() + std::time::Duration::from_secs(2);
        self.footer_notice = Some((text, expiry));
    }

    /// Show a footer notice for a specific duration.
    pub fn flash_footer_notice_for(&mut self, text: String, dur: std::time::Duration) {
        let expiry = std::time::Instant::now() + dur;
        self.footer_notice = Some((text, expiry));
    }

    // Control footer hint visibility
    pub fn set_show_reasoning_hint(&mut self, show: bool) {
        if self.show_reasoning_hint != show {
            self.show_reasoning_hint = show;
        }
    }

    pub fn set_show_diffs_hint(&mut self, show: bool) {
        if self.show_diffs_hint != show {
            self.show_diffs_hint = show;
        }
    }

    pub fn set_access_mode_label(&mut self, label: Option<String>) {
        self.access_mode_label = label;
        self.access_mode_label_expiry = None;
        self.access_mode_hint_expiry = None;
    }
    pub fn set_access_mode_label_ephemeral(&mut self, label: String, dur: std::time::Duration) {
        self.access_mode_label = Some(label);
        let expiry = std::time::Instant::now() + dur;
        self.access_mode_label_expiry = Some(expiry);
        self.access_mode_hint_expiry = Some(expiry);
    }
    pub fn set_access_mode_hint_for(&mut self, dur: std::time::Duration) {
        self.access_mode_hint_expiry = Some(std::time::Instant::now() + dur);
    }

    pub fn set_reasoning_state(&mut self, shown: bool) {
        self.reasoning_shown = shown;
    }

    // Map technical status messages to user-friendly ones
    fn map_status_message(technical_message: &str) -> String {
        let lower = technical_message.to_lowercase();

        // Thinking/reasoning patterns
        if lower.contains("reasoning")
            || lower.contains("thinking")
            || lower.contains("planning")
            || lower.contains("waiting for model")
            || lower.contains("model")
        {
            "Thinking".to_string()
        }
        // Tool/command execution patterns
        else if lower.contains("tool")
            || lower.contains("command")
            || lower.contains("running command")
            || lower.contains("running")
            || lower.contains("bash")
            || lower.contains("shell")
        {
            "Using tools".to_string()
        }
        // Browser activity
        else if lower.contains("browser")
            || lower.contains("chrome")
            || lower.contains("cdp")
            || lower.contains("navigate")
            || lower.contains("url")
            || lower.contains("screenshot")
        {
            "Browsing".to_string()
        }
        // Multi-agent orchestration
        else if lower.contains("agent")
            || lower.contains("agents")
            || lower.contains("orchestrating")
            || lower.contains("coordinating")
        {
            "Agents".to_string()
        }
        // Response generation patterns
        else if lower.contains("generating")
            || lower.contains("responding")
            || lower.contains("streaming")
            || lower.contains("writing response")
            || lower.contains("assistant")
            || lower.contains("chat completions")
            || lower.contains("completion")
        {
            "Responding".to_string()
        }
        // Transient network/stream retry patterns → keep spinner visible with a
        // clear reconnecting message so the user knows we are still working.
        else if lower.contains("retrying")
            || lower.contains("reconnecting")
            || lower.contains("disconnected")
            || lower.contains("stream error")
            || lower.contains("stream closed")
            || lower.contains("timeout")
        {
            "Reconnecting".to_string()
        }
        // File/code editing patterns
        else if lower.contains("editing")
            || lower.contains("writing")
            || lower.contains("modifying")
            || lower.contains("creating file")
            || lower.contains("updating")
            || lower.contains("patch")
        {
            "Coding".to_string()
        }
        // Catch some common technical terms
        else if lower.contains("processing") || lower.contains("analyzing") {
            "Thinking".to_string()
        } else if lower.contains("reading") || lower.contains("searching") {
            "Reading".to_string()
        } else {
            // Default fallback - use "working" for unknown status
            "Working".to_string()
        }
    }

    pub fn desired_height(&self, width: u16) -> u16 {
        // Calculate hint/popup height
        let hint_height = match &self.active_popup {
            ActivePopup::None => 1u16,
            ActivePopup::Command(c) => c.calculate_required_height(),
            ActivePopup::File(c) => c.calculate_required_height(),
        };

        // IMPORTANT: `width` here is the full BottomPane width. Subtract the
        // configured outer padding, border, and inner padding to match wrapping.
        let content_width = width.saturating_sub(crate::layout_consts::COMPOSER_CONTENT_WIDTH_OFFSET);
        let content_lines = self.textarea.desired_height(content_width).max(1); // At least 1 line

        // Total input height: content + border (2) only, no vertical padding
        // Minimum of 3 ensures at least 1 visible line with border
        let input_height = (content_lines + 2).max(3);

        input_height + hint_height
    }

    pub fn cursor_pos(&self, area: Rect) -> Option<(u16, u16)> {
        // Split area: textarea with border at top, hints/popup at bottom
        let hint_height = if matches!(self.active_popup, ActivePopup::None) {
            1
        } else {
            match &self.active_popup {
                ActivePopup::Command(popup) => popup.calculate_required_height(),
                ActivePopup::File(popup) => popup.calculate_required_height(),
                ActivePopup::None => 1,
            }
        };
        // Calculate dynamic height based on content
        let content_width = area.width.saturating_sub(4); // Account for border and padding
        let content_lines = self.textarea.desired_height(content_width).max(1);
        let desired_input_height = (content_lines + 2).max(3); // Parent layout enforces max

        // Use desired height but don't exceed available space
        let input_height = desired_input_height.min(area.height.saturating_sub(hint_height));
        let [input_area, _] = Layout::vertical([
            Constraint::Length(input_height),
            Constraint::Length(hint_height),
        ])
        .areas(area);

        // Get inner area of the bordered input box
        let input_block = Block::default().borders(Borders::ALL);
        let textarea_rect = input_block.inner(input_area);

        // Apply the same inner padding as in render (horizontal only).
        let padded_textarea_rect = textarea_rect.inner(Margin::new(crate::layout_consts::COMPOSER_INNER_HPAD.into(), 0));

        let state = self.textarea_state.borrow();
        self.textarea
            .cursor_pos_with_state(padded_textarea_rect, &state)
    }

    /// Returns true if the composer currently contains no user input.
    #[allow(dead_code)]
    pub(crate) fn is_empty(&self) -> bool {
        self.textarea.is_empty()
    }

    /// Update the cached *context-left* percentage and refresh the placeholder
    /// text. The UI relies on the placeholder to convey the remaining
    /// context when the composer is empty.
    pub(crate) fn set_token_usage(
        &mut self,
        total_token_usage: TokenUsage,
        last_token_usage: TokenUsage,
        model_context_window: Option<u64>,
    ) {
        let initial_prompt_tokens = self
            .token_usage_info
            .as_ref()
            .map(|info| info.initial_prompt_tokens)
            .unwrap_or_else(|| last_token_usage.cached_input_tokens.unwrap_or(0));

        self.token_usage_info = Some(TokenUsageInfo {
            total_token_usage,
            last_token_usage,
            model_context_window,
            initial_prompt_tokens,
        });
    }

    /// Record the history metadata advertised by `SessionConfiguredEvent` so
    /// that the composer can navigate cross-session history.
    pub(crate) fn set_history_metadata(&mut self, log_id: u64, entry_count: usize) {
        self.history.set_metadata(log_id, entry_count);
    }

    /// Integrate an asynchronous response to an on-demand history lookup. If
    /// the entry is present and the offset matches the current cursor we
    /// immediately populate the textarea.
    pub(crate) fn on_history_entry_response(
        &mut self,
        log_id: u64,
        offset: usize,
        entry: Option<String>,
    ) -> bool {
        let Some(text) = self.history.on_entry_response(log_id, offset, entry) else {
            return false;
        };
        self.textarea.set_text(&text);
        self.textarea.set_cursor(0);
        true
    }

    pub fn handle_paste(&mut self, pasted: String) -> bool {
        let char_count = pasted.chars().count();
        // If the pasted text looks like a base64/data-URI image, decode it and insert as a path.
        if let Ok((path, info)) = try_decode_base64_image_to_temp_png(&pasted) {
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("image.png");
            let placeholder = format!("[image: {}]", filename);
            // Insert placeholder and notify chat widget about the mapping.
            self.textarea.insert_str(&placeholder);
            self.textarea.insert_str(" ");
            self.typed_anything = true; // Mark that user has interacted via paste
            self.app_event_tx
                .send(crate::app_event::AppEvent::RegisterPastedImage { placeholder: placeholder.clone(), path });
            self.flash_footer_notice(format!("Added image {}x{} (PNG)", info.width, info.height));
        } else if char_count > LARGE_PASTE_CHAR_THRESHOLD {
            let placeholder = format!("[Pasted Content {char_count} chars]");
            self.textarea.insert_str(&placeholder);
            self.pending_pastes.push((placeholder, pasted));
            self.typed_anything = true; // Mark that user has interacted via paste
        } else if self.handle_paste_image_path(pasted.clone()) {
            self.textarea.insert_str(" ");
            self.typed_anything = true; // Mark that user has interacted via paste
        } else if pasted.trim().is_empty() {
            // No textual content pasted — try reading an image directly from the OS clipboard.
            match paste_image_to_temp_png() {
                Ok((path, info)) => {
                    let filename = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("image.png");
                    let placeholder = format!("[image: {}]", filename);
                    self.textarea.insert_str(&placeholder);
                    self.textarea.insert_str(" ");
                    self.typed_anything = true; // Mark that user has interacted via paste
                    self.app_event_tx
                        .send(crate::app_event::AppEvent::RegisterPastedImage { placeholder: placeholder.clone(), path });
                    // Give a small visual confirmation in the footer.
                    self.flash_footer_notice(format!(
                        "Added image {}x{} (PNG)",
                        info.width, info.height
                    ));
                }
                Err(_) => {
                    // Fall back to doing nothing special; keep composer unchanged.
                }
            }
        } else {
            self.textarea.insert_str(&pasted);
            self.typed_anything = true; // Mark that user has interacted via paste
        }
        self.sync_command_popup();
        if matches!(self.active_popup, ActivePopup::Command(_)) {
            self.dismissed_file_popup_token = None;
        } else {
            self.sync_file_search_popup();
        }
        true
    }

    /// Heuristic handling for pasted paths: if the pasted text looks like a
    /// filesystem path (including file:// URLs and Windows paths), insert the
    /// normalized path directly into the composer and return true. The caller
    /// will add a trailing space to separate from subsequent input.
    fn handle_paste_image_path(&mut self, pasted: String) -> bool {
        if let Some(path) = normalize_pasted_path(&pasted) {
            // Insert the normalized path verbatim. We don't attempt to load the
            // file or special-case images here; higher layers handle attachments.
            self.textarea.insert_str(&path.to_string_lossy());
            return true;
        }
        false
    }


    /// Clear all composer input and reset transient state like pending pastes
    /// and history navigation.
    pub(crate) fn clear_text(&mut self) {
        self.textarea.set_text("");
        self.pending_pastes.clear();
        self.history.reset_navigation();
    }

    #[allow(dead_code)]
    pub(crate) fn flush_paste_burst_if_due(&mut self) -> bool {
        let now = Instant::now();
        if let Some(pasted) = self.paste_burst.flush_if_due(now) {
            let _ = self.handle_paste(pasted);
            return true;
        }
        false
    }

    pub(crate) fn is_in_paste_burst(&self) -> bool {
        self.paste_burst.is_active()
    }

    pub(crate) fn recommended_paste_flush_delay() -> Duration {
        PasteBurst::recommended_flush_delay()
    }

    /// Integrate results from an asynchronous file search.
    pub(crate) fn on_file_search_result(&mut self, query: String, matches: Vec<FileMatch>) {
        // Handle one-off Tab-triggered case first: only open if matches exist.
        if self.pending_tab_file_query.as_ref() == Some(&query) {
            // Clear pending regardless of result to avoid repeats.
            self.pending_tab_file_query = None;

            if matches.is_empty() {
                return; // do not open popup when no matches to avoid flicker
            }

            match &mut self.active_popup {
                ActivePopup::File(popup) => popup.set_matches(&query, matches),
                _ => {
                    let mut popup = FileSearchPopup::new();
                    popup.set_query(&query);
                    popup.set_matches(&query, matches);
                    self.active_popup = ActivePopup::File(popup);
                }
            }
            self.current_file_query = Some(query);
            self.dismissed_file_popup_token = None;
            return;
        }

        // Otherwise, only apply if user is still editing a token matching the query
        // and that token qualifies for auto-trigger (i.e., @ or ./).
        let current_opt = Self::current_completion_token(&self.textarea);
        let Some(current_token) = current_opt else { return; };
        if !current_token.starts_with(&query) { return; }

        if let ActivePopup::File(popup) = &mut self.active_popup {
            popup.set_matches(&query, matches);
        }
    }

    pub fn set_ctrl_c_quit_hint(&mut self, show: bool, has_focus: bool) {
        self.ctrl_c_quit_hint = show;
        self.set_has_focus(has_focus);
    }

    pub(crate) fn insert_str(&mut self, text: &str) {
        self.textarea.insert_str(text);
        self.typed_anything = true; // Mark that user has interacted via programmatic insertion
        self.sync_command_popup();
        self.sync_file_search_popup();
    }

    /// Close the file-search popup if it is currently active. Returns true if closed.
    pub(crate) fn close_file_popup_if_active(&mut self) -> bool {
        match self.active_popup {
            ActivePopup::File(_) => {
                self.active_popup = ActivePopup::None;
                true
            }
            _ => false,
        }
    }

    /// Handle a key event coming from the main UI.
    pub fn handle_key_event(&mut self, key_event: KeyEvent) -> (InputResult, bool) {
        // Any non-Down key clears the sticky flag; handled before popup routing
        if !matches!(key_event.code, KeyCode::Down) {
            self.next_down_scrolls_history = false;
        }
        let result = match &mut self.active_popup {
            ActivePopup::Command(_) => self.handle_key_event_with_slash_popup(key_event),
            ActivePopup::File(_) => self.handle_key_event_with_file_popup(key_event),
            ActivePopup::None => self.handle_key_event_without_popup(key_event),
        };

        // Update (or hide/show) popup after processing the key.
        self.sync_command_popup();
        if matches!(self.active_popup, ActivePopup::Command(_)) {
            self.dismissed_file_popup_token = None;
        } else {
            self.sync_file_search_popup();
        }

        result
    }

    // popup_active removed; callers use explicit state or rely on App policy.

    /// Handle key event when the slash-command popup is visible.
    fn handle_key_event_with_slash_popup(&mut self, key_event: KeyEvent) -> (InputResult, bool) {
        let ActivePopup::Command(popup) = &mut self.active_popup else {
            unreachable!();
        };

        match key_event {
            // Allow Shift+Up to navigate history even when slash popup is active.
            KeyEvent { code: KeyCode::Up, modifiers, .. } => {
                if modifiers.contains(KeyModifiers::SHIFT) {
                    return self.handle_key_event_without_popup(key_event);
                }
                // If there are 0 or 1 items, let Up behave normally (cursor/history/scroll)
                if popup.match_count() <= 1 {
                    return self.handle_key_event_without_popup(key_event);
                }
                popup.move_up();
                (InputResult::None, true)
            }
            // Allow Shift+Down to navigate history even when slash popup is active.
            KeyEvent { code: KeyCode::Down, modifiers, .. } => {
                if modifiers.contains(KeyModifiers::SHIFT) {
                    return self.handle_key_event_without_popup(key_event);
                }
                // If there are 0 or 1 items, let Down behave normally (cursor/history/scroll)
                if popup.match_count() <= 1 {
                    return self.handle_key_event_without_popup(key_event);
                }
                popup.move_down();
                (InputResult::None, true)
            }
            KeyEvent {
                code: KeyCode::Esc, ..
            } => {
                // Dismiss the slash popup; keep the current input untouched.
                self.active_popup = ActivePopup::None;
                (InputResult::None, true)
            }
            KeyEvent {
                code: KeyCode::Tab, ..
            } => {
                if let Some(sel) = popup.selected_item() {
                    let first_line = self.textarea.text().lines().next().unwrap_or("");

                    match sel {
                        CommandItem::Builtin(cmd) => {
                            let starts_with_cmd = first_line
                                .trim_start()
                                .starts_with(&format!("/{}", cmd.command()));
                            if !starts_with_cmd {
                                self.textarea.set_text(&format!("/{} ", cmd.command()));
                            }
                        }
                        CommandItem::UserPrompt(idx) => {
                            if let Some(name) = popup.prompt_name(idx) {
                                let starts_with_cmd =
                                    first_line.trim_start().starts_with(&format!("/{name}"));
                                if !starts_with_cmd {
                                    self.textarea.set_text(&format!("/{name} "));
                                }
                            }
                        }
                        CommandItem::Subagent(i) => {
                            if let Some(name) = popup.subagent_name(i) {
                                let starts_with_cmd = first_line
                                    .trim_start()
                                    .starts_with(&format!("/{}", name));
                                if !starts_with_cmd {
                                    self.textarea.set_text(&format!("/{} ", name));
                                }
                            }
                        }
                    }
                    // After completing, place the cursor at the end of the
                    // slash command so the user can immediately type args.
                    let new_cursor = self.textarea.text().len();
                    self.textarea.set_cursor(new_cursor);
                }
                (InputResult::None, true)
            }
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                if let Some(sel) = popup.selected_item() {
                    // Get the full command text before clearing
                    let command_text = self.textarea.text().to_string();

                    // Record the exact slash command that was typed
                    self.history.record_local_submission(&command_text);

                    match sel {
                        CommandItem::Builtin(cmd) => {
                            // Check if this is a prompt-expanding command that will trigger agents
                            if cmd.is_prompt_expanding() {
                                self.app_event_tx.send(crate::app_event::AppEvent::PrepareAgents);
                            }
                            // Send command to the app layer with full text.
                            self.app_event_tx
                                .send(crate::app_event::AppEvent::DispatchCommand(cmd, command_text.clone()));
                            // Clear textarea and dismiss popup
                            self.textarea.set_text("");
                            self.active_popup = ActivePopup::None;
                            return (InputResult::Command(cmd), true);
                        }
                        CommandItem::UserPrompt(idx) => {
                            let prompt_content = popup
                                .prompt_content(idx)
                                .map(|s| s.to_string());
                            self.textarea.set_text("");
                            self.active_popup = ActivePopup::None;
                            if let Some(contents) = prompt_content {
                                return (InputResult::Submitted(contents), true);
                            }
                            return (InputResult::None, true);
                        }
                        CommandItem::Subagent(i) => {
                            // If the current input already starts with this subagent slash,
                            // treat Enter as submit (close popup and forward to default handler).
                            if let Some(name) = popup.subagent_name(i) {
                                let first_line = command_text.lines().next().unwrap_or("");
                                let starts_with = first_line.trim_start().starts_with(&format!("/{}", name));
                                if starts_with {
                                    // Dismiss popup and submit normally (this will handle args or empty string)
                                    self.active_popup = ActivePopup::None;
                                    return self.handle_key_event_without_popup(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
                                } else {
                                    // Insert the subagent slash with a trailing space for args
                                    self.textarea.set_text(&format!("/{} ", name));
                                    let new_cursor = self.textarea.text().len();
                                    self.textarea.set_cursor(new_cursor);
                                    return (InputResult::None, true);
                                }
                            }
                            return (InputResult::None, true);
                        }
                    }
                }
                // Fallback to default newline handling if no command selected.
                self.handle_key_event_without_popup(key_event)
            }
            input => self.handle_input_basic(input),
        }
    }
    #[inline]
    fn clamp_to_char_boundary(text: &str, pos: usize) -> usize {
        let mut p = pos.min(text.len());
        if p < text.len() && !text.is_char_boundary(p) {
            p = text
                .char_indices()
                .map(|(i, _)| i)
                .take_while(|&i| i <= p)
                .last()
                .unwrap_or(0);
        }
        p
    }

    #[inline]
    #[allow(dead_code)]
    fn handle_non_ascii_char(&mut self, input: KeyEvent) -> (InputResult, bool) {
        if let Some(pasted) = self.paste_burst.flush_before_modified_input() {
            self.handle_paste(pasted);
        }
        self.textarea.input(input);
        let text_after = self.textarea.text();
        self.pending_pastes
            .retain(|(placeholder, _)| text_after.contains(placeholder));
        (InputResult::None, true)
    }

    /// Handle key events when file search popup is visible.
    fn handle_key_event_with_file_popup(&mut self, key_event: KeyEvent) -> (InputResult, bool) {
        let ActivePopup::File(popup) = &mut self.active_popup else {
            unreachable!();
        };

        match key_event {
            KeyEvent { code: KeyCode::Up, modifiers, .. } => {
                if modifiers.contains(KeyModifiers::SHIFT) {
                    return self.handle_key_event_without_popup(key_event);
                }
                // If there are 0 or 1 items, let Up behave normally (cursor/history/scroll)
                if popup.match_count() <= 1 {
                    return self.handle_key_event_without_popup(key_event);
                }
                popup.move_up();
                (InputResult::None, true)
            }
            KeyEvent { code: KeyCode::Down, modifiers, .. } => {
                if modifiers.contains(KeyModifiers::SHIFT) {
                    return self.handle_key_event_without_popup(key_event);
                }
                // If there are 0 or 1 items, let Down behave normally (cursor/history/scroll)
                if popup.match_count() <= 1 {
                    return self.handle_key_event_without_popup(key_event);
                }
                popup.move_down();
                (InputResult::None, true)
            }
            KeyEvent {
                code: KeyCode::Esc, ..
            } => {
                // Hide popup without modifying text, remember token to avoid immediate reopen.
                if let Some(tok) = Self::current_completion_token(&self.textarea) {
                    self.dismissed_file_popup_token = Some(tok.to_string());
                }
                self.active_popup = ActivePopup::None;
                (InputResult::None, true)
            }
            KeyEvent {
                code: KeyCode::Tab, ..
            }
            | KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                if let Some(sel) = popup.selected_match() {
                    let sel_path = sel.to_string();
                    // Drop popup borrow before using self mutably again.
                    self.insert_selected_path(&sel_path);
                    self.active_popup = ActivePopup::None;
                    return (InputResult::None, true);
                }
                (InputResult::None, false)
            }
            input => self.handle_input_basic(input),
        }
    }

    /// Extract the `@token` that the cursor is currently positioned on, if any.
    ///
    /// The returned string **does not** include the leading `@`.
    ///
    /// Behavior:
    /// - The cursor may be anywhere inside the token (including on the
    ///   leading `@`). It does not need to be at the end of the line.
    /// - A token is delimited by ASCII whitespace (space, tab, newline).
    /// - If the token under the cursor starts with `@`, that token is
    ///   returned without the leading `@`. This includes the case where the
    ///   token is just "@" (empty query), which is used to trigger a UI hint
    fn current_at_token(textarea: &TextArea) -> Option<String> {
        let cursor_offset = textarea.cursor();
        let text = textarea.text();

        // Adjust the provided byte offset to the nearest valid char boundary at or before it.
        let mut safe_cursor = cursor_offset.min(text.len());
        // If we're not on a char boundary, move back to the start of the current char.
        if safe_cursor < text.len() && !text.is_char_boundary(safe_cursor) {
            // Find the last valid boundary <= cursor_offset.
            safe_cursor = text
                .char_indices()
                .map(|(i, _)| i)
                .take_while(|&i| i <= cursor_offset)
                .last()
                .unwrap_or(0);
        }

        // Split the line around the (now safe) cursor position.
        let before_cursor = &text[..safe_cursor];
        let after_cursor = &text[safe_cursor..];

        // Detect whether we're on whitespace at the cursor boundary.
        let at_whitespace = if safe_cursor < text.len() {
            text[safe_cursor..]
                .chars()
                .next()
                .map(|c| c.is_whitespace())
                .unwrap_or(false)
        } else {
            false
        };

        // Left candidate: token containing the cursor position.
        let start_left = before_cursor
            .char_indices()
            .rfind(|(_, c)| c.is_whitespace())
            .map(|(idx, c)| idx + c.len_utf8())
            .unwrap_or(0);
        let end_left_rel = after_cursor
            .char_indices()
            .find(|(_, c)| c.is_whitespace())
            .map(|(idx, _)| idx)
            .unwrap_or(after_cursor.len());
        let end_left = safe_cursor + end_left_rel;
        let token_left = if start_left < end_left {
            Some(&text[start_left..end_left])
        } else {
            None
        };

        // Right candidate: token immediately after any whitespace from the cursor.
        let ws_len_right: usize = after_cursor
            .chars()
            .take_while(|c| c.is_whitespace())
            .map(|c| c.len_utf8())
            .sum();
        let start_right = safe_cursor + ws_len_right;
        let end_right_rel = text[start_right..]
            .char_indices()
            .find(|(_, c)| c.is_whitespace())
            .map(|(idx, _)| idx)
            .unwrap_or(text.len() - start_right);
        let end_right = start_right + end_right_rel;
        let token_right = if start_right < end_right {
            Some(&text[start_right..end_right])
        } else {
            None
        };

        let left_at = token_left
            .filter(|t| t.starts_with('@'))
            .map(|t| t[1..].to_string());
        let right_at = token_right
            .filter(|t| t.starts_with('@'))
            .map(|t| t[1..].to_string());

        if at_whitespace {
            if right_at.is_some() {
                return right_at;
            }
            if token_left.is_some_and(|t| t == "@") {
                return None;
            }
            return left_at;
        }
        if after_cursor.starts_with('@') {
            return right_at.or(left_at);
        }
        left_at.or(right_at)
    }

    /// Extract the completion token under the cursor for auto file search.
    ///
    /// Auto-trigger only for:
    /// - explicit @tokens (without the leading '@' in the return value)
    /// - tokens starting with "./" (relative paths)
    ///
    /// Returns the token text (without a leading '@' if present). Any other
    /// tokens should not auto-trigger completion; they may be handled on Tab.
    fn current_completion_token(textarea: &TextArea) -> Option<String> {
        // Prefer explicit @tokens when present.
        if let Some(tok) = Self::current_at_token(textarea) {
            return Some(tok);
        }

        // Otherwise, consider the generic token under the cursor, but only
        // auto-trigger for tokens starting with "./".
        let cursor_offset = textarea.cursor();
        let text = textarea.text();

        let mut safe_cursor = cursor_offset.min(text.len());
        if safe_cursor < text.len() && !text.is_char_boundary(safe_cursor) {
            safe_cursor = text
                .char_indices()
                .map(|(i, _)| i)
                .take_while(|&i| i <= cursor_offset)
                .last()
                .unwrap_or(0);
        }

        let before_cursor = &text[..safe_cursor];
        let after_cursor = &text[safe_cursor..];

        let start_idx = before_cursor
            .char_indices()
            .rfind(|(_, c)| c.is_whitespace())
            .map(|(idx, c)| idx + c.len_utf8())
            .unwrap_or(0);
        let end_rel_idx = after_cursor
            .char_indices()
            .find(|(_, c)| c.is_whitespace())
            .map(|(idx, _)| idx)
            .unwrap_or(after_cursor.len());
        let end_idx = safe_cursor + end_rel_idx;

        if start_idx >= end_idx {
            return None;
        }

        let token = &text[start_idx..end_idx];

        // Strip a leading '@' if the user typed it but we didn't catch it
        // (paranoia; current_at_token should have handled this case).
        let token_stripped = token.strip_prefix('@').unwrap_or(token);

        if token_stripped.starts_with("./") {
            return Some(token_stripped.to_string());
        }

        None
    }

    /// Extract the generic token under the cursor (no special rules).
    /// Used for Tab-triggered one-off file searches.
    fn current_generic_token(textarea: &TextArea) -> Option<String> {
        let cursor_offset = textarea.cursor();
        let text = textarea.text();

        let mut safe_cursor = cursor_offset.min(text.len());
        if safe_cursor < text.len() && !text.is_char_boundary(safe_cursor) {
            safe_cursor = text
                .char_indices()
                .map(|(i, _)| i)
                .take_while(|&i| i <= cursor_offset)
                .last()
                .unwrap_or(0);
        }

        let before_cursor = &text[..safe_cursor];
        let after_cursor = &text[safe_cursor..];

        let start_idx = before_cursor
            .char_indices()
            .rfind(|(_, c)| c.is_whitespace())
            .map(|(idx, c)| idx + c.len_utf8())
            .unwrap_or(0);
        let end_rel_idx = after_cursor
            .char_indices()
            .find(|(_, c)| c.is_whitespace())
            .map(|(idx, _)| idx)
            .unwrap_or(after_cursor.len());
        let end_idx = safe_cursor + end_rel_idx;

        if start_idx >= end_idx { return None; }

        Some(text[start_idx..end_idx].trim().to_string())
    }

    /// Replace the active `@token` (the one under the cursor) with `path`.
    ///
    /// The algorithm mirrors `current_at_token` so replacement works no matter
    /// where the cursor is within the token and regardless of how many
    /// `@tokens` exist in the line.
    fn insert_selected_path(&mut self, path: &str) {
        let cursor_offset = self.textarea.cursor();
        let text = self.textarea.text();
        // Clamp to a valid char boundary to avoid panics when slicing.
        let safe_cursor = Self::clamp_to_char_boundary(text, cursor_offset);

        let before_cursor = &text[..safe_cursor];
        let after_cursor = &text[safe_cursor..];

        // Determine token boundaries.
        let start_idx = before_cursor
            .char_indices()
            .rfind(|(_, c)| c.is_whitespace())
            .map(|(idx, c)| idx + c.len_utf8())
            .unwrap_or(0);

        let end_rel_idx = after_cursor
            .char_indices()
            .find(|(_, c)| c.is_whitespace())
            .map(|(idx, _)| idx)
            .unwrap_or(after_cursor.len());
        let end_idx = safe_cursor + end_rel_idx;

        // Replace the slice `[start_idx, end_idx)` with the chosen path and a trailing space.
        let mut new_text =
            String::with_capacity(text.len() - (end_idx - start_idx) + path.len() + 1);
        new_text.push_str(&text[..start_idx]);
        new_text.push_str(path);
        new_text.push(' ');
        new_text.push_str(&text[end_idx..]);

        self.textarea.set_text(&new_text);
        let new_cursor = start_idx.saturating_add(path.len()).saturating_add(1);
        self.textarea.set_cursor(new_cursor);
    }

    /// Handle key event when no popup is visible.
    fn handle_key_event_without_popup(&mut self, key_event: KeyEvent) -> (InputResult, bool) {
        match key_event {
            KeyEvent {
                code: KeyCode::Char('d'),
                modifiers: crossterm::event::KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                ..
            } if self.is_empty() => {
                self.app_event_tx.send(crate::app_event::AppEvent::ExitRequest);
                (InputResult::None, true)
            }
            // -------------------------------------------------------------
            // Shift+Tab — rotate access preset (Read Only → Write with Approval → Full Access)
            // -------------------------------------------------------------
            KeyEvent { code: KeyCode::BackTab, .. } => {
                self.app_event_tx.send(crate::app_event::AppEvent::CycleAccessMode);
                (InputResult::None, true)
            }
            // -------------------------------------------------------------
            // Tab-press file search when not using @ or ./ and not in slash cmd
            // -------------------------------------------------------------
            KeyEvent { code: KeyCode::Tab, .. } => {
                // Suppress Tab completion only while the cursor is within the
                // slash command head (before the first space). Allow Tab-based
                // file search in the arguments of /plan, /solve, etc.
                if self.is_cursor_in_slash_command_head() {
                    return (InputResult::None, false);
                }

                // If already showing a file popup, let the dedicated handler manage Tab.
                if matches!(self.active_popup, ActivePopup::File(_)) {
                    return (InputResult::None, false);
                }

                // If an @ token is present or token starts with ./, rely on auto-popup.
                if Self::current_completion_token(&self.textarea).is_some() {
                    return (InputResult::None, false);
                }

                // Use the generic token under cursor for a one-off search.
                if let Some(tok) = Self::current_generic_token(&self.textarea) {
                    if !tok.is_empty() {
                        self.pending_tab_file_query = Some(tok.clone());
                        self.app_event_tx.send(crate::app_event::AppEvent::StartFileSearch(tok));
                        // Do not show a popup yet; wait for results and only
                        // show if there are matches to avoid flicker.
                        return (InputResult::None, true);
                    }
                }
                (InputResult::None, false)
            }
            // -------------------------------------------------------------
            // Handle Esc key — leave to App-level policy (clear/stop/backtrack)
            // -------------------------------------------------------------
            KeyEvent { code: KeyCode::Esc, .. } => {
                // Do nothing here so App can implement global Esc ordering.
                (InputResult::None, false)
            }
            // -------------------------------------------------------------
            // Up/Down key handling - check modifiers to determine action
            // -------------------------------------------------------------
            KeyEvent {
                code: KeyCode::Up | KeyCode::Down,
                modifiers,
                ..
            } => {
                // Check if Shift is held for history navigation
                if modifiers.contains(KeyModifiers::SHIFT) {
                    // History navigation with Shift+Up/Down
                    if self
                        .history
                        .should_handle_navigation(self.textarea.text(), self.textarea.cursor())
                    {
                        let replace_text = match key_event.code {
                            KeyCode::Up => self
                                .history
                                .navigate_up(self.textarea.text(), &self.app_event_tx),
                            KeyCode::Down => self.history.navigate_down(&self.app_event_tx),
                            _ => None,
                        };
                        if let Some(text) = replace_text {
                            self.textarea.set_text(&text);
                            self.textarea.set_cursor(0);
                            return (InputResult::None, true);
                        }
                    }
                    // If history navigation didn't happen, just ignore the key
                    (InputResult::None, false)
                } else {
                    // No Shift modifier — move cursor within the input first.
                    // Only when already at the top-left/bottom-right should Up/Down scroll chat.
                    if self.textarea.is_empty() {
                        return match key_event.code {
                            KeyCode::Up => (InputResult::ScrollUp, false),
                            KeyCode::Down => (InputResult::ScrollDown, false),
                            _ => (InputResult::None, false),
                        };
                    }

                    let before = self.textarea.cursor();
                    let len = self.textarea.text().len();
                    match key_event.code {
                        KeyCode::Up => {
                            if before == 0 {
                                (InputResult::ScrollUp, false)
                            } else {
                                // Move up a visual/logical line; if already on first line, TextArea moves to start.
                                self.textarea.input(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
                                (InputResult::None, true)
                            }
                        }
                        KeyCode::Down => {
                            // If sticky is set, prefer chat ScrollDown once
                            if self.next_down_scrolls_history {
                                self.next_down_scrolls_history = false;
                                return (InputResult::ScrollDown, false);
                            }
                            if before == len {
                                (InputResult::ScrollDown, false)
                            } else {
                                // Move down a visual/logical line; if already on last line, TextArea moves to end.
                                self.textarea.input(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
                                (InputResult::None, true)
                            }
                        }
                        _ => (InputResult::None, false),
                    }
                }
            }
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                // Record the exact text that was typed (before replacement)
                let original_text = self.textarea.text().to_string();

                let mut text = self.textarea.text().to_string();
                self.textarea.set_text("");

                // Replace all pending pastes in the text
                for (placeholder, actual) in &self.pending_pastes {
                    if text.contains(placeholder) {
                        text = text.replace(placeholder, actual);
                    }
                }
                self.pending_pastes.clear();

                if text.is_empty() {
                    (InputResult::None, true)
                } else {
                    // Check if this is a prompt-expanding command that will trigger agents
                    let trimmed = original_text.trim();
                    if trimmed.starts_with("/plan ")
                        || trimmed.starts_with("/solve ")
                        || trimmed.starts_with("/code ")
                    {
                        self.app_event_tx.send(crate::app_event::AppEvent::PrepareAgents);
                    }

                    self.history.record_local_submission(&original_text);
                    (InputResult::Submitted(text), true)
                }
            }
            input => self.handle_input_basic(input),
        }
    }

    /// Handle generic Input events that modify the textarea content.
    fn handle_input_basic(&mut self, input: KeyEvent) -> (InputResult, bool) {
        let text_before = self.textarea.text().to_string();

        // Special handling for backspace on placeholders
        if let KeyEvent {
            code: KeyCode::Backspace,
            ..
        } = input
        {
            if self.try_remove_placeholder_at_cursor() {
                // Text was modified, reset history navigation
                self.history.reset_navigation();
                return (InputResult::None, true);
            }
        }

        // Normal input handling
        self.textarea.input(input);
        let text_after = self.textarea.text();

        // If text changed, reset history navigation state
        if text_before != text_after {
            self.history.reset_navigation();
            if !text_after.is_empty() { self.typed_anything = true; }
        }

        // Check if any placeholders were removed and remove their corresponding pending pastes
        self.pending_pastes
            .retain(|(placeholder, _)| text_after.contains(placeholder));

        (InputResult::None, true)
    }

    /// Attempts to remove a placeholder if the cursor is at the end of one.
    /// Returns true if a placeholder was removed.
    fn try_remove_placeholder_at_cursor(&mut self) -> bool {
        let text = self.textarea.text();
        let p = Self::clamp_to_char_boundary(text, self.textarea.cursor());

        // Find any placeholder that ends at the cursor position
        let placeholder_to_remove = self.pending_pastes.iter().find_map(|(ph, _)| {
            if p < ph.len() {
                return None;
            }
            let potential_ph_start = p - ph.len();
            // Use `get` to avoid panicking on non-char-boundary indices.
            match text.get(potential_ph_start..p) {
                Some(slice) if slice == ph => Some(ph.clone()),
                _ => None,
            }
        });

        if let Some(placeholder) = placeholder_to_remove {
            self.textarea.replace_range(p - placeholder.len()..p, "");
            self.pending_pastes.retain(|(ph, _)| ph != &placeholder);
            true
        } else {
            false
        }
    }

    /// Synchronize `self.command_popup` with the current text in the
    /// textarea. This must be called after every modification that can change
    /// the text so the popup is shown/updated/hidden as appropriate.
    fn sync_command_popup(&mut self) {
        let first_line = self.textarea.text().lines().next().unwrap_or("");
        let input_starts_with_slash = first_line.starts_with('/');
        // Keep the slash popup only while the cursor is within the command head
        // (before the first space). This allows @-file completion for arguments
        // in commands like "/plan" and "/solve".
        let in_slash_head = self.is_cursor_in_slash_command_head();
        match &mut self.active_popup {
            ActivePopup::Command(popup) => {
                if input_starts_with_slash && in_slash_head {
                    popup.on_composer_text_change(first_line.to_string());
                } else {
                    self.active_popup = ActivePopup::None;
                }
            }
            _ => {
                if input_starts_with_slash && in_slash_head {
                    let mut command_popup = CommandPopup::new_with_filter(self.using_chatgpt_auth);
                    // Load saved subagent commands to include in autocomplete (exclude built-ins)
                    if let Ok(cfg) = codex_core::config::Config::load_with_cli_overrides(vec![], codex_core::config::ConfigOverrides::default()) {
                        let mut names: Vec<String> = cfg
                            .subagent_commands
                            .iter()
                            .map(|c| c.name.clone())
                            .filter(|n| {
                                let l = n.to_ascii_lowercase();
                                l != "plan" && l != "solve" && l != "code"
                            })
                            .collect();
                        // Stable sort for presentation
                        names.sort();
                        command_popup.set_subagent_commands(names);
                    }
                    command_popup.on_composer_text_change(first_line.to_string());
                    self.active_popup = ActivePopup::Command(command_popup);
                    // Notify app: composer expanded due to slash popup
                    self.app_event_tx.send(crate::app_event::AppEvent::ComposerExpanded);
                }
            }
        }
    }

    #[allow(dead_code)]
    pub(crate) fn set_custom_prompts(&mut self, prompts: Vec<CustomPrompt>) {
        if let ActivePopup::Command(popup) = &mut self.active_popup {
            popup.set_prompts(prompts);
        }
    }

    /// Synchronize `self.file_search_popup` with the current text in the textarea.
    /// Note this is only called when self.active_popup is NOT Command.
    fn sync_file_search_popup(&mut self) {
        // Determine if there is a token underneath the cursor worth completing.
        let query = match Self::current_completion_token(&self.textarea) {
            Some(token) => token,
            None => {
                self.active_popup = ActivePopup::None;
                self.dismissed_file_popup_token = None;
                return;
            }
        };

        // If user dismissed popup for this exact query, don't reopen until text changes.
        if self.dismissed_file_popup_token.as_ref() == Some(&query) {
            return;
        }

        // Trigger file search as soon as at least 1 character is typed.
        // The popup shows an idle hint for an empty query handled above.
        if query.chars().count() >= 1 {
            self.app_event_tx
                .send(crate::app_event::AppEvent::StartFileSearch(query.clone()));
        }

        match &mut self.active_popup {
            ActivePopup::File(popup) => {
                if query.is_empty() {
                    popup.set_empty_prompt();
                } else {
                    popup.set_query(&query);
                }
            }
            _ => {
                let mut popup = FileSearchPopup::new();
                if query.is_empty() {
                    popup.set_empty_prompt();
                } else {
                    popup.set_query(&query);
                }
                self.active_popup = ActivePopup::File(popup);
            }
        }

        self.current_file_query = Some(query);
        self.dismissed_file_popup_token = None;
    }

    fn set_has_focus(&mut self, has_focus: bool) {
        self.has_focus = has_focus;
    }

    // -------------------------------------------------------------
    // History navigation helpers (used by ChatWidget at scroll boundaries)
    // -------------------------------------------------------------
    pub(crate) fn try_history_up(&mut self) -> bool {
        if !self
            .history
            .should_handle_navigation(self.textarea.text(), self.textarea.cursor())
        {
            return false;
        }
        if let Some(text) = self.history.navigate_up(self.textarea.text(), &self.app_event_tx) {
            self.textarea.set_text(&text);
            self.textarea.set_cursor(0);
        }
        true
    }

    pub(crate) fn try_history_down(&mut self) -> bool {
        // Only meaningful when browsing or when original text is recorded
        if !self
            .history
            .should_handle_navigation(self.textarea.text(), self.textarea.cursor())
        {
            return false;
        }
        if let Some(text) = self.history.navigate_down(&self.app_event_tx) {
            self.textarea.set_text(&text);
            self.textarea.set_cursor(0);
        }
        true
    }

    pub(crate) fn history_is_browsing(&self) -> bool { self.history.is_browsing() }

    pub(crate) fn mark_next_down_scrolls_history(&mut self) {
        self.next_down_scrolls_history = true;
    }
}

impl WidgetRef for ChatComposer {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let popup_height = match &self.active_popup {
            ActivePopup::Command(popup) => popup.calculate_required_height(),
            ActivePopup::File(popup) => popup.calculate_required_height(),
            ActivePopup::None => 1,
        };
        // Split area: textarea with border at top, hints/popup at bottom
        let hint_height = if matches!(self.active_popup, ActivePopup::None) {
            1
        } else {
            popup_height
        };

        // Calculate dynamic height based on content
        let content_width = area.width.saturating_sub(4); // Account for border and padding
        let content_lines = self.textarea.desired_height(content_width).max(1);
        let desired_input_height = (content_lines + 2).max(3); // Parent layout enforces max

        // Use desired height but don't exceed available space
        let input_height = desired_input_height.min(area.height.saturating_sub(hint_height));
        let [input_area, hint_area] = Layout::vertical([
            Constraint::Length(input_height),
            Constraint::Length(hint_height),
        ])
        .areas(area);
        match &self.active_popup {
            ActivePopup::Command(popup) => {
                popup.render_ref(hint_area, buf);
            }
            ActivePopup::File(popup) => {
                popup.render_ref(hint_area, buf);
            }
            ActivePopup::None => {
                let bottom_line_rect = hint_area;

                let key_hint_style = Style::default().fg(crate::colors::function());
                let label_style = Style::default().fg(crate::colors::text_dim());
                // Left side: padding + notices (and Ctrl+C again-to-quit notice if active)
                let mut left_spans: Vec<Span> = Vec::new();
                left_spans.push(Span::from("  "));

                // Access mode indicator (Read Only / Write with Approval / Full Access)
                // When the label is ephemeral, hide it after expiry. The "(Shift+Tab change)"
                // suffix is shown for a short time even for persistent labels.
                let show_access_label = if let Some(until) = self.access_mode_label_expiry {
                    std::time::Instant::now() <= until
                } else { true };
                if show_access_label {
                    if let Some(label) = &self.access_mode_label {
                        // Access label without bold per design
                        left_spans.push(Span::from(label.clone()).style(label_style));
                        // Show the hint suffix while the hint timer is active; if the whole label
                        // is ephemeral, keep the suffix visible for the same duration.
                        let show_suffix = if let Some(until) = self.access_mode_hint_expiry {
                            std::time::Instant::now() <= until
                        } else {
                            // If label itself is ephemeral, mirror its lifetime for the suffix
                            self.access_mode_label_expiry.is_some()
                        };
                        if show_suffix {
                            left_spans.push(Span::from("  (").style(label_style));
                            left_spans.push(Span::from("Shift+Tab").style(key_hint_style));
                            left_spans.push(Span::from(" change)").style(label_style));
                        }
                    }
                }

                if self.ctrl_c_quit_hint {
                    // Treat as a notice; keep on the left
                    if !self.access_mode_label.is_none() { left_spans.push(Span::from("   ")); }
                    left_spans.push(Span::from("Ctrl+C").style(key_hint_style));
                    left_spans.push(Span::from(" again to quit").style(label_style));
                }

                // Append ephemeral footer notice if present and not expired
                if let Some((msg, until)) = &self.footer_notice {
                    if std::time::Instant::now() <= *until {
                        if !self.ctrl_c_quit_hint { left_spans.push(Span::from("   ")); }
                        left_spans.push(Span::from(msg.clone()).style(Style::default().add_modifier(Modifier::DIM)));
                    }
                }

                // Right side: command key hints (Ctrl+R/D/H), token usage, and a small auth notice
                // when using an API key instead of ChatGPT auth. We elide hints first if space is tight.
                let mut right_spans: Vec<Span> = Vec::new();

                // Prepare token usage spans (always shown when available)
                let mut token_spans: Vec<Span> = Vec::new();
                if let Some(token_usage_info) = &self.token_usage_info {
                    let token_usage = &token_usage_info.total_token_usage;
                    let used_str = format_with_thousands(token_usage.blended_total());
                    token_spans.push(Span::from(used_str).style(label_style.add_modifier(Modifier::BOLD)));
                    token_spans.push(Span::from(" tokens ").style(label_style));
                    if let Some(context_window) = token_usage_info.model_context_window {
                        let last_token_usage = &token_usage_info.last_token_usage;
                        let percent_remaining: u8 = if context_window > 0 {
                            let percent = 100.0
                                - (last_token_usage.tokens_in_context_window() as f32
                                    / context_window as f32
                                    * 100.0);
                            percent.clamp(0.0, 100.0) as u8
                        } else { 100 };
                        token_spans.push(Span::from("(").style(label_style));
                        token_spans.push(Span::from(percent_remaining.to_string()).style(label_style.add_modifier(Modifier::BOLD)));
                        token_spans.push(Span::from("% left)").style(label_style));
                    }
                }

                // Helper to build hint spans based on inclusion flags
                let help_label = self.help_hint_label.clone();
                let reason_label = self.reasoning_hint_label.clone();
                let diffs_label = self.diffs_hint_label.clone();
                let build_hints = |include_reasoning: bool, include_diff: bool| -> Vec<Span> {
                    let mut spans: Vec<Span> = Vec::new();
                    if !self.ctrl_c_quit_hint {
                        if self.show_reasoning_hint && include_reasoning {
                            if !spans.is_empty() { spans.push(Span::from("  •  ").style(label_style)); }
                            spans.push(Span::from(reason_label.clone()).style(key_hint_style));
                            let label = if self.reasoning_shown { " hide reasoning" } else { " show reasoning" };
                            spans.push(Span::from(label).style(label_style));
                        }
                        if self.show_diffs_hint && include_diff {
                            if !spans.is_empty() { spans.push(Span::from("  •  ").style(label_style)); }
                            spans.push(Span::from(diffs_label.clone()).style(key_hint_style));
                            spans.push(Span::from(" diff viewer").style(label_style));
                        }
                        // Always show help at the end of the command hints
                        if !spans.is_empty() { spans.push(Span::from("  •  ").style(label_style)); }
                        spans.push(Span::from(help_label.clone()).style(key_hint_style));
                        spans.push(Span::from(" help").style(label_style));
                    }
                    spans
                };

                // Start with all hints included
                let mut include_reasoning = true;
                let mut include_diff = true;
                let mut hint_spans = build_hints(include_reasoning, include_diff);

                // Measure function for spans length
                let measure = |spans: &Vec<Span>| -> usize {
                    spans.iter().map(|s| s.content.chars().count()).sum()
                };

                // Compute spacer between left and right to make right content right-aligned
                let left_len: usize = left_spans.iter().map(|s| s.content.chars().count()).sum();
                let total_width = bottom_line_rect.width as usize;
                let trailing_pad = 1usize; // one space on the right edge

                // Optional auth notice: show a small "API key" tag when not using ChatGPT auth
                let mut auth_spans: Vec<Span> = Vec::new();
                if !self.using_chatgpt_auth {
                    auth_spans.push(Span::from("API key").style(label_style));
                }

                // We'll add separators between sections when both are present
                let sep_len = "  •  ".chars().count();
                let combined_len = |h: &Vec<Span>, t: &Vec<Span>, a: &Vec<Span>| -> usize {
                    let mut len = measure(h) + measure(t) + measure(a);
                    if !h.is_empty() && !t.is_empty() { len += sep_len; }
                    if (!h.is_empty() || !t.is_empty()) && !a.is_empty() { len += sep_len; }
                    len
                };

                // Elide hints in order until content fits
                while left_len + combined_len(&hint_spans, &token_spans, &auth_spans) + trailing_pad > total_width {
                    if include_reasoning {
                        include_reasoning = false;
                    } else if include_diff {
                        include_diff = false;
                    } else if !auth_spans.is_empty() {
                        // If still too tight, drop the auth tag as a last resort
                        auth_spans.clear();
                    } else {
                        break;
                    }
                    hint_spans = build_hints(include_reasoning, include_diff);
                }

                // Compose final right spans: hints, optional separator, then tokens
                if !hint_spans.is_empty() { right_spans.extend(hint_spans); }
                if !right_spans.is_empty() && !token_spans.is_empty() {
                    right_spans.push(Span::from("  •  ").style(label_style));
                }
                right_spans.extend(token_spans);
                // Append auth notice at the very end (right-most) if present
                if !right_spans.is_empty() && !auth_spans.is_empty() {
                    right_spans.push(Span::from("  •  ").style(label_style));
                }
                right_spans.extend(auth_spans);

                // Recompute spacer after elision
                let right_len: usize = right_spans.iter().map(|s| s.content.chars().count()).sum();
                let spacer = if total_width > left_len + right_len + trailing_pad {
                    " ".repeat(total_width - left_len - right_len - trailing_pad)
                } else { String::from(" ") };

                let mut line_spans = left_spans;
                line_spans.push(Span::from(spacer));
                line_spans.extend(right_spans);
                line_spans.push(Span::from(" "));

                Line::from(line_spans)
                    .style(
                        Style::default()
                            .fg(crate::colors::text_dim())
                            .add_modifier(Modifier::DIM),
                    )
                    .render_ref(bottom_line_rect, buf);
            }
        }
        // Draw border around input area with optional "Coding" title when task is running
        let mut input_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(crate::colors::border()))
            // Fill input block with theme background so underlying content
            // never shows through when the composer grows/shrinks.
            .style(Style::default().bg(crate::colors::background()));

        if self.is_task_running {
            use std::time::{SystemTime, UNIX_EPOCH};
            // Use selected spinner style
            let now_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();
            let def = crate::spinner::current_spinner();
            let spinner_str = crate::spinner::frame_at_time(def, now_ms);

            // Create centered title with spinner and spaces
            let title_line = Line::from(vec![
                Span::raw(" "), // Space before spinner
                Span::styled(spinner_str, Style::default().fg(crate::colors::info())),
                Span::styled(
                    format!(" {}... ", self.status_message),
                    Style::default().fg(crate::colors::info()),
                ), // Space after spinner and after text
            ])
            .centered();
            input_block = input_block.title(title_line);
        }

        let textarea_rect = input_block.inner(input_area);
        input_block.render_ref(input_area, buf);

        // Add padding inside the text area (1 char horizontal only, no vertical padding)
        let padded_textarea_rect = textarea_rect.inner(Margin::new(1, 0));

        let mut state = self.textarea_state.borrow_mut();
        StatefulWidgetRef::render_ref(&(&self.textarea), padded_textarea_rect, buf, &mut state);
        // Only show placeholder if there's no chat history AND no text typed
        if !self.typed_anything && self.textarea.text().is_empty() {
            let placeholder = crate::greeting::greeting_placeholder();
            Line::from(placeholder)
                .style(Style::default().dim())
                .render_ref(padded_textarea_rect, buf);
        }

        // Draw a high-contrast cursor overlay under the terminal cursor using the theme's
        // `cursor` color. This improves visibility on dark themes where the terminal's own
        // cursor color may be hard to see or user-defined.
        //
        // Implementation notes:
        // - We compute the visible cursor position using the same `state` (scroll) used to
        //   render the textarea so the overlay aligns with wrapped lines.
        // - We paint the underlying cell with bg=theme.cursor and fg=theme.background.
        //   This provides contrast regardless of light/dark theme.
        // - The hardware cursor is still positioned via `frame.set_cursor_position` at the
        //   app layer; this overlay ensures visibility independent of terminal settings.
        drop(state); // release the borrow before computing position again
        if let Some((cx, cy)) = self
            .textarea
            .cursor_pos_with_state(padded_textarea_rect, &self.textarea_state.borrow())
        {
            let overlay_style = Style::default()
                .bg(crate::theme::current_theme().cursor)
                .fg(crate::colors::background());
            if cx < buf.area.width.saturating_add(buf.area.x)
                && cy < buf.area.height.saturating_add(buf.area.y)
            {
                let cell = &mut buf[(cx, cy)];
                // Preserve the displayed character but apply the overlay colors
                cell.set_style(overlay_style);
            }
        }
    }
}

#[cfg(all(test, feature = "legacy_tests"))]
mod tests {
    use crate::app_event::AppEvent;
    use crate::bottom_pane::AppEventSender;
    use crate::bottom_pane::ChatComposer;
    use crate::bottom_pane::InputResult;
    use crate::bottom_pane::chat_composer::LARGE_PASTE_CHAR_THRESHOLD;
    use crate::bottom_pane::textarea::TextArea;

    #[test]
    fn test_current_at_token_basic_cases() {
        let test_cases = vec![
            // Valid @ tokens
            ("@hello", 3, Some("hello".to_string()), "Basic ASCII token"),
            (
                "@file.txt",
                4,
                Some("file.txt".to_string()),
                "ASCII with extension",
            ),
            (
                "hello @world test",
                8,
                Some("world".to_string()),
                "ASCII token in middle",
            ),
            (
                "@test123",
                5,
                Some("test123".to_string()),
                "ASCII with numbers",
            ),
            // Unicode examples
            ("@İstanbul", 3, Some("İstanbul".to_string()), "Turkish text"),
            (
                "@testЙЦУ.rs",
                8,
                Some("testЙЦУ.rs".to_string()),
                "Mixed ASCII and Cyrillic",
            ),
            ("@诶", 2, Some("诶".to_string()), "Chinese character"),
            ("@👍", 2, Some("👍".to_string()), "Emoji token"),
            // Invalid cases (should return None)
            ("hello", 2, None, "No @ symbol"),
            (
                "@",
                1,
                Some("".to_string()),
                "Only @ symbol triggers empty query",
            ),
            ("@ hello", 2, None, "@ followed by space"),
            ("test @ world", 6, None, "@ with spaces around"),
        ];

        for (input, cursor_pos, expected, description) in test_cases {
            let mut textarea = TextArea::new();
            textarea.insert_str(input);
            textarea.set_cursor(cursor_pos);

            let result = ChatComposer::current_at_token(&textarea);
            assert_eq!(
                result, expected,
                "Failed for case: {description} - input: '{input}', cursor: {cursor_pos}"
            );
        }
    }

    #[test]
    fn test_current_at_token_cursor_positions() {
        let test_cases = vec![
            // Different cursor positions within a token
            ("@test", 0, Some("test".to_string()), "Cursor at @"),
            ("@test", 1, Some("test".to_string()), "Cursor after @"),
            ("@test", 5, Some("test".to_string()), "Cursor at end"),
            // Multiple tokens - cursor determines which token
            ("@file1 @file2", 0, Some("file1".to_string()), "First token"),
            (
                "@file1 @file2",
                8,
                Some("file2".to_string()),
                "Second token",
            ),
            // Edge cases
            ("@", 0, Some("".to_string()), "Only @ symbol"),
            ("@a", 2, Some("a".to_string()), "Single character after @"),
            ("", 0, None, "Empty input"),
        ];

        for (input, cursor_pos, expected, description) in test_cases {
            let mut textarea = TextArea::new();
            textarea.insert_str(input);
            textarea.set_cursor(cursor_pos);

            let result = ChatComposer::current_at_token(&textarea);
            assert_eq!(
                result, expected,
                "Failed for cursor position case: {description} - input: '{input}', cursor: {cursor_pos}",
            );
        }
    }

    #[test]
    fn test_current_at_token_whitespace_boundaries() {
        let test_cases = vec![
            // Space boundaries
            (
                "aaa@aaa",
                4,
                None,
                "Connected @ token - no completion by design",
            ),
            (
                "aaa @aaa",
                5,
                Some("aaa".to_string()),
                "@ token after space",
            ),
            (
                "test @file.txt",
                7,
                Some("file.txt".to_string()),
                "@ token after space",
            ),
            // Full-width space boundaries
            (
                "test　@İstanbul",
                8,
                Some("İstanbul".to_string()),
                "@ token after full-width space",
            ),
            (
                "@ЙЦУ　@诶",
                10,
                Some("诶".to_string()),
                "Full-width space between Unicode tokens",
            ),
            // Tab and newline boundaries
            (
                "test\t@file",
                6,
                Some("file".to_string()),
                "@ token after tab",
            ),
        ];

        for (input, cursor_pos, expected, description) in test_cases {
            let mut textarea = TextArea::new();
            textarea.insert_str(input);
            textarea.set_cursor(cursor_pos);

            let result = ChatComposer::current_at_token(&textarea);
            assert_eq!(
                result, expected,
                "Failed for whitespace boundary case: {description} - input: '{input}', cursor: {cursor_pos}",
            );
        }
    }

    #[test]
    fn handle_paste_small_inserts_text() {
        use crossterm::event::KeyCode;
        use crossterm::event::KeyEvent;
        use crossterm::event::KeyModifiers;

        let (tx, _rx) = std::sync::mpsc::channel();
        let sender = AppEventSender::new(tx);
        let mut composer = ChatComposer::new(true, sender, false);

        let needs_redraw = composer.handle_paste("hello".to_string());
        assert!(needs_redraw);
        assert_eq!(composer.textarea.text(), "hello");
        assert!(composer.pending_pastes.is_empty());

        let (result, _) =
            composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        match result {
            InputResult::Submitted(text) => assert_eq!(text, "hello"),
            _ => panic!("expected Submitted"),
        }
    }

    #[test]
    fn handle_paste_large_uses_placeholder_and_replaces_on_submit() {
        use crossterm::event::KeyCode;
        use crossterm::event::KeyEvent;
        use crossterm::event::KeyModifiers;

        let (tx, _rx) = std::sync::mpsc::channel();
        let sender = AppEventSender::new(tx);
        let mut composer = ChatComposer::new(true, sender, false);

        let large = "x".repeat(LARGE_PASTE_CHAR_THRESHOLD + 10);
        let needs_redraw = composer.handle_paste(large.clone());
        assert!(needs_redraw);
        let placeholder = format!("[Pasted Content {} chars]", large.chars().count());
        assert_eq!(composer.textarea.text(), placeholder);
        assert_eq!(composer.pending_pastes.len(), 1);
        assert_eq!(composer.pending_pastes[0].0, placeholder);
        assert_eq!(composer.pending_pastes[0].1, large);

        let (result, _) =
            composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        match result {
            InputResult::Submitted(text) => assert_eq!(text, large),
            _ => panic!("expected Submitted"),
        }
        assert!(composer.pending_pastes.is_empty());
    }

    #[test]
    fn edit_clears_pending_paste() {
        use crossterm::event::KeyCode;
        use crossterm::event::KeyEvent;
        use crossterm::event::KeyModifiers;

        let large = "y".repeat(LARGE_PASTE_CHAR_THRESHOLD + 1);
        let (tx, _rx) = std::sync::mpsc::channel();
        let sender = AppEventSender::new(tx);
        let mut composer = ChatComposer::new(true, sender, false);

        composer.handle_paste(large);
        assert_eq!(composer.pending_pastes.len(), 1);

        // Any edit that removes the placeholder should clear pending_paste
        composer.handle_key_event(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert!(composer.pending_pastes.is_empty());
    }

    #[test]
    fn ui_snapshots() {
        use crossterm::event::KeyCode;
        use crossterm::event::KeyEvent;
        use crossterm::event::KeyModifiers;
        use insta::assert_snapshot;
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let (tx, _rx) = std::sync::mpsc::channel();
        let sender = AppEventSender::new(tx);
        let mut terminal = match Terminal::new(TestBackend::new(100, 10)) {
            Ok(t) => t,
            Err(e) => panic!("Failed to create terminal: {e}"),
        };

        let test_cases = vec![
            ("empty", None),
            ("small", Some("short".to_string())),
            ("large", Some("z".repeat(LARGE_PASTE_CHAR_THRESHOLD + 5))),
            ("multiple_pastes", None),
            ("backspace_after_pastes", None),
        ];

        for (name, input) in test_cases {
            // Create a fresh composer for each test case
            let mut composer = ChatComposer::new(true, sender.clone(), false);

            if let Some(text) = input {
                composer.handle_paste(text);
            } else if name == "multiple_pastes" {
                // First large paste
                composer.handle_paste("x".repeat(LARGE_PASTE_CHAR_THRESHOLD + 3));
                // Second large paste
                composer.handle_paste("y".repeat(LARGE_PASTE_CHAR_THRESHOLD + 7));
                // Small paste
                composer.handle_paste(" another short paste".to_string());
            } else if name == "backspace_after_pastes" {
                // Three large pastes
                composer.handle_paste("a".repeat(LARGE_PASTE_CHAR_THRESHOLD + 2));
                composer.handle_paste("b".repeat(LARGE_PASTE_CHAR_THRESHOLD + 4));
                composer.handle_paste("c".repeat(LARGE_PASTE_CHAR_THRESHOLD + 6));
                // Move cursor to end and press backspace
                composer.textarea.set_cursor(composer.textarea.text().len());
                composer.handle_key_event(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
            }

            terminal
                .draw(|f| f.render_widget_ref(composer, f.area()))
                .unwrap_or_else(|e| panic!("Failed to draw {name} composer: {e}"));

            assert_snapshot!(name, terminal.backend());
        }
    }

    // Test helper: simulate human typing with a brief delay and flush the paste-burst buffer
    fn type_chars_humanlike(composer: &mut ChatComposer, chars: &[char]) {
        use crossterm::event::KeyCode;
        use crossterm::event::KeyEvent;
        use crossterm::event::KeyModifiers;
        for &ch in chars {
            let _ = composer.handle_key_event(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
            std::thread::sleep(ChatComposer::recommended_paste_flush_delay());
            let _ = composer.flush_paste_burst_if_due();
        }
    }

    #[test]
    fn slash_init_dispatches_command_and_does_not_submit_literal_text() {
        use crossterm::event::KeyCode;
        use crossterm::event::KeyEvent;
        use crossterm::event::KeyModifiers;
        use std::sync::mpsc::TryRecvError;

        let (tx, rx) = std::sync::mpsc::channel();
        let sender = AppEventSender::new(tx);
        let mut composer = ChatComposer::new(true, sender, false);

        // Type the slash command.
        type_chars_humanlike(&mut composer, &['/', 'i', 'n', 'i', 't']);

        // Press Enter to dispatch the selected command.
        let (result, _needs_redraw) =
            composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        // When a slash command is dispatched, the composer should return a
        // Command result (not submit literal text) and clear its textarea.
        match result {
            InputResult::None | InputResult::ScrollUp | InputResult::ScrollDown => {}
            InputResult::Submitted(text) => {
                panic!("expected command dispatch, but composer submitted literal text: {text}")
            }
            InputResult::None => panic!("expected Command result for '/init'"),
        }
        assert!(composer.textarea.is_empty(), "composer should be cleared");

        // Verify a DispatchCommand event for the "init" command was sent.
        match rx.try_recv() {
            Ok(AppEvent::DispatchCommand(cmd, text)) => {
                assert_eq!(cmd.command(), "init");
                assert_eq!(text, "/init");
            }
            Ok(_other) => panic!("unexpected app event"),
            Err(TryRecvError::Empty) => panic!("expected a DispatchCommand event for '/init'"),
            Err(TryRecvError::Disconnected) => panic!("app event channel disconnected"),
        }
    }

    #[test]
    fn slash_tab_completion_moves_cursor_to_end() {
        use crossterm::event::KeyCode;
        use crossterm::event::KeyEvent;
        use crossterm::event::KeyModifiers;

        let (tx, _rx) = std::sync::mpsc::channel();
        let sender = AppEventSender::new(tx);
        let mut composer = ChatComposer::new(
            true,
            sender,
            false,
            "Ask Codex to do anything".to_string(),
            false,
        );

        type_chars_humanlike(&mut composer, &['/', 'c']);

        let (_result, _needs_redraw) =
            composer.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));

        assert_eq!(composer.textarea.text(), "/compact ");
        assert_eq!(composer.textarea.cursor(), composer.textarea.text().len());
    }

    #[test]
    fn slash_mention_dispatches_command_and_inserts_at() {
        use crossterm::event::KeyCode;
        use crossterm::event::KeyEvent;
        use crossterm::event::KeyModifiers;
        use std::sync::mpsc::TryRecvError;

        let (tx, rx) = std::sync::mpsc::channel();
        let sender = AppEventSender::new(tx);
        let mut composer = ChatComposer::new(true, sender, false);

        type_chars_humanlike(&mut composer, &['/', 'm', 'e', 'n', 't', 'i', 'o', 'n']);

        let (result, _needs_redraw) =
            composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        match result {
            InputResult::None | InputResult::ScrollUp | InputResult::ScrollDown => {}
            InputResult::Submitted(text) => {
                panic!("expected command dispatch, but composer submitted literal text: {text}")
            }
            InputResult::None => panic!("expected Command result for '/mention'"),
        }
        assert!(composer.textarea.is_empty(), "composer should be cleared");

        match rx.try_recv() {
            Ok(AppEvent::DispatchCommand(cmd, _)) => {
                assert_eq!(cmd.command(), "mention");
                composer.insert_str("@");
            }
            Ok(_other) => panic!("unexpected app event"),
            Err(TryRecvError::Empty) => panic!("expected a DispatchCommand event for '/mention'"),
            Err(TryRecvError::Disconnected) => {
                panic!("app event channel disconnected")
            }
        }
        assert_eq!(composer.textarea.text(), "@");
    }

    #[test]
    fn test_multiple_pastes_submission() {
        use crossterm::event::KeyCode;
        use crossterm::event::KeyEvent;
        use crossterm::event::KeyModifiers;

        let (tx, _rx) = std::sync::mpsc::channel();
        let sender = AppEventSender::new(tx);
        let mut composer = ChatComposer::new(true, sender, false);

        // Define test cases: (paste content, is_large)
        let test_cases = [
            ("x".repeat(LARGE_PASTE_CHAR_THRESHOLD + 3), true),
            (" and ".to_string(), false),
            ("y".repeat(LARGE_PASTE_CHAR_THRESHOLD + 7), true),
        ];

        // Expected states after each paste
        let mut expected_text = String::new();
        let mut expected_pending_count = 0;

        // Apply all pastes and build expected state
        let states: Vec<_> = test_cases
            .iter()
            .map(|(content, is_large)| {
                composer.handle_paste(content.clone());
                if *is_large {
                    let placeholder = format!("[Pasted Content {} chars]", content.chars().count());
                    expected_text.push_str(&placeholder);
                    expected_pending_count += 1;
                } else {
                    expected_text.push_str(content);
                }
                (expected_text.clone(), expected_pending_count)
            })
            .collect();

        // Verify all intermediate states were correct
        assert_eq!(
            states,
            vec![
                (
                    format!("[Pasted Content {} chars]", test_cases[0].0.chars().count()),
                    1
                ),
                (
                    format!(
                        "[Pasted Content {} chars] and ",
                        test_cases[0].0.chars().count()
                    ),
                    1
                ),
                (
                    format!(
                        "[Pasted Content {} chars] and [Pasted Content {} chars]",
                        test_cases[0].0.chars().count(),
                        test_cases[2].0.chars().count()
                    ),
                    2
                ),
            ]
        );

        // Submit and verify final expansion
        let (result, _) =
            composer.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        if let InputResult::Submitted(text) = result {
            assert_eq!(text, format!("{} and {}", test_cases[0].0, test_cases[2].0));
        } else {
            panic!("expected Submitted");
        }
    }

    #[test]
    fn test_placeholder_deletion() {
        use crossterm::event::KeyCode;
        use crossterm::event::KeyEvent;
        use crossterm::event::KeyModifiers;

        let (tx, _rx) = std::sync::mpsc::channel();
        let sender = AppEventSender::new(tx);
        let mut composer = ChatComposer::new(true, sender, false);

        // Define test cases: (content, is_large)
        let test_cases = [
            ("a".repeat(LARGE_PASTE_CHAR_THRESHOLD + 5), true),
            (" and ".to_string(), false),
            ("b".repeat(LARGE_PASTE_CHAR_THRESHOLD + 6), true),
        ];

        // Apply all pastes
        let mut current_pos = 0;
        let states: Vec<_> = test_cases
            .iter()
            .map(|(content, is_large)| {
                composer.handle_paste(content.clone());
                if *is_large {
                    let placeholder = format!("[Pasted Content {} chars]", content.chars().count());
                    current_pos += placeholder.len();
                } else {
                    current_pos += content.len();
                }
                (
                    composer.textarea.text().to_string(),
                    composer.pending_pastes.len(),
                    current_pos,
                )
            })
            .collect();

        // Delete placeholders one by one and collect states
        let mut deletion_states = vec![];

        // First deletion
        composer.textarea.set_cursor(states[0].2);
        composer.handle_key_event(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        deletion_states.push((
            composer.textarea.text().to_string(),
            composer.pending_pastes.len(),
        ));

        // Second deletion
        composer.textarea.set_cursor(composer.textarea.text().len());
        composer.handle_key_event(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        deletion_states.push((
            composer.textarea.text().to_string(),
            composer.pending_pastes.len(),
        ));

        // Verify all states
        assert_eq!(
            deletion_states,
            vec![
                (" and [Pasted Content 1006 chars]".to_string(), 1),
                (" and ".to_string(), 0),
            ]
        );
    }

    #[test]
    fn test_partial_placeholder_deletion() {
        use crossterm::event::KeyCode;
        use crossterm::event::KeyEvent;
        use crossterm::event::KeyModifiers;

        let (tx, _rx) = std::sync::mpsc::channel();
        let sender = AppEventSender::new(tx);
        let mut composer = ChatComposer::new(true, sender, false);

        // Define test cases: (cursor_position_from_end, expected_pending_count)
        let test_cases = [
            5, // Delete from middle - should clear tracking
            0, // Delete from end - should clear tracking
        ];

        let paste = "x".repeat(LARGE_PASTE_CHAR_THRESHOLD + 4);
        let placeholder = format!("[Pasted Content {} chars]", paste.chars().count());

        let states: Vec<_> = test_cases
            .into_iter()
            .map(|pos_from_end| {
                composer.handle_paste(paste.clone());
                composer
                    .textarea
                    .set_cursor((placeholder.len() - pos_from_end) as usize);
                composer.handle_key_event(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
                let result = (
                    composer.textarea.text().contains(&placeholder),
                    composer.pending_pastes.len(),
                );
                composer.textarea.set_text("");
                result
            })
            .collect();

        assert_eq!(
            states,
            vec![
                (false, 0), // After deleting from middle
                (false, 0), // After deleting from end
            ]
        );
    }

    #[test]
    fn test_typed_anything_on_paste() {
        let (tx, _rx) = std::sync::mpsc::channel();
        let sender = AppEventSender::new(tx);
        let mut composer = ChatComposer::new(true, sender, false);

        // Initially typed_anything should be false
        assert!(!composer.typed_anything);

        // After pasting text, typed_anything should be true
        composer.handle_paste("test content".to_string());
        assert!(composer.typed_anything);
        assert_eq!(composer.textarea.text(), "test content");

        // Clear text (simulating submit) - typed_anything should remain true
        composer.clear_text();
        assert!(composer.typed_anything);
        assert_eq!(composer.textarea.text(), "");
    }

    #[test]
    fn test_typed_anything_on_large_paste() {
        let (tx, _rx) = std::sync::mpsc::channel();
        let sender = AppEventSender::new(tx);
        let mut composer = ChatComposer::new(true, sender, false);

        // Initially typed_anything should be false
        assert!(!composer.typed_anything);

        // After pasting large content with placeholder, typed_anything should be true
        let large = "x".repeat(LARGE_PASTE_CHAR_THRESHOLD + 100);
        composer.handle_paste(large.clone());
        assert!(composer.typed_anything);

        // Clear text - typed_anything should remain true
        composer.clear_text();
        assert!(composer.typed_anything);
    }

    #[test]
    fn test_typed_anything_on_insert_str() {
        let (tx, _rx) = std::sync::mpsc::channel();
        let sender = AppEventSender::new(tx);
        let mut composer = ChatComposer::new(true, sender, false);

        // Initially typed_anything should be false
        assert!(!composer.typed_anything);

        // After insert_str (e.g., from slash command), typed_anything should be true
        composer.insert_str("@file.txt");
        assert!(composer.typed_anything);
        assert_eq!(composer.textarea.text(), "@file.txt");

        // Clear text - typed_anything should remain true
        composer.clear_text();
        assert!(composer.typed_anything);
        assert_eq!(composer.textarea.text(), "");
    }

    #[test]
    fn test_placeholder_behavior_after_user_interaction() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;
        use ratatui::buffer::Buffer;

        let (tx, _rx) = std::sync::mpsc::channel();
        let sender = AppEventSender::new(tx);
        let mut composer = ChatComposer::new(true, sender, false);

        // Create a test terminal
        let backend = TestBackend::new(100, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        // Initially, placeholder should be shown (typed_anything is false, text is empty)
        terminal.draw(|f| {
            let area = f.area();
            let mut buf = Buffer::empty(area);
            composer.render_ref(area, &mut buf);
            // Check that placeholder is rendered (it would contain greeting text)
            let content = buf.content();
            let has_placeholder = content.iter().any(|cell| {
                let s = cell.symbol();
                s.contains("today") || s.contains("tonight") || s.contains("evening")
            });
            assert!(has_placeholder, "Placeholder should be visible initially");
        }).unwrap();

        // After pasting, placeholder should not be shown even if text is cleared
        composer.handle_paste("test".to_string());
        composer.clear_text();

        terminal.draw(|f| {
            let area = f.area();
            let mut buf = Buffer::empty(area);
            composer.render_ref(area, &mut buf);
            // Check that placeholder is NOT rendered
            let content = buf.content();
            let has_placeholder = content.iter().any(|cell| {
                let s = cell.symbol();
                s.contains("today") || s.contains("tonight") || s.contains("evening")
            });
            assert!(!has_placeholder, "Placeholder should not be visible after user interaction");
        }).unwrap();
    }

    #[test]
    fn test_new_session_resets_typed_anything() {
        let (tx, _rx) = std::sync::mpsc::channel();
        let sender = AppEventSender::new(tx);
        let mut composer = ChatComposer::new(true, sender.clone(), false);

        // Type something
        composer.insert_str("test");
        assert!(composer.typed_anything);

        // Simulate new session by creating a new composer (this is what happens on /new)
        let composer2 = ChatComposer::new(true, sender, false);
        assert!(!composer2.typed_anything, "New session should reset typed_anything");
    }
}
