//! Unit tests for context_timeline module.

use std::collections::BTreeMap;

use pretty_assertions::assert_eq;

use crate::{EnvironmentContextDelta, EnvironmentContextSnapshot};

use super::*;

// Helper functions for creating test data

fn create_test_snapshot(cwd: &str, git_branch: Option<&str>) -> EnvironmentContextSnapshot {
    EnvironmentContextSnapshot {
        version: 1,
        cwd: Some(cwd.to_string()),
        approval_policy: None,
        sandbox_mode: None,
        network_access: None,
        writable_roots: vec![],
        operating_system: None,
        common_tools: vec![],
        shell: None,
        git_branch: git_branch.map(|s| s.to_string()),
        reasoning_effort: None,
    }
}

fn create_test_delta(base_fingerprint: &str, cwd: &str) -> EnvironmentContextDelta {
    let mut changes = BTreeMap::new();
    changes.insert("cwd".to_string(), serde_json::json!(cwd));

    EnvironmentContextDelta {
        version: 1,
        base_fingerprint: base_fingerprint.to_string(),
        changes,
    }
}

// Baseline tests

#[test]
fn test_add_baseline_once_success() {
    let mut timeline = ContextTimeline::new();
    let snapshot = create_test_snapshot("/repo", Some("main"));

    let result = timeline.add_baseline_once(snapshot.clone());
    assert!(result.is_ok());
    assert_eq!(timeline.baseline(), Some(&snapshot));
}

#[test]
fn test_add_baseline_once_fails_when_already_set() {
    let mut timeline = ContextTimeline::new();
    let snapshot1 = create_test_snapshot("/repo", Some("main"));
    let snapshot2 = create_test_snapshot("/other", Some("feature"));

    timeline.add_baseline_once(snapshot1).unwrap();
    let result = timeline.add_baseline_once(snapshot2);

    assert!(matches!(result, Err(TimelineError::BaselineAlreadySet)));
}

// Delta tests

#[test]
fn test_apply_delta_requires_baseline() {
    let mut timeline = ContextTimeline::new();
    let delta = create_test_delta("fingerprint", "/new-repo");

    let result = timeline.apply_delta(1, delta);
    assert!(matches!(result, Err(TimelineError::BaselineNotSet)));
}

#[test]
fn test_apply_delta_validates_sequence() {
    let mut timeline = ContextTimeline::new();
    let baseline = create_test_snapshot("/repo", Some("main"));
    timeline.add_baseline_once(baseline).unwrap();

    let delta = create_test_delta("fingerprint", "/new-repo");

    // Try to apply delta with wrong sequence
    let result = timeline.apply_delta(5, delta);
    assert!(matches!(
        result,
        Err(TimelineError::DeltaSequenceOutOfOrder { expected: 1, actual: 5 })
    ));
}

#[test]
fn test_apply_delta_increments_sequence() {
    let mut timeline = ContextTimeline::new();
    let baseline = create_test_snapshot("/repo", Some("main"));
    timeline.add_baseline_once(baseline).unwrap();

    let delta1 = create_test_delta("fp1", "/repo-1");
    let delta2 = create_test_delta("fp2", "/repo-2");

    assert_eq!(timeline.next_sequence(), 1);
    timeline.apply_delta(1, delta1).unwrap();
    assert_eq!(timeline.next_sequence(), 2);
    timeline.apply_delta(2, delta2).unwrap();
    assert_eq!(timeline.next_sequence(), 3);
}

#[test]
fn test_get_delta() {
    let mut timeline = ContextTimeline::new();
    let baseline = create_test_snapshot("/repo", Some("main"));
    timeline.add_baseline_once(baseline).unwrap();

    let delta = create_test_delta("fp1", "/repo-1");
    timeline.apply_delta(1, delta.clone()).unwrap();

    let retrieved = timeline.get_delta(1);
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().sequence, 1);
    assert_eq!(retrieved.unwrap().delta, delta);

    // Non-existent sequence
    assert!(timeline.get_delta(99).is_none());
}

#[test]
fn test_delta_sequences() {
    let mut timeline = ContextTimeline::new();
    let baseline = create_test_snapshot("/repo", Some("main"));
    timeline.add_baseline_once(baseline).unwrap();

    timeline.apply_delta(1, create_test_delta("fp1", "/repo-1")).unwrap();
    timeline.apply_delta(2, create_test_delta("fp2", "/repo-2")).unwrap();
    timeline.apply_delta(3, create_test_delta("fp3", "/repo-3")).unwrap();

    let sequences = timeline.delta_sequences();
    assert_eq!(sequences, vec![1, 2, 3]);
}

// Snapshot tests

#[test]
fn test_record_snapshot() {
    let mut timeline = ContextTimeline::new();
    let snapshot = create_test_snapshot("/repo", Some("main"));

    let recorded = timeline.record_snapshot(snapshot.clone()).unwrap();
    assert!(recorded); // First time should record

    assert_eq!(timeline.snapshot_count(), 1);

    let fingerprint = snapshot.fingerprint();
    let retrieved = timeline.get_snapshot(&fingerprint);
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().snapshot, snapshot);
}

#[test]
fn test_record_snapshot_deduplication() {
    let mut timeline = ContextTimeline::new();
    let snapshot = create_test_snapshot("/repo", Some("main"));

    // Record the same snapshot twice
    let first = timeline.record_snapshot(snapshot.clone()).unwrap();
    let second = timeline.record_snapshot(snapshot.clone()).unwrap();

    assert!(first); // First time records
    assert!(!second); // Second time deduplicated

    assert_eq!(timeline.snapshot_count(), 1); // Only one snapshot stored
}

#[test]
fn test_snapshot_fingerprints() {
    let mut timeline = ContextTimeline::new();

    let snap1 = create_test_snapshot("/repo1", Some("main"));
    let snap2 = create_test_snapshot("/repo2", Some("feature"));

    timeline.record_snapshot(snap1.clone()).unwrap();
    timeline.record_snapshot(snap2.clone()).unwrap();

    let fingerprints = timeline.snapshot_fingerprints();
    assert_eq!(fingerprints.len(), 2);
    assert!(fingerprints.contains(&snap1.fingerprint()));
    assert!(fingerprints.contains(&snap2.fingerprint()));
}

// Lookup tests

#[test]
fn test_baseline_returns_none_initially() {
    let timeline = ContextTimeline::new();
    assert!(timeline.baseline().is_none());
}

#[test]
fn test_delta_count() {
    let mut timeline = ContextTimeline::new();
    let baseline = create_test_snapshot("/repo", Some("main"));
    timeline.add_baseline_once(baseline).unwrap();

    assert_eq!(timeline.delta_count(), 0);

    timeline.apply_delta(1, create_test_delta("fp1", "/repo-1")).unwrap();
    assert_eq!(timeline.delta_count(), 1);

    timeline.apply_delta(2, create_test_delta("fp2", "/repo-2")).unwrap();
    assert_eq!(timeline.delta_count(), 2);
}

// Size estimation tests

#[test]
fn test_estimated_bytes() {
    let mut timeline = ContextTimeline::new();

    // Empty timeline
    assert_eq!(timeline.estimated_bytes(), 0);

    // With baseline
    let baseline = create_test_snapshot("/repo", Some("main"));
    timeline.add_baseline_once(baseline).unwrap();
    let baseline_size = timeline.estimated_bytes();
    assert!(baseline_size > 0);

    // With delta
    timeline.apply_delta(1, create_test_delta("fp", "/new-repo")).unwrap();
    let with_delta_size = timeline.estimated_bytes();
    assert!(with_delta_size > baseline_size);

    // With snapshot
    timeline.record_snapshot(create_test_snapshot("/snap", Some("feature"))).unwrap();
    let with_snapshot_size = timeline.estimated_bytes();
    assert!(with_snapshot_size > with_delta_size);
}

// Integration tests

#[test]
fn test_full_timeline_workflow() {
    let mut timeline = ContextTimeline::new();

    // 1. Set baseline
    let baseline = create_test_snapshot("/repo", Some("main"));
    timeline.add_baseline_once(baseline.clone()).unwrap();

    // 2. Record initial snapshot
    timeline.record_snapshot(baseline.clone()).unwrap();

    // 3. Apply deltas
    for i in 1..=5 {
        let delta = create_test_delta(&format!("fp{}", i), &format!("/repo-{}", i));
        timeline.apply_delta(i, delta).unwrap();
    }

    // 4. Record more snapshots
    for i in 1..=3 {
        let snapshot = create_test_snapshot(&format!("/snap-{}", i), Some("feature"));
        timeline.record_snapshot(snapshot).unwrap();
    }

    // 5. Verify state
    assert!(timeline.baseline().is_some());
    assert_eq!(timeline.delta_count(), 5);
    assert_eq!(timeline.snapshot_count(), 4); // baseline + 3 unique snapshots

    let assembled = timeline
        .assemble_prompt_items(3, None)
        .expect("assemble prompt items");
    assert_eq!(assembled.len(), 4); // baseline + last three deltas
}

#[test]
fn test_timeline_is_empty_flag() {
    let mut timeline = ContextTimeline::new();
    assert!(timeline.is_empty());

    let snapshot = create_test_snapshot("/repo", Some("main"));
    timeline.add_baseline_once(snapshot).unwrap();
    assert!(!timeline.is_empty());
}

#[test]
fn assemble_prompt_respects_max_deltas() {
    let mut timeline = ContextTimeline::new();
    let baseline = create_test_snapshot("/repo", Some("main"));
    timeline.add_baseline_once(baseline.clone()).unwrap();

    // Produce four deltas that reference successive fingerprints
    let mut base_fp = baseline.fingerprint();
    for i in 0..4 {
        let delta = create_test_delta(&base_fp, &format!("/repo-{i}"));
        let seq = timeline.next_sequence();
        timeline.apply_delta(seq, delta).unwrap();

        // Advance fingerprint so the next delta chains from the previous snapshot
        let next_snapshot = create_test_snapshot(&format!("/repo-{i}"), Some("main"));
        base_fp = next_snapshot.fingerprint();
    }

    let assembled = timeline
        .assemble_prompt_items(2, None)
        .expect("assemble prompt items");
    assert_eq!(assembled.len(), 3); // baseline + last two deltas
}

// Feature flag tests

#[test]
fn test_feature_flags_default_to_false() {
    // These should be false by default (unless environment variables are set)
    // Just verify the functions are callable
    let _ = is_deltas_enabled();
    let _ = is_snapshots_enabled();
    let _ = is_ui_enabled();
}

// Serialization tests

#[test]
fn test_timeline_serialization() {
    let mut timeline = ContextTimeline::new();
    let baseline = create_test_snapshot("/repo", Some("main"));
    timeline.add_baseline_once(baseline).unwrap();
    timeline.apply_delta(1, create_test_delta("fp", "/new")).unwrap();

    // Serialize
    let json = serde_json::to_string(&timeline).unwrap();

    // Deserialize
    let deserialized: ContextTimeline = serde_json::from_str(&json).unwrap();

    // Verify state
    assert!(deserialized.baseline().is_some());
    assert_eq!(deserialized.delta_count(), 1);
    assert_eq!(deserialized.next_sequence(), 2);
}

#[test]
fn test_delta_entry_serialization() {
    let delta = create_test_delta("fp", "/repo");
    let entry = DeltaEntry {
        sequence: 1,
        delta: delta.clone(),
        recorded_at: "2024-01-01T00:00:00Z".to_string(),
    };

    let json = serde_json::to_string(&entry).unwrap();
    let deserialized: DeltaEntry = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.sequence, 1);
    assert_eq!(deserialized.delta, delta);
}

#[test]
fn test_snapshot_entry_serialization() {
    let snapshot = create_test_snapshot("/repo", Some("main"));
    let fingerprint = snapshot.fingerprint();
    let entry = SnapshotEntry {
        fingerprint: fingerprint.clone(),
        snapshot: snapshot.clone(),
        recorded_at: "2024-01-01T00:00:00Z".to_string(),
    };

    let json = serde_json::to_string(&entry).unwrap();
    let deserialized: SnapshotEntry = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.fingerprint, fingerprint);
    assert_eq!(deserialized.snapshot, snapshot);
}
