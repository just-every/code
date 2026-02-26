# Hermia Coder Ecosystem: Validation Checklist + Release Runbook

Date: 2026-02-16
Owner: Hermia Coder maintainers
Status: Active

This document defines the operational path from local validation to production release for the `just-every/code` fork.

It is aligned to the existing automation:
- Local build gate: `build-fast.sh`
- Local pre-release gate: `pre-release.sh`
- PR artifact pipeline: `.github/workflows/preview-build.yml`
- Mainline release pipeline: `.github/workflows/release.yml`

## 1. Release Entry Criteria

Do not start release work until all items are true.

- Target branch is `main`, and local branch is up to date with `origin/main`.
- Scope and risk are documented in the PR/commit series.
- No unresolved high-severity bugs are open for touched areas.
- Any behavior change has matching tests (or explicit rationale for no test).

## 2. Operational Validation Checklist

### 2.1 Mandatory local gate

Run from repository root:

```bash
./build-fast.sh
```

Pass criteria:
- Exit code is zero.
- Build produces no errors.
- Build produces no warnings.

### 2.2 Main branch preflight (required before push-to-main release)

Run from repository root:

```bash
./pre-release.sh
```

`pre-release.sh` currently validates:
- CLI build (`cargo build --locked --profile dev-fast --bin code`)
- CLI smoke checks (`scripts/ci-tests.sh` with `SKIP_CARGO_TESTS=1`)
- Workspace tests (`cargo nextest run --no-fail-fast --locked`)

Pass criteria:
- All three phases complete successfully.
- No retries needed due to flaky checks.

### 2.3 CI parity checks

Confirm local behavior matches CI expectations in `.github/workflows/release.yml`:

- Rust toolchain resolves from `code-rs/rust-toolchain.toml`.
- Linux fast E2E preflight is green (`preflight-tests` job equivalent).
- Multi-target binary packaging assumptions remain valid:
  - Linux: `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`
  - macOS: `x86_64-apple-darwin`, `aarch64-apple-darwin`
  - Windows: `x86_64-pc-windows-msvc`

### 2.4 Fleet-sensitive verification (when model/provider code changes)

Run this section if touching provider routing, agent execution, or endpoint wiring.

- Verify every configured local model endpoint returns healthy responses.
- Run at least one streamed chat completion against the primary endpoint.
- Verify fallback/secondary model route behavior if routing logic changed.
- Record response latency deltas versus prior baseline.

Suggested output artifact:
- `docs/plans/release-evidence/<date>-fleet-check.md` with endpoint health and latency notes.

### 2.5 Regression matrix by change type

Use the smallest matrix that still covers risk.

- Core/Rust execution changes:
  - `./build-fast.sh`
  - `./pre-release.sh`
- CLI packaging/release changes:
  - Above, plus inspect `release.yml` target/package steps for drift
- UI/TUI behavior changes:
  - Above, plus focused snapshot/manual regression checks

### 2.6 Milestone 1 core evidence requirements

For Milestone 1 (HTTP-native subagents in `code-rs/core`), attach evidence that
captures all of the following:

- Config parsing coverage for HTTP agent fields.
- HTTP dispatch coverage proving direct endpoint execution.
- Slash-agent enablement coverage for HTTP-only agents.
- Subprocess regression coverage proving non-HTTP agents still run unchanged.
- Validation notes for `/plan`, `/code`, `/solve`, streaming, and tool-use checks.

Store this in:
- `docs/plans/release-evidence/<date>-m1-http-subagents.md`

## 3. Staged Release Runbook

### Stage 0: PR preview artifacts

Trigger path:
- Pull request open/sync (non-draft, non-`upstream-merge`) via `preview-build.yml`

Expected outputs:
- Cross-platform preview artifacts uploaded
- Prerelease bundle published for PR validation

Go/no-go:
- All preview targets build successfully
- Reviewer validates install/run on at least one primary platform

### Stage 1: Mainline release trigger

Trigger path:
- Merge to `main` (non-ignored paths) starts `release.yml`

Critical jobs to watch:
- `npm-auth-check`
- `preflight-tests`
- `determine-version`
- `build-binaries`
- `cross-platform-artifact-smoke`
- `release`

Monitoring command (works with authenticated `gh`, and falls back to GitHub REST API for public repos when `gh` auth is unavailable):

```bash
scripts/wait-for-gh-run.sh --workflow Release --branch main --repo just-every/code
```

### Stage 2: Publish verification

After workflow success, verify:

- Git tag exists for computed version (`vX.Y.Z`).
- GitHub release is created with expected binary assets.
- npm package `@just-every/code` is published at the same version.
- Platform binary packages are published and resolvable.
- Homebrew tap update step succeeded (if triggered by workflow path).

### Stage 3: Immediate smoke window

Within 30 minutes of publish:

- Run `code --version` from freshly installed package(s).
- Run `/plan`, `/code`, and `/solve` once each using representative prompts.
- Validate streamed token output is visible during at least one run.
- Validate one shell command/tool-use flow.
- Confirm no startup crash on Linux, macOS, and Windows sample hosts.

Automation note:

- `release.yml` now enforces `cross-platform-artifact-smoke` before publish, covering startup/completion smoke on Linux x64/arm64, macOS x64/arm64, and Windows x64 from produced release artifacts.
- Manual smoke still focuses on post-publish `/plan` `/code` `/solve`, streaming visibility, and tool-use behavior.

### Stage 4: 24-hour watch

- Monitor issues/PR comments for install failures and regressions.
- Track crash reports and severe user-facing defects.
- If defects are critical, execute rollback policy immediately.

## 4. Rollback Policy (Fix-Forward First)

Because published versions and artifacts are externally consumed quickly, use fix-forward as default.

### 4.1 Severity classification

- Critical: install blocked, data loss risk, command execution unsafe.
- High: major feature broken or severe regression without workaround.
- Medium/Low: workaround exists or impact is limited.

### 4.2 Actions by severity

- Critical:
  - Pause promotion/announcements.
  - Cut emergency patch release (`+1` patch version) with minimal scoped fix.
  - Add clear release-note warning on bad version.
- High:
  - Schedule expedited patch release.
  - Publish workaround and affected scope.
- Medium/Low:
  - Batch into next planned patch cycle.

### 4.3 Rollback execution checklist

- Reproduce and isolate failing behavior.
- Implement minimal corrective patch with tests.
- Re-run `./build-fast.sh` and `./pre-release.sh`.
- Merge and re-run release pipeline.
- Post incident summary with root cause and prevention item.

## 5. Post-Deployment Monitoring and Evidence

Collect these artifacts for each release:

- Link to successful `release.yml` workflow run.
- Version/tag and publication timestamps.
- Smoke-check transcript (platform + command + result).
- Incident log (if any), including remediation release.

Store under:
- `docs/plans/release-evidence/<version>.md`

## 6. Known Gaps and Planned Automation

Current gaps:
- No enforced performance baseline gate in CI.
- No explicit canary cohort before broad publish.
- No centralized release health dashboard in-repo.

Planned improvements:
- Add benchmark regression guard for hot paths.
- Add optional canary release lane prior to full promotion.
- Add automated post-release health check summary artifact.
