# Spec Kit Implementation Tasks

| ID | Title | Description | Dependencies | Owner | Status |
| --- | --- | --- | --- | --- | --- |
| T1 | Rename guardrail commands | Update `SlashCommand` enum, dispatcher, docs to use `/spec-ops-*` names and add deprecation warnings for old aliases. | None | Code | Done |
| T2 | Update shell telemetry names | Rename `spec_ops_review.sh` → `spec_ops_audit.sh`, adjust telemetry JSON keys, and update evidence scripts. | T1 | Code | Done |
| T3 | Implement `/spec-plan` prompt | Build spec-aware prompt formatter that loads SPEC docs via MCP/local-memory and launches Gemini/Claude/GPT consensus. | T1 | Claude MAX | Done |
| T4 | Implement `/spec-tasks` prompt | Extend multi-agent synthesis to update SPEC.md Tasks table and `docs/SPEC-*/tasks.md` with acceptance mapping. | T3 | Claude MAX | Done |
| T5 | Implement `/spec-implement` prompt | Orchestrate agent collaboration on diffs, leveraging GPT Pro for execution and local-memory for context. | T3 | Code | Done |
| T6 | Implement `/spec-validate` prompt | Compare telemetry from validation harness against acceptance criteria; record evidence in local-memory. | T3 | Gemini Ultra | Done |
| T7 | Implement `/spec-audit` prompt | Produce go/no-go memo referencing MCP evidence, emit consensus verdict JSON, and surface agent disagreements. | T6 | Claude MAX | Done |
| T8 | Implement `/spec-unlock` prompt | Require unlock justification memo tied to SPEC.md state before invoking shell unlocker. | T5 | Gemini Ultra | Done |
| T9 | MCP server enablement | Configure repo-search, doc-index, shell-lite, git-status, spec-registry MCP servers and expose to agents. | T1 | Code | Done |
| T10 | Local-memory migration | Mirror Byterover entries into local-memory domains and update retrieval/write-back hooks. Baseline + migration tooling scaffolding in place; finish tests, runtime fallbacks, and schedule Oct 2 run. | T1 | Code | In Progress |
| T11 | `/spec-auto` orchestrator | Build state machine that chains guardrail and multi-agent stages, records checkpoints, and supports resume/skip flags. | T3,T4,T5,T6,T7,T8 | Code | Done |
| T12 | Consensus diff reviewer | Implement MCP tool that compares agent outputs, persists verdicts, and blocks `/spec-auto` until consensus degradation is resolved. | T3 | Gemini Ultra | Done |
| T13 | Telemetry schema enforcement | Add validation to fail `/spec-auto` when JSON evidence is missing or malformed. | T2 | Code | Backlog |
| T14 | Documentation updates | Refresh `docs/slash-commands.md`, AGENTS.md, onboarding materials to reflect new commands and workflows. | T1-T8 | Code | In Progress |
| T15 | Nightly sync check | Create script/job that reconciles local-memory entries with evidence logs to detect drift. | T10 | Code | Backlog |
| T16 | Evaluate Uniprof MCP integration | Pilot `uniprof` MCP to capture /spec-validate profiling evidence and document flamegraph workflow. | T9 | Code | Backlog |
| T17 | Justfile service orchestration | Replace devserver MCP with `just` recipes for starting/stopping Kavedarr API, worker, and dashboard. | T9 | Code | Done |
| T18 | Evaluate HAL HTTP MCP integration | Wire HAL to Kavedarr staging OpenAPI spec for automated API smoke validation with secret management. | T9 | Code | In Progress |
| T19 | Evaluate Postgres MCP integration | Assess Postgres MCP against staging telemetry databases and design read-only credential strategy. | T9 | Code | Backlog |
