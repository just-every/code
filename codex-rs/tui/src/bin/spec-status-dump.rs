use std::env;

use anyhow::{Context, Result, anyhow};
use chrono::Local;
use codex_tui::spec_status::{
    SpecStatusArgs, StageCue, collect_report, degraded_warning, render_dashboard,
};
use serde::Serialize;

fn main() -> Result<()> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        return Err(anyhow!("Usage: spec-status-dump <SPEC-ID> [--stale-hours <n>]").into());
    }

    let repo_root = env::current_dir().context("determining current directory")?;
    let arg_string = args.join(" ");
    let spec_args = SpecStatusArgs::from_input(&arg_string)?;
    let report = collect_report(&repo_root, spec_args)?;

    let payload = ReportPayload::from_report(&report);
    serde_json::to_writer_pretty(std::io::stdout(), &payload).context("writing JSON")?;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReportPayload {
    schema_version: &'static str,
    spec_id: String,
    generated_at: String,
    stale_cutoff_hours: i64,
    degraded: bool,
    warnings: Vec<String>,
    render: RenderBlock,
    tracker: Option<TrackerPayload>,
    stages: Vec<StagePayload>,
    evidence: EvidencePayload,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RenderBlock {
    markdown: Vec<String>,
    degraded_warning: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TrackerPayload {
    raw: String,
    status: Option<String>,
    owners: Option<String>,
    branch: Option<String>,
    last_validation: Option<String>,
    notes: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StagePayload {
    stage: String,
    cue: String,
    cue_icon: String,
    updated_at: Option<String>,
    stale: bool,
    consensus: ConsensusPayload,
    guardrail: Option<GuardrailPayload>,
    notes: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConsensusPayload {
    status: Option<String>,
    disagreement: bool,
    agents: Vec<AgentPayload>,
    latest_timestamp: Option<String>,
    synthesis_status: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentPayload {
    agent: String,
    status: String,
    model: Option<String>,
    reasoning_mode: Option<String>,
    timestamp: Option<String>,
    notes: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GuardrailPayload {
    schema_version: String,
    updated_at: String,
    baseline_status: Option<String>,
    tool_status: Option<String>,
    policy_prefilter_status: Option<String>,
    policy_final_status: Option<String>,
    hal_status: Option<String>,
    hal_failed_checks: Vec<String>,
    lock_status: Option<String>,
    hook_status: Option<String>,
    scenarios: Vec<ScenarioPayload>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ScenarioPayload {
    name: String,
    status: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EvidencePayload {
    footprint_bytes: u64,
    commands_bytes: u64,
    consensus_bytes: u64,
    latest_artifact: Option<String>,
    threshold: Option<String>,
    top_entries: Vec<EvidenceEntryPayload>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EvidenceEntryPayload {
    path: String,
    bytes: u64,
}

impl ReportPayload {
    fn from_report(report: &codex_tui::spec_status::SpecStatusReport) -> Self {
        let markdown = render_dashboard(report);
        let degraded_flag = !report.warnings.is_empty();
        let degraded_text = degraded_warning(report);

        let tracker = report.tracker_row.as_ref().map(|row| TrackerPayload {
            raw: row.raw.clone(),
            status: row.status.clone(),
            owners: row.owners.clone(),
            branch: row.branch.clone(),
            last_validation: row.last_validation.clone(),
            notes: row.notes.clone(),
        });

        let stages = report
            .stage_snapshots
            .iter()
            .map(|snapshot| {
                let updated_at = snapshot
                    .guardrail
                    .as_ref()
                    .map(|g| g.timestamp.to_rfc3339())
                    .or_else(|| {
                        snapshot
                            .consensus
                            .latest_timestamp
                            .map(|ts| ts.to_rfc3339())
                    });

                let consensus_status = if snapshot.consensus.disagreement {
                    Some("conflict".to_string())
                } else if snapshot.consensus.agents.is_empty() {
                    None
                } else {
                    Some("ok".to_string())
                };

                let mut notes = snapshot.notes.clone();
                if snapshot.is_stale {
                    notes.push(format!("stale>{}h", report.stale_cutoff.num_hours()));
                }
                if snapshot.consensus.agents.is_empty() {
                    notes.push("consensus missing".into());
                }
                if snapshot.consensus.disagreement {
                    notes.push("consensus conflict".into());
                }
                if snapshot.guardrail.is_none() {
                    notes.push("no guardrail telemetry".into());
                }

                StagePayload {
                    stage: snapshot.stage.slug().to_string(),
                    cue: stage_cue(snapshot.cue).into(),
                    cue_icon: snapshot.cue.icon().into(),
                    updated_at,
                    stale: snapshot.is_stale,
                    consensus: ConsensusPayload {
                        status: consensus_status,
                        disagreement: snapshot.consensus.disagreement,
                        agents: snapshot
                            .consensus
                            .agents
                            .iter()
                            .map(|agent| AgentPayload {
                                agent: agent.agent.clone(),
                                status: match agent.status {
                                    codex_tui::spec_status::AgentStatus::Ok => "ok".into(),
                                    codex_tui::spec_status::AgentStatus::Conflicted => {
                                        "conflict".into()
                                    }
                                    codex_tui::spec_status::AgentStatus::Error => "error".into(),
                                },
                                model: agent.model.clone(),
                                reasoning_mode: agent.reasoning_mode.clone(),
                                timestamp: agent.timestamp.map(|dt| dt.to_rfc3339()),
                                notes: agent.notes.clone(),
                            })
                            .collect(),
                        latest_timestamp: snapshot
                            .consensus
                            .latest_timestamp
                            .map(|ts| ts.to_rfc3339()),
                        synthesis_status: snapshot.consensus.synthesis_status.clone(),
                    },
                    guardrail: snapshot
                        .guardrail
                        .as_ref()
                        .map(|guardrail| GuardrailPayload {
                            schema_version: guardrail.schema_version.clone(),
                            updated_at: guardrail.timestamp.to_rfc3339(),
                            baseline_status: guardrail.baseline_status.clone(),
                            tool_status: guardrail.tool_status.clone(),
                            policy_prefilter_status: guardrail.policy_prefilter_status.clone(),
                            policy_final_status: guardrail.policy_final_status.clone(),
                            hal_status: guardrail.hal_status.clone(),
                            hal_failed_checks: guardrail.hal_failed_checks.clone(),
                            lock_status: guardrail.lock_status.clone(),
                            hook_status: guardrail.hook_status.clone(),
                            scenarios: guardrail
                                .scenarios
                                .iter()
                                .filter_map(|scenario| {
                                    if scenario.name.is_empty() || scenario.status.is_empty() {
                                        None
                                    } else {
                                        Some(ScenarioPayload {
                                            name: scenario.name.clone(),
                                            status: scenario.status.clone(),
                                        })
                                    }
                                })
                                .collect(),
                        }),
                    notes,
                }
            })
            .collect();

        let evidence = EvidencePayload {
            footprint_bytes: report.evidence.combined_bytes,
            commands_bytes: report.evidence.commands_bytes,
            consensus_bytes: report.evidence.consensus_bytes,
            latest_artifact: report.evidence.latest_artifact.map(|dt| dt.to_rfc3339()),
            threshold: report.evidence.threshold.map(|t| match t {
                codex_tui::spec_status::EvidenceThreshold::Warning => "warning".into(),
                codex_tui::spec_status::EvidenceThreshold::Critical => "critical".into(),
            }),
            top_entries: report
                .evidence
                .top_entries
                .iter()
                .map(|entry| EvidenceEntryPayload {
                    path: entry.path.clone(),
                    bytes: entry.bytes,
                })
                .collect(),
        };

        ReportPayload {
            schema_version: "1.1",
            spec_id: report.spec_id.clone(),
            generated_at: Local::now().to_rfc3339(),
            stale_cutoff_hours: report.stale_cutoff.num_hours(),
            degraded: degraded_flag,
            warnings: report.warnings.clone(),
            render: RenderBlock {
                markdown,
                degraded_warning: degraded_text,
            },
            tracker,
            stages,
            evidence,
        }
    }
}

fn stage_cue(cue: StageCue) -> &'static str {
    match cue {
        StageCue::Pass => "pass",
        StageCue::Warn => "warn",
        StageCue::Pending => "pending",
    }
}
