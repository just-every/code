use std::fs;
use std::path::Path;

use codex_tui::spec_status::{
    SpecStatusArgs, StageCue, StageKind, collect_report, render_dashboard,
};
use tempfile::TempDir;
use walkdir::WalkDir;

#[test]
fn fixture_healthy_marks_pass() {
    let tmp = load_fixture("healthy");
    let report = collect_fixture_report(tmp.path(), "SPEC-FIX-HEALTHY");
    assert!(report.warnings.is_empty());
    let plan = find_stage(&report, StageKind::Plan);
    assert_eq!(plan.cue, StageCue::Pass);
    assert!(!plan.is_stale);
}

#[test]
fn fixture_stale_sets_warning() {
    let tmp = load_fixture("stale");
    let report = collect_fixture_report(tmp.path(), "SPEC-FIX-STALE");
    let plan = find_stage(&report, StageKind::Plan);
    assert!(plan.is_stale);
    assert!(report.warnings.iter().any(|w| w.contains("stale")));
}

#[test]
fn fixture_missing_doc_warns_packet_health() {
    let tmp = load_fixture("missing-doc");
    let report = collect_fixture_report(tmp.path(), "SPEC-FIX-MISS");
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("tasks.md missing"))
    );
    assert!(report.packet.docs.get("tasks.md").is_some());
}

#[test]
fn fixture_conflict_flags_consensus() {
    let tmp = load_fixture("conflict");
    let report = collect_fixture_report(tmp.path(), "SPEC-FIX-CONFLICT");
    let plan = find_stage(&report, StageKind::Plan);
    assert!(plan.consensus.disagreement);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("Consensus conflicts"))
    );
}

#[test]
fn fixture_hal_skipped_does_not_fail() {
    let tmp = load_fixture("hal-skipped");
    let report = collect_fixture_report(tmp.path(), "SPEC-FIX-HALSKIP");
    assert!(
        report
            .warnings
            .iter()
            .all(|w| !w.contains("HAL telemetry reported failure"))
    );
}

#[test]
fn oversized_fixture_reports_threshold_and_top_entries() {
    let tmp = load_fixture("oversized");
    let repo_root = tmp.path();
    let evidence_dir = repo_root
        .join("docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-FIX-OVERSIZED");
    fs::create_dir_all(&evidence_dir).expect("create evidence dir");
    fs::write(evidence_dir.join("big.bin"), vec![0u8; 26 * 1024 * 1024]).expect("write big file");

    let report = collect_fixture_report(repo_root, "SPEC-FIX-OVERSIZED");
    assert!(report.evidence.threshold.is_some());
    assert!(!report.evidence.top_entries.is_empty());
}

#[test]
fn render_dashboard_includes_warning_section_when_needed() {
    let tmp = load_fixture("stale");
    let report = collect_fixture_report(tmp.path(), "SPEC-FIX-STALE");
    let markdown = render_dashboard(&report).join("\n");
    assert!(markdown.contains("## Warnings"));
    assert!(markdown.contains("stale"));
}

fn load_fixture(name: &str) -> TempDir {
    let tmp = TempDir::new().expect("create temp repo");
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let src_root = Path::new(manifest_dir)
        .join("tests/fixtures/spec_status")
        .join(name);
    copy_recursively(&src_root, tmp.path()).expect("copy fixture");
    tmp
}

fn collect_fixture_report(
    repo_root: &Path,
    spec_id: &str,
) -> codex_tui::spec_status::SpecStatusReport {
    let args = SpecStatusArgs {
        spec_id: spec_id.to_string(),
        stale_hours: 24,
    };
    collect_report(repo_root, args).expect("collect report")
}

fn find_stage<'a>(
    report: &'a codex_tui::spec_status::SpecStatusReport,
    stage: StageKind,
) -> &'a codex_tui::spec_status::StageSnapshot {
    report
        .stage_snapshots
        .iter()
        .find(|snapshot| snapshot.stage == stage)
        .expect("stage snapshot")
}

fn copy_recursively(src: &Path, dst: &Path) -> std::io::Result<()> {
    for entry in WalkDir::new(src) {
        let entry = entry?;
        let relative = entry.path().strip_prefix(src).unwrap();
        let target = dst.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}
