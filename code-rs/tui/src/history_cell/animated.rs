use super::*;
use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

pub(crate) struct AnimatedWelcomeCell {
    start_time: Instant,
    completed: Cell<bool>,
    fade_start: RefCell<Option<Instant>>,
    faded_out: Cell<bool>,
    available_height: Cell<Option<u16>>,
    variant: Cell<Option<crate::glitch_animation::IntroArtSize>>,
    version_label: String,
    hidden: Cell<bool>,
}

impl AnimatedWelcomeCell {
    pub(crate) fn new() -> Self {
        Self {
            start_time: Instant::now(),
            completed: Cell::new(false),
            fade_start: RefCell::new(None),
            faded_out: Cell::new(false),
            available_height: Cell::new(None),
            variant: Cell::new(None),
            version_label: format!("v{}", code_version::version()),
            hidden: Cell::new(false),
        }
    }

    pub(crate) fn set_available_height(&self, height: u16) {
        let prev = self.available_height.get();
        if prev.map_or(true, |current| height > current) {
            self.available_height.set(Some(height));
        }
    }

    fn fade_start(&self) -> Option<Instant> {
        *self.fade_start.borrow()
    }

    fn set_fade_start(&self) {
        let mut slot = self.fade_start.borrow_mut();
        if slot.is_none() {
            *slot = Some(Instant::now());
        }
    }

    pub(crate) fn begin_fade(&self) {
        self.set_fade_start();
    }

    pub(crate) fn should_remove(&self) -> bool {
        self.faded_out.get()
    }

}

impl HistoryCell for AnimatedWelcomeCell {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn kind(&self) -> HistoryCellType {
        HistoryCellType::AnimatedWelcome
    }

    fn display_lines(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(""),
            Line::from("Welcome to Code"),
            Line::from(crate::greeting::greeting_placeholder()),
            Line::from(""),
        ]
    }

    fn desired_height(&self, width: u16) -> u16 {
        let variant_for_width = crate::glitch_animation::intro_art_size_for_width(width);
        let h = crate::glitch_animation::intro_art_height(variant_for_width);
        h.saturating_add(3)
    }

    fn has_custom_render(&self) -> bool {
        true
    }

    fn custom_render(&self, area: Rect, buf: &mut Buffer) {
        self.custom_render_with_skip(area, buf, 0);
    }

    fn custom_render_with_skip(&self, area: Rect, buf: &mut Buffer, skip_rows: u16) {
        if self.hidden.get() {
            return;
        }

        // Clear the full allocated area first so repositioning the art (e.g.,
        // bottom-aligning when there's vertical slack) doesn't leave stale
        // pixels behind in the padding rows.
        let bg = crate::colors::background();
        for y in area.y..area.y.saturating_add(area.height) {
            for x in area.x..area.x.saturating_add(area.width) {
                let cell = &mut buf[(x, y)];
                cell.set_bg(bg);
                cell.set_symbol(" ");
            }
        }

        let height_hint = self.available_height.get().unwrap_or(area.height);
        let current_variant = crate::glitch_animation::intro_art_size_for_area(
            area.width,
            height_hint.saturating_sub(3),
        );
        let previous_variant = self.variant.get();
        let variant_changed = previous_variant.map_or(false, |v| v != current_variant);

        if variant_changed {
            self.variant.set(Some(current_variant));
            // Keep `completed` as-is so the intro animation continues when the
            // size adjusts mid-run instead of jumping to the final frame.
        } else if previous_variant.is_none() {
            // First render: set the variant without suppressing the intro animation.
            self.variant.set(Some(current_variant));
        }

        let variant_for_render = current_variant;

        let art_height = crate::glitch_animation::intro_art_height(current_variant);
        let full_height = art_height.saturating_add(3);
        // Prefer the logo low in the cell so spare lines sit above it. Keep a
        // small bottom gap (up to 2 rows) when there's room; otherwise center.
        let slack = full_height.saturating_sub(art_height);
        let bottom_pad = slack.min(2);
        let top_pad = slack.saturating_sub(bottom_pad);
        let art_top = top_pad;
        let art_bottom = art_top.saturating_add(art_height);
        let vis_top = skip_rows;
        let vis_bottom = skip_rows.saturating_add(area.height);
        let intersection_top = art_top.max(vis_top);
        let intersection_bottom = art_bottom.min(vis_bottom);
        if intersection_bottom <= intersection_top {
            return;
        }
        let row_offset = intersection_top.saturating_sub(art_top);
        let y_offset = intersection_top.saturating_sub(vis_top);
        let visible_height = intersection_bottom.saturating_sub(intersection_top);
        let positioned_area = Rect {
            x: area.x,
            y: area.y.saturating_add(y_offset),
            width: area.width,
            height: visible_height,
        };

        let fade_duration = Duration::from_millis(800);

        if let Some(fade_time) = self.fade_start() {
            let fade_elapsed = fade_time.elapsed();
            if fade_elapsed < fade_duration && !self.faded_out.get() {
                let fade_progress = fade_elapsed.as_secs_f32() / fade_duration.as_secs_f32();
                let alpha = 1.0 - fade_progress;
                crate::glitch_animation::render_intro_animation_with_size_and_alpha_offset(
                    positioned_area,
                    buf,
                    1.0,
                    alpha,
                    current_variant,
                    &self.version_label,
                    row_offset,
                );
            } else {
                self.faded_out.set(true);
            }
            return;
        }

        let animation_duration = Duration::from_secs(2);

        let elapsed = self.start_time.elapsed();
        let progress = if variant_changed {
            1.0
        } else if elapsed < animation_duration && !self.completed.get() {
            elapsed.as_secs_f32() / animation_duration.as_secs_f32()
        } else {
            self.completed.set(true);
            1.0
        };

        crate::glitch_animation::render_intro_animation_with_size_and_alpha_offset(
            positioned_area,
            buf,
            progress,
            1.0,
            variant_for_render,
            &self.version_label,
            row_offset,
        );
    }

    fn is_animating(&self) -> bool {
        let animation_duration = Duration::from_secs(2);
        if !self.completed.get() {
            if self.start_time.elapsed() < animation_duration {
                return true;
            }
            self.completed.set(true);
        }

        if let Some(fade_time) = self.fade_start() {
            if !self.faded_out.get() {
                if fade_time.elapsed() < Duration::from_millis(800) {
                    return true;
                }
                self.faded_out.set(true);
            }
        }

        false
    }

    fn trigger_fade(&self) {
        AnimatedWelcomeCell::begin_fade(self);
    }

    fn should_remove(&self) -> bool {
        AnimatedWelcomeCell::should_remove(self)
    }
}

pub(crate) fn new_animated_welcome() -> AnimatedWelcomeCell {
    AnimatedWelcomeCell::new()
}
