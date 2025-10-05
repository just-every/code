use std::cell::Cell;
use std::f32::consts::TAU;
use std::time::{Duration, Instant};

use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::style::Color;

pub struct HeaderBorderWeaveEffect {
    enabled: Cell<bool>,
    started_at: Cell<Option<Instant>>,
    next_frame: Cell<Option<Instant>>,
}

impl HeaderBorderWeaveEffect {
    pub const FRAME_INTERVAL: Duration = HeaderWaveEffect::FRAME_INTERVAL;

    pub fn new() -> Self {
        Self {
            enabled: Cell::new(false),
            started_at: Cell::new(None),
            next_frame: Cell::new(None),
        }
    }

    pub fn set_enabled(&self, enabled: bool, now: Instant) {
        self.enabled.set(enabled);
        if enabled {
            self.started_at.set(Some(now));
            self.next_frame.set(None);
        } else {
            self.started_at.set(None);
            self.next_frame.set(None);
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.get()
    }

    pub fn schedule_if_needed(&self, now: Instant) -> bool {
        if !self.enabled.get() {
            return false;
        }
        match self.next_frame.get() {
            Some(due) if due > now => false,
            _ => {
                self.next_frame
                    .set(Some(now + Self::FRAME_INTERVAL));
                true
            }
        }
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer, now: Instant) {
        if !self.enabled.get() {
            return;
        }
        if area.width < 2 || area.height < 2 {
            return;
        }
        let start = self.started_at.get().unwrap_or_else(|| {
            self.started_at.set(Some(now));
            now
        });
        let elapsed = now.saturating_duration_since(start).as_secs_f32();
        let tick_interval = Self::FRAME_INTERVAL.as_secs_f32().max(0.001);
        let tick = (elapsed / tick_interval).floor() as u32;
        let phase = wrap_unit(elapsed * 0.25);
        render_border_weave(area, buf, phase, tick);
    }
}

/// Declarative background animation used by the TUI status header.
pub struct HeaderWaveEffect {
    enabled: Cell<bool>,
    started_at: Cell<Option<Instant>>,
    next_frame: Cell<Option<Instant>>,
}

impl HeaderWaveEffect {
    /// Animation cadence; keep this in sync with the ScheduleFrameIn duration.
    pub const FRAME_INTERVAL: Duration = Duration::from_millis(120);

    pub fn new() -> Self {
        Self {
            enabled: Cell::new(false),
            started_at: Cell::new(None),
            next_frame: Cell::new(None),
        }
    }

    pub fn set_enabled(&self, enabled: bool, now: Instant) {
        self.enabled.set(enabled);
        if enabled {
            self.started_at.set(Some(now));
            self.next_frame.set(None);
        } else {
            self.started_at.set(None);
            self.next_frame.set(None);
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.get()
    }

    /// Returns true if the caller should schedule another frame tick.
    pub fn schedule_if_needed(&self, now: Instant) -> bool {
        if !self.enabled.get() {
            return false;
        }
        match self.next_frame.get() {
            Some(due) if due > now => false,
            _ => {
                self.next_frame.set(Some(now + Self::FRAME_INTERVAL));
                true
            }
        }
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer, now: Instant) {
        if !self.enabled.get() {
            return;
        }
        let start = self.started_at.get().unwrap_or_else(|| {
            self.started_at.set(Some(now));
            now
        });
        let elapsed = now.saturating_duration_since(start).as_secs_f32();

        if area.width < 2 || area.height < 2 {
            return;
        }

        let inner = area.inner(Margin::new(1, 1));
        if inner.width < 3 || inner.height == 0 {
            return;
        }
        let stripe_area = inner.inner(Margin::new(1, 0));
        if stripe_area.width == 0 || stripe_area.height == 0 {
            return;
        }

        let travel = wrap_unit(elapsed * 0.65);
        let width = stripe_area.width as usize;
        let base_bg = crate::colors::background();

        for x_idx in 0..width {
            let x = stripe_area.x + x_idx as u16;
            let rel = x_idx as f32 / width.max(1) as f32;
            let hue = wrap_unit(travel + (rel - 0.5).abs() * 0.45 + rel * 0.3);
            let tint = crate::colors::mix_toward(base_bg, spectral_color(hue), 0.22);
            for y in stripe_area.y..stripe_area.y + stripe_area.height {
                let cell = &mut buf[(x, y)];
                cell.set_symbol(" ");
                cell.set_bg(tint);
            }
        }
    }
}

fn wrap_unit(value: f32) -> f32 {
    let mut v = value % 1.0;
    if v < 0.0 {
        v += 1.0;
    }
    v
}

fn spectral_color(t: f32) -> Color {
    let angle = wrap_unit(t) * TAU;
    let r = (angle.sin() * 0.5 + 0.5).powf(0.55);
    let g = ((angle + TAU / 3.0).sin() * 0.5 + 0.5).powf(0.55);
    let b = ((angle + 2.0 * TAU / 3.0).sin() * 0.5 + 0.5).powf(0.55);
    Color::Rgb(
        (r * 255.0).clamp(0.0, 255.0) as u8,
        (g * 255.0).clamp(0.0, 255.0) as u8,
        (b * 255.0).clamp(0.0, 255.0) as u8,
    )
}

fn render_border_weave(area: Rect, buf: &mut Buffer, phase: f32, tick: u32) {
    let positions = border_positions(area);
    if positions.is_empty() {
        return;
    }
    let base_bg = crate::colors::background();
    let info = crate::colors::info();
    let wave = ((phase * TAU).sin() * 0.5) + 0.5;
    let highlight_strength = 0.65 + wave * 0.25;
    let highlight = crate::colors::mix_toward(base_bg, info, highlight_strength);
    let midtone = crate::colors::mix_toward(base_bg, info, 0.5);
    let lowtone = crate::colors::mix_toward(base_bg, info, 0.3);
    for (idx, &(x, y)) in positions.iter().enumerate() {
        let tick_step = (tick as usize) % 12;
        let phase_idx = (idx + (12 - tick_step)) % 12;
        let color = if phase_idx < 4 {
            highlight
        } else if phase_idx < 8 {
            midtone
        } else {
            lowtone
        };
        let cell = &mut buf[(x, y)];
        cell.set_fg(color);
        cell.set_bg(base_bg);
    }
}

fn border_positions(area: Rect) -> Vec<(u16, u16)> {
    if area.width < 2 || area.height < 2 {
        return Vec::new();
    }
    let mut positions = Vec::new();
    let x0 = area.x;
    let x1 = area.x + area.width.saturating_sub(1);
    let y0 = area.y;
    let y1 = area.y + area.height.saturating_sub(1);

    for x in x0..=x1 {
        positions.push((x, y0));
    }
    if y1 > y0 {
        for y in y0 + 1..=y1 {
            positions.push((x1, y));
        }
    }
    if x1 > x0 {
        for x in (x0..x1).rev() {
            positions.push((x, y1));
        }
    }
    if y1 > y0 + 1 {
        for y in (y0 + 1..y1).rev() {
            positions.push((x0, y));
        }
    }

    positions
}
