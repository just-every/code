use crate::app_event_sender::AppEventSender;
use codex_core::config::Config;
use codex_core::git_info::collect_git_info;
use std::process::Command;

/// Source of a GitHub API token used by the watcher.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TokenSource {
    /// `GITHUB_TOKEN` or `GH_TOKEN` environment variable.
    Env,
    /// Fetched via `gh auth token` from the GitHub CLI.
    GhCli,
}

/// Obtain a GitHub token, preferring environment variables and falling back to `gh`.
/// Returns the token string and its source if available.
pub(super) fn get_github_token() -> Option<(String, TokenSource)> {
    if let Ok(t) = std::env::var("GITHUB_TOKEN") {
        if !t.is_empty() {
            return Some((t, TokenSource::Env));
        }
    }
    if let Ok(t) = std::env::var("GH_TOKEN") {
        if !t.is_empty() {
            return Some((t, TokenSource::Env));
        }
    }
    // Fallback: use GitHub CLI if installed and logged in.
    if let Ok(out) = Command::new("gh").args(["auth", "token"]).output() {
        if out.status.success() {
            let token = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !token.is_empty() {
                return Some((token, TokenSource::GhCli));
            }
        }
    }
    None
}

/// Start a background task to watch GitHub Actions for the latest push and
/// surface a failure message if any run for the pushed commit completes with a
/// non-success conclusion.
pub(super) fn maybe_watch_after_push(
    app_event_tx: AppEventSender,
    config: Config,
    command: &[String],
) {
    // Only proceed when enabled in config and the command appears to be a git push.
    if !config.github.check_workflows_on_push {
        return;
    }
    if command.is_empty() {
        return;
    }
    if !command[0].eq_ignore_ascii_case("git") {
        return;
    }
    let is_push = command
        .iter()
        .skip(1)
        .any(|c| c.eq_ignore_ascii_case("push"));
    if !is_push {
        return;
    }

    // Spawn a detached task so we don't block UI; clone what we need.
    let tx = app_event_tx.clone();
    tokio::spawn(async move {
        // Gather repo info (branch, sha, origin URL) from the current cwd.
        let Some(git) = collect_git_info(&config.cwd).await else {
            return;
        };
        let (Some(head_sha), branch_opt, Some(repo_url)) =
            (git.commit_hash, git.branch, git.repository_url)
        else {
            return;
        };
        let Some((owner, repo)) = parse_owner_repo(&repo_url) else {
            return;
        };

        // Build API endpoint and client (optionally with token if present).
        let api_base = format!("https://api.github.com/repos/{owner}/{repo}/actions/runs");
        let token = get_github_token().map(|(t, _)| t);
        let client = reqwest::Client::builder()
            .user_agent("codex-cli-rs/github-monitor")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        // Two-phase polling:
        // 1) Appearance window: up to 2 minutes to see a workflow run start
        // 2) Completion window: once found, poll that single run for up to 60 minutes
        let mut found_run_id: Option<u64> = None;
        let mut attempt: u32 = 0;
        let branch = branch_opt.unwrap_or_default();
        const APPEAR_POLL_INTERVAL_MS: u64 = 5_000; // 5s
        const APPEAR_TIMEOUT_MS: u64 = 120_000; // 2m
        const RUN_POLL_INTERVAL_MS: u64 = 10_000; // 10s
        const RUN_TIMEOUT_MS: u64 = 60 * 60 * 1_000; // 60m
        let appear_attempts_max = (APPEAR_TIMEOUT_MS / APPEAR_POLL_INTERVAL_MS) as u32; // 24
        while attempt < appear_attempts_max {
            attempt += 1;
            // Query latest runs for this branch and event=push; filter head_sha client-side.
            let url = format!(
                "{api_base}?per_page=20&event=push&branch={}",
                urlencoding::encode(&branch)
            );
            let mut req = client.get(&url);
            if let Some(ref t) = token {
                req = req.bearer_auth(t);
            }

            let resp = match req.send().await {
                Ok(r) => r,
                Err(_) => {
                    sleep_ms(APPEAR_POLL_INTERVAL_MS).await;
                    continue;
                }
            };
            if !resp.status().is_success() {
                sleep_ms(APPEAR_POLL_INTERVAL_MS).await;
                continue;
            }
            let body = match resp.json::<serde_json::Value>().await {
                Ok(v) => v,
                Err(_) => {
                    sleep_ms(APPEAR_POLL_INTERVAL_MS).await;
                    continue;
                }
            };
            let runs = body
                .get("workflow_runs")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            // Look for the run corresponding to this commit.
            for run in runs {
                let run_sha = run.get("head_sha").and_then(|v| v.as_str()).unwrap_or("");
                if run_sha.eq_ignore_ascii_case(&head_sha) {
                    found_run_id = run.get("id").and_then(|v| v.as_u64());
                    // If it is already completed, check outcome now; otherwise continue polling below.
                    if let Some(status) = run.get("status").and_then(|v| v.as_str()) {
                        if status == "completed" {
                            let conclusion = run
                                .get("conclusion")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown");
                            if conclusion != "success" {
                                let html =
                                    run.get("html_url").and_then(|v| v.as_str()).unwrap_or("");
                                surface_failure(
                                    &tx, &owner, &repo, &branch, &head_sha, html, conclusion,
                                );
                            }
                            return;
                        }
                    }
                }
            }

            // If we found a run but it's not completed yet, poll the single run until it completes
            if let Some(run_id) = found_run_id {
                let run_url = format!("{api_base}/{run_id}");
                let mut elapsed_ms: u64 = 0;
                loop {
                    let mut req = client.get(&run_url);
                    if let Some(ref t) = token {
                        req = req.bearer_auth(t);
                    }
                    let resp = match req.send().await {
                        Ok(r) => r,
                        Err(_) => {
                            sleep_ms(RUN_POLL_INTERVAL_MS).await;
                            elapsed_ms = elapsed_ms.saturating_add(RUN_POLL_INTERVAL_MS);
                            if elapsed_ms >= RUN_TIMEOUT_MS {
                                return;
                            }
                            continue;
                        }
                    };
                    if !resp.status().is_success() {
                        sleep_ms(RUN_POLL_INTERVAL_MS).await;
                        elapsed_ms = elapsed_ms.saturating_add(RUN_POLL_INTERVAL_MS);
                        if elapsed_ms >= RUN_TIMEOUT_MS {
                            return;
                        }
                        continue;
                    }
                    let run = match resp.json::<serde_json::Value>().await {
                        Ok(v) => v,
                        Err(_) => {
                            sleep_ms(RUN_POLL_INTERVAL_MS).await;
                            elapsed_ms = elapsed_ms.saturating_add(RUN_POLL_INTERVAL_MS);
                            if elapsed_ms >= RUN_TIMEOUT_MS {
                                return;
                            }
                            continue;
                        }
                    };
                    let status = run.get("status").and_then(|v| v.as_str()).unwrap_or("");
                    if status == "completed" {
                        let conclusion = run
                            .get("conclusion")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");
                        let html = run.get("html_url").and_then(|v| v.as_str()).unwrap_or("");
                        if conclusion != "success" {
                            surface_failure(
                                &tx, &owner, &repo, &branch, &head_sha, html, conclusion,
                            );
                        }
                        return;
                    }
                    sleep_ms(RUN_POLL_INTERVAL_MS).await;
                    elapsed_ms = elapsed_ms.saturating_add(RUN_POLL_INTERVAL_MS);
                    if elapsed_ms >= RUN_TIMEOUT_MS {
                        return;
                    }
                }
            }

            // Not found yet; wait and retry.
            sleep_ms(APPEAR_POLL_INTERVAL_MS).await;
        }
    });
}

fn parse_owner_repo(url: &str) -> Option<(String, String)> {
    // git@github.com:owner/repo.git or https://github.com/owner/repo(.git)
    if let Some(rest) = url.strip_prefix("git@github.com:") {
        let s = rest.trim_end_matches(".git");
        let mut parts = s.splitn(2, '/');
        let owner = parts.next()?.to_string();
        let repo = parts.next()?.to_string();
        return Some((owner, repo));
    }
    if let Some(pos) = url.find("github.com/") {
        let s = &url[pos + "github.com/".len()..];
        let s = s.trim_end_matches(".git");
        let mut parts = s.splitn(2, '/');
        let owner = parts.next()?.to_string();
        let repo = parts.next()?.to_string();
        return Some((owner, repo));
    }
    None
}

fn surface_failure(
    tx: &AppEventSender,
    owner: &str,
    repo: &str,
    branch: &str,
    sha: &str,
    url: &str,
    conclusion: &str,
) {
    let short = &sha[..std::cmp::min(7, sha.len())];
    let msg = if url.is_empty() {
        format!("❌ GitHub Actions failed for {owner}/{repo}@{short} on {branch}: {conclusion}")
    } else {
        format!(
            "❌ GitHub Actions failed for {owner}/{repo}@{short} on {branch}: {conclusion} — {url}"
        )
    };
    tx.send_background_event(msg);
}

async fn sleep_ms(ms: u64) {
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
}
