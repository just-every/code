# Rust/codex-rs

In the codex-rs folder where the rust code lives:

- Crate names are prefixed with `codex-`. For example, the `core` folder's crate is named `codex-core`
- When using format! and you can inline variables into {}, always do that.
- Treat `codex-rs` as a read-only mirror of `openai/codex:main`; edit Rust sources under `code-rs` instead.

Completion/build step

- Always validate using `./build-fast.sh` from the repo root. This is the single required check and must pass cleanly.
- `./build-fast.sh` can take 20+min to run from a cold cache!!! Please use long timeout when running `./build-fast.sh` or waiting for it to complete.
- Policy: All errors AND all warnings must be fixed before you’re done. Treat any compiler warning as a failure and address it (rename unused vars with `_`, remove `mut`, delete dead code, etc.).
- Do not run additional format/lint/test commands on completion (e.g., `just fmt`, `just fix`, `cargo test`) unless explicitly requested for a specific task.
- ***NEVER run rustfmt***
- Before pushing to `main`, run `./pre-release.sh` to mirror the release preflight (dev-fast build, CLI smokes, workspace nextest).

Optional regression checks (recommended when touching the Rust workspace):

- `cargo nextest run --no-fail-fast` — runs all workspace tests with the TUI helpers automatically enabled. The suite is green after the resume fixtures/git-init fallback updates; older Git builds may print a warning when falling back from `--initial-branch`, but tests still pass.
- Focused sweeps stay quick and green: `cargo test -p code-tui --features test-helpers`, `cargo test -p code-cloud-tasks --tests`, and `cargo test -p mcp-types --tests`.

When debugging regressions or bugs, write a failing test (or targeted reproduction script) first and confirm it captures the issue before touching code—if it can’t fail, you can’t be confident the fix works.

## Documentation hygiene

- Keep docs clean, clear, and current; prune stale instructions instead of piling on caveats.
- Avoid excessive verbosity; prioritize concise guidance over long narratives.
- Do not document minor or non-core features; focus on system-critical flows and expectations.
- Never commit temporary "working" docs, plans, or scratch notes.

## Strict Ordering In The TUI History

The TUI enforces strict, per‑turn ordering for all streamed content. Every
stream insert (Answer or Reasoning) must be associated with a stable
`(request_ordinal, output_index, sequence_number)` key provided by the model.

- A stream insert MUST carry a non‑empty stream id. The UI seeds an order key
  for `(kind, id)` from the event's `OrderMeta` before any insert.
- The TUI WILL NOT insert streaming content without a stream id. Any attempt to
  insert without an id is dropped with an error log to make the issue visible
  during development.

## Commit Messages

- Review staged changes before every commit: `git --no-pager diff --staged --stat` (and skim `git --no-pager diff --staged` if needed).
- Write a descriptive subject that explains what changed and why. Avoid placeholders like "chore: commit local work".
- Prefer Conventional Commits with an optional scope: `feat(tui/history): …`, `fix(core/exec): …`, `docs(agents): …`.
- Keep the subject ≤ 72 chars; add a short body if rationale or context helps future readers.
- Use imperative, present tense: "add", "fix", "update" (not "added", "fixes").
- For merge commits, skip custom prefixes like `merge(main<-origin/main):`. Use a clear subject such as `Merge origin/main: <what changed and how conflicts were resolved>`.

Examples:

- `feat(tui/history): show exit code and duration for Exec cells`
- `fix(core/codex): handle SIGINT in on_exec_command_begin to avoid orphaned child`
- `docs(agents): clarify commit-message expectations`

## How to Git Push

### Merge-and-Push Policy (Do Not Rebase)

When the user asks you to "push" local work:

- Never rebase in this flow. Do not use `git pull --rebase` or attempt to replay local commits.
- Prefer a simple merge of `origin/main` into the current branch, keeping our local history intact.
- If the remote only has trivial release metadata changes (e.g., `codex-cli/package.json` version bumps), adopt the remote version for those files and keep ours for everything else unless the user specifies otherwise.
- If in doubt or if conflicts touch non-trivial areas, pause and ask before resolving.

Quick procedure (merge-only):

- Commit your local work first:
  - Review: `git --no-pager diff --stat` and `git --no-pager diff`
  - Stage + commit: `git add -A && git commit -m "<descriptive message of local changes>"`
- Fetch remote: `git fetch origin`
- Merge without auto-commit: `git merge --no-ff --no-commit origin/main` (stops before committing so you can choose sides)
- Resolve policy:
  - Default to ours: `git checkout --ours .`
  - Take remote for trivial package/version files as needed, e.g.: `git checkout --theirs codex-cli/package.json`
- Stage and commit the merge with a descriptive message, e.g.:
  - `git add -A && git commit -m "Merge origin/main: adopt remote version bumps; keep ours elsewhere (<areas>)"`
- Run `./build-fast.sh` and then `git push`

## Command Execution Architecture

The command execution flow in Codex follows an event-driven pattern:

1. **Core Layer** (`codex-core/src/codex.rs`):
   - `on_exec_command_begin()` initiates command execution
   - Creates `EventMsg::ExecCommandBegin` events with command details

2. **TUI Layer** (`codex-tui/src/chatwidget.rs`):
   - `handle_codex_event()` processes execution events
   - Manages `RunningCommand` state for active commands
   - Creates `HistoryCell::Exec` for UI rendering

3. **History Cell** (`codex-tui/src/history_cell.rs`):
   - `new_active_exec_command()` - Creates cell for running command
   - `new_completed_exec_command()` - Updates with final output
   - Handles syntax highlighting via `ParsedCommand`

This architecture separates concerns between execution logic (core), UI state management (chatwidget), and rendering (history_cell).

### Auto Drive Escape Handling

- All Auto Drive escape routing lives in `code-rs/tui/src/chatwidget.rs`. The
  `ChatWidget::auto_should_handle_global_esc` helper decides whether the global
  Esc handler in `app.rs` should defer to Auto Drive, and
  `ChatWidget::handle_key_event` owns the actual stop / pause behaviour. When
  you need to tweak Esc semantics, update those two locations together.
- The approval pane must *never* swallow Esc. `code-rs/tui/src/bottom_pane/auto_coordinator_view.rs`
  intentionally lets Esc (and the other approval shortcuts) bubble back to the
  chat widget; keep this contract intact when editing the view layer.
- Avoid adding additional Esc handlers elsewhere for Auto Drive flows. Doing
  so breaks the modal-first ordering in `app.rs` and prevents users from
  reliably stopping a run.

## Writing New UI Regression Tests

- Start with `make_chatwidget_manual()` (or `make_chatwidget_manual_with_sender()`) to build a `ChatWidget` in isolation with in-memory channels.
- Simulate user input by defining a small enum (`ScriptStep`) and feeding key events via `chat.handle_key_event()`; see `run_script()` in `tests.rs` for a ready-to-use helper that also pumps `AppEvent`s.
- After the scripted interaction, render with a `ratatui::Terminal`/`TestBackend`, then use `buffer_to_string()` (wraps `strip_ansi_escapes`) to normalize ANSI output before asserting.
- Prefer snapshot assertions (`assert_snapshot!`) or rich string comparisons so UI regressions are obvious. Keep snapshots deterministic by trimming trailing space and driving commit ticks just like the existing tests do.
- When adding fixtures or updating snapshots, gate rewrites behind an opt-in env var (e.g., `UPDATE_IDEAL=1`) so baseline refreshes remain explicit.

## VT100 Snapshot Harness

- The VT100 harness lives under `code-rs/tui/tests/vt100_chatwidget_snapshot.rs`. It renders the live `ChatWidget` UI into a `Terminal<VT100Backend>` so snapshots capture the exact PTY output the user sees (including frame chrome, composer rows, and streaming inserts).
- Use `ChatWidgetHarness` helpers from `code_tui::test_helpers` to seed history events and drain `AppEvent`s. Call `render_chat_widget_to_vt100(width, height)` for a single frame, or `render_chat_widget_frames_to_vt100(&[(w,h), ...])` to simulate successive draws while streaming.
- The harness now exports `layout_metrics()` so tests can assert scroll offsets and viewport heights without spelunking through private fields.
- Snapshots are deterministic: tests set `CODEX_TUI_FAKE_HOUR=12` automatically so greeting text (“What can I code for you today?”) doesn’t oscillate. If you need a different hour in a test, override the env var before constructing the harness.
- To add a new scenario, push history/events onto the harness, call `render_*_to_vt100`, and either `insta::assert_snapshot!` the frame(s) or manually assert string contents. For multi-frame streaming, push deltas/events first, then capture frames in the order the UI would display them.
- Run all VT100 snapshots via:
  - `cargo test -p code-tui --test vt100_chatwidget_snapshot --features test-helpers -- --nocapture`
- When you intentionally change rendering, review the `.snap.new` files that appear in `code-rs/tui/tests/snapshots/` and accept them with `cargo insta review` / `cargo insta accept` (limit to this test where possible).

### Monitor Release Workflows After Pushing

- Use `scripts/wait-for-gh-run.sh` to follow GitHub Actions releases without spamming manual `gh` commands.
- Typical release check right after a push: `scripts/wait-for-gh-run.sh --workflow Release --branch main`.
- If you already know the run ID (e.g., from webhook output), run `scripts/wait-for-gh-run.sh --run <run-id>`.
- Adjust the poll cadence via `--interval <seconds>` (defaults to 8). The script exits 0 on success and 1 on failure, so it can gate local automation.
- Pass `--failure-logs` to automatically dump logs for any job that does not finish successfully.
- Dependencies: GitHub CLI (`gh`) and `jq` must be available in `PATH`.
