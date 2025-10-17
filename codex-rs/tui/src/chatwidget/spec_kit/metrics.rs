//! Observability metrics for spec-kit operations (T90)
//!
//! Collects and exports metrics for:
//! - Success/failure rates per stage
//! - Timing distributions (p50, p95, p99)
//! - Error frequency and types
//! - Quality gate statistics
//!
//! REBASE-SAFE: New file, 99.5% isolation, minimal instrumentation in handler.rs

use once_cell::sync::Lazy;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use crate::spec_prompts::SpecStage;

/// Global metrics instance (thread-safe)
pub static METRICS: Lazy<SpecKitMetrics> = Lazy::new(SpecKitMetrics::new);

/// Metrics collector for spec-kit operations
#[derive(Debug)]
pub struct SpecKitMetrics {
    // Success/failure counters
    stage_success: AtomicU64,
    stage_failure: AtomicU64,
    quality_gate_success: AtomicU64,
    quality_gate_escalations: AtomicU64,

    // Timing histograms (stage → durations)
    stage_timings: Mutex<HashMap<String, Vec<Duration>>>,

    // Error tracking (error_type → count)
    error_counts: Mutex<HashMap<String, u64>>,

    // Quality gate stats
    auto_resolutions: AtomicU64,
    gpt5_validations: AtomicU64,
    human_escalations: AtomicU64,
}

impl SpecKitMetrics {
    /// Create new metrics collector
    pub fn new() -> Self {
        Self {
            stage_success: AtomicU64::new(0),
            stage_failure: AtomicU64::new(0),
            quality_gate_success: AtomicU64::new(0),
            quality_gate_escalations: AtomicU64::new(0),
            stage_timings: Mutex::new(HashMap::new()),
            error_counts: Mutex::new(HashMap::new()),
            auto_resolutions: AtomicU64::new(0),
            gpt5_validations: AtomicU64::new(0),
            human_escalations: AtomicU64::new(0),
        }
    }

    /// Record successful stage completion
    pub fn record_stage_success(&self, stage: SpecStage, duration: Duration) {
        self.stage_success.fetch_add(1, Ordering::Relaxed);
        self.record_timing(stage.command_name(), duration);
    }

    /// Record stage failure
    pub fn record_stage_failure(&self, stage: SpecStage, duration: Duration) {
        self.stage_failure.fetch_add(1, Ordering::Relaxed);
        self.record_timing(stage.command_name(), duration);
    }

    /// Record timing for a stage
    fn record_timing(&self, stage: &str, duration: Duration) {
        if let Ok(mut timings) = self.stage_timings.lock() {
            timings
                .entry(stage.to_string())
                .or_insert_with(Vec::new)
                .push(duration);
        }
    }

    /// Record error occurrence
    pub fn record_error(&self, error_type: &str) {
        if let Ok(mut errors) = self.error_counts.lock() {
            *errors.entry(error_type.to_string()).or_insert(0) += 1;
        }
    }

    /// Record quality gate statistics
    pub fn record_quality_gate_outcome(&self, auto_resolved: usize, gpt5_validated: usize, escalated: usize) {
        self.auto_resolutions.fetch_add(auto_resolved as u64, Ordering::Relaxed);
        self.gpt5_validations.fetch_add(gpt5_validated as u64, Ordering::Relaxed);
        self.human_escalations.fetch_add(escalated as u64, Ordering::Relaxed);

        if escalated > 0 {
            self.quality_gate_escalations.fetch_add(1, Ordering::Relaxed);
        } else {
            self.quality_gate_success.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Get current metrics snapshot
    pub fn snapshot(&self) -> MetricsSnapshot {
        let stage_timings = self.stage_timings.lock()
            .map(|t| t.clone())
            .unwrap_or_default();

        let error_counts = self.error_counts.lock()
            .map(|e| e.clone())
            .unwrap_or_default();

        MetricsSnapshot {
            stage_success: self.stage_success.load(Ordering::Relaxed),
            stage_failure: self.stage_failure.load(Ordering::Relaxed),
            quality_gate_success: self.quality_gate_success.load(Ordering::Relaxed),
            quality_gate_escalations: self.quality_gate_escalations.load(Ordering::Relaxed),
            auto_resolutions: self.auto_resolutions.load(Ordering::Relaxed),
            gpt5_validations: self.gpt5_validations.load(Ordering::Relaxed),
            human_escalations: self.human_escalations.load(Ordering::Relaxed),
            stage_timings,
            error_counts,
        }
    }

    /// Export metrics in Prometheus format
    pub fn export_prometheus(&self) -> String {
        let snap = self.snapshot();
        let mut output = String::new();

        // Counter metrics
        output.push_str(&format!("# HELP speckit_stage_success_total Total successful stages\n"));
        output.push_str(&format!("# TYPE speckit_stage_success_total counter\n"));
        output.push_str(&format!("speckit_stage_success_total {}\n\n", snap.stage_success));

        output.push_str(&format!("# HELP speckit_stage_failure_total Total failed stages\n"));
        output.push_str(&format!("# TYPE speckit_stage_failure_total counter\n"));
        output.push_str(&format!("speckit_stage_failure_total {}\n\n", snap.stage_failure));

        // Quality gate metrics
        output.push_str(&format!("# HELP speckit_auto_resolutions_total Auto-resolved quality issues\n"));
        output.push_str(&format!("# TYPE speckit_auto_resolutions_total counter\n"));
        output.push_str(&format!("speckit_auto_resolutions_total {}\n\n", snap.auto_resolutions));

        output.push_str(&format!("# HELP speckit_gpt5_validations_total GPT-5 validated issues\n"));
        output.push_str(&format!("# TYPE speckit_gpt5_validations_total counter\n"));
        output.push_str(&format!("speckit_gpt5_validations_total {}\n\n", snap.gpt5_validations));

        output.push_str(&format!("# HELP speckit_human_escalations_total Human-answered issues\n"));
        output.push_str(&format!("# TYPE speckit_human_escalations_total counter\n"));
        output.push_str(&format!("speckit_human_escalations_total {}\n\n", snap.human_escalations));

        // Timing histograms
        for (stage, durations) in &snap.stage_timings {
            if durations.is_empty() {
                continue;
            }

            let mut sorted = durations.clone();
            sorted.sort();

            let p50 = Self::percentile(&sorted, 50);
            let p95 = Self::percentile(&sorted, 95);
            let p99 = Self::percentile(&sorted, 99);

            output.push_str(&format!("# HELP speckit_stage_duration_seconds Stage duration percentiles\n"));
            output.push_str(&format!("# TYPE speckit_stage_duration_seconds summary\n"));
            output.push_str(&format!("speckit_stage_duration_seconds{{stage=\"{}\",quantile=\"0.5\"}} {:.2}\n", stage, p50.as_secs_f64()));
            output.push_str(&format!("speckit_stage_duration_seconds{{stage=\"{}\",quantile=\"0.95\"}} {:.2}\n", stage, p95.as_secs_f64()));
            output.push_str(&format!("speckit_stage_duration_seconds{{stage=\"{}\",quantile=\"0.99\"}} {:.2}\n", stage, p99.as_secs_f64()));
            output.push_str("\n");
        }

        // Error counts
        for (error_type, count) in &snap.error_counts {
            output.push_str(&format!("speckit_errors_total{{type=\"{}\"}} {}\n", error_type, count));
        }

        output
    }

    /// Export metrics in JSON format
    pub fn export_json(&self) -> Result<String, serde_json::Error> {
        let snap = self.snapshot();
        serde_json::to_string_pretty(&snap)
    }

    /// Calculate percentile from sorted durations
    fn percentile(sorted: &[Duration], p: usize) -> Duration {
        if sorted.is_empty() {
            return Duration::ZERO;
        }
        let idx = (sorted.len() * p / 100).min(sorted.len() - 1);
        sorted[idx]
    }

    /// Reset all metrics (for testing)
    #[cfg(test)]
    pub fn reset(&self) {
        self.stage_success.store(0, Ordering::Relaxed);
        self.stage_failure.store(0, Ordering::Relaxed);
        self.quality_gate_success.store(0, Ordering::Relaxed);
        self.quality_gate_escalations.store(0, Ordering::Relaxed);
        self.auto_resolutions.store(0, Ordering::Relaxed);
        self.gpt5_validations.store(0, Ordering::Relaxed);
        self.human_escalations.store(0, Ordering::Relaxed);

        if let Ok(mut timings) = self.stage_timings.lock() {
            timings.clear();
        }
        if let Ok(mut errors) = self.error_counts.lock() {
            errors.clear();
        }
    }
}

impl Default for SpecKitMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Point-in-time metrics snapshot
#[derive(Debug, Clone, Serialize)]
pub struct MetricsSnapshot {
    pub stage_success: u64,
    pub stage_failure: u64,
    pub quality_gate_success: u64,
    pub quality_gate_escalations: u64,
    pub auto_resolutions: u64,
    pub gpt5_validations: u64,
    pub human_escalations: u64,
    #[serde(skip)]  // Durations don't serialize easily
    pub stage_timings: HashMap<String, Vec<Duration>>,
    pub error_counts: HashMap<String, u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_creation() {
        let metrics = SpecKitMetrics::new();
        let snap = metrics.snapshot();
        assert_eq!(snap.stage_success, 0);
        assert_eq!(snap.stage_failure, 0);
    }

    #[test]
    fn test_record_stage_success() {
        let metrics = SpecKitMetrics::new();
        metrics.reset();

        metrics.record_stage_success(SpecStage::Plan, Duration::from_secs(10));
        metrics.record_stage_success(SpecStage::Tasks, Duration::from_secs(15));

        let snap = metrics.snapshot();
        assert_eq!(snap.stage_success, 2);
        assert_eq!(snap.stage_failure, 0);
    }

    #[test]
    fn test_record_stage_failure() {
        let metrics = SpecKitMetrics::new();
        metrics.reset();

        metrics.record_stage_failure(SpecStage::Validate, Duration::from_secs(5));

        let snap = metrics.snapshot();
        assert_eq!(snap.stage_success, 0);
        assert_eq!(snap.stage_failure, 1);
    }

    #[test]
    fn test_record_quality_gate_outcome() {
        let metrics = SpecKitMetrics::new();
        metrics.reset();

        metrics.record_quality_gate_outcome(5, 2, 1);  // 5 auto, 2 gpt5, 1 escalated

        let snap = metrics.snapshot();
        assert_eq!(snap.auto_resolutions, 5);
        assert_eq!(snap.gpt5_validations, 2);
        assert_eq!(snap.human_escalations, 1);
        assert_eq!(snap.quality_gate_escalations, 1);  // Had escalations
    }

    #[test]
    fn test_record_error() {
        let metrics = SpecKitMetrics::new();
        metrics.reset();

        metrics.record_error("ConsensusFailure");
        metrics.record_error("ConsensusFailure");
        metrics.record_error("FileNotFound");

        let snap = metrics.snapshot();
        assert_eq!(snap.error_counts.get("ConsensusFailure"), Some(&2));
        assert_eq!(snap.error_counts.get("FileNotFound"), Some(&1));
    }

    #[test]
    fn test_export_prometheus_format() {
        let metrics = SpecKitMetrics::new();
        metrics.reset();
        metrics.record_stage_success(SpecStage::Plan, Duration::from_secs(10));

        let output = metrics.export_prometheus();
        assert!(output.contains("speckit_stage_success_total 1"));
        assert!(output.contains("HELP"));
        assert!(output.contains("TYPE"));
    }

    #[test]
    fn test_export_json_format() {
        let metrics = SpecKitMetrics::new();
        metrics.reset();
        metrics.record_stage_success(SpecStage::Plan, Duration::from_secs(10));

        let json = metrics.export_json().expect("export json");
        assert!(json.contains("stage_success"));
        assert!(json.contains("\"stage_success\": 1"));
    }

    #[test]
    fn test_percentile_calculation() {
        let durations = vec![
            Duration::from_secs(1),
            Duration::from_secs(2),
            Duration::from_secs(3),
            Duration::from_secs(4),
            Duration::from_secs(5),
        ];

        let p50 = SpecKitMetrics::percentile(&durations, 50);
        assert_eq!(p50, Duration::from_secs(3));  // Median

        let p100 = SpecKitMetrics::percentile(&durations, 100);
        assert_eq!(p100, Duration::from_secs(5));  // Max
    }

    #[test]
    fn test_percentile_empty_list() {
        let durations: Vec<Duration> = vec![];
        let p50 = SpecKitMetrics::percentile(&durations, 50);
        assert_eq!(p50, Duration::ZERO);
    }

    #[test]
    fn test_reset_clears_all_metrics() {
        let metrics = SpecKitMetrics::new();
        metrics.record_stage_success(SpecStage::Plan, Duration::from_secs(10));
        metrics.record_error("test");

        metrics.reset();

        let snap = metrics.snapshot();
        assert_eq!(snap.stage_success, 0);
        assert_eq!(snap.error_counts.len(), 0);
    }
}
