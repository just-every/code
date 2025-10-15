//! Shared popup-related constants for bottom pane widgets.

/// Maximum number of rows any popup should attempt to display.
/// Keep this consistent across all popups for a uniform feel.
pub(crate) const MAX_POPUP_ROWS: usize = 8;

/// Standard footer hint text used by popups.
/// Prefix with underscore to avoid dead_code warnings if not referenced.
pub(crate) const _STANDARD_POPUP_HINT_LINE: &str = "Press Enter to confirm or Esc to go back";
