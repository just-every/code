# Spec Tracker

| Order | Task ID | Title | Status | Owners | PRD | Branch | PR | Last Validation | Evidence | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | T1 | Rename guardrail commands | Done | Code |  |  |  |  |  | Completed during initial rollout |
| 2 | T2 | Update shell telemetry names | Done | Code |  |  |  | 2025-09-26 | docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/20250926-024834Z-code-mcp-list.json | Renamed guardrail command to `/spec-ops-audit`, updated telemetry prefix + tests |
| 3 | T3 | Implement `/spec-plan` prompt | Done | Claude MAX |  |  |  |  |  |  |
| 4 | T4 | Implement `/spec-tasks` prompt | Done | Claude MAX |  |  |  |  |  |  |
| 5 | T5 | Implement `/spec-implement` prompt | Done | Code |  |  |  |  |  |  |
| 6 | T6 | Implement `/spec-validate` prompt | Done | Gemini Ultra |  |  |  |  |  |  |
| 7 | T7 | Implement `/spec-audit` prompt | Done | Claude MAX |  |  |  |  |  |  |
| 8 | T8 | Implement `/spec-unlock` prompt | Done | Gemini Ultra |  |  |  |  |  |  |
| 9 | T9 | MCP server enablement | Done | Code |  |  |  | 2025-09-26 | docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/20250926-231931Z-code-mcp-list.json | Added default MCP configs (repo_search/doc_index/shell_lite/git_status/uniprof/hal) and CLI documentation. |
| 10 | T10 | Local-memory migration | Done | Code | docs/SPEC-KIT-010-local-memory-migration/PRD.md | feat/spec-auto-telemetry |  | 2025-09-28 | docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-010/migration_apply_20250928T1800Z.json | Dry-run/apply evidence captured; runbook committed; `cargo test -p codex-tui spec_auto` |
| 11 | T11 | `/spec-auto` orchestrator | Done | Code |  | feat/spec-auto-telemetry |  | 2025-09-26 | docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/20250926-025004Z-codex-mcp-client-git-status.json | Wired MCP evidence lookup; `cargo test -p codex-tui spec_auto` |
| 12 | T12 | Consensus diff reviewer | Done | Gemini Ultra |  |  |  | 2025-09-27 | docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus | `/spec-auto` halts on degraded verdicts; prompts emit model metadata; integration tests cover happy/degraded consensus. |
| 13 | T13 | Telemetry schema enforcement | Done | Code | docs/SPEC-KIT-013-telemetry-schema-guard/PRD.md | feat/spec-auto-telemetry |  | 2025-09-27 | docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-013/spec-plan_2025-09-27T18:35:18Z-748128599.json | Schema validators + unit tests landed; `cargo test -p codex-tui spec_auto` |
| 14 | T14 | Documentation updates | In Progress | Code | docs/SPEC-KIT-014-docs-refresh/PRD.md | feat/spec-auto-telemetry |  |  | docs/SPEC-KIT-014-docs-refresh/spec.md | 2025-09-29: Slash-commands/AGENTS/getting-started/RESTART updated with telemetry + HAL guidance; `scripts/doc-structure-validate.sh --mode=templates` added and dry-run passes |
| 15 | T15 | Nightly sync check | Done | Code | docs/SPEC-KIT-015-nightly-sync/PRD.md | feat/spec-auto-telemetry |  | 2025-09-27 | docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-015/nightly_sync_detect_20250927T215031Z.log | Drift detector script implemented; sample run captures missing-memory report |
| 16 | T18 | HAL HTTP MCP integration | In Progress | Code | docs/SPEC-KIT-018-hal-http-mcp/spec.md | feat/spec-auto-telemetry |  | 2025-09-29 | docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-018/spec-validate_2025-09-29T14:54:35Z-3088619300.json | 2025-09-29: Captured healthy/degraded HAL telemetry (`20250929-114636Z`, `20250929-114708Z`, `20250929-145435Z`) plus `hal.summary` evidence (`20250929-123303Z`/`20250929-123329Z`); templates delivered, docs/prompts sync pending review |
| 17 | T20 | Guardrail hardening | In Progress | Code | docs/SPEC-OPS-004-integrated-coder-hooks/spec.md | feat/spec-auto-telemetry |  |  | docs/SPEC-OPS-004-integrated-coder-hooks/notes/guardrail-hardening.md | 2025-09-29: Telemetry flag docs + rollout checklist drafted; `hal.summary` emitted in guardrail telemetry; CI parity dry run + validator patch next |
