use std::cmp::Reverse;
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Duration, Local, Utc};
use serde::Deserialize;
use walkdir::WalkDir;

const DOCS_ROOT: &str = "docs";
const COMMAND_EVIDENCE_ROOT: &str = "docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands";
const CONSENSUS_EVIDENCE_ROOT: &str = "docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus";
const DEFAULT_STALE_HOURS: i64 = 24;
const WARN_FOOTPRINT_BYTES: u64 = 20 * 1024 * 1024; // 20 MB
const CRITICAL_FOOTPRINT_BYTES: u64 = 25 * 1024 * 1024; // 25 MB
const MAX_TOP_EVIDENCE_ENTRIES: usize = 3;

#[derive(Debug, Clone)]
pub struct SpecStatusArgs {
    pub spec_id: String,
    pub stale_hours: i64,
}

impl SpecStatusArgs {
    pub fn from_input(input: &str) -> Result<Self> {
        let mut spec_id: Option<String> = None;
        let mut stale_hours = DEFAULT_STALE_HOURS;

        let tokens: Vec<String> = input.split_whitespace().map(|s| s.to_string()).collect();

        let mut idx = 0;
        while idx < tokens.len() {
            let token = tokens[idx].clone();
            if token.starts_with("--stale-hours") {
                let value = if let Some(eq_pos) = token.find('=') {
                    token[(eq_pos + 1)..].to_string()
                } else {
                    idx += 1;
                    tokens
                        .get(idx)
                        .cloned()
                        .ok_or_else(|| anyhow!("`--stale-hours` requires a value"))?
                };
                stale_hours = value
                    .parse::<i64>()
                    .context("invalid value for --stale-hours")?;
                idx += 1;
            } else if token.starts_with('-') {
                return Err(anyhow!("Unknown flag `{token}`"));
            } else if spec_id.is_none() {
                spec_id = Some(token);
                idx += 1;
            } else {
                return Err(anyhow!("Unexpected extra argument `{token}`"));
            }
        }

        let spec_id = spec_id.ok_or_else(|| {
            anyhow!("/spec-status requires a SPEC ID (e.g. /spec-status SPEC-KIT-DEMO)")
        })?;

        Ok(Self {
            spec_id,
            stale_hours,
        })
    }
}

#[derive(Debug, Clone)]
pub struct SpecStatusReport {
    pub spec_id: String,
    pub generated_at: DateTime<Local>,
    pub stale_cutoff: Duration,
    pub packet: PacketStatus,
    pub tracker_row: Option<TrackerRow>,
    pub stage_snapshots: Vec<StageSnapshot>,
    pub evidence: EvidenceMetrics,
    pub agent_summary: AgentCoverage,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PacketStatus {
    pub directory: Option<PathBuf>,
    pub docs: HashMap<&'static str, bool>,
}

#[derive(Debug, Clone)]
pub struct TrackerRow {
    pub raw: String,
    pub order: Option<String>,
    pub task_id: Option<String>,
    pub title: Option<String>,
    pub status: Option<String>,
    pub owners: Option<String>,
    pub prd: Option<String>,
    pub branch: Option<String>,
    pub pr: Option<String>,
    pub last_validation: Option<String>,
    pub evidence: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StageSnapshot {
    pub stage: StageKind,
    pub guardrail: Option<GuardrailRecord>,
    pub consensus: StageConsensus,
    pub cue: StageCue,
    pub is_stale: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct GuardrailRecord {
    pub schema_version: String,
    pub timestamp: DateTime<Utc>,
    pub baseline_status: Option<String>,
    pub tool_status: Option<String>,
    pub lock_status: Option<String>,
    pub hook_status: Option<String>,
    pub policy_prefilter_status: Option<String>,
    pub policy_final_status: Option<String>,
    pub hal_status: Option<String>,
    pub hal_failed_checks: Vec<String>,
    pub scenarios: Vec<ScenarioStatus>,
}

#[derive(Debug, Clone)]
pub struct ScenarioStatus {
    pub name: String,
    pub status: String,
}

#[derive(Debug, Clone, Default)]
pub struct StageConsensus {
    pub agents: Vec<AgentOutcome>,
    pub synthesis_status: Option<String>,
    pub disagreement: bool,
    pub latest_timestamp: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct AgentOutcome {
    pub agent: String,
    pub model: Option<String>,
    pub reasoning_mode: Option<String>,
    pub status: AgentStatus,
    pub notes: Vec<String>,
    pub timestamp: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    Ok,
    Conflicted,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageCue {
    Pass,
    Warn,
    Pending,
}

impl StageCue {
    pub fn icon(self) -> &'static str {
        match self {
            StageCue::Pass => "✅",
            StageCue::Warn => "⚠",
            StageCue::Pending => "⏳",
        }
    }
}

#[derive(Debug, Clone)]
pub struct EvidenceMetrics {
    pub commands_bytes: u64,
    pub consensus_bytes: u64,
    pub combined_bytes: u64,
    pub latest_artifact: Option<DateTime<Utc>>,
    pub threshold: Option<EvidenceThreshold>,
    pub top_entries: Vec<EvidenceEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceThreshold {
    Warning,
    Critical,
}

#[derive(Debug, Clone)]
pub struct EvidenceEntry {
    pub path: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Default)]
pub struct AgentCoverage {
    pub per_stage: HashMap<StageKind, Vec<AgentOutcome>>, // snapshot copy for rendering convenience
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum StageKind {
    Plan,
    Tasks,
    Implement,
    Validate,
    Audit,
    Unlock,
}

impl StageKind {
    pub fn all() -> &'static [StageKind] {
        &[
            StageKind::Plan,
            StageKind::Tasks,
            StageKind::Implement,
            StageKind::Validate,
            StageKind::Audit,
            StageKind::Unlock,
        ]
    }

    pub fn slug(&self) -> &'static str {
        match self {
            StageKind::Plan => "plan",
            StageKind::Tasks => "tasks",
            StageKind::Implement => "implement",
            StageKind::Validate => "validate",
            StageKind::Audit => "audit",
            StageKind::Unlock => "unlock",
        }
    }

    pub fn display(&self) -> &'static str {
        match self {
            StageKind::Plan => "Plan",
            StageKind::Tasks => "Tasks",
            StageKind::Implement => "Implement",
            StageKind::Validate => "Validate",
            StageKind::Audit => "Audit",
            StageKind::Unlock => "Unlock",
        }
    }
}

pub fn collect_report(repo_root: &Path, args: SpecStatusArgs) -> Result<SpecStatusReport> {
    let now_local = Local::now();
    let stale_cutoff = Duration::hours(args.stale_hours);

    let packet = collect_packet_status(repo_root, &args.spec_id)?;
    let tracker_row = read_tracker_row(repo_root, &args.spec_id)?;

    let mut warnings = Vec::new();

    if packet.directory.is_none() {
        warnings.push(format!(
            "SPEC packet directory not found under docs/ for {}",
            args.spec_id
        ));
    } else {
        for (doc, present) in &packet.docs {
            if !present {
                warnings.push(format!("{} missing in SPEC packet", doc));
            }
        }
    }

    if tracker_row.is_none() {
        warnings.push(format!("SPEC.md tracker row missing for {}", args.spec_id));
    }

    let stage_snapshots = StageKind::all()
        .iter()
        .map(|stage| collect_stage_snapshot(repo_root, &args.spec_id, *stage, stale_cutoff))
        .collect::<Result<Vec<_>>>()?;

    let mut agent_summary = AgentCoverage::default();
    for snap in &stage_snapshots {
        if !snap.consensus.agents.is_empty() {
            agent_summary
                .per_stage
                .entry(snap.stage)
                .or_default()
                .extend(snap.consensus.agents.clone());
        }
        if matches!(snap.cue, StageCue::Warn) {
            if let Some(guardrail) = &snap.guardrail {
                if let Some(status) = &guardrail.policy_final_status {
                    if status.eq_ignore_ascii_case("failed") {
                        warnings.push(format!(
                            "{} policy final check failed for {}",
                            snap.stage.display(),
                            args.spec_id
                        ));
                    }
                }
                if let Some(status) = &guardrail.hal_status {
                    if status.eq_ignore_ascii_case("failed") {
                        let details = if guardrail.hal_failed_checks.is_empty() {
                            String::from("failed checks not reported")
                        } else {
                            guardrail.hal_failed_checks.join(", ")
                        };
                        warnings.push(format!(
                            "HAL telemetry reported failure for {} stage: {}",
                            snap.stage.display(),
                            details
                        ));
                    }
                }
            }
            if snap.consensus.disagreement {
                warnings.push(format!(
                    "Consensus conflicts detected for {} stage ({}).",
                    snap.stage.display(),
                    args.spec_id
                ));
            }
            if snap.is_stale {
                warnings.push(format!(
                    "{} telemetry stale (> {}h old)",
                    snap.stage.display(),
                    args.stale_hours
                ));
            }
        }
    }

    let evidence = compute_evidence_metrics(repo_root, &args.spec_id)?;
    if matches!(evidence.threshold, Some(EvidenceThreshold::Critical)) {
        warnings.push(format!(
            "Evidence footprint for {} exceeds {} MB",
            args.spec_id,
            CRITICAL_FOOTPRINT_BYTES / (1024 * 1024)
        ));
    } else if matches!(evidence.threshold, Some(EvidenceThreshold::Warning)) {
        warnings.push(format!(
            "Evidence footprint for {} exceeds {} MB",
            args.spec_id,
            WARN_FOOTPRINT_BYTES / (1024 * 1024)
        ));
    }

    Ok(SpecStatusReport {
        spec_id: args.spec_id,
        generated_at: now_local,
        stale_cutoff,
        packet,
        tracker_row,
        stage_snapshots,
        evidence,
        agent_summary,
        warnings,
    })
}

pub fn render_dashboard(report: &SpecStatusReport) -> Vec<String> {
    let mut lines = Vec::new();

    lines.push(format!(
        "# SPEC Status: {} ({})",
        report.spec_id,
        report.generated_at.format("%Y-%m-%d %H:%M:%S")
    ));

    lines.push(String::new());
    lines.extend(render_packet_section(&report.packet));

    lines.push(String::new());
    lines.extend(render_tracker_section(report));

    lines.push(String::new());
    lines.extend(render_stage_section(report));

    lines.push(String::new());
    lines.extend(render_evidence_section(&report.evidence));

    lines.push(String::new());
    lines.extend(render_agent_section(&report.agent_summary));

    if !report.warnings.is_empty() {
        lines.push(String::new());
        lines.push("## Warnings".into());
        for warning in &report.warnings {
            lines.push(format!("- ⚠ {}", warning));
        }
    }

    lines
}

fn render_packet_section(packet: &PacketStatus) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push("## Packet".into());
    match &packet.directory {
        Some(dir) => lines.push(format!("- Directory: {}", dir.display())),
        None => lines.push("- Directory: ❌ missing".into()),
    }
    let mut docs: Vec<_> = packet.docs.iter().collect();
    docs.sort_by_key(|(doc, _)| *doc);
    for (doc, &present) in docs {
        let icon = if present { "✅" } else { "⚠" };
        lines.push(format!("- {} {doc}", icon));
    }
    lines
}

fn render_tracker_section(report: &SpecStatusReport) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push("## Tracker".into());
    if let Some(row) = &report.tracker_row {
        lines.push(format!(
            "- Status: {}",
            row.status.as_deref().unwrap_or("unknown")
        ));
        if let Some(branch) = row.branch.as_deref().filter(|b| !b.is_empty()) {
            lines.push(format!("- Branch: {}", branch));
        }
        if let Some(last_validation) = row
            .last_validation
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            lines.push(format!("- Last validation: {}", last_validation));
        }
        if let Some(notes) = row.notes.as_deref().filter(|value| !value.is_empty()) {
            lines.push(format!("- Notes: {}", notes));
        }
        lines.push(format!("- Table row: {}", row.raw));
    } else {
        lines.push("- ⚠ SPEC.md row not found".into());
    }
    lines
}

fn render_stage_section(report: &SpecStatusReport) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push("## Stage Summary".into());
    for snapshot in &report.stage_snapshots {
        lines.push(render_stage_line(report, snapshot));
        for detail in render_stage_details(report, snapshot) {
            lines.push(detail);
        }
    }
    lines
}

fn render_stage_line(report: &SpecStatusReport, snapshot: &StageSnapshot) -> String {
    let mut parts = Vec::new();
    if let Some(guardrail) = &snapshot.guardrail {
        parts.push(format!(
            "baseline:{}",
            guardrail
                .baseline_status
                .as_deref()
                .unwrap_or("unknown")
                .to_lowercase()
        ));
        if let Some(status) = &guardrail.policy_final_status {
            parts.push(format!("policy:{}", status.to_lowercase()));
        }
        if let Some(status) = &guardrail.tool_status {
            parts.push(format!("tool:{}", status.to_lowercase()));
        }
        if let Some(status) = &guardrail.hal_status {
            parts.push(format!("hal:{}", status.to_lowercase()));
        }
        if let Some(status) = &guardrail.lock_status {
            parts.push(format!("lock:{}", status.to_lowercase()));
        }
        parts.push(format!(
            "updated:{}",
            format_datetime_utc(&guardrail.timestamp)
        ));
    } else {
        parts.push("no guardrail telemetry".into());
    }

    if snapshot.is_stale {
        parts.push(format!(
            "stale>{}h",
            report.stale_cutoff.num_hours()
        ));
    }

    if snapshot.consensus.disagreement {
        parts.push("consensus=conflict".into());
    } else if snapshot.consensus.agents.is_empty() {
        parts.push("consensus=missing".into());
    } else {
        parts.push("consensus=ok".into());
    }

    format!(
        "- {} {} — {}",
        snapshot.cue.icon(),
        snapshot.stage.display(),
        parts.join(" • ")
    )
}

fn render_stage_details(report: &SpecStatusReport, snapshot: &StageSnapshot) -> Vec<String> {
    let mut details = Vec::new();

    if let Some(guardrail) = &snapshot.guardrail {
        if guardrail.has_failures() {
            details.push("  - Guardrail reported non-passing status".into());
        }
    }

    if snapshot.is_stale {
        let last = snapshot
            .guardrail
            .as_ref()
            .map(|g| format_datetime_utc(&g.timestamp))
            .or_else(|| {
                snapshot
                    .consensus
                    .latest_timestamp
                    .map(|ts| format_datetime_utc(&ts))
            })
            .unwrap_or_else(|| "unknown".into());
        details.push(format!(
            "  - Stale for >{}h (last updated {})",
            report.stale_cutoff.num_hours(),
            last
        ));
    }

    if snapshot.consensus.disagreement {
        details.push("  - Consensus conflicts detected".into());
    }

    if snapshot.consensus.agents.is_empty() {
        details.push("  - No consensus artifacts recorded".into());
    } else {
        let agents = snapshot
            .consensus
            .agents
            .iter()
            .map(|outcome| {
                format!(
                    "{}({})",
                    outcome.agent,
                    match outcome.status {
                        AgentStatus::Ok => "ok",
                        AgentStatus::Conflicted => "conflict",
                        AgentStatus::Error => "error",
                    }
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        details.push(format!("  - Agents: {}", agents));
    }

    for note in &snapshot.notes {
        details.push(format!("  - {}", note));
    }

    details
}

fn render_evidence_section(evidence: &EvidenceMetrics) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push("## Evidence".into());
    for banner in evidence_banners(evidence) {
        lines.push(format!("- {}", banner));
    }
    if let Some(latest) = evidence.latest_artifact {
        lines.push(format!(
            "- Latest artifact: {} UTC",
            format_datetime_utc(&latest)
        ));
    }
    if !evidence.top_entries.is_empty() {
        lines.push("- Top offenders:".into());
        for entry in &evidence.top_entries {
            lines.push(format!(
                "  - {} ({})",
                entry.path,
                format_filesize(entry.bytes)
            ));
        }
    }
    lines
}

fn render_agent_section(summary: &AgentCoverage) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push("## Agent Coverage".into());
    if summary.per_stage.is_empty() {
        lines.push("- No consensus artifacts recorded".into());
        return lines;
    }

    let mut keys: Vec<_> = summary.per_stage.keys().copied().collect();
    keys.sort();
    for stage in keys {
        let mut outcomes = summary.per_stage[&stage].clone();
        outcomes.sort_by(|a, b| a.agent.cmp(&b.agent));
        let parts = outcomes
            .iter()
            .map(|outcome| format_agent_outcome(outcome))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!("- {}: {}", stage.display(), parts));
    }

    lines
}

fn format_agent_outcome(outcome: &AgentOutcome) -> String {
    let mut chunk = format!(
        "{}({})",
        outcome.agent,
        match outcome.status {
            AgentStatus::Ok => "ok",
            AgentStatus::Conflicted => "conflict",
            AgentStatus::Error => "error",
        }
    );

    if let Some(model) = &outcome.model {
        if !model.is_empty() {
            chunk.push_str(&format!("•{}", model));
        }
    }
    if let Some(reasoning) = &outcome.reasoning_mode {
        if !reasoning.is_empty() {
            chunk.push_str(&format!("•{}", reasoning));
        }
    }
    chunk
}

fn evidence_banners(evidence: &EvidenceMetrics) -> Vec<String> {
    let mut banners = Vec::new();
    let footprint = format!(
        "Footprint {} (commands {} • consensus {})",
        format_filesize(evidence.combined_bytes),
        format_filesize(evidence.commands_bytes),
        format_filesize(evidence.consensus_bytes)
    );

    match evidence.threshold {
        Some(EvidenceThreshold::Critical) => banners.push(format!(
            "❌ {} — critical threshold {} MB exceeded",
            footprint,
            CRITICAL_FOOTPRINT_BYTES / (1024 * 1024)
        )),
        Some(EvidenceThreshold::Warning) => banners.push(format!(
            "⚠ {} — warning threshold {} MB exceeded",
            footprint,
            WARN_FOOTPRINT_BYTES / (1024 * 1024)
        )),
        None => banners.push(format!("✅ {}", footprint)),
    }

    banners
}

pub fn degraded_warning(report: &SpecStatusReport) -> Option<String> {
    if report.warnings.is_empty() {
        None
    } else {
        if report.warnings.len() == 1 {
            Some(format!("⚠ {}", report.warnings[0]))
        } else {
            Some(format!(
                "⚠ {} issues detected (see Warnings section)",
                report.warnings.len()
            ))
        }
    }
}

fn collect_packet_status(repo_root: &Path, spec_id: &str) -> Result<PacketStatus> {
    let docs_root = repo_root.join(DOCS_ROOT);
    let mut entry = PacketStatus {
        directory: None,
        docs: HashMap::new(),
    };

    let target_dir = find_spec_directory(&docs_root, spec_id);
    entry.directory = target_dir
        .as_ref()
        .and_then(|dir| dir.strip_prefix(repo_root).ok())
        .map(|p| p.to_path_buf());

    let doc_map = [
        ("PRD.md", "PRD.md"),
        ("plan.md", "plan.md"),
        ("spec.md", "spec.md"),
        ("tasks.md", "tasks.md"),
    ];

    for (label, filename) in doc_map {
        let present = target_dir
            .as_ref()
            .map(|dir| dir.join(filename).exists())
            .unwrap_or(false);
        entry.docs.insert(label, present);
    }

    Ok(entry)
}

fn find_spec_directory(root: &Path, spec_id: &str) -> Option<PathBuf> {
    if !root.exists() {
        return None;
    }

    let spec_prefix = spec_id.to_ascii_uppercase();
    for entry in root.read_dir().ok()?.flatten() {
        if entry.file_type().ok()?.is_dir() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(&spec_prefix) {
                return Some(entry.path());
            }
        }
    }
    None
}

fn read_tracker_row(repo_root: &Path, spec_id: &str) -> Result<Option<TrackerRow>> {
    let tracker = repo_root.join("SPEC.md");
    if !tracker.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(tracker).context("reading SPEC.md")?;
    let row = content
        .lines()
        .find(|line| line.contains(spec_id))
        .and_then(parse_tracker_row);

    Ok(row)
}

fn parse_tracker_row(line: &str) -> Option<TrackerRow> {
    let trimmed = line.trim();
    if trimmed.is_empty() || !trimmed.starts_with('|') {
        return None;
    }

    let cells: Vec<String> = trimmed
        .split('|')
        .map(|cell| cell.trim())
        .filter(|cell| !cell.is_empty())
        .map(String::from)
        .collect();

    if cells.len() < 4 {
        return None;
    }

    Some(TrackerRow {
        raw: trimmed.to_string(),
        order: cells.get(0).cloned(),
        task_id: cells.get(1).cloned(),
        title: cells.get(2).cloned(),
        status: cells.get(3).cloned(),
        owners: cells.get(4).cloned(),
        prd: cells.get(5).cloned(),
        branch: cells.get(6).cloned(),
        pr: cells.get(7).cloned(),
        last_validation: cells.get(8).cloned(),
        evidence: cells.get(9).cloned(),
        notes: cells.get(10).cloned(),
    })
}

fn collect_stage_snapshot(
    repo_root: &Path,
    spec_id: &str,
    stage: StageKind,
    stale_cutoff: Duration,
) -> Result<StageSnapshot> {
    let guardrail = find_latest_guardrail(repo_root, spec_id, stage)?;
    let consensus = collect_consensus(repo_root, spec_id, stage)?;

    let mut notes = Vec::new();
    let mut cue = StageCue::Pending;
    let mut is_stale = false;

    if let Some(record) = &guardrail {
        cue = StageCue::Pass;
        if let Some(status) = &record.policy_final_status {
            if !status.eq_ignore_ascii_case("passed") {
                cue = StageCue::Warn;
                notes.push(format!("policy final status: {}", status));
            }
        }
        if let Some(status) = &record.baseline_status {
            if !status.eq_ignore_ascii_case("passed") {
                cue = StageCue::Warn;
                notes.push(format!("baseline status: {}", status));
            }
        }
        if let Some(status) = &record.tool_status {
            if !status.eq_ignore_ascii_case("passed") && !status.eq_ignore_ascii_case("ok") {
                cue = StageCue::Warn;
                notes.push(format!("tool status: {}", status));
            }
        }
        if let Some(status) = &record.lock_status {
            if !status.eq_ignore_ascii_case("passed") && !status.eq_ignore_ascii_case("ok") {
                cue = StageCue::Warn;
                notes.push(format!("lock status: {}", status));
            }
        }
        if let Some(status) = &record.hook_status {
            if !status.eq_ignore_ascii_case("passed") && !status.eq_ignore_ascii_case("ok") {
                cue = StageCue::Warn;
                notes.push(format!("hook status: {}", status));
            }
        }
        if let Some(status) = &record.hal_status {
            if status.eq_ignore_ascii_case("failed") {
                cue = StageCue::Warn;
                notes.push("HAL status: failed".into());
            } else if status.eq_ignore_ascii_case("skipped") {
                notes.push("HAL status: skipped".into());
            }
        }

        let age = Local::now().with_timezone(&Utc) - record.timestamp;
        if age > stale_cutoff {
            is_stale = true;
            cue = StageCue::Warn;
        }

        for scenario in &record.scenarios {
            if !scenario.status.eq_ignore_ascii_case("passed") {
                notes.push(format!(
                    "scenario {} => {}",
                    scenario.name,
                    scenario.status
                ));
            }
        }
    }

    if consensus.disagreement {
        cue = StageCue::Warn;
    }

    for agent in &consensus.agents {
        match agent.status {
            AgentStatus::Conflicted => notes.push(format!(
                "agent {} reported conflicts",
                agent.agent
            )),
            AgentStatus::Error => {
                let detail = if agent.notes.is_empty() {
                    String::from("error")
                } else {
                    agent.notes.join("; ")
                };
                notes.push(format!("agent {} error: {}", agent.agent, detail));
            }
            AgentStatus::Ok => {}
        }
    }

    if guardrail.is_none() && consensus.agents.is_empty() {
        cue = StageCue::Pending;
    }

    Ok(StageSnapshot {
        stage,
        guardrail,
        consensus,
        cue,
        is_stale,
        notes,
    })
}

fn find_latest_guardrail(
    repo_root: &Path,
    spec_id: &str,
    stage: StageKind,
) -> Result<Option<GuardrailRecord>> {
    let stage_slug = format!("spec-{}", stage.slug());
    let root = repo_root.join(COMMAND_EVIDENCE_ROOT).join(spec_id);
    if !root.exists() {
        return Ok(None);
    }

    let mut latest_path: Option<PathBuf> = None;
    let mut latest_time: SystemTime = SystemTime::UNIX_EPOCH;

    for entry in WalkDir::new(&root).max_depth(1).into_iter().flatten() {
        if entry.path().extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let filename = entry.file_name().to_string_lossy();
        if !filename.starts_with(&stage_slug) {
            continue;
        }
        let metadata = entry.metadata()?;
        let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        if modified >= latest_time {
            latest_time = modified;
            latest_path = Some(entry.path().to_path_buf());
        }
    }

    if let Some(path) = latest_path {
        let record = parse_guardrail_record(&path)?;
        return Ok(Some(record));
    }

    Ok(None)
}

fn parse_guardrail_record(path: &Path) -> Result<GuardrailRecord> {
    let mut file = fs::File::open(path)
        .with_context(|| format!("opening guardrail telemetry {}", path.display()))?;
    let metadata = file.metadata().ok();
    let mut raw = String::new();
    file.read_to_string(&mut raw)?;
    let sanitized = sanitize_json_notes(&raw);
    let value: GuardrailJson = serde_json::from_str(&sanitized)
        .with_context(|| format!("parsing guardrail telemetry {}", path.display()))?;

    let timestamp = value
        .timestamp
        .as_deref()
        .and_then(|ts| DateTime::parse_from_rfc3339(ts).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .or_else(|| {
            metadata
                .and_then(|m| m.modified().ok())
                .map(DateTime::<Utc>::from)
        })
        .ok_or_else(|| anyhow!("telemetry missing timestamp: {}", path.display()))?;

    let hal_status = value.hal.as_ref().and_then(|hal| hal.status.clone());
    let hal_failed_checks = value
        .hal
        .as_ref()
        .and_then(|hal| hal.failed_checks.clone())
        .unwrap_or_default();

    let scenarios = value
        .scenarios
        .unwrap_or_default()
        .into_iter()
        .filter_map(|s| match (s.name, s.status) {
            (Some(name), Some(status)) => Some(ScenarioStatus { name, status }),
            _ => None,
        })
        .collect();

    let schema_version = value
        .schema_version
        .map(|v| v.into_string())
        .unwrap_or_else(|| "1".into());

    Ok(GuardrailRecord {
        schema_version,
        timestamp,
        baseline_status: value.baseline.and_then(|b| b.status),
        tool_status: value.tool.and_then(|t| t.status),
        lock_status: value.lock_status,
        hook_status: value.hook_status,
        policy_prefilter_status: value
            .policy
            .as_ref()
            .and_then(|p| p.prefilter.as_ref())
            .and_then(|p| p.status.clone()),
        policy_final_status: value
            .policy
            .as_ref()
            .and_then(|p| p.final_status.as_ref())
            .and_then(|p| p.status.clone()),
        hal_status,
        hal_failed_checks,
        scenarios,
    })
}

fn collect_consensus(repo_root: &Path, spec_id: &str, stage: StageKind) -> Result<StageConsensus> {
    let root = repo_root.join(CONSENSUS_EVIDENCE_ROOT).join(spec_id);
    if !root.exists() {
        return Ok(StageConsensus::default());
    }

    let mut per_agent: HashMap<String, (AgentOutcome, SystemTime)> = HashMap::new();
    let mut synthesis_status: Option<String> = None;

    for entry in WalkDir::new(&root).max_depth(1).into_iter().flatten() {
        if entry.path().extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let filename = entry.file_name().to_string_lossy();
        if !filename.starts_with(&format!("spec-{}", stage.slug())) {
            continue;
        }

        let metadata = entry.metadata()?;
        let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);

        let data: ConsensusJson = serde_json::from_reader(fs::File::open(entry.path())?)
            .with_context(|| format!("parsing consensus file {}", entry.path().display()))?;

        if let Some(consensus) = data.consensus.as_ref() {
            if synthesis_status.is_none() {
                synthesis_status = consensus.synthesis_status.clone();
            }
        }

        let agent_name = data
            .agent
            .unwrap_or_else(|| infer_agent_from_filename(&filename));
        let status = if let Some(_err) = data.error.as_ref() {
            AgentStatus::Error
        } else if let Some(consensus) = data.consensus.as_ref() {
            if let Some(conflicts) = consensus.conflicts.as_ref() {
                if !conflicts.is_empty() {
                    AgentStatus::Conflicted
                } else {
                    AgentStatus::Ok
                }
            } else {
                AgentStatus::Ok
            }
        } else {
            AgentStatus::Ok
        };

        let mut notes = Vec::new();
        if let Some(consensus) = data.consensus.as_ref() {
            if let Some(conflicts) = consensus.conflicts.as_ref() {
                for conflict in conflicts {
                    notes.push(conflict.clone());
                }
            }
        }
        if let Some(err) = data.error.as_ref() {
            notes.push(err.clone());
        }

        let outcome = AgentOutcome {
            agent: agent_name.clone(),
            model: data.model,
            reasoning_mode: data.reasoning_mode,
            status,
            notes,
            timestamp: metadata.modified().ok().map(DateTime::<Utc>::from),
        };

        let update = match per_agent.get(&agent_name) {
            Some((_existing, existing_time)) => modified >= *existing_time,
            None => true,
        };

        if update {
            per_agent.insert(agent_name, (outcome, modified));
        }
    }

    let mut agents: Vec<_> = per_agent
        .into_iter()
        .map(|(_, (outcome, _))| outcome)
        .collect();
    agents.sort_by_key(|o| o.agent.clone());

    let disagreement = agents.iter().any(|agent| agent.status != AgentStatus::Ok);
    let latest_timestamp = agents.iter().filter_map(|a| a.timestamp).max();

    Ok(StageConsensus {
        agents,
        synthesis_status,
        disagreement,
        latest_timestamp,
    })
}

fn infer_agent_from_filename(filename: &str) -> String {
    filename
        .split('_')
        .last()
        .and_then(|part| part.strip_suffix(".json"))
        .unwrap_or("unknown")
        .to_string()
}

fn compute_evidence_metrics(repo_root: &Path, spec_id: &str) -> Result<EvidenceMetrics> {
    let commands_root = repo_root.join(COMMAND_EVIDENCE_ROOT).join(spec_id);
    let consensus_root = repo_root.join(CONSENSUS_EVIDENCE_ROOT).join(spec_id);

    let commands_bytes = compute_directory_size(&commands_root)?;
    let consensus_bytes = compute_directory_size(&consensus_root)?;
    let combined_bytes = commands_bytes + consensus_bytes;

    let threshold = if combined_bytes >= CRITICAL_FOOTPRINT_BYTES {
        Some(EvidenceThreshold::Critical)
    } else if combined_bytes >= WARN_FOOTPRINT_BYTES {
        Some(EvidenceThreshold::Warning)
    } else {
        None
    };

    let latest_artifact = latest_timestamp(&[commands_root.clone(), consensus_root.clone()]);

    let mut entries = Vec::new();
    collect_top_evidence_entries(repo_root, &commands_root, &mut entries)?;
    collect_top_evidence_entries(repo_root, &consensus_root, &mut entries)?;

    entries.sort_by_key(|e| Reverse(e.bytes));
    entries.truncate(MAX_TOP_EVIDENCE_ENTRIES);

    Ok(EvidenceMetrics {
        commands_bytes,
        consensus_bytes,
        combined_bytes,
        latest_artifact,
        threshold,
        top_entries: entries,
    })
}

fn collect_top_evidence_entries(
    repo_root: &Path,
    root: &Path,
    entries: &mut Vec<EvidenceEntry>,
) -> Result<()> {
    if !root.exists() {
        return Ok(());
    }

    for entry in WalkDir::new(root).into_iter().flatten() {
        if entry.file_type().is_file() {
            let bytes = entry.metadata()?.len();
            if bytes == 0 {
                continue;
            }
            if let Ok(rel) = entry.path().strip_prefix(repo_root) {
                entries.push(EvidenceEntry {
                    path: rel.to_string_lossy().replace("\\", "/"),
                    bytes,
                });
            }
        }
    }

    Ok(())
}

fn compute_directory_size(root: &Path) -> Result<u64> {
    if !root.exists() {
        return Ok(0);
    }
    let mut total = 0u64;
    for entry in WalkDir::new(root).into_iter().flatten() {
        if entry.file_type().is_file() {
            total += entry.metadata()?.len();
        }
    }
    Ok(total)
}

fn latest_timestamp(paths: &[PathBuf]) -> Option<DateTime<Utc>> {
    paths
        .iter()
        .filter(|p| p.exists())
        .flat_map(|root| WalkDir::new(root).into_iter().flatten())
        .filter_map(|entry| entry.metadata().ok()?.modified().ok())
        .map(DateTime::<Utc>::from)
        .max()
}

fn sanitize_json_notes(raw: &str) -> String {
    let mut output = String::with_capacity(raw.len());
    for line in raw.lines() {
        if line.contains("\"note\"") {
            if let Some(colon_idx) = line.find(':') {
                let after_colon = &line[colon_idx + 1..];
                if let Some(first_quote_offset) = after_colon.find('"') {
                    let value_start = colon_idx + 1 + first_quote_offset;
                    let remaining = &line[value_start + 1..];
                    if let Some(last_quote_rel) = remaining.rfind('"') {
                        let value_end = value_start + 1 + last_quote_rel;
                        let value = &line[value_start + 1..value_end];
                        let escaped = value.replace("\\", "\\\\").replace("\"", "\\\"");
                        let mut sanitized = String::new();
                        sanitized.push_str(&line[..value_start + 1]);
                        sanitized.push_str(&escaped);
                        sanitized.push_str(&line[value_end..]);
                        output.push_str(&sanitized);
                        output.push('\n');
                        continue;
                    }
                }
            }
        }
        output.push_str(line);
        output.push('\n');
    }
    output
}

#[derive(Debug, Deserialize)]
struct GuardrailJson {
    #[serde(rename = "schemaVersion")]
    pub schema_version: Option<StringOrInt>,
    pub timestamp: Option<String>,
    pub baseline: Option<BaselineJson>,
    pub tool: Option<ToolJson>,
    #[serde(default)]
    pub policy: Option<PolicyJson>,
    pub hal: Option<HalJson>,
    pub lock_status: Option<String>,
    pub hook_status: Option<String>,
    pub scenarios: Option<Vec<ScenarioJson>>,
}

#[derive(Debug, Deserialize)]
struct ConsensusJson {
    pub agent: Option<String>,
    pub model: Option<String>,
    pub reasoning_mode: Option<String>,
    pub error: Option<String>,
    pub consensus: Option<ConsensusDetailJson>,
}

#[derive(Debug, Deserialize)]
struct ConsensusDetailJson {
    pub conflicts: Option<Vec<String>>,
    pub synthesis_status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BaselineJson {
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ToolJson {
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PolicyJson {
    #[serde(rename = "prefilter")]
    pub prefilter: Option<PolicyStatusJson>,
    #[serde(rename = "final")]
    pub final_status: Option<PolicyStatusJson>,
}

#[derive(Debug, Deserialize)]
struct PolicyStatusJson {
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct HalJson {
    pub status: Option<String>,
    #[serde(rename = "failed_checks")]
    pub failed_checks: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct ScenarioJson {
    pub name: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum StringOrInt {
    String(String),
    Int(i64),
}

impl Default for StringOrInt {
    fn default() -> Self {
        StringOrInt::String("1".into())
    }
}

impl StringOrInt {
    fn into_string(self) -> String {
        match self {
            StringOrInt::String(s) => s,
            StringOrInt::Int(i) => i.to_string(),
        }
    }

}

impl GuardrailRecord {
    pub fn has_failures(&self) -> bool {
        let statuses = [
            self.baseline_status.as_deref(),
            self.tool_status.as_deref(),
            self.policy_prefilter_status.as_deref(),
            self.policy_final_status.as_deref(),
            self.lock_status.as_deref(),
            self.hook_status.as_deref(),
            self.hal_status.as_deref(),
        ];
        statuses.iter().any(|status| match status {
            Some(value) => {
                !value.eq_ignore_ascii_case("passed")
                    && !value.eq_ignore_ascii_case("ok")
                    && !value.eq_ignore_ascii_case("skipped")
            }
            None => false,
        })
    }
}

fn format_datetime_utc(dt: &DateTime<Utc>) -> String {
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}

fn format_filesize(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".into();
    }
    let mut value = bytes as f64;
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut unit_index = 0;
    while value >= 1024.0 && unit_index < units.len() - 1 {
        value /= 1024.0;
        unit_index += 1;
    }
    format!("{:.1} {}", value, units[unit_index])
}
