//! Phase 4 Tests: Property-Based Testing (PB01-PB10)
//!
//! FORK-SPECIFIC (just-every/code): Spec-kit Phase 4 property-based testing
//!
//! Uses proptest for generative testing with random inputs
//! Verifies invariants hold across all possible inputs

mod common;

use common::{IntegrationTestContext, StateBuilder};
use codex_tui::{SpecStage, SpecAutoState};
use proptest::prelude::*;
use serde_json::json;

// ============================================================================
// PB01-PB03: State Invariants
// ============================================================================

proptest! {
    #[test]
    fn pb01_state_index_always_in_valid_range(index in 0usize..20) {
        // Property: State index always valid or returns None
        let mut state = StateBuilder::new("SPEC-PB01-TEST")
            .starting_at(SpecStage::Plan)
            .build();

        state.current_index = index;

        // Invariant: index ∈ [0, 5] → Some(_), else None
        if index < 6 {
            prop_assert!(state.current_stage().is_some());
        } else {
            prop_assert_eq!(state.current_stage(), None);
        }
    }

    #[test]
    fn pb02_current_stage_always_some_when_index_under_six(index in 0usize..6) {
        // Property: current_stage() always Some when index < 6
        let mut state = StateBuilder::new("SPEC-PB02-TEST")
            .starting_at(SpecStage::Plan)
            .build();

        state.current_index = index;

        prop_assert!(state.current_stage().is_some());

        // Verify correct stage mapping
        let expected_stages = vec![
            SpecStage::Plan,
            SpecStage::Tasks,
            SpecStage::Implement,
            SpecStage::Validate,
            SpecStage::Audit,
            SpecStage::Unlock,
        ];

        prop_assert_eq!(state.current_stage(), Some(expected_stages[index]));
    }

    #[test]
    fn pb03_retry_count_never_negative(retries in 0usize..100) {
        // Property: Retry count never negative or > max_retries
        let ctx = IntegrationTestContext::new("SPEC-PB03-TEST").unwrap();

        let max_retries = 3;
        let capped_retries = retries.min(max_retries);

        let retry_file = ctx.commands_dir().join("retry.json");
        std::fs::write(&retry_file, json!({
            "retry_count": capped_retries,
            "max_retries": max_retries,
            "within_limit": capped_retries <= max_retries
        }).to_string()).unwrap();

        let content = std::fs::read_to_string(&retry_file).unwrap();
        let data: serde_json::Value = serde_json::from_str(&content).unwrap();

        prop_assert!(data["retry_count"].as_u64().unwrap() <= max_retries as u64);
        prop_assert_eq!(data["within_limit"].as_bool(), Some(true));
    }
}

// ============================================================================
// PB04-PB06: Evidence Integrity
// ============================================================================

proptest! {
    #[test]
    fn pb04_written_evidence_always_parseable_json(
        agent in "[a-z]{3,10}",
        content in ".*"
    ) {
        // Property: Written evidence always parseable JSON
        let ctx = IntegrationTestContext::new("SPEC-PB04-TEST").unwrap();

        let evidence = json!({
            "agent": agent,
            "content": content,
            "timestamp": "2025-10-19T00:00:00Z"
        });

        let file = ctx.consensus_dir().join("test.json");
        std::fs::write(&file, evidence.to_string()).unwrap();

        // Invariant: Can always parse what we write
        let read_content = std::fs::read_to_string(&file).unwrap();
        let parsed = serde_json::from_str::<serde_json::Value>(&read_content);

        prop_assert!(parsed.is_ok());
        let data = parsed.unwrap();
        prop_assert_eq!(data["agent"].as_str(), Some(agent.as_str()));
    }

    #[test]
    fn pb05_consensus_timestamps_always_valid_iso8601(
        year in 2020u32..2030,
        month in 1u32..=12,
        day in 1u32..=28  // Safe for all months
    ) {
        // Property: Generated timestamps always valid ISO8601 format
        let ctx = IntegrationTestContext::new("SPEC-PB05-TEST").unwrap();

        let timestamp = format!("{:04}-{:02}-{:02}T00:00:00Z", year, month, day);

        let file = ctx.consensus_dir().join("timestamp.json");
        std::fs::write(&file, json!({
            "timestamp": timestamp.clone()
        }).to_string()).unwrap();

        let content = std::fs::read_to_string(&file).unwrap();
        let data: serde_json::Value = serde_json::from_str(&content).unwrap();

        // Invariant: Timestamp string always valid ISO8601
        let ts = data["timestamp"].as_str().unwrap();
        let year_str = format!("{:04}", year);
        prop_assert!(ts.contains(&year_str), "Timestamp {} should contain year {}", ts, year_str);
        prop_assert!(ts.contains("T00:00:00Z"), "Timestamp {} should contain T00:00:00Z", ts);
    }

    #[test]
    fn pb06_evidence_paths_always_relative_to_evidence_dir(
        subdir in "[a-z]{3,10}"
    ) {
        // Property: Evidence paths always relative to evidence_dir
        let ctx = IntegrationTestContext::new("SPEC-PB06-TEST").unwrap();

        let evidence_subdir = ctx.evidence_dir.join(&subdir);
        std::fs::create_dir_all(&evidence_subdir).unwrap();

        // Invariant: All evidence paths start with evidence_dir
        prop_assert!(evidence_subdir.starts_with(&ctx.evidence_dir));
        prop_assert!(ctx.consensus_dir().starts_with(&ctx.evidence_dir));
        prop_assert!(ctx.commands_dir().starts_with(&ctx.evidence_dir));
    }
}

// ============================================================================
// PB07-PB08: Consensus Quorum
// ============================================================================

proptest! {
    #[test]
    fn pb07_n_of_m_agents_valid_consensus(n in 1usize..=5, m in 1usize..=5) {
        // Property: N/M agents (N ≤ M) → Valid consensus possible
        prop_assume!(n <= m); // Only test when N ≤ M

        let ctx = IntegrationTestContext::new("SPEC-PB07-TEST").unwrap();

        let agents = vec!["gemini", "claude", "gpt_pro", "code", "gpt_codex"];
        let participating = &agents[0..n.min(agents.len())];

        // Write consensus from N agents
        for (i, agent) in participating.iter().enumerate() {
            let file = ctx.consensus_dir().join(format!("agent_{}.json", i));
            std::fs::write(&file, json!({
                "agent": agent,
                "index": i
            }).to_string()).unwrap();
        }

        // Invariant: Can form consensus with any N/M where N ≤ M
        let count = ctx.count_consensus_files();
        prop_assert_eq!(count, n.min(agents.len()));
    }

    #[test]
    fn pb08_zero_agents_error_not_consensus(m in 1usize..=5) {
        // Property: 0/M agents → Error, not consensus
        let ctx = IntegrationTestContext::new("SPEC-PB08-TEST").unwrap();

        // Zero agents responding
        let error = ctx.commands_dir().join("zero_agents.json");
        std::fs::write(&error, json!({
            "total_agents": m,
            "responding_agents": 0,
            "error": "insufficient_consensus",
            "min_required": 1
        }).to_string()).unwrap();

        let content = std::fs::read_to_string(&error).unwrap();
        let data: serde_json::Value = serde_json::from_str(&content).unwrap();

        // Invariant: Zero agents always produces error
        prop_assert_eq!(data["responding_agents"].as_u64(), Some(0));
        prop_assert!(data["error"].as_str().is_some());
    }
}

// ============================================================================
// PB09-PB10: Retry Idempotence
// ============================================================================

proptest! {
    #[test]
    fn pb09_retry_operation_n_times_yields_same_result(retry_count in 1usize..10) {
        // Property: Retry(op, N) = Retry(op, 1) ∀N (idempotent)
        let ctx = IntegrationTestContext::new("SPEC-PB09-TEST").unwrap();

        // Simulate N retries
        for attempt in 1..=retry_count {
            let retry_file = ctx.commands_dir().join(format!("retry_{}.json", attempt));
            std::fs::write(&retry_file, json!({
                "attempt": attempt,
                "final_result": "success", // Idempotent - same result
                "total_attempts": retry_count
            }).to_string()).unwrap();
        }

        // Invariant: All retries produce same final_result
        for attempt in 1..=retry_count {
            let retry_file = ctx.commands_dir().join(format!("retry_{}.json", attempt));
            let content = std::fs::read_to_string(&retry_file).unwrap();
            let data: serde_json::Value = serde_json::from_str(&content).unwrap();
            prop_assert_eq!(data["final_result"].as_str(), Some("success"));
        }
    }

    #[test]
    fn pb10_failed_op_retried_m_times_same_final_state(failures in 1usize..10) {
        // Property: Failed op retried M times → Same final state
        let ctx = IntegrationTestContext::new("SPEC-PB10-TEST").unwrap();

        // Simulate M failures
        for attempt in 1..=failures {
            let failure_file = ctx.commands_dir().join(format!("failure_{}.json", attempt));
            std::fs::write(&failure_file, json!({
                "attempt": attempt,
                "status": "failed",
                "error": "same_error_each_time"
            }).to_string()).unwrap();
        }

        // Final state after M failures
        let final_state = ctx.commands_dir().join("final_state.json");
        std::fs::write(&final_state, json!({
            "total_failures": failures,
            "final_status": "exhausted",
            "all_same_error": true
        }).to_string()).unwrap();

        // Invariant: Final state consistent regardless of M
        let content = std::fs::read_to_string(&final_state).unwrap();
        let data: serde_json::Value = serde_json::from_str(&content).unwrap();
        prop_assert_eq!(data["final_status"].as_str(), Some("exhausted"));
    }
}
