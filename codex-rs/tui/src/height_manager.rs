//! Centralized vertical layout management for the chat UI.
//!
//! This module provides a HeightManager that stabilizes per-frame layout by
//! applying small-change hysteresis and quantized HUD heights. It is designed
//! to be minimally invasive and can be enabled via an environment flag.

use ratatui::layout::{Constraint, Layout, Rect};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HeightEvent {
    Resize,
    HudToggle(bool),
    ComposerModeChange,
    HistoryFinalize,
    RunBegin,
    RunEnd,
    UserScroll,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct HeightManagerConfig {
    /// Number of consecutive frames to tolerate +/-1 row changes.
    pub hysteresis_n: u8,
    /// HUD height is rounded to this quantum (rows).
    pub hud_quantum: u16,
    /// Frames that a quantized HUD change must persist before applying.
    pub hud_confirm_frames: u8,
    /// Max bottom pane height as percent of total terminal height.
    pub bottom_percent_cap: u8,
}

impl Default for HeightManagerConfig {
    fn default() -> Self {
        Self {
            hysteresis_n: 2,
            hud_quantum: 2,
            hud_confirm_frames: 2,
            // Allow taller bottom popups (e.g., editors) when needed
            bottom_percent_cap: 60,
        }
    }
}

#[cfg(debug_assertions)]
#[derive(Default)]
struct DevCounters {
    frames: u64,
    _hysteresis_applied: u64,
    hud_quantized: u64,
}

pub(crate) struct HeightManager {
    cfg: HeightManagerConfig,
    last_area: Option<Rect>,
    last_bottom: Option<u16>,
    last_hud: Option<u16>,
    /// Tracks frames where a small (+/-1) change was ignored.
    small_change_count: u8,
    /// Tracks frames to confirm a 1-row bottom pane decrease before applying.
    bottom_small_change_count: u8,
    /// Pending HUD value with confirmation counter.
    hud_pending: Option<(u16, u8)>,
    /// When set by an explicit event, bypass hysteresis once.
    bypass_once: bool,
    #[cfg(debug_assertions)]
    counters: DevCounters,
}

impl HeightManager {
    pub(crate) fn new(cfg: HeightManagerConfig) -> Self {
        Self {
            cfg,
            last_area: None,
            last_bottom: None,
            last_hud: None,
            small_change_count: 0,
            bottom_small_change_count: 0,
            hud_pending: None,
            bypass_once: false,
            #[cfg(debug_assertions)]
            counters: DevCounters::default(),
        }
    }

    pub(crate) fn record_event(&mut self, event: HeightEvent) {
        match event {
            HeightEvent::Resize
            | HeightEvent::HudToggle(_)
            | HeightEvent::ComposerModeChange
            | HeightEvent::HistoryFinalize
            | HeightEvent::RunBegin
            | HeightEvent::RunEnd => {
                // Next begin_frame applies new values immediately.
                self.bypass_once = true;
                // Clear lingering small-change counters so we do not suppress real updates.
                self.small_change_count = 0;
                // Also clear HUD pending confirmation on explicit events.
                self.hud_pending = None;
            }
            HeightEvent::UserScroll => {
                // Scrolling should not force a jump, but do clear small-change accumulation.
                self.small_change_count = 0;
            }
        }
    }

    /// Compute the vertical layout for this frame. The bottom pane desired
    /// height is provided by the caller. If `hud_present` is true, a HUD area
    /// will be included with a quantized and stabilized height.
    pub(crate) fn begin_frame(
        &mut self,
        area: Rect,
        hud_present: bool,
        bottom_desired_height: u16,
        font_cell: (u16, u16),
        // Optional target height for HUD computed by caller (e.g., stacked/collapsed layout).
        // When None, a default aspect-based calculation is used.
        hud_target_override: Option<u16>,
        // When false, do not reserve rows for the status bar.
        status_enabled: bool,
    ) -> Vec<Rect> {
        #[cfg(debug_assertions)]
        {
            self.counters.frames += 1;
        }

        // Detect resize and treat as an explicit event.
        if self.last_area.map(|r| (r.width, r.height)) != Some((area.width, area.height)) {
            self.record_event(HeightEvent::Resize);
            self.last_area = Some(area);
        }

        // Status bar height is fixed at 3 when enabled.
        let status_h = if status_enabled { 3u16 } else { 0u16 };

        // Cap the bottom pane to a percentage of screen height, with a minimum of 5 rows.
        let percent_cap: u16 = ((area.height as u32).saturating_mul(self.cfg.bottom_percent_cap as u32) / 100) as u16;
        let bottom_cap = percent_cap.max(5);
        let desired = bottom_desired_height.max(5).min(bottom_cap);

        // Bottom pane policy: Grow immediately, confirm small decreases over a few frames
        let bottom_h = match (self.last_bottom, self.bypass_once) {
            (Some(_), true) => desired,
            (Some(prev), false) => {
                if desired > prev {
                    // grow immediately, reset counter
                    self.bottom_small_change_count = 0;
                    desired
                } else if desired < prev {
                    let diff = prev - desired;
                    if diff == 1 {
                        // Require N consecutive frames before accepting a 1-row shrink
                        if self.bottom_small_change_count + 1 >= self.cfg.hysteresis_n {
                            self.bottom_small_change_count = 0;
                            desired
                        } else {
                            self.bottom_small_change_count = self.bottom_small_change_count.saturating_add(1);
                            prev
                        }
                    } else {
                        // Larger decreases apply immediately
                        self.bottom_small_change_count = 0;
                        desired
                    }
                } else {
                    // unchanged
                    self.bottom_small_change_count = 0;
                    prev
                }
            }
            (None, _) => desired,
        };
        self.last_bottom = Some(bottom_h);
        // Clear bypass after use
        if self.bypass_once { self.bypass_once = false; }

        // Determine HUD height if present.
        let mut hud_h: u16;
        if hud_present {
            let override_target = hud_target_override.is_some();
            // Use caller-provided target when available; otherwise fall back to
            // an aspect-based estimate similar to the older preview logic.
            let mut target = if let Some(t) = hud_target_override { t } else {
                // Compute HUD target height using 16:9 aspect on full inner width.
                let padded_area = Rect { x: area.x + 1, y: area.y, width: area.width.saturating_sub(2), height: area.height };
                let inner_cols = padded_area.width.saturating_sub(2);
                let (cw, ch) = font_cell;
                let number = (inner_cols as u32) * 3 * (cw as u32);
                let denom = 4 * (ch as u32);
                ((number / denom) as u16).saturating_add(1) // include borders budget
            };

            // Keep within budget: reserve space for status + bottom + >=1 row history.
            let vertical_budget = area
                .height
                .saturating_sub(status_h)
                .saturating_sub(bottom_h)
                .saturating_sub(1);
            let min_height = if override_target { 3 } else { 4 };
            if vertical_budget < min_height {
                target = vertical_budget;
            } else {
                target = target.clamp(min_height, vertical_budget);
            }

            let quantized = if override_target {
                target
            } else {
                // Quantize to configured row quantum.
                let q = self.cfg.hud_quantum.max(1);
                let floored = (target / q) * q;
                if floored == 0 { q } else { floored }
            };

            // Require consecutive-frame confirmation for HUD changes unless bypassed.
            hud_h = self.apply_hud_confirmation(quantized);
        } else {
            // Clear HUD state when not present.
            self.hud_pending = None;
            self.last_hud = None;
            hud_h = 0;
        }

        // Ensure the history area has at least one row; reduce HUD first if needed.
        let min_history = 1u16;
        let total_non_history = status_h + bottom_h + hud_h;
        if total_non_history.saturating_add(min_history) > area.height {
            let overflow = total_non_history.saturating_add(min_history) - area.height;
            if hud_h > 0 {
                let reduce = hud_h.min(overflow);
                hud_h -= reduce;
            }
        }

        // Build rects in the same order used by ChatWidget::layout_areas.
        if hud_h > 0 {
            Layout::vertical([
                Constraint::Length(status_h),
                Constraint::Length(hud_h),
                Constraint::Fill(1),
                Constraint::Length(bottom_h),
            ])
            .areas::<4>(area)
            .to_vec()
        } else {
            Layout::vertical([
                Constraint::Length(status_h),
                Constraint::Fill(1),
                Constraint::Length(bottom_h),
            ])
            .areas::<3>(area)
            .to_vec()
        }
    }

    fn apply_hud_confirmation(&mut self, quantized: u16) -> u16 {
        if self.bypass_once {
            self.bypass_once = false;
            self.hud_pending = None;
            self.last_hud = Some(quantized);
            return quantized;
        }

        match (self.last_hud, self.hud_pending) {
            (Some(last), None) if last == quantized => last,
            (Some(last), None) if last != quantized => {
                // Start confirmation window.
                self.hud_pending = Some((quantized, 1));
                last
            }
            // Fallback for guarded patterns above to satisfy exhaustiveness.
            (Some(last), None) => last,
            (Some(last), Some((pending, n))) => {
                if pending == quantized {
                    if n + 1 >= self.cfg.hud_confirm_frames {
                        // Apply and clear pending.
                        #[cfg(debug_assertions)]
                        {
                            self.counters.hud_quantized += 1;
                        }
                        self.hud_pending = None;
                        self.last_hud = Some(quantized);
                        quantized
                    } else {
                        self.hud_pending = Some((pending, n + 1));
                        last
                    }
                } else {
                    // Changed again; restart confirmation.
                    self.hud_pending = Some((quantized, 1));
                    last
                }
            }
            (None, _) => {
                // First value applies immediately.
                self.hud_pending = None;
                self.last_hud = Some(quantized);
                quantized
            }
        }
    }
}

// Centralized HeightManager is always enabled.
