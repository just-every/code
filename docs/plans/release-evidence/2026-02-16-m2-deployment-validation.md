# Milestone 2 Evidence: Deployment Validation Sweep

Date: 2026-02-16
Scope: staged release runbook validation as far as this local environment allows

## Environment and Boundaries

- Repo: `just-every/code` (`origin` remote confirmed).
- Local host: Linux only.
- `gh` CLI was installed during this sweep (`gh version 2.45.0`).
- `gh` is not authenticated here (`GH_TOKEN`/`GITHUB_TOKEN` unset).
- `scripts/wait-for-gh-run.sh` now supports automatic GitHub REST API fallback, so run polling still works for public repos without authenticated `gh`.
- Live publication actions (GitHub release creation, npm publish, Homebrew push) are validated via public API/read-side checks, not by re-running publish jobs from here.

## Stage 0: PR Preview Artifacts

| Check | Command | Result |
|---|---|---|
| Latest preview run status | `curl -fsSL 'https://api.github.com/repos/just-every/code/actions/workflows/preview-build.yml/runs?per_page=5' | jq ...` | Latest run `21165557853` is `completed/action_required` (2026-01-20). |
| Latest successful preview run | `curl -fsSL 'https://api.github.com/repos/just-every/code/actions/workflows/preview-build.yml/runs?per_page=100' | jq ...` | Latest success `20976905673` (2026-01-13). |
| Preview job coverage | `curl -fsSL 'https://api.github.com/repos/just-every/code/actions/runs/20976905673/jobs?per_page=100' | jq ...` | All target build jobs + `Publish prerelease (all targets)` succeeded. |
| Preview artifacts present | `curl -fsSL 'https://api.github.com/repos/just-every/code/actions/runs/20976905673/artifacts?per_page=100' | jq ...` | 5 artifacts present: linux x64/aarch64 musl, macOS x64/arm64, windows x64. |

## Stage 1: Mainline Release Trigger + Parity

| Check | Command | Result |
|---|---|---|
| Workflow parity (toolchain + gates + targets) | `python3`/`grep` against `.github/workflows/release.yml` and `code-rs/rust-toolchain.toml` | Parity confirmed: toolchain `1.90.0`, `cargo build --locked --profile dev-fast --bin code`, `cargo nextest run --no-fail-fast --locked`, expected 5 release targets. |
| Preview matrix parity | `grep -nE 'target: ...' .github/workflows/preview-build.yml` | Preview workflow carries matching 5-target matrix. |
| Latest release workflow runs on `main` | `curl -fsSL 'https://api.github.com/repos/just-every/code/actions/workflows/release.yml/runs?branch=main&per_page=10' | jq ...` | Latest run `22050457338` is `success` (2026-02-16). |
| Critical release jobs | `curl -fsSL 'https://api.github.com/repos/just-every/code/actions/runs/22050457338/jobs?per_page=100' | jq ...` | `Validate npm auth`, `Preflight Tests`, `Determine Version`, all 5 `Build ...`, and `Publish to npm` all succeeded. |
| Monitor helper readiness | `bash scripts/wait-for-gh-run.sh --help` | Help output OK. |
| Install `gh` | `sudo apt-get install -y gh` | Pass: `gh version 2.45.0`. |
| Monitor helper execution by run ID (no `gh` auth) | `env -u GH_TOKEN -u GITHUB_TOKEN bash scripts/wait-for-gh-run.sh --repo just-every/code --run 22050457338 --interval 1` | Pass via API fallback backend; run concluded success with live job summary. |
| Monitor helper execution by workflow+branch (no `gh` auth) | `env -u GH_TOKEN -u GITHUB_TOKEN bash scripts/wait-for-gh-run.sh --repo just-every/code --workflow Release --branch main --interval 1` | Pass via API fallback backend; auto-selected latest run and returned success. |

## Stage 2: Publish Verification

| Check | Command | Result |
|---|---|---|
| Tag exists | `git ls-remote --tags origin v0.6.70` | Tag exists (`refs/tags/v0.6.70`). |
| Release exists + assets | `curl -fsSL 'https://api.github.com/repos/just-every/code/releases/tags/v0.6.70' | jq ...` | Stable release published 2026-02-16 with 9 assets (linux/macos tar+zst, windows zip). |
| npm package version alignment | `npm view` for `@just-every/code` + 5 platform packages | All report `0.6.70`. |
| Platform package resolvability | `npm view <pkg>@0.6.70 dist.tarball dist.integrity` | Tarball URLs and integrity hashes resolve for all 5 platform packages. |
| Homebrew tap update | `curl -fsSL 'https://raw.githubusercontent.com/just-every/homebrew-tap/main/Formula/Code.rb'` | `Formula/Code.rb` references `version "v0.6.70"` and matching release URLs. |

## Stage 3: Immediate Smoke Window (Local-Executable Portion)

| Check | Command | Result |
|---|---|---|
| Cross-platform smoke automation enforced pre-publish | `python3` assertion against `.github/workflows/release.yml` | Pass: `cross-platform-artifact-smoke` job exists, covers linux x64/arm64 + macOS x64/arm64 + windows x64, and `release` now depends on it. |
| Fresh release binary starts | Download + extract `code-x86_64-unknown-linux-musl.tar.gz`, then `./code-x86_64-unknown-linux-musl --version` | Pass: `code 0.6.70`. |
| `/plan` full completion smoke | `/tmp/code-smoke-v0.6.70/code-x86_64-unknown-linux-musl exec --skip-git-repo-check --cd /tmp/m2-smoke --json --max-seconds 90 '/plan create a two-step plan to verify readme.txt exists and can be read'` | Pass: completed with final `agent_message` (see `/tmp/m2-plan.jsonl`). |
| `/code` full completion smoke | `/tmp/code-smoke-v0.6.70/code-x86_64-unknown-linux-musl exec --skip-git-repo-check --cd /tmp/m2-smoke --json --max-seconds 120 '/code write a one-line shell command that prints HELLO and explain in one sentence'` | Pass: completed with final `agent_message` and verified `echo HELLO` execution (see `/tmp/m2-code.jsonl`). |
| `/solve` full completion smoke | `/tmp/code-smoke-v0.6.70/code-x86_64-unknown-linux-musl exec --skip-git-repo-check --cd /tmp/m2-smoke --json --max-seconds 120 '/solve quickly diagnose: rg is missing on PATH; give concise fix steps'` | Pass: completed with concise diagnosis + fix steps (see `/tmp/m2-solve.jsonl`). |
| Streaming visibility (local proxy) | `cargo test -p code-tui --test ui_smoke smoke_streaming_assistant_message -- --nocapture` | Pass. |
| Tool-use flow (local proxy) | `cargo test -p code-core --test tool_hooks tool_hooks_fire_for_shell_exec -- --nocapture` | Pass. |

Notes:
- This environment cannot run macOS/Windows binaries natively; those startup checks remain live release-stage checks.
- Full slash-command completions were executed (not only dispatch/path checks).

## Stage 4: Rollback Readiness

| Check | Command | Result |
|---|---|---|
| Rollback doc path present | `sed -n '1,260p' docs/plans/2026-02-16-hermia-coder-ecosystem.md` | Stage 4 + rollback policy/checklist present and actionable. |
| Release-notes guard script | `scripts/check-release-notes-version.sh` | Pass in current workspace state. |
| Monitor script operability | `bash scripts/wait-for-gh-run.sh --help` plus unauthenticated `--run ...` and `--workflow ...` probes | Pass; API fallback works without `GH_TOKEN` for public repos. |

## Local vs Live Boundary Summary

| Area | Validated here | Requires live release env |
|---|---|---|
| Workflow definition parity | Yes | No |
| Historical workflow outcomes (public API) | Yes | No |
| Live run polling via `scripts/wait-for-gh-run.sh` | Yes (API fallback validated locally without `gh` auth) | No for public repos; private repos still require token/auth |
| Tag/release/npm/homebrew read-side verification | Yes | No |
| Linux fresh-binary smoke | Yes | No |
| macOS/Windows runtime smoke enforcement | Yes (automated in `release.yml` via `cross-platform-artifact-smoke`) | Runtime evidence appears on next release run |
| Full `/plan` `/code` `/solve` completion | Yes (executed to completion locally with release binary) | Live publish-window re-check still recommended |

## Post-Edit Gate Re-Run

These were re-run after the auto-review P1 write-mode HTTP semantics fix, release-monitoring/smoke automation hardening changes, and merge with `origin/main`.

| Gate | Command | Result |
|---|---|---|
| Local build gate | `./build-fast.sh` | Pass |
| Local pre-release gate | `./pre-release.sh` | Pass (`nextest` run ID `d3a38480-1f55-4698-ac7a-1aede91170ff`, 1364 passed / 4 skipped) |

## Fresh Live Release Run Attempt (Post-Change)

Goal was to push the post-change commits and validate a fresh `release.yml` run.

| Step | Command | Result |
|---|---|---|
| Push to main | `git push origin main` | Blocked: `remote: Permission to just-every/code.git denied to hermia-ai` + HTTP 403 |
| Check GH CLI auth | `gh auth status -h github.com` | No authenticated GitHub host |
| Check token env | `env | grep -E '^(GH_TOKEN|GITHUB_TOKEN)='` | No token present |
| Check SSH credential path | `ssh -o BatchMode=yes -T git@github.com` | Blocked: `Permission denied (publickey)` |

Exhaustion outcome:
- No available credential in this environment can push to `just-every/code`, so a fresh post-change live release run cannot be triggered from here.
- Commits prepared locally for landing:
  - `58e91d6f6` (`feat(core/release): ship HTTP agents and release hardening`)
  - `939c76d19` (`Merge origin/main: sync upstream release updates and keep Hermia deployment hardening`)

## Final GO/NO-GO

| Item | Status | Evidence |
|---|---|---|
| Run monitoring without authenticated `gh` | GO | `scripts/wait-for-gh-run.sh` succeeded via API fallback for both `--run` and `--workflow --branch` paths with `GH_TOKEN`/`GITHUB_TOKEN` unset. |
| Cross-platform smoke enforcement before publish | GO | `release.yml` includes `cross-platform-artifact-smoke` (linux x64/arm64, macOS x64/arm64, windows x64), and `release` depends on it. |
| Private-repo monitoring without auth | NO-GO boundary | REST fallback can require token for private repositories; current proof is for public repo `just-every/code`. |
| Fresh post-change live release run | NO-GO (hard permission block) | Push to `origin/main` blocked by 403 (no usable HTTPS/SSH credential in environment). |
| Published-run execution evidence for new automation | NO-GO boundary | Blocked until push permission is available and a new release workflow run executes. |
