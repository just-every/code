//! Spec-Kit command handlers as free functions
//!
//! These functions are extracted from chatwidget.rs to isolate spec-kit code.
//! Using free functions instead of methods to avoid Rust borrow checker issues.

use super::super::ChatWidget; // Parent module (friend access to private fields)
use crate::app_event::BackgroundPlacement;
use crate::history_cell::HistoryCellType;
use crate::spec_status::{SpecStatusArgs, collect_report, degraded_warning, render_dashboard};

/// Handle /speckit.status command (native dashboard)
pub fn handle_spec_status(widget: &mut ChatWidget, raw_args: String) {
    let trimmed = raw_args.trim();
    let args = match SpecStatusArgs::from_input(trimmed) {
        Ok(args) => args,
        Err(err) => {
            widget.history_push(crate::history_cell::new_error_event(err.to_string()));
            widget.request_redraw();
            return;
        }
    };

    match collect_report(&widget.config.cwd, args) {
        Ok(report) => {
            let mut lines = render_dashboard(&report);
            if let Some(warning) = degraded_warning(&report) {
                lines.insert(1, warning);
            }
            let message = lines.join("\n");
            widget.insert_background_event_with_placement(message, BackgroundPlacement::Tail);
            widget.request_redraw();
        }
        Err(err) => {
            widget.history_push(crate::history_cell::new_error_event(format!(
                "spec-status failed: {err}"
            )));
            widget.request_redraw();
        }
    }
}

/// Halt /speckit.auto pipeline with error message
pub fn halt_spec_auto_with_error(widget: &mut ChatWidget, reason: String) {
    let resume_hint = widget
        .spec_auto_state
        .as_ref()
        .and_then(|s| s.current_stage())
        .map(|stage| {
            format!(
                "/spec-auto {} --from {}",
                widget.spec_auto_state.as_ref().unwrap().spec_id,
                stage.command_name()
            )
        })
        .unwrap_or_default();

    widget.history_push(crate::history_cell::PlainHistoryCell::new(
        vec![
            ratatui::text::Line::from("⚠ /spec-auto halted"),
            ratatui::text::Line::from(reason),
            ratatui::text::Line::from(""),
            ratatui::text::Line::from("Resume with:"),
            ratatui::text::Line::from(resume_hint),
        ],
        HistoryCellType::Error,
    ));

    widget.spec_auto_state = None;
}

/// Handle /spec-consensus command (inspect consensus artifacts)
pub fn handle_spec_consensus(widget: &mut ChatWidget, raw_args: String) {
    // Delegate to ChatWidget method for now (complex logic with many private helpers)
    widget.handle_spec_consensus_impl(raw_args);
}

// Additional handler functions will be added here in subsequent commits
