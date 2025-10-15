use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use ratatui::buffer::Cell as BufferCell;
use ratatui::text::Line;

use crate::insert_history::word_wrap_lines;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Memoized layout data for history rendering.
pub(crate) struct HistoryRenderState {
    pub(crate) layout_cache: RefCell<HashMap<(usize, u16), Rc<CachedLayout>>>,
    pub(crate) height_cache_last_width: Cell<u16>,
    pub(crate) prefix_sums: RefCell<Vec<u16>>,
    pub(crate) last_prefix_width: Cell<u16>,
    pub(crate) last_prefix_count: Cell<usize>,
    pub(crate) prefix_valid: Cell<bool>,
}

impl HistoryRenderState {
    pub(crate) fn new() -> Self {
        Self {
            layout_cache: RefCell::new(HashMap::new()),
            height_cache_last_width: Cell::new(0),
            prefix_sums: RefCell::new(Vec::new()),
            last_prefix_width: Cell::new(0),
            last_prefix_count: Cell::new(0),
            prefix_valid: Cell::new(false),
        }
    }

    pub(crate) fn invalidate_height_cache(&self) {
        self.layout_cache.borrow_mut().clear();
        self.prefix_sums.borrow_mut().clear();
        self.prefix_valid.set(false);
    }

    pub(crate) fn handle_width_change(&self, width: u16) {
        if self.height_cache_last_width.get() != width {
            self.layout_cache.borrow_mut().clear();
            self.prefix_sums.borrow_mut().clear();
            self.prefix_valid.set(false);
            self.height_cache_last_width.set(width);
        }
    }

    pub(crate) fn ensure_layout<F>(&self, idx: usize, width: u16, build_lines: F) -> LayoutRef
    where
        F: FnOnce() -> Vec<Line<'static>>,
    {
        if width == 0 {
            return LayoutRef {
                data: Rc::new(CachedLayout {
                    lines: Vec::new(),
                    rows: Vec::new(),
                }),
                freshly_computed: false,
            };
        }

        let key = (idx, width);
        if let Some(layout) = self.layout_cache.borrow().get(&key).cloned() {
            return LayoutRef {
                data: layout,
                freshly_computed: false,
            };
        }

        let lines = build_lines();
        let wrapped = if lines.is_empty() {
            Vec::new()
        } else {
            word_wrap_lines(&lines, width)
        };
        let rows = build_cached_rows(&wrapped, width);
        let layout = Rc::new(CachedLayout {
            lines: wrapped,
            rows,
        });
        self.layout_cache
            .borrow_mut()
            .insert(key, Rc::clone(&layout));
        LayoutRef {
            data: layout,
            freshly_computed: true,
        }
    }
}

#[derive(Clone)]
pub(crate) struct LayoutRef {
    pub(crate) data: Rc<CachedLayout>,
    pub(crate) freshly_computed: bool,
}

impl LayoutRef {
    pub(crate) fn layout(&self) -> Rc<CachedLayout> {
        Rc::clone(&self.data)
    }

    pub(crate) fn line_count(&self) -> usize {
        self.data.lines.len()
    }
}

impl Default for HistoryRenderState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub(crate) struct CachedLayout {
    pub(crate) lines: Vec<Line<'static>>,
    pub(crate) rows: Vec<Box<[BufferCell]>>,
}

fn build_cached_rows(lines: &[Line<'static>], width: u16) -> Vec<Box<[BufferCell]>> {
    let target_width = width as usize;
    lines
        .iter()
        .map(|line| build_cached_row(line, target_width))
        .collect()
}

fn build_cached_row(line: &Line<'static>, target_width: usize) -> Box<[BufferCell]> {
    if target_width == 0 {
        return Box::new([]);
    }

    let mut cells = vec![BufferCell::default(); target_width];
    let mut x: u16 = 0;
    let mut remaining = target_width as u16;

    for span in &line.spans {
        if remaining == 0 {
            break;
        }
        let span_style = line.style.patch(span.style);
        for symbol in UnicodeSegmentation::graphemes(span.content.as_ref(), true) {
            if symbol.chars().any(|ch| ch.is_control()) {
                continue;
            }
            let symbol_width = UnicodeWidthStr::width(symbol) as u16;
            if symbol_width == 0 {
                continue;
            }
            if symbol_width > remaining {
                remaining = 0;
                break;
            }

            let idx = x as usize;
            if idx >= target_width {
                remaining = 0;
                break;
            }

            cells[idx].set_symbol(symbol).set_style(span_style);

            let next_symbol = x.saturating_add(symbol_width);
            x = x.saturating_add(1);
            while x < next_symbol {
                let fill_idx = x as usize;
                if fill_idx >= target_width {
                    remaining = 0;
                    break;
                }
                cells[fill_idx].reset();
                x = x.saturating_add(1);
            }
            if remaining == 0 {
                break;
            }
            if x >= target_width as u16 {
                remaining = 0;
                break;
            }
            remaining = target_width as u16 - x;
            if remaining == 0 {
                break;
            }
        }
        if remaining == 0 {
            break;
        }
    }

    cells.into_boxed_slice()
}
