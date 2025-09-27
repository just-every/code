# Plan: T10 Local-memory Migration
## Inputs
- Spec: docs/SPEC-KIT-010-local-memory-migration/spec.md (0f4b5f12)
- Constitution: memory/constitution.md (missing in repo; reference template as needed)

## Work Breakdown
1. Inventory Byterover domains, local-memory schema, and existing helper scripts; define migration mapping plus dry-run/report requirements.
2. Implement migration utility (dry-run + apply) that pulls Byterover entries via MCP and writes normalised records into local-memory with evidence logging.
3. Update Codex CLI/TUI integrations so slash commands, consensus verdicts, and Spec Ops hooks read/write using local-memory by default and persist any Byterover fallbacks.
4. Document the workflow, update SPEC tracker, and capture validation evidence for the migration run.

## Acceptance Mapping
| Requirement (Spec) | Validation Step | Test/Check Artifact |
| --- | --- | --- |
| R1: Deterministic migration command | Dry-run + apply on fixture data; verify import counts match | cargo test / integration script + evidence JSON |
| R2: Runtime uses local-memory first | Unit/integration tests exercising slash command hydration and consensus storage | `cargo test -p codex-tui spec_auto::local_memory_*` |
| R3: Evidence + reporting | Execute migration tool and store log/JSON under SPEC-KIT-010 | docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-010/*.json |
| R4: Documentation updated | Manual review of AGENTS/RESTART updates | Updated docs |

## Risks & Unknowns
- Need Byterover MCP credentials in local env; tests may need mock transports.
- Existing local-memory entries might collide with migrated IDs; need strategy for dedupe.
- Runtime fallback paths must avoid infinite loops between Byterover and local-memory.

## Consensus & Risks (Multi-AI)
- Solo Codex planning; record requirement to rerun consensus plan with full agent stack when available.
- No agent disagreements captured (degraded mode).

## Exit Criteria (Done)
- Migration tool merged with tests and evidence run
- Local-memory-first behaviour verified in CLI/TUI flows
- Documentation + SPEC tracker updated and linted
