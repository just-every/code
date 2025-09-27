# Plan: T14 Documentation Refresh
## Inputs
- Spec: docs/SPEC-KIT-014-docs-refresh/spec.md (3f3c34f0)
- Constitution: memory/constitution.md (missing in repo; using latest template reference)

## Work Breakdown
1. Diff current docs (slash-commands, AGENTS, onboarding) to identify stale content vs new Spec Kit workflow.
2. Draft updates referencing telemetry schema & model strategy; add troubleshooting guidance.
3. Review and polish language; ensure links and cross-references resolve.
4. Update SPEC tracker entry and run doc lint/checks if configured.

## Acceptance Mapping
| Requirement (Spec) | Validation Step | Test/Check Artifact |
| --- | --- | --- |
| R1: Slash command reference updated | Manual diff review; ensure `/spec-ops-*` section reflects schema | docs/slash-commands.md |
| R2: Guardrail constitution covers telemetry schema | Verify AGENTS.md includes schema summary/link | AGENTS.md |
| R3: Onboarding references evidence + validation commands | docs/getting-started.md updated | docs/getting-started.md |
| R4: Troubleshooting guidance added | Section added to AGENTS.md or RESTART.md | Updated doc snippet |

## Risks & Unknowns
- Missing constitution/product requirements may require noting placeholders.
- Need to ensure messaging stays concise without duplicating Spec Kit blueprint.

## Consensus & Risks (Multi-AI)
- Agreement: Solo Codex planning (Gemini/Claude/Qwen unavailable); document requirement to rerun with full agent lineup if needed.
- Disagreement & resolution: n/a.

## Exit Criteria (Done)
- Docs merged with updated content and cross-links
- SPEC tracker updated with validation note
- `scripts/spec-kit/lint_tasks.py` passes
