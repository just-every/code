use std::time::Duration;
use std::time::Instant;

// Heuristic thresholds for detecting paste-like input bursts.
// Detect quickly to avoid showing typed prefix before paste is recognized
#[allow(dead_code)]
const PASTE_BURST_MIN_CHARS: u16 = 3;
const PASTE_BURST_CHAR_INTERVAL: Duration = Duration::from_millis(8);
#[allow(dead_code)]
const PASTE_ENTER_SUPPRESS_WINDOW: Duration = Duration::from_millis(120);

#[derive(Default)]
pub(crate) struct PasteBurst {
    last_plain_char_time: Option<Instant>,
    #[allow(dead_code)]
    consecutive_plain_char_burst: u16,
    #[allow(dead_code)]
    burst_window_until: Option<Instant>,
    buffer: String,
    active: bool,
    // Hold first fast char briefly to avoid rendering flicker
    pending_first_char: Option<(char, Instant)>,
}

#[allow(dead_code)]
pub(crate) enum CharDecision {
    /// Start buffering and retroactively capture some already-inserted chars.
    BeginBuffer { retro_chars: u16 },
    /// We are currently buffering; append the current char into the buffer.
    BufferAppend,
    /// Do not insert/render this char yet; temporarily save the first fast
    /// char while we wait to see if a paste-like burst follows.
    RetainFirstChar,
    /// Begin buffering using the previously saved first char (no retro grab needed).
    BeginBufferFromPending,
}

#[allow(dead_code)]
pub(crate) struct RetroGrab {
    pub start_byte: usize,
    pub grabbed: String,
}

impl PasteBurst {
    /// Lightweight path: record an unmodified character keystroke to help
    /// detect paste-like bursts even when bracketed paste is unavailable.
    ///
    /// When several plain characters arrive within `PASTE_BURST_CHAR_INTERVAL`,
    /// we open a short window during which `Enter` will be treated as a newline
    /// insert instead of a submit. This prevents multi-line pastes from firing
    /// off multiple submissions when terminals fall back to per-key delivery.
    pub fn record_plain_char_for_enter_window(&mut self, now: Instant) {
        let within_interval = self
            .last_plain_char_time
            .is_some_and(|prev| now.duration_since(prev) <= PASTE_BURST_CHAR_INTERVAL);

        match (self.last_plain_char_time, within_interval) {
            (_, true) => {
                self.consecutive_plain_char_burst =
                    self.consecutive_plain_char_burst.saturating_add(1);
            }
            _ => {
                self.consecutive_plain_char_burst = 1;
            }
        }
        self.last_plain_char_time = Some(now);

        // Only arm Enter suppression when input timing looks paste-like: either
        // chars are arriving inside the burst interval or we already detected
        // a longer burst.
        if within_interval || self.consecutive_plain_char_burst >= PASTE_BURST_MIN_CHARS {
            self.burst_window_until = Some(now + PASTE_ENTER_SUPPRESS_WINDOW);
        } else {
            self.burst_window_until = None;
        }
    }

    /// Return true when a recent burst suggests Enter should insert a newline
    /// instead of submitting the composer.
    pub fn enter_should_insert_newline(&self, now: Instant) -> bool {
        self.burst_window_until
            .is_some_and(|until| now <= until)
    }

    /// True when the most recent plain char arrived within the burst interval.
    pub fn recent_plain_char(&self, now: Instant) -> bool {
        self.last_plain_char_time
            .is_some_and(|prev| now.duration_since(prev) <= PASTE_BURST_CHAR_INTERVAL)
    }

    /// Keep the Enter-suppression window alive for subsequent newlines in the
    /// same paste burst.
    pub fn extend_enter_window(&mut self, now: Instant) {
        self.burst_window_until = Some(now + PASTE_ENTER_SUPPRESS_WINDOW);
    }

    /// Clear the Enter guard when encountering non-character input paths.
    pub fn clear_enter_window(&mut self) {
        self.consecutive_plain_char_burst = 0;
        self.last_plain_char_time = None;
        self.burst_window_until = None;
        self.pending_first_char = None;
    }

    /// Recommended delay to wait between simulated keypresses (or before
    /// scheduling a UI tick) so that a pending fast keystroke is flushed
    /// out of the burst detector as normal typed input.
    ///
    /// Primarily used by tests and by the TUI to reliably cross the
    /// paste-burst timing threshold.
    #[allow(dead_code)]
    pub fn recommended_flush_delay() -> Duration {
        PASTE_BURST_CHAR_INTERVAL + Duration::from_millis(1)
    }

    /// Entry point: decide how to treat a plain char with current timing.
    #[allow(dead_code)]
    pub fn on_plain_char(&mut self, ch: char, now: Instant) -> CharDecision {
        match self.last_plain_char_time {
            Some(prev) if now.duration_since(prev) <= PASTE_BURST_CHAR_INTERVAL => {
                self.consecutive_plain_char_burst =
                    self.consecutive_plain_char_burst.saturating_add(1)
            }
            _ => self.consecutive_plain_char_burst = 1,
        }
        self.last_plain_char_time = Some(now);

        if self.active {
            self.burst_window_until = Some(now + PASTE_ENTER_SUPPRESS_WINDOW);
            return CharDecision::BufferAppend;
        }

        // If we already held a first char and receive a second fast char,
        // start buffering without retro-grabbing (we never rendered the first).
        if let Some((held, held_at)) = self.pending_first_char {
            if now.duration_since(held_at) <= PASTE_BURST_CHAR_INTERVAL {
                self.active = true;
                // take() to clear pending; we already captured the held char above
                let _ = self.pending_first_char.take();
                self.buffer.push(held);
                self.burst_window_until = Some(now + PASTE_ENTER_SUPPRESS_WINDOW);
                return CharDecision::BeginBufferFromPending;
            }
        }

        if self.consecutive_plain_char_burst >= PASTE_BURST_MIN_CHARS {
            return CharDecision::BeginBuffer {
                retro_chars: self.consecutive_plain_char_burst.saturating_sub(1),
            };
        }

        // Save the first fast char very briefly to see if a burst follows.
        self.pending_first_char = Some((ch, now));
        CharDecision::RetainFirstChar
    }

    /// Flush the buffered burst if the inter-key timeout has elapsed.
    ///
    /// Returns Some(String) when either:
    /// - We were actively buffering paste-like input and the buffer is now
    ///   emitted as a single pasted string; or
    /// - We had saved a single fast first-char with no subsequent burst and we
    ///   now emit that char as normal typed input.
    ///
    /// Returns None if the timeout has not elapsed or there is nothing to flush.
    pub fn flush_if_due(&mut self, now: Instant) -> Option<String> {
        let timed_out = self
            .last_plain_char_time
            .is_some_and(|t| now.duration_since(t) > PASTE_BURST_CHAR_INTERVAL);
        if timed_out && self.is_active_internal() {
            self.active = false;
            let out = std::mem::take(&mut self.buffer);
            Some(out)
        } else if timed_out {
            // If we were saving a single fast char and no burst followed,
            // flush it as normal typed input.
            if let Some((ch, _at)) = self.pending_first_char.take() {
                Some(ch.to_string())
            } else {
                None
            }
        } else {
            None
        }
    }

    /// While bursting: accumulate a newline into the buffer instead of
    /// submitting the textarea.
    ///
    /// Returns true if a newline was appended (we are in a burst context),
    /// false otherwise.
    #[allow(dead_code)]
    pub fn append_newline_if_active(&mut self, now: Instant) -> bool {
        if self.is_active() {
            self.buffer.push('\n');
            self.burst_window_until = Some(now + PASTE_ENTER_SUPPRESS_WINDOW);
            true
        } else {
            false
        }
    }

    /// Decide if Enter should insert a newline (burst context) vs submit.
    #[allow(dead_code)]
    pub fn newline_should_insert_instead_of_submit(&self, now: Instant) -> bool {
        let in_burst_window = self.burst_window_until.is_some_and(|until| now <= until);
        self.is_active() || in_burst_window
    }

    /// Keep the burst window alive.
    #[allow(dead_code)]
    pub fn extend_window(&mut self, now: Instant) {
        self.burst_window_until = Some(now + PASTE_ENTER_SUPPRESS_WINDOW);
    }

    /// Begin buffering with retroactively grabbed text.
    #[allow(dead_code)]
    pub fn begin_with_retro_grabbed(&mut self, grabbed: String, now: Instant) {
        if !grabbed.is_empty() {
            self.buffer.push_str(&grabbed);
        }
        self.active = true;
        self.burst_window_until = Some(now + PASTE_ENTER_SUPPRESS_WINDOW);
    }

    /// Append a char into the burst buffer.
    #[allow(dead_code)]
    pub fn append_char_to_buffer(&mut self, ch: char, now: Instant) {
        self.buffer.push(ch);
        self.burst_window_until = Some(now + PASTE_ENTER_SUPPRESS_WINDOW);
    }

    /// Decide whether to begin buffering by retroactively capturing recent
    /// chars from the slice before the cursor.
    ///
    /// Heuristic: if the retro-grabbed slice contains any whitespace or is
    /// sufficiently long (>= 16 characters), treat it as paste-like to avoid
    /// rendering the typed prefix momentarily before the paste is recognized.
    /// This favors responsiveness and prevents flicker for typical pastes
    /// (URLs, file paths, multiline text) while not triggering on short words.
    ///
    /// Returns Some(RetroGrab) with the start byte and grabbed text when we
    /// decide to buffer retroactively; otherwise None.
    #[allow(dead_code)]
    pub fn decide_begin_buffer(
        &mut self,
        now: Instant,
        before: &str,
        retro_chars: usize,
    ) -> Option<RetroGrab> {
        let start_byte = retro_start_index(before, retro_chars);
        let grabbed = before[start_byte..].to_string();
        let looks_pastey =
            grabbed.chars().any(char::is_whitespace) || grabbed.chars().count() >= 16;
        if looks_pastey {
            // Note: caller is responsible for removing this slice from UI text.
            self.begin_with_retro_grabbed(grabbed.clone(), now);
            Some(RetroGrab {
                start_byte,
                grabbed,
            })
        } else {
            None
        }
    }

    /// Before applying modified/non-char input: flush buffered burst immediately.
    pub fn flush_before_modified_input(&mut self) -> Option<String> {
        if self.is_active() {
            self.active = false;
            Some(std::mem::take(&mut self.buffer))
        } else {
            None
        }
    }

    /// Clear only the timing window and any pending first-char.
    ///
    /// Does not emit or clear the buffered text itself; callers should have
    /// already flushed (if needed) via one of the flush methods above.
    #[allow(dead_code)]
    pub fn clear_window_after_non_char(&mut self) {
        self.consecutive_plain_char_burst = 0;
        self.last_plain_char_time = None;
        self.burst_window_until = None;
        self.active = false;
        self.pending_first_char = None;
    }

    /// Returns true if we are in any paste-burst related transient state
    /// (actively buffering, have a non-empty buffer, or have saved the first
    /// fast char while waiting for a potential burst).
    pub fn is_active(&self) -> bool {
        self.is_active_internal() || self.pending_first_char.is_some()
    }

    fn is_active_internal(&self) -> bool {
        self.active || !self.buffer.is_empty()
    }

    #[allow(dead_code)]
    pub fn clear_after_explicit_paste(&mut self) {
        self.last_plain_char_time = None;
        self.consecutive_plain_char_burst = 0;
        self.burst_window_until = None;
        self.active = false;
        self.buffer.clear();
        self.pending_first_char = None;
    }
}

#[allow(dead_code)]
pub(crate) fn retro_start_index(before: &str, retro_chars: usize) -> usize {
    if retro_chars == 0 {
        return before.len();
    }
    before
        .char_indices()
        .rev()
        .nth(retro_chars.saturating_sub(1))
        .map(|(idx, _)| idx)
        .unwrap_or(0)
}
