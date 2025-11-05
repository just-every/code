//! Core timeline data structure and operations.

use std::collections::{BTreeMap, HashMap};

use code_protocol::models::ResponseItem;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{EnvironmentContextDelta, EnvironmentContextSnapshot};

#[derive(Debug, Error)]
pub enum TimelineError {
    #[error("Baseline already set")]
    BaselineAlreadySet,

    #[error("Baseline not set")]
    BaselineNotSet,

    #[error("Delta sequence out of order: expected {expected}, got {actual}")]
    DeltaSequenceOutOfOrder { expected: u64, actual: u64 },

    #[error("Snapshot already exists with fingerprint: {0}")]
    SnapshotAlreadyExists(String),

    #[error("Delta not found for sequence: {0}")]
    DeltaNotFound(u64),

    #[error("Snapshot not found for fingerprint: {0}")]
    SnapshotNotFound(String),
}

/// Entry representing a delta in the timeline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeltaEntry {
    pub sequence: u64,
    pub delta: EnvironmentContextDelta,
    /// Timestamp when this delta was recorded (ISO 8601).
    pub recorded_at: String,
}

/// Entry representing a snapshot in the timeline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SnapshotEntry {
    pub fingerprint: String,
    pub snapshot: EnvironmentContextSnapshot,
    /// Timestamp when this snapshot was recorded (ISO 8601).
    pub recorded_at: String,
}

/// Core timeline structure managing baseline, deltas, and snapshots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextTimeline {
    /// Baseline snapshot (immutable once set).
    baseline: Option<EnvironmentContextSnapshot>,
    /// Deltas indexed by sequence number.
    deltas: BTreeMap<u64, DeltaEntry>,
    /// Snapshots indexed by fingerprint for deduplication.
    snapshots: HashMap<String, SnapshotEntry>,
    /// Next expected sequence number for delta append.
    next_sequence: u64,
}

impl Default for ContextTimeline {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextTimeline {
    /// Creates a new empty timeline.
    pub fn new() -> Self {
        Self {
            baseline: None,
            deltas: BTreeMap::new(),
            snapshots: HashMap::new(),
            next_sequence: 1,
        }
    }

    /// Sets the baseline snapshot. Can only be called once.
    ///
    /// # Errors
    ///
    /// Returns `TimelineError::BaselineAlreadySet` if baseline is already set.
    pub fn add_baseline_once(
        &mut self,
        snapshot: EnvironmentContextSnapshot,
    ) -> Result<(), TimelineError> {
        if self.baseline.is_some() {
            return Err(TimelineError::BaselineAlreadySet);
        }
        self.baseline = Some(snapshot);
        Ok(())
    }

    /// Applies a delta with sequence validation.
    ///
    /// # Errors
    ///
    /// - Returns `TimelineError::BaselineNotSet` if baseline hasn't been set.
    /// - Returns `TimelineError::DeltaSequenceOutOfOrder` if sequence doesn't match expected.
    pub fn apply_delta(&mut self, sequence: u64, delta: EnvironmentContextDelta) -> Result<(), TimelineError> {
        if self.baseline.is_none() {
            return Err(TimelineError::BaselineNotSet);
        }

        if sequence != self.next_sequence {
            return Err(TimelineError::DeltaSequenceOutOfOrder {
                expected: self.next_sequence,
                actual: sequence,
            });
        }

        let entry = DeltaEntry {
            sequence,
            delta,
            recorded_at: current_timestamp(),
        };

        self.deltas.insert(sequence, entry);
        self.next_sequence += 1;

        Ok(())
    }

    /// Records a snapshot with hash-based deduplication.
    ///
    /// If a snapshot with the same fingerprint already exists, returns `Ok(false)`.
    /// Otherwise, records the snapshot and returns `Ok(true)`.
    pub fn record_snapshot(&mut self, snapshot: EnvironmentContextSnapshot) -> Result<bool, TimelineError> {
        let fingerprint = snapshot.fingerprint();

        if self.snapshots.contains_key(&fingerprint) {
            return Ok(false); // Already exists, deduplicated
        }

        let entry = SnapshotEntry {
            fingerprint: fingerprint.clone(),
            snapshot,
            recorded_at: current_timestamp(),
        };

        self.snapshots.insert(fingerprint, entry);
        Ok(true)
    }

    // Lookup helpers

    /// Returns a reference to the baseline snapshot if set.
    pub fn baseline(&self) -> Option<&EnvironmentContextSnapshot> {
        self.baseline.as_ref()
    }

    /// Returns a reference to the delta entry for the given sequence.
    pub fn get_delta(&self, sequence: u64) -> Option<&DeltaEntry> {
        self.deltas.get(&sequence)
    }

    /// Returns a reference to the snapshot entry for the given fingerprint.
    pub fn get_snapshot(&self, fingerprint: &str) -> Option<&SnapshotEntry> {
        self.snapshots.get(fingerprint)
    }

    /// Returns all delta sequences in order.
    pub fn delta_sequences(&self) -> Vec<u64> {
        self.deltas.keys().copied().collect()
    }

    /// Returns all snapshot fingerprints.
    pub fn snapshot_fingerprints(&self) -> Vec<String> {
        self.snapshots.keys().cloned().collect()
    }

    /// Returns the number of deltas stored.
    pub fn delta_count(&self) -> usize {
        self.deltas.len()
    }

    /// Returns the number of snapshots stored.
    pub fn snapshot_count(&self) -> usize {
        self.snapshots.len()
    }

    /// Returns the next expected sequence number.
    pub fn next_sequence(&self) -> u64 {
        self.next_sequence
    }

    /// Returns `true` when no baseline or deltas are stored.
    pub fn is_empty(&self) -> bool {
        self.baseline.is_none() && self.deltas.is_empty()
    }

    /// Assemble prompt-ready `ResponseItem`s from the current timeline.
    ///
    /// Includes the baseline (if present) and the most recent `max_deltas`
    /// deltas in chronological order. Each item is tagged with `stream_id`
    /// when provided so downstream ordering remains stable.
    pub fn assemble_prompt_items(
        &self,
        max_deltas: usize,
        stream_id: Option<&str>,
    ) -> serde_json::Result<Vec<ResponseItem>> {
        let mut items = Vec::new();

        if let Some(snapshot) = self.baseline() {
            items.push(snapshot.to_response_item_with_id(stream_id)?);
        }

        if max_deltas > 0 {
            let mut recent: Vec<&DeltaEntry> = self
                .deltas
                .values()
                .rev()
                .take(max_deltas)
                .collect();
            recent.reverse();

            for entry in recent {
                items.push(entry.delta.to_response_item_with_id(stream_id)?);
            }
        }

        Ok(items)
    }

    // Pruning entry points

    /// Estimates the total memory usage in bytes.
    ///
    /// This is a rough estimate for Phase 2A. Future phases can refine this.
    pub fn estimated_bytes(&self) -> usize {
        let baseline_bytes = self.baseline.as_ref().map_or(0, estimate_snapshot_bytes);

        let deltas_bytes: usize = self.deltas.values().map(estimate_delta_bytes).sum();

        let snapshots_bytes: usize = self.snapshots.values().map(|entry| estimate_snapshot_bytes(&entry.snapshot)).sum();

        baseline_bytes + deltas_bytes + snapshots_bytes
    }
}

/// Rough estimate of snapshot size in bytes.
fn estimate_snapshot_bytes(snapshot: &EnvironmentContextSnapshot) -> usize {
    // Very rough estimate based on JSON serialization
    serde_json::to_string(snapshot).map(|s| s.len()).unwrap_or(0)
}

/// Rough estimate of delta entry size in bytes.
fn estimate_delta_bytes(entry: &DeltaEntry) -> usize {
    serde_json::to_string(&entry.delta).map(|s| s.len()).unwrap_or(0)
        + entry.recorded_at.len()
        + 8 // sequence u64
}

/// Returns current timestamp in ISO 8601 format.
fn current_timestamp() -> String {
    chrono::Utc::now().to_rfc3339()
}
