//! Telemetry module for tracking retention and budgeting metrics.
//!
//! This module provides counters and metrics for monitoring the effectiveness
//! of the retention policy, including:
//! - Bytes saved compared to legacy approach
//! - Delta count statistics
//! - Snapshot deduplication drops
//! - Budget constraint enforcement

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::sync::OnceLock;

/// Global telemetry counters for retention operations.
#[derive(Debug, Default)]
pub struct RetentionTelemetry {
    /// Total bytes removed by retention policy across all sessions
    total_bytes_removed: AtomicUsize,
    /// Total bytes kept after retention policy
    total_bytes_kept: AtomicUsize,
    /// Total number of environment deltas removed
    total_deltas_removed: AtomicUsize,
    /// Total number of environment deltas kept
    total_deltas_kept: AtomicUsize,
    /// Total number of environment baselines removed
    total_baselines_removed: AtomicUsize,
    /// Total number of browser snapshots removed
    total_snapshots_removed: AtomicUsize,
    /// Total number of snapshots dropped due to deduplication
    total_dedup_drops: AtomicUsize,
    /// Total number of items dropped due to byte budget constraints
    total_budget_drops: AtomicUsize,
    /// Total number of retention operations performed
    total_operations: AtomicU64,
    /// Total number of baseline resend fallbacks triggered
    total_baseline_resends: AtomicUsize,
    /// Total number of delta gap detections (out-of-order or missing sequences)
    total_delta_gaps: AtomicUsize,
    /// Total number of snapshot attempts recorded
    total_snapshot_attempts: AtomicUsize,
    /// Total number of snapshot dedup hits
    total_snapshot_dedup_hits: AtomicUsize,
}

impl RetentionTelemetry {
    /// Creates a new telemetry instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records statistics from a retention operation.
    pub fn record_retention(&self, stats: &crate::retention::RetentionStats) {
        self.total_bytes_removed
            .fetch_add(stats.bytes_removed, Ordering::Relaxed);
        self.total_bytes_kept
            .fetch_add(stats.bytes_kept, Ordering::Relaxed);
        self.total_deltas_removed
            .fetch_add(stats.removed_env_deltas, Ordering::Relaxed);
        self.total_deltas_kept
            .fetch_add(stats.kept_env_deltas, Ordering::Relaxed);
        self.total_baselines_removed
            .fetch_add(stats.removed_env_baselines, Ordering::Relaxed);
        self.total_snapshots_removed
            .fetch_add(stats.removed_browser_snapshots, Ordering::Relaxed);
        self.total_budget_drops
            .fetch_add(stats.dropped_for_budget, Ordering::Relaxed);
        self.total_operations.fetch_add(1, Ordering::Relaxed);
    }

    /// Records a snapshot deduplication drop.
    pub fn record_dedup_drop(&self) {
        self.total_dedup_drops.fetch_add(1, Ordering::Relaxed);
        self.total_snapshot_attempts.fetch_add(1, Ordering::Relaxed);
        self.total_snapshot_dedup_hits.fetch_add(1, Ordering::Relaxed);
    }

    /// Records a successfully stored snapshot (non-deduplicated).
    pub fn record_snapshot_commit(&self) {
        self.total_snapshot_attempts.fetch_add(1, Ordering::Relaxed);
    }

    /// Records a baseline resend fallback.
    pub fn record_baseline_resend(&self) {
        self.total_baseline_resends.fetch_add(1, Ordering::Relaxed);
    }

    /// Records a detected delta gap that required recovery.
    pub fn record_delta_gap(&self) {
        self.total_delta_gaps.fetch_add(1, Ordering::Relaxed);
    }

    /// Returns the total bytes saved by retention policy.
    pub fn bytes_saved(&self) -> usize {
        self.total_bytes_removed.load(Ordering::Relaxed)
    }

    /// Returns the total bytes kept after retention.
    pub fn bytes_kept(&self) -> usize {
        self.total_bytes_kept.load(Ordering::Relaxed)
    }

    /// Returns the total number of deltas removed.
    pub fn deltas_removed(&self) -> usize {
        self.total_deltas_removed.load(Ordering::Relaxed)
    }

    /// Returns the total number of deltas kept.
    pub fn deltas_kept(&self) -> usize {
        self.total_deltas_kept.load(Ordering::Relaxed)
    }

    /// Returns the total delta count (kept + removed).
    pub fn total_delta_count(&self) -> usize {
        self.deltas_kept() + self.deltas_removed()
    }

    /// Returns the total number of baselines removed.
    pub fn baselines_removed(&self) -> usize {
        self.total_baselines_removed.load(Ordering::Relaxed)
    }

    /// Returns the total number of browser snapshots removed.
    pub fn snapshots_removed(&self) -> usize {
        self.total_snapshots_removed.load(Ordering::Relaxed)
    }

    /// Returns the total number of snapshot deduplication drops.
    pub fn dedup_drops(&self) -> usize {
        self.total_dedup_drops.load(Ordering::Relaxed)
    }

    /// Returns the total number of items dropped due to budget constraints.
    pub fn budget_drops(&self) -> usize {
        self.total_budget_drops.load(Ordering::Relaxed)
    }

    /// Returns the total number of retention operations performed.
    pub fn operations_count(&self) -> u64 {
        self.total_operations.load(Ordering::Relaxed)
    }

    /// Returns the total number of baseline resend fallbacks triggered.
    pub fn baseline_resends(&self) -> usize {
        self.total_baseline_resends.load(Ordering::Relaxed)
    }

    /// Returns the total number of delta gap detections.
    pub fn delta_gap_detections(&self) -> usize {
        self.total_delta_gaps.load(Ordering::Relaxed)
    }

    /// Returns the total snapshot attempts recorded.
    pub fn snapshot_attempts(&self) -> usize {
        self.total_snapshot_attempts.load(Ordering::Relaxed)
    }

    /// Returns the total number of snapshot dedup hits.
    pub fn snapshot_dedup_hits(&self) -> usize {
        self.total_snapshot_dedup_hits.load(Ordering::Relaxed)
    }

    /// Returns the dedup ratio if at least one attempt has been recorded.
    pub fn snapshot_dedup_ratio(&self) -> Option<f64> {
        let attempts = self.snapshot_attempts();
        if attempts == 0 {
            return None;
        }
        Some(self.snapshot_dedup_hits() as f64 / attempts as f64)
    }

    /// Resets all counters to zero (primarily for testing).
    #[cfg(test)]
    pub fn reset(&self) {
        self.total_bytes_removed.store(0, Ordering::Relaxed);
        self.total_bytes_kept.store(0, Ordering::Relaxed);
        self.total_deltas_removed.store(0, Ordering::Relaxed);
        self.total_deltas_kept.store(0, Ordering::Relaxed);
        self.total_baselines_removed.store(0, Ordering::Relaxed);
        self.total_snapshots_removed.store(0, Ordering::Relaxed);
        self.total_dedup_drops.store(0, Ordering::Relaxed);
        self.total_budget_drops.store(0, Ordering::Relaxed);
        self.total_operations.store(0, Ordering::Relaxed);
        self.total_baseline_resends.store(0, Ordering::Relaxed);
        self.total_delta_gaps.store(0, Ordering::Relaxed);
        self.total_snapshot_attempts.store(0, Ordering::Relaxed);
        self.total_snapshot_dedup_hits.store(0, Ordering::Relaxed);
    }

    /// Returns a snapshot of current metrics as a formatted string.
    pub fn summary(&self) -> String {
        let attempts = self.snapshot_attempts();
        let dedup_hits = self.snapshot_dedup_hits();
        let ratio_pct = self.snapshot_dedup_ratio().unwrap_or(0.0) * 100.0;
        format!(
            "Retention Telemetry:\n\
             - Operations: {}\n\
             - Bytes saved: {} ({:.1} KB)\n\
             - Bytes kept: {} ({:.1} KB)\n\
             - Deltas: {} kept, {} removed (total: {})\n\
             - Baselines removed: {}\n\
             - Browser snapshots removed: {}\n\
             - Dedup drops: {} (hits/attempts: {}/{}, {:.1}%)\n\
             - Budget drops: {}\n\
             - Baseline resends: {}\n\
             - Delta gaps detected: {}",
            self.operations_count(),
            self.bytes_saved(),
            self.bytes_saved() as f64 / 1024.0,
            self.bytes_kept(),
            self.bytes_kept() as f64 / 1024.0,
            self.deltas_kept(),
            self.deltas_removed(),
            self.total_delta_count(),
            self.baselines_removed(),
            self.snapshots_removed(),
            self.dedup_drops(),
            dedup_hits,
            attempts,
            ratio_pct,
            self.budget_drops(),
            self.baseline_resends(),
            self.delta_gap_detections()
        )
    }
}

/// Global telemetry instance (gated by env_ctx_v2).
static GLOBAL_TELEMETRY: OnceLock<Arc<RetentionTelemetry>> = OnceLock::new();

/// Returns a reference to the global telemetry instance.
pub fn global_telemetry() -> Arc<RetentionTelemetry> {
    Arc::clone(GLOBAL_TELEMETRY.get_or_init(|| Arc::new(RetentionTelemetry::new())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retention::RetentionStats;

    #[test]
    fn test_telemetry_recording() {
        let telemetry = RetentionTelemetry::new();

        let stats = RetentionStats {
            bytes_removed: 1000,
            bytes_kept: 2000,
            removed_env_deltas: 2,
            kept_env_deltas: 3,
            removed_env_baselines: 1,
            removed_browser_snapshots: 1,
            dropped_for_budget: 0,
            ..Default::default()
        };

        telemetry.record_retention(&stats);

        assert_eq!(telemetry.bytes_saved(), 1000);
        assert_eq!(telemetry.bytes_kept(), 2000);
        assert_eq!(telemetry.deltas_removed(), 2);
        assert_eq!(telemetry.deltas_kept(), 3);
        assert_eq!(telemetry.total_delta_count(), 5);
        assert_eq!(telemetry.baselines_removed(), 1);
        assert_eq!(telemetry.snapshots_removed(), 1);
        assert_eq!(telemetry.operations_count(), 1);
    }

    #[test]
    fn test_telemetry_accumulation() {
        let telemetry = RetentionTelemetry::new();

        let stats1 = RetentionStats {
            bytes_removed: 500,
            bytes_kept: 1000,
            removed_env_deltas: 1,
            kept_env_deltas: 2,
            ..Default::default()
        };

        let stats2 = RetentionStats {
            bytes_removed: 300,
            bytes_kept: 800,
            removed_env_deltas: 1,
            kept_env_deltas: 3,
            ..Default::default()
        };

        telemetry.record_retention(&stats1);
        telemetry.record_retention(&stats2);

        assert_eq!(telemetry.bytes_saved(), 800);
        assert_eq!(telemetry.bytes_kept(), 1800);
        assert_eq!(telemetry.deltas_removed(), 2);
        assert_eq!(telemetry.deltas_kept(), 5);
        assert_eq!(telemetry.operations_count(), 2);
    }

    #[test]
    fn test_dedup_drops() {
        let telemetry = RetentionTelemetry::new();

        telemetry.record_dedup_drop();
        telemetry.record_dedup_drop();

        assert_eq!(telemetry.dedup_drops(), 2);
        assert_eq!(telemetry.snapshot_attempts(), 2);
        assert_eq!(telemetry.snapshot_dedup_hits(), 2);
        assert_eq!(telemetry.snapshot_dedup_ratio(), Some(1.0));
    }

    #[test]
    fn test_snapshot_commit_tracking() {
        let telemetry = RetentionTelemetry::new();

        telemetry.record_snapshot_commit();
        telemetry.record_snapshot_commit();

        assert_eq!(telemetry.snapshot_attempts(), 2);
        assert_eq!(telemetry.snapshot_dedup_hits(), 0);
        assert_eq!(telemetry.snapshot_dedup_ratio(), Some(0.0));
    }

    #[test]
    fn test_gap_and_resend_counters() {
        let telemetry = RetentionTelemetry::new();

        telemetry.record_delta_gap();
        telemetry.record_delta_gap();
        telemetry.record_baseline_resend();

        assert_eq!(telemetry.delta_gap_detections(), 2);
        assert_eq!(telemetry.baseline_resends(), 1);
    }

    #[test]
    fn test_summary_format() {
        let telemetry = RetentionTelemetry::new();

        let stats = RetentionStats {
            bytes_removed: 2048,
            bytes_kept: 4096,
            removed_env_deltas: 1,
            kept_env_deltas: 2,
            removed_env_baselines: 1,
            removed_browser_snapshots: 1,
            dropped_for_budget: 2,
            ..Default::default()
        };

        telemetry.record_retention(&stats);
        telemetry.record_dedup_drop();

        let summary = telemetry.summary();
        assert!(summary.contains("Operations: 1"));
        assert!(summary.contains("Bytes saved: 2048"));
        assert!(summary.contains("Deltas: 2 kept, 1 removed"));
        assert!(summary.contains("Dedup drops: 1"));
        assert!(summary.contains("Budget drops: 2"));
    }
}
