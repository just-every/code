This PR merges the latest `openai/codex@main` into our repository using the default `-X ours` strategy.

Summary
- Strategy: prefer our changes on conflict; accept upstream additions where non-conflicting.
- TUI policy: kept our themes/browser/agents; incorporated trivial upstream modules and tests.
- Docs: kept our `AGENTS.md`, `CHANGELOG.md`, and `README.md` versions.
- Workflows: retained ours; excluded upstream workflow files from this branch due to token permission limits.

Build Validation
- Ran `./build-fast.sh` from repo root.
- Status: PASS (no warnings).

Notable Follow-ups
- New TUI tests and modules from upstream are included; no functional divergences observed in build.
- If desired, we can selectively adopt additional upstream TUI improvements in subsequent PRs.

See MERGE_REPORT.md for a concise list of incorporated, dropped, and other changes.
