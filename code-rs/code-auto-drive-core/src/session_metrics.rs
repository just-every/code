use std::collections::VecDeque;

use code_core::protocol::TokenUsage;

const DEFAULT_PROMPT_ESTIMATE: u64 = 4_000;

#[derive(Debug, Clone)]
pub struct SessionMetrics {
    running_total: TokenUsage,
    last_turn: TokenUsage,
    turn_count: u32,
    replay_updates: u32,
    duplicate_items: u32,
    recent_prompt_tokens: VecDeque<u64>,
    window: usize,
}

impl Default for SessionMetrics {
    fn default() -> Self {
        Self::new(3)
    }
}

impl SessionMetrics {
    pub fn new(window: usize) -> Self {
        Self {
            running_total: TokenUsage::default(),
            last_turn: TokenUsage::default(),
            turn_count: 0,
            replay_updates: 0,
            duplicate_items: 0,
            recent_prompt_tokens: VecDeque::with_capacity(window),
            window: window.max(1),
        }
    }

    pub fn record_turn(&mut self, usage: &TokenUsage) {
        self.running_total.add_assign(usage);
        self.last_turn = usage.clone();
        self.turn_count = self.turn_count.saturating_add(1);
        self.push_prompt_observation(usage.non_cached_input());
    }

    pub fn record_turn_without_usage(&mut self, estimated_prompt_tokens: u64) {
        self.turn_count = self.turn_count.saturating_add(1);
        self.push_prompt_observation(estimated_prompt_tokens);
    }

    pub fn sync_absolute(&mut self, total: TokenUsage, last: TokenUsage, turn_count: u32) {
        self.running_total = total;
        self.last_turn = last.clone();
        self.turn_count = turn_count;
        self.replay_updates = 0;
        self.duplicate_items = 0;
        self.recent_prompt_tokens.clear();
        self.push_prompt_observation(last.non_cached_input());
    }

    pub fn running_total(&self) -> &TokenUsage {
        &self.running_total
    }

    pub fn last_turn(&self) -> &TokenUsage {
        &self.last_turn
    }

    pub fn turn_count(&self) -> u32 {
        self.turn_count
    }

    pub fn has_recorded_usage(&self) -> bool {
        !self.running_total.is_zero() || !self.last_turn.is_zero()
    }

    pub fn blended_total(&self) -> u64 {
        self.running_total.blended_total()
    }

    pub fn estimated_next_prompt_tokens(&self) -> u64 {
        if !self.recent_prompt_tokens.is_empty() {
            let sum: u64 = self.recent_prompt_tokens.iter().copied().sum();
            return sum / self.recent_prompt_tokens.len() as u64;
        }
        let fallback = self.last_turn.non_cached_input();
        if fallback > 0 {
            fallback
        } else {
            DEFAULT_PROMPT_ESTIMATE
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new(self.window);
    }

    pub fn record_replay(&mut self) {
        self.replay_updates = self.replay_updates.saturating_add(1);
    }

    pub fn replay_updates(&self) -> u32 {
        self.replay_updates
    }

    pub fn record_duplicate_items(&mut self, count: usize) {
        if count == 0 {
            return;
        }
        self.duplicate_items = self
            .duplicate_items
            .saturating_add(count.min(u32::MAX as usize) as u32);
    }

    pub fn set_duplicate_items(&mut self, count: u32) {
        self.duplicate_items = count;
    }

    pub fn set_replay_updates(&mut self, count: u32) {
        self.replay_updates = count;
    }

    pub fn duplicate_items(&self) -> u32 {
        self.duplicate_items
    }

    /// Returns a loop detection warning message if the session shows signs of
    /// repetitive behavior (replays or high duplicate counts). Returns `None`
    /// if no concerning patterns are detected.
    ///
    /// This guidance is intended to be injected into the coordinator's context
    /// to help it break out of unproductive loops.
    pub fn loop_detection_warning(&self) -> Option<String> {
        let replays = self.replay_updates;
        let duplicates = self.duplicate_items;

        const REPLAY_WARNING_THRESHOLD: u32 = 2;
        const REPLAY_CRITICAL_THRESHOLD: u32 = 4;
        const DUPLICATE_WARNING_THRESHOLD: u32 = 3;

        if replays >= REPLAY_CRITICAL_THRESHOLD {
            return Some(format!(
                "LOOP DETECTED: {replays} consecutive replay attempts observed. \
                The same commands are being issued repeatedly without progress. \
                STOP and reassess: (1) Check if the task is already complete, \
                (2) Try a fundamentally different approach, \
                (3) If stuck, use finish_failed with a clear explanation of the blocker."
            ));
        }

        if replays >= REPLAY_WARNING_THRESHOLD {
            return Some(format!(
                "Potential loop detected: {replays} replay attempts observed. \
                Recent actions may be repeating. Consider: \
                (1) Verifying actual progress was made, \
                (2) Trying a different strategy if the current approach isn't working, \
                (3) Checking for already-completed conditions before retrying."
            ));
        }

        if duplicates >= DUPLICATE_WARNING_THRESHOLD {
            return Some(format!(
                "Repetition detected: {duplicates} duplicate items in conversation history. \
                This may indicate a stuck state. Ensure each action produces new, \
                meaningful progress toward the goal."
            ));
        }

        None
    }

    fn push_prompt_observation(&mut self, tokens: u64) {
        if tokens == 0 {
            return;
        }
        if self.recent_prompt_tokens.len() == self.window {
            self.recent_prompt_tokens.pop_front();
        }
        self.recent_prompt_tokens.push_back(tokens);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(input: u64, output: u64) -> TokenUsage {
        TokenUsage {
            input_tokens: input,
            cached_input_tokens: 0,
            output_tokens: output,
            reasoning_output_tokens: 0,
            total_tokens: input + output,
        }
    }

    #[test]
    fn record_turn_tracks_totals_and_estimate() {
        let mut metrics = SessionMetrics::default();
        metrics.record_turn(&usage(1_000, 500));
        metrics.record_turn(&usage(4_000, 2_000));

        assert_eq!(metrics.turn_count(), 2);
        assert_eq!(metrics.running_total().input_tokens, 5_000);
        assert_eq!(metrics.running_total().output_tokens, 2_500);

        // Average of observed prompt tokens (non-cached input)
        assert_eq!(metrics.estimated_next_prompt_tokens(), 2_500);
        assert_eq!(metrics.duplicate_items(), 0);
        assert_eq!(metrics.replay_updates(), 0);
    }

    #[test]
    fn record_turn_without_usage_does_not_mark_usage() {
        let mut metrics = SessionMetrics::default();
        assert!(!metrics.has_recorded_usage());

        metrics.record_turn_without_usage(2_000);
        assert!(!metrics.has_recorded_usage());

        metrics.record_turn(&usage(1_000, 500));
        assert!(metrics.has_recorded_usage());
    }

    #[test]
    fn sync_absolute_resets_window() {
        let mut metrics = SessionMetrics::default();
        metrics.record_turn(&usage(1_000, 500));
        metrics.sync_absolute(usage(10_000, 4_000), usage(3_000, 1_000), 3);

        assert_eq!(metrics.turn_count(), 3);
        assert_eq!(metrics.running_total().input_tokens, 10_000);
        assert_eq!(metrics.last_turn().input_tokens, 3_000);
        assert_eq!(metrics.estimated_next_prompt_tokens(), 3_000);
        assert_eq!(metrics.duplicate_items(), 0);
        assert_eq!(metrics.replay_updates(), 0);
    }

    #[test]
    fn record_replay_increments_counter() {
        let mut metrics = SessionMetrics::default();
        metrics.record_replay();
        metrics.record_replay();
        assert_eq!(metrics.replay_updates(), 2);
    }

    #[test]
    fn loop_detection_warning_returns_none_when_no_issues() {
        let metrics = SessionMetrics::default();
        assert!(metrics.loop_detection_warning().is_none());
    }

    #[test]
    fn loop_detection_warning_at_replay_warning_threshold() {
        let mut metrics = SessionMetrics::default();
        metrics.record_replay();
        assert!(metrics.loop_detection_warning().is_none());

        metrics.record_replay();
        let warning = metrics.loop_detection_warning();
        assert!(warning.is_some());
        assert!(warning.unwrap().contains("Potential loop detected"));
    }

    #[test]
    fn loop_detection_warning_at_replay_critical_threshold() {
        let mut metrics = SessionMetrics::default();
        for _ in 0..4 {
            metrics.record_replay();
        }
        let warning = metrics.loop_detection_warning();
        assert!(warning.is_some());
        let warning_text = warning.unwrap();
        assert!(warning_text.contains("LOOP DETECTED"));
        assert!(warning_text.contains("4 consecutive replay"));
    }

    #[test]
    fn loop_detection_warning_on_duplicate_items() {
        let mut metrics = SessionMetrics::default();
        metrics.record_duplicate_items(2);
        assert!(metrics.loop_detection_warning().is_none());

        metrics.record_duplicate_items(1);
        let warning = metrics.loop_detection_warning();
        assert!(warning.is_some());
        assert!(warning.unwrap().contains("Repetition detected"));
    }

    #[test]
    fn loop_detection_warning_replay_takes_priority_over_duplicates() {
        let mut metrics = SessionMetrics::default();
        metrics.record_duplicate_items(5);
        metrics.record_replay();
        metrics.record_replay();

        let warning = metrics.loop_detection_warning();
        assert!(warning.is_some());
        assert!(warning.unwrap().contains("Potential loop detected"));
    }
}
