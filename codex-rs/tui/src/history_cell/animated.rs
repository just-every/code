use super::*;
use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

pub(crate) struct AnimatedWelcomeCell {
    start_time: Instant,
    completed: Cell<bool>,
    fade_start: RefCell<Option<Instant>>,
    faded_out: Cell<bool>,
    locked_height: Cell<Option<u16>>,
    hidden: Cell<bool>,
}

impl AnimatedWelcomeCell {
    pub(crate) fn new() -> Self {
        Self {
            start_time: Instant::now(),
            completed: Cell::new(false),
            fade_start: RefCell::new(None),
            faded_out: Cell::new(false),
            locked_height: Cell::new(None),
            hidden: Cell::new(false),
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
        if let Some(h) = self.locked_height.get() {
            return h.saturating_add(3);
        }

        let cols: u16 = 23;
        let base_rows: u16 = 7;
        let max_scale: u16 = 3;
        let scale = if width >= cols {
            (width / cols).min(max_scale).max(1)
        } else {
            1
        };
        let h = base_rows.saturating_mul(scale);
        self.locked_height.set(Some(h));
        h.saturating_add(3)
    }

    fn has_custom_render(&self) -> bool {
        true
    }

    fn custom_render(&self, area: Rect, buf: &mut Buffer) {
        if self.hidden.get() {
            return;
        }

        let locked_h = self.locked_height.get().unwrap_or(21);
        let height = locked_h.min(area.height);
        let positioned_area = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height,
        };

        let fade_duration = Duration::from_millis(800);

        if let Some(fade_time) = self.fade_start() {
            let fade_elapsed = fade_time.elapsed();
            if fade_elapsed < fade_duration && !self.faded_out.get() {
                let fade_progress = fade_elapsed.as_secs_f32() / fade_duration.as_secs_f32();
                let alpha = 1.0 - fade_progress;
                crate::glitch_animation::render_intro_animation_with_alpha(
                    positioned_area,
                    buf,
                    1.0,
                    alpha,
                );
            } else {
                self.faded_out.set(true);
            }
            return;
        }

        let elapsed = self.start_time.elapsed();
        let animation_duration = Duration::from_secs(2);
        if elapsed < animation_duration && !self.completed.get() {
            let progress = elapsed.as_secs_f32() / animation_duration.as_secs_f32();
            crate::glitch_animation::render_intro_animation(positioned_area, buf, progress);
        } else {
            self.completed.set(true);
            crate::glitch_animation::render_intro_animation(positioned_area, buf, 1.0);
        }
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
