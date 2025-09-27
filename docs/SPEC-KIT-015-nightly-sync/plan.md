# Plan: T15 Nightly Sync Drift Detector
## Inputs
- Spec: docs/SPEC-KIT-015-nightly-sync/spec.md (a2583e60)
- Constitution: memory/constitution.md (missing in repo; reference template as needed)

## Work Breakdown
1. Inventory `code local-memory export` output and evidence directory structure; define mapping (SPEC ID ↔ memory tags).
2. Implement drift detection script (parse telemetry JSON, compare to memory entries, produce report + exit codes).
3. Add allowlist/config support and documentation for scheduling/remediation.
4. Update SPEC tracker notes and run `scripts/spec-kit/lint_tasks.py` after changes.

## Acceptance Mapping
| Requirement (Spec) | Validation Step | Test/Check Artifact |
| --- | --- | --- |
| R1: Compare local-memory vs. evidence | Unit/integration test invoking script on sample data | tests or scripts/spec-kit/nightly_sync_detect.py --sample |
| R2: Exit codes + JSON output | Script tested with drift/no drift scenarios | CI log / sample output |
| R3: Documentation | Updates to RESTART.md or docs/spec-kit | Updated doc sections |
| R4: Allowlist/config docs | README or inline comments | Script usage docs |

## Risks & Unknowns
- Need reliable way to read local-memory export during nightly job (may require new CLI command).
- Evidence volume could grow; may need batching or caching.

## Consensus & Risks (Multi-AI)
- Solo Codex draft (other agents unavailable). Note requirement to re-run `/plan` with full agent stack if needed.

## Exit Criteria (Done)
- Tool implemented with tests
- Docs updated with scheduling/remediation guidance
- SPEC tracker updated with latest run evidence

## Usage Notes
- Export memories before running: `./codex-rs/target/debug/code local-memory export --output tmp/memories.jsonl`.
- Detect drift manually: `python3 scripts/spec-kit/nightly_sync_detect.py --memory tmp/memories.jsonl --json-out docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-015/nightly_sync_detect_<timestamp>.json --pretty`.
- Exit code 0 = no drift, 1 = drift detected, 2 = execution error (missing inputs, parse failure).
- Allowlist sustained divergences via `--allowlist path/to/allowlist.txt`; entries accept globbed evidence paths or SPEC IDs.
- Schedule nightly via cron or CI: export memories, run detector, archive JSON/log artifacts, and alert on non-zero exit status.
