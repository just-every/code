//! Performance statistics support for ChatWidget.
//!
//! Kept as a separate module to keep `chatwidget.rs` lean. Pure data + helpers
//! with no UI dependencies so it is easy to unit‑test in isolation.

#[derive(Default, Clone, Debug)]
pub struct PerfStats {
    pub frames: u64,
    pub prefix_rebuilds: u64,
    pub height_hits_total: u64,
    pub height_misses_total: u64,
    pub height_hits_render: u64,
    pub height_misses_render: u64,
    pub ns_total_height: u128,
    pub ns_render_loop: u128,
    // Full widget render wall time (outer render wrapper)
    pub ns_widget_render_total: u128,
    // Explicit paint/clear hotspots we perform outside ratatui widgets
    pub ns_history_clear: u128,
    pub cells_history_clear: u64,
    pub ns_gutter_paint: u128,
    pub cells_gutter_paint: u64,
    // Diff overlay fills
    pub ns_overlay_scrim: u128,
    pub cells_overlay_scrim: u64,
    pub ns_overlay_body_bg: u128,
    pub cells_overlay_body_bg: u64,
    pub render_requests_full: u64,
    pub render_requests_visible: u64,
    // Hotspots: time spent computing heights on cache misses
    pub hot_total: std::collections::HashMap<(usize, u16), ItemStat>,
    pub hot_render: std::collections::HashMap<(usize, u16), ItemStat>,
    // Aggregation by cell kind/label
    pub per_kind_total: std::collections::HashMap<String, ItemStat>,
    pub per_kind_render: std::collections::HashMap<String, ItemStat>,
    // Scroll instrumentation
    pub scroll_events: u64,
    pub scroll_lines_requested: u64,
    pub scroll_render_frames: u64,
    pub scroll_lines_rendered: u64,
    pub ns_scroll_render: u128,
    // Undo/resume instrumentation
    pub undo_restore_events: u64,
    pub ns_undo_restore: u128,
}

#[derive(Default, Clone, Debug)]
pub struct ItemStat {
    pub calls: u64,
    pub ns: u128,
}

impl PerfStats {
    pub fn reset(&mut self) { *self = PerfStats::default(); }

    pub fn summary(&self) -> String {
        let ms_total_height = (self.ns_total_height as f64) / 1_000_000.0;
        let ms_render = (self.ns_render_loop as f64) / 1_000_000.0;
        let ms_widget = (self.ns_widget_render_total as f64) / 1_000_000.0;
        let ms_hist_clear = (self.ns_history_clear as f64) / 1_000_000.0;
        let ms_gutter = (self.ns_gutter_paint as f64) / 1_000_000.0;
        let ms_scrim = (self.ns_overlay_scrim as f64) / 1_000_000.0;
        let ms_overlay_body = (self.ns_overlay_body_bg as f64) / 1_000_000.0;
        let mut out = String::new();
        out.push_str(&format!(
            "perf: frames={}\n  prefix_rebuilds={}\n  render_requests: full={} visible={}\n  height_cache: total hits={} misses={}\n  height_cache (render): hits={} misses={}\n  time: total_height={:.2}ms render_visible={:.2}ms\n  time: widget_render_total={:.2}ms\n  paint: history_clear={:.2}ms (cells={}) gutter_bg={:.2}ms (cells={})\n  paint: overlay_scrim={:.2}ms (cells={}) overlay_body_bg={:.2}ms (cells={})",
            self.frames,
            self.prefix_rebuilds,
            self.render_requests_full,
            self.render_requests_visible,
            self.height_hits_total,
            self.height_misses_total,
            self.height_hits_render,
            self.height_misses_render,
            ms_total_height,
            ms_render,
            ms_widget,
            ms_hist_clear,
            self.cells_history_clear,
            ms_gutter,
            self.cells_gutter_paint,
            ms_scrim,
            self.cells_overlay_scrim,
            ms_overlay_body,
            self.cells_overlay_body_bg,
        ));

        if self.scroll_events > 0 || self.scroll_render_frames > 0 {
            let ms_scroll = (self.ns_scroll_render as f64) / 1_000_000.0;
            out.push_str(&format!(
                "\n  scroll: events={} lines_requested={} render_frames={} lines_rendered={} render_time={:.2}ms",
                self.scroll_events,
                self.scroll_lines_requested,
                self.scroll_render_frames,
                self.scroll_lines_rendered,
                ms_scroll,
            ));
        }

        if self.undo_restore_events > 0 {
            let ms_total = (self.ns_undo_restore as f64) / 1_000_000.0;
            let avg = if self.undo_restore_events > 0 {
                ms_total / self.undo_restore_events as f64
            } else {
                0.0
            };
            out.push_str(&format!(
                "\n  undo_restore: events={} total={:.2}ms avg={:.2}ms",
                self.undo_restore_events,
                ms_total,
                avg,
            ));
        }

        // Top hotspots by (index,width)
        let mut top_total: Vec<(&(usize, u16), &ItemStat)> = self.hot_total.iter().collect();
        top_total.sort_by_key(|(_, s)| std::cmp::Reverse(s.ns));
        let mut top_render: Vec<(&(usize, u16), &ItemStat)> = self.hot_render.iter().collect();
        top_render.sort_by_key(|(_, s)| std::cmp::Reverse(s.ns));

        if !top_total.is_empty() {
            out.push_str("\n\n  hot items (total height, cache misses):\n");
            for ((idx, w), stat) in top_total.into_iter().take(5) {
                out.push_str(&format!(
                    "    (idx={}, width={}) calls={} time={:.2}ms\n",
                    idx,
                    w,
                    stat.calls,
                    (stat.ns as f64) / 1_000_000.0,
                ));
            }
        }

        if !top_render.is_empty() {
            out.push_str("\n  hot items (render visible, cache misses):\n");
            for ((idx, w), stat) in top_render.into_iter().take(5) {
                out.push_str(&format!(
                    "    (idx={}, width={}) calls={} time={:.2}ms\n",
                    idx,
                    w,
                    stat.calls,
                    (stat.ns as f64) / 1_000_000.0,
                ));
            }
        }

        // Per-kind aggregation
        if !self.per_kind_total.is_empty() {
            let mut v: Vec<(&String, &ItemStat)> = self.per_kind_total.iter().collect();
            v.sort_by_key(|(_, s)| std::cmp::Reverse(s.ns));
            out.push_str("\n  by kind (total height):\n");
            for (k, s) in v.into_iter().take(5) {
                out.push_str(&format!(
                    "    {} calls={} time={:.2}ms\n",
                    k,
                    s.calls,
                    (s.ns as f64) / 1_000_000.0,
                ));
            }
        }

        if !self.per_kind_render.is_empty() {
            let mut v: Vec<(&String, &ItemStat)> = self.per_kind_render.iter().collect();
            v.sort_by_key(|(_, s)| std::cmp::Reverse(s.ns));
            out.push_str("\n  by kind (render visible):\n");
            for (k, s) in v.into_iter().take(5) {
                out.push_str(&format!(
                    "    {} calls={} time={:.2}ms\n",
                    k,
                    s.calls,
                    (s.ns as f64) / 1_000_000.0,
                ));
            }
        }

        out
    }

    #[allow(dead_code)]
    pub fn record_total(&mut self, key: (usize, u16), kind: &str, ns: u128) {
        let e = self.hot_total.entry(key).or_insert_with(ItemStat::default);
        e.calls = e.calls.saturating_add(1);
        e.ns = e.ns.saturating_add(ns);
        let ek = self
            .per_kind_total
            .entry(kind.to_string())
            .or_insert_with(ItemStat::default);
        ek.calls = ek.calls.saturating_add(1);
        ek.ns = ek.ns.saturating_add(ns);
    }

    pub fn record_render(&mut self, key: (usize, u16), kind: &str, ns: u128) {
        let e = self.hot_render.entry(key).or_insert_with(ItemStat::default);
        e.calls = e.calls.saturating_add(1);
        e.ns = e.ns.saturating_add(ns);
        let ek = self
            .per_kind_render
            .entry(kind.to_string())
            .or_insert_with(ItemStat::default);
        ek.calls = ek.calls.saturating_add(1);
        ek.ns = ek.ns.saturating_add(ns);
    }

    pub fn record_scroll_trigger(&mut self, lines: u64) {
        self.scroll_events = self.scroll_events.saturating_add(1);
        self.scroll_lines_requested = self.scroll_lines_requested.saturating_add(lines);
    }

    pub fn record_scroll_render(&mut self, lines: u64, ns: u128) {
        self.scroll_render_frames = self.scroll_render_frames.saturating_add(1);
        self.scroll_lines_rendered = self.scroll_lines_rendered.saturating_add(lines);
        self.ns_scroll_render = self.ns_scroll_render.saturating_add(ns);
    }

    pub fn record_undo_restore(&mut self, ns: u128) {
        self.undo_restore_events = self.undo_restore_events.saturating_add(1);
        self.ns_undo_restore = self.ns_undo_restore.saturating_add(ns);
    }
}
