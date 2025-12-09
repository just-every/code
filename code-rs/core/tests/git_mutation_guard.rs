//! Guardrail to catch new direct `git` invocations in TUI/auto-drive without epoch bumping.
//!
//! This is intentionally simple: it scans source files for `Command::new("git")` and
//! fails if they appear outside a small allow-list of files that already route through
//! epoch-bumping helpers. When you add a new mutating git call, either use the shared
//! helper that bumps `snapshot epoch` or extend the allow-list with a justification.

use std::fs;

const TARGET_DIRS: &[&str] = &[
    "code-rs/tui/src",
    "code-rs/code-auto-drive-core/src",
];

// Files allowed to host direct git invocations. They must also demonstrate an
// epoch bump reference (either direct `bump_snapshot_epoch` or a documented helper).
const ALLOW_LIST: &[(&str, &[&str])] = &[
    // chatwidget uses run_git_command (bumps epoch) and ghost snapshots with post_commit_hook.
    ("code-rs/tui/src/chatwidget.rs", &["bump_snapshot_epoch", "run_git_command"]),
    // get_git_diff is read-only; no epoch bump required, still allowed.
    ("code-rs/tui/src/get_git_diff.rs", &[]),
    // auto_coordinator bumps epoch on mutating verbs inside run_git_command.
    (
        "code-rs/code-auto-drive-core/src/auto_coordinator.rs",
        &["bump_snapshot_epoch", "run_git_command"],
    ),
    // controller currently no direct git calls; kept to avoid churn, require no markers.
    ("code-rs/code-auto-drive-core/src/controller.rs", &[]),
];

#[test]
fn new_git_invocations_must_use_epoch_helpers() {
    let mut offenders = Vec::new();

    for dir in TARGET_DIRS {
        let walker = walkdir::WalkDir::new(dir).into_iter();
        for entry in walker.filter_map(Result::ok).filter(|e| e.file_type().is_file()) {
            let path = entry.path();
            let path_str = path.to_string_lossy();
            if !path_str.ends_with(".rs") {
                continue;
            }
            let contents = match fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            if !contents.contains("Command::new(\"git\")") {
                continue;
            }
            if let Some((_, markers)) = ALLOW_LIST
                .iter()
                .find(|(p, _)| path_str.ends_with(p))
            {
                // If the allowlisted file performs mutations, enforce presence of epoch markers.
                if !markers.is_empty()
                    && markers.iter().all(|m| !contents.contains(m))
                {
                    offenders.push(format!(
                        "{} (allowlisted but missing epoch marker(s): {:?})",
                        path_str, markers
                    ));
                }
                continue;
            }
            offenders.push(path_str.to_string());
        }
    }

    if !offenders.is_empty() {
        panic!(
            "Found new direct git invocations without epoch bumping: {:?}. Use the shared helpers or add an allow-list entry with justification.",
            offenders
        );
    }
}
