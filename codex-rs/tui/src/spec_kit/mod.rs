//! Spec-Kit multi-agent automation framework
//!
//! This module isolates all spec-kit functionality from upstream TUI code
//! to minimize rebase conflict surface area.

mod handler;
mod state;

pub use handler::SpecKitHandler;
pub use state::{SpecAutoPhase, SpecAutoState};
