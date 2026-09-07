//! Immutable session mode shared by history, evidence projection, and reviewer policy.
//! Resolve the rollout flag at session construction, not independently at each consumer.

use codex_features::Feature;
use codex_features::Features;

/// Selects legacy compatibility or thread-owned evidence for a session's lifetime.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum GuardianContextMode {
    #[default]
    Legacy,
    ThreadOwned,
}

impl GuardianContextMode {
    pub(crate) fn from_features(features: &Features) -> Self {
        if features.enabled(Feature::GuardianThreadContext) {
            Self::ThreadOwned
        } else {
            Self::Legacy
        }
    }
}
