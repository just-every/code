//! Context Timeline Module
//!
//! Encapsulates baseline + delta + snapshot timeline storage for environment context.
//! Provides APIs for managing context state over time with:
//! - Baseline storage (immutable, set once)
//! - Sequence-aware delta append
//! - Hash-deduplicated snapshots
//! - Retention/pruning hooks
//!
//! Phase 2A: Core implementation with feature flag gating.
//! Not yet integrated into SessionState.
//!
//! # Feature Flags
//!
//! - `CTX_DELTAS`: Enable delta tracking
//! - `CTX_SNAPSHOTS`: Enable snapshot storage
//! - `CTX_UI`: Enable UI features (future phase)

use crate::flags;

mod storage;
mod timeline;

pub use timeline::{ContextTimeline, DeltaEntry, SnapshotEntry, TimelineError};

/// Returns true if delta tracking is enabled.
pub fn is_deltas_enabled() -> bool {
    *flags::CTX_DELTAS
}

/// Returns true if snapshot storage is enabled.
pub fn is_snapshots_enabled() -> bool {
    *flags::CTX_SNAPSHOTS
}

/// Returns true if UI features are enabled.
pub fn is_ui_enabled() -> bool {
    *flags::CTX_UI
}

#[cfg(test)]
mod tests;
