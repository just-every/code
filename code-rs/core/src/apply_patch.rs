use anyhow::Context as _;
use anyhow::Result;
use crate::acp::AcpFileSystem;
use crate::codex::Session;
use crate::patch_harness::run_patch_harness;
use crate::protocol::FileChange;
use crate::protocol::ReviewDecision;
use crate::safety::assess_patch_safety;
use crate::safety::SafetyCheck;
use code_apply_patch::AffectedPaths;
use code_apply_patch::ApplyPatchAction;
use code_apply_patch::ApplyPatchFileChange;
use code_apply_patch::FileSystem;
use code_apply_patch::StdFileSystem;
use code_apply_patch::print_summary;
use code_protocol::models::FunctionCallOutputPayload;
use code_protocol::models::ResponseInputItem;
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;

pub const CODEX_APPLY_PATCH_ARG1: &str = "--codex-run-as-apply-patch";

pub(crate) struct ApplyPatchRun {
    pub auto_approved: bool,
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
    pub harness_summary_json: Option<String>,
}

pub(crate) enum ApplyPatchResult {
    Applied(ApplyPatchRun),
    Reply(ResponseInputItem),
}

pub(crate) async fn apply_patch(
    sess: &Session,
    sub_id: &str,
    call_id: &str,
    attempt_req: u64,
    output_index: Option<u32>,
    action: ApplyPatchAction,
) -> ApplyPatchResult {
    let (harness_summary_json, harness_status_message) = {
        let mut summary_json: Option<String> = None;
        let mut status_message: Option<String> = None;
        let validation_cfg = sess.validation_config();
        let github_cfg = sess.get_github_config();
        if let (Ok(validation_cfg), Ok(github_cfg)) = (validation_cfg.read(), github_cfg.read()) {
            if let Some((mut findings, mut ran_checks)) = run_patch_harness(
                &action,
                sess.get_cwd(),
                &*validation_cfg,
                &*github_cfg,
            ) {
                const MAX_ISSUES: usize = 12;
                let total_issues = findings.len();
                let truncated = total_issues > MAX_ISSUES;
                if truncated {
                    findings.truncate(MAX_ISSUES);
                }
                findings.retain(|finding| {
                    finding.tool.trim().len() <= 120 && finding.message.trim().len() <= 800
                });
                let issues_json: Vec<serde_json::Value> = findings
                    .iter()
                    .map(|finding| {
                        let relative_file = finding
                            .file
                            .as_ref()
                            .and_then(|path| path.strip_prefix(sess.get_cwd()).ok())
                            .map(|path| path.display().to_string());
                        json!({
                            "tool": finding.tool,
                            "file": relative_file,
                            "msg": finding.message,
                        })
                    })
                    .collect();
                summary_json = Some(
                    json!({
                        "validation": {
                            "issues": issues_json,
                            "checks": ran_checks,
                            "issue_count": total_issues,
                            "truncated": truncated,
                        }
                    })
                    .to_string(),
                );

                let mut lines: Vec<String> = Vec::new();
                if total_issues == 0 {
                    lines.push("✅ Validate New Code: no issues".to_string());
                } else {
                    lines.push(format!("❌ Validate New Code: {total_issues} issue(s)"));
                    for finding in findings.iter() {
                        let mut parts = vec![finding.tool.clone()];
                        if let Some(rel) = finding
                            .file
                            .as_ref()
                            .and_then(|p| p.strip_prefix(sess.get_cwd()).ok())
                            .map(|p| p.display().to_string())
                        {
                            parts.push(rel);
                        }
                        let mut msg = finding.message.clone();
                        if msg.len() > 160 {
                            msg.truncate(157);
                            msg.push_str("…");
                        }
                        parts.push(msg);
                        lines.push(format!("• {}", parts.join(" — ")));
                    }
                    if truncated {
                        let remaining = total_issues - findings.len();
                        lines.push(format!("… plus {remaining} more issue(s)"));
                    }
                }
                if ran_checks.is_empty() {
                    lines.push("Checks run: none".to_string());
                } else {
                    ran_checks.sort();
                    lines.push(format!("Checks run: {}", ran_checks.join(", ")));
                }
                status_message = Some(lines.join("\n"));
            }
        }
        (summary_json, status_message)
    };

    if let Some(message) = harness_status_message.as_ref() {
        let order = sess.next_background_order(sub_id, attempt_req, output_index);
        sess
            .notify_background_event_with_order(sub_id, order, message.clone())
            .await;
    }

    let auto_approved = match assess_patch_safety(
        &action,
        sess.get_approval_policy(),
        sess.get_sandbox_policy(),
        sess.get_cwd(),
    ) {
        SafetyCheck::AutoApprove { .. } => true,
        SafetyCheck::AskUser => {
            let rx = sess
                .request_patch_approval(sub_id.to_owned(), call_id.to_owned(), &action, None, None)
                .await;
            match rx.await.unwrap_or_default() {
                ReviewDecision::Approved | ReviewDecision::ApprovedForSession => false,
                ReviewDecision::Denied | ReviewDecision::Abort => {
                    return ApplyPatchResult::Reply(ResponseInputItem::FunctionCallOutput {
                        call_id: call_id.to_owned(),
                        output: FunctionCallOutputPayload {
                            body: code_protocol::models::FunctionCallOutputBody::Text("patch rejected by user".to_string()),
                            success: Some(false)},
                    });
                }
            }
        }
        SafetyCheck::Reject { reason } => {
            return ApplyPatchResult::Reply(ResponseInputItem::FunctionCallOutput {
                call_id: call_id.to_owned(),
                output: FunctionCallOutputPayload {
                    body: code_protocol::models::FunctionCallOutputBody::Text(format!("patch rejected: {reason}")),
                    success: Some(false)},
            });
        }
    };
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let result = if let Some(client_tools) = sess.client_tools() {
        let fs = AcpFileSystem::new(sess.session_uuid(), client_tools, sess.mcp_connection_manager());
        apply_changes_from_apply_patch_and_report(&action, &mut stdout, &mut stderr, &fs).await
    } else {
        apply_changes_from_apply_patch_and_report(&action, &mut stdout, &mut stderr, &StdFileSystem).await
    };

    let stdout = String::from_utf8_lossy(&stdout).to_string();
    let stderr = String::from_utf8_lossy(&stderr).to_string();
    let success = result.is_ok();

    ApplyPatchResult::Applied(ApplyPatchRun {
        auto_approved,
        stdout,
        stderr,
        success,
        harness_summary_json,
    })
}

pub(crate) fn convert_apply_patch_to_protocol(
    action: &ApplyPatchAction,
) -> HashMap<PathBuf, FileChange> {
    let changes = action.changes();
    let mut result = HashMap::with_capacity(changes.len());
    for (path, change) in changes {
        let protocol_change = match change {
            ApplyPatchFileChange::Add { content } => FileChange::Add {
                content: content.clone(),
            },
            ApplyPatchFileChange::Delete { content: _ } => FileChange::Delete,
            ApplyPatchFileChange::Update {
                unified_diff,
                move_path,
                new_content,
            } => {
                let original_content = std::fs::read_to_string(path).unwrap_or_default();
                FileChange::Update {
                    unified_diff: unified_diff.clone(),
                    move_path: move_path.clone(),
                    original_content,
                    new_content: new_content.clone(),
                }
            }
        };
        result.insert(path.clone(), protocol_change);
    }
    result
}

pub(crate) fn get_writable_roots(cwd: &Path) -> Vec<PathBuf> {
    let mut writable_roots = Vec::new();
    if cfg!(target_os = "macos") {
        writable_roots.push(std::env::temp_dir());

        if let Ok(home_dir) = std::env::var("HOME") {
            let pyenv_dir = PathBuf::from(home_dir).join(".pyenv");
            writable_roots.push(pyenv_dir);
        }
    }

    writable_roots.push(cwd.to_path_buf());

    writable_roots
}

async fn apply_changes_from_apply_patch_and_report(
    action: &ApplyPatchAction,
    stdout: &mut impl std::io::Write,
    stderr: &mut impl std::io::Write,
    fs: &impl FileSystem,
) -> std::io::Result<()> {
    match apply_changes_from_apply_patch(action, fs).await {
        Ok(affected_paths) => {
            print_summary(&affected_paths, stdout)?;
        }
        Err(err) => {
            writeln!(stderr, "{err:#}")?;
        }
    }

    Ok(())
}

async fn apply_changes_from_apply_patch(
    action: &ApplyPatchAction,
    fs: &impl FileSystem,
) -> Result<AffectedPaths> {
    let mut added: Vec<PathBuf> = Vec::new();
    let mut modified: Vec<PathBuf> = Vec::new();
    let mut deleted: Vec<PathBuf> = Vec::new();

    for (path, change) in action.changes() {
        match change {
            ApplyPatchFileChange::Add { content } => {
                if let Some(parent) = path.parent() {
                    if !parent.as_os_str().is_empty() {
                        std::fs::create_dir_all(parent).with_context(|| {
                            format!("Failed to create parent directories for {}", path.display())
                        })?;
                    }
                }
                fs.write_text_file(path, content.clone())
                    .await
                    .with_context(|| format!("Failed to write file {}", path.display()))?;
                added.push(path.clone());
            }
            ApplyPatchFileChange::Delete { content: _ } => {
                std::fs::remove_file(path)
                    .with_context(|| format!("Failed to delete file {}", path.display()))?;
                deleted.push(path.clone());
            }
            ApplyPatchFileChange::Update {
                move_path,
                new_content,
                ..
            } => {
                if let Some(move_path) = move_path {
                    if let Some(parent) = move_path.parent() {
                        if !parent.as_os_str().is_empty() {
                            std::fs::create_dir_all(parent).with_context(|| {
                                format!("Failed to create parent directories for {}", move_path.display())
                            })?;
                        }
                    }

                    std::fs::rename(path, move_path)
                        .with_context(|| format!("Failed to rename file {}", path.display()))?;
                    fs.write_text_file(move_path, new_content.clone()).await?;
                    modified.push(move_path.clone());
                    deleted.push(path.clone());
                } else {
                    fs.write_text_file(path, new_content.clone()).await?;
                    modified.push(path.clone());
                }
            }
        }
    }

    Ok(AffectedPaths {
        added,
        modified,
        deleted,
    })
}
