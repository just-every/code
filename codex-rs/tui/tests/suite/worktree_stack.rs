//! Exercises stack-sensitive session transitions through the real TUI event loop.

use super::focus_palette::PtyCodex;
use super::focus_palette::write_test_config;
use anyhow::Result;
use anyhow::ensure;
use codex_worktree::ManagedWorktree;
use codex_worktree::WorktreeManager;
use codex_worktree::WorktreeSettings;
use core_test_support::responses;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;
use std::time::Instant;
use wiremock::matchers::body_string_contains;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn picker_side_worktree_fork_and_cd_run_on_the_production_stack() -> Result<()> {
    let repository = tempfile::tempdir_in("/tmp")?;
    let root = repository.path().canonicalize()?;
    let status = Command::new("git")
        .args(["init", "-q", "-b", "main"])
        .arg(&root)
        .status()?;
    ensure!(status.success(), "initialize test repository");
    fs::write(root.join("README.md"), "stack transition test\n")?;
    ensure!(
        Command::new("git")
            .args(["-C"])
            .arg(&root)
            .args(["add", "README.md"])
            .status()?
            .success(),
        "stage test repository"
    );
    ensure!(
        Command::new("git")
            .args(["-C"])
            .arg(&root)
            .args([
                "-c",
                "user.name=Codex Test",
                "-c",
                "user.email=test@example.invalid"
            ])
            .args(["commit", "--no-gpg-sign", "-qm", "initial"])
            .status()?
            .success(),
        "commit test repository"
    );

    let codex_home = tempfile::tempdir_in("/tmp")?;
    write_test_config(codex_home.path(), &root)?;
    let server = responses::start_mock_server().await;
    let config_path = codex_home.path().join("config.toml");
    let config = fs::read_to_string(&config_path)?
        .replace("model_provider = \"openai\"", "model_provider = \"test\"");
    fs::write(
        &config_path,
        format!(
            "{config}\n[model_providers.test]\nname = \"Mock\"\nbase_url = \"{}/v1\"\nwire_api = \"responses\"\nrequires_openai_auth = false\nsupports_websockets = false\n",
            server.uri()
        ),
    )?;
    let _reply = responses::mount_sse_once_match(
        &server,
        body_string_contains("marker requested by this test"),
        responses::sse(vec![
            responses::ev_assistant_message("saved", "STACK_SAVED_HISTORY"),
            responses::ev_completed("saved-response"),
        ]),
    )
    .await;

    let worktrees = WorktreeManager::new(WorktreeSettings::for_cli(
        codex_home.path(),
        /*desktop*/ None,
    )?);
    // The launcher gives codex-main and Tokio workers explicit 16 MiB stacks.
    let mut terminal = PtyCodex::start(
        &root,
        codex_home,
        &[
            "-c",
            "features.worktrees=true",
            "-c",
            "features.shell_snapshot=false",
            "Reply with the marker requested by this test.",
        ],
    )?;
    terminal.wait_for_startup()?;
    terminal.wait_for_screen("STACK_SAVED_HISTORY")?;
    terminal.wait_for_screen("Ask Codex to do anything")?;

    submit(&mut terminal, "/side")?;
    terminal.wait_for_screen("Side from main thread")?;
    terminal.ensure_running()?;
    terminal.write_input(b"\x03")?;
    terminal.wait_for_screen("STACK_SAVED_HISTORY")?;

    submit(&mut terminal, "/resume")?;
    terminal.wait_for_screen("Resume a previous session")?;
    terminal.ensure_running()?;
    terminal.write_input(b"\x1b")?;
    terminal.wait_for_screen("Ask Codex to do anything")?;

    submit(&mut terminal, "/worktree")?;
    terminal.wait_for_screen("Continue current conversation")?;
    terminal.write_input(b"1")?;
    let forked = wait_for_owned_worktrees(&mut terminal, &worktrees, &root, /*count*/ 1)?;
    let fork_bucket = forked[0].root.parent().expect("bucket");
    terminal.wait_for_screen(&format!(
        "/{}/",
        fork_bucket.file_name().unwrap().to_string_lossy()
    ))?;
    terminal.ensure_running()?;

    submit(&mut terminal, &format!("/cd {}", root.display()))?;
    terminal.wait_for_screen(&format!("Working directory changed to: {}", root.display()))?;
    terminal.ensure_running()?;

    submit(&mut terminal, "/worktree")?;
    terminal.wait_for_screen("Start new conversation")?;
    terminal.write_input(b"2")?;
    let created = wait_for_owned_worktrees(&mut terminal, &worktrees, &root, /*count*/ 2)?;
    ensure!(
        created
            .iter()
            .any(|checkout| checkout.root == forked[0].root)
    );
    let second = created
        .iter()
        .find(|checkout| checkout.root != forked[0].root)
        .expect("second checkout");
    ensure!(
        worktrees.owner(&forked[0].root)? != worktrees.owner(&second.root)?,
        "new worktree reused the forked thread owner"
    );
    let second_bucket = second.root.parent().expect("bucket");
    terminal.wait_for_screen(&format!(
        "/{}/",
        second_bucket.file_name().unwrap().to_string_lossy()
    ))?;
    terminal.ensure_running()?;
    Ok(())
}

fn submit(terminal: &mut PtyCodex, input: &str) -> Result<()> {
    terminal.write_input(input.as_bytes())?;
    terminal.wait_for_screen(input)?;
    // Bulk PTY writes resemble a paste; wait out the Enter suppression window.
    std::thread::sleep(Duration::from_millis(/*millis*/ 250));
    terminal.write_input(b"\r")?;
    Ok(())
}

fn wait_for_owned_worktrees(
    terminal: &mut PtyCodex,
    manager: &WorktreeManager,
    source: &Path,
    count: usize,
) -> Result<Vec<ManagedWorktree>> {
    let deadline = Instant::now() + Duration::from_secs(/*secs*/ 30);
    while Instant::now() < deadline {
        terminal.read_output(Duration::from_millis(/*millis*/ 50))?;
        let checkouts = manager.list(source).unwrap_or_default();
        if checkouts.len() == count
            && checkouts
                .iter()
                .all(|checkout| manager.owner(&checkout.root).ok().flatten().is_some())
        {
            return Ok(checkouts);
        }
        terminal.ensure_running()?;
    }
    anyhow::bail!(
        "did not bind {count} managed checkouts; screen:\n{}",
        terminal.screen_contents()
    )
}
