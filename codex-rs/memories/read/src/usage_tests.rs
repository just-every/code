//! Regression coverage for versioned memory paths in shell usage telemetry.

use super::MemoriesUsageKind;
use super::memories_usage_kinds_from_command;
use pretty_assertions::assert_eq;

#[test]
fn classifies_reads_in_both_memory_versions() {
    for root in ["memories", "memories_v2"] {
        let command = format!(
            "cat /tmp/.codex/{root}/memory_summary.md && cat /tmp/.codex/{root}/rollout_summaries/thread.md"
        );
        assert_eq!(
            memories_usage_kinds_from_command(&command),
            vec![
                MemoriesUsageKind::MemorySummary,
                MemoriesUsageKind::RolloutSummaries,
            ],
            "memory root: {root}"
        );
    }
}
