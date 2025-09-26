# Spec Kit Implementation Tasks

| ID | Title | Description | Dependencies | Owner | Status |
| --- | --- | --- | --- | --- | --- |
| T1 | Rename guardrail commands | Update `SlashCommand` enum, dispatcher, docs to use `/spec-ops-*` names and add deprecation warnings for old aliases. | None | Code | Done |
| T2 | Update shell telemetry names | Rename `spec_ops_review.sh` → `spec_ops_audit.sh`, adjust telemetry JSON keys, and update evidence scripts. | T1 | Code | Done |
| T3 | Implement `/spec-plan` prompt | Build spec-aware prompt formatter that loads SPEC docs via MCP/local-memory and launches Gemini/Claude/GPT consensus. | T1 | Claude MAX | Done |
| T4 | Implement `/spec-tasks` prompt | Extend multi-agent synthesis to update SPEC.md Tasks table and `docs/SPEC-*/tasks.md` with acceptance mapping. | T3 | Claude MAX | Done |
| T5 | Implement `/spec-implement` prompt | Orchestrate agent collaboration on diffs, leveraging GPT Pro for execution and local-memory for context. | T3 | Code | Done |
| T6 | Implement `/spec-validate` prompt | Compare telemetry from validation harness against acceptance criteria; record evidence in local-memory. | T3 | Gemini Ultra | Done |
| T7 | Implement `/spec-review` prompt | Produce go/no-go memo referencing MCP evidence; ensure conflicts resolved and logged. | T6 | Claude MAX | Done |
| T8 | Implement `/spec-unlock` prompt | Require unlock justification memo tied to SPEC.md state before invoking shell unlocker. | T5 | Gemini Ultra | Done |
| T9 | MCP server enablement | Configure repo-search, doc-index, shell-lite, git-status, spec-registry MCP servers and expose to agents. | T1 | Code | Backlog |
| T10 | Local-memory migration | Mirror Byterover entries into local-memory domains and update retrieval/write-back hooks. | T1 | Code | Backlog |
| T11 | `/spec-auto` orchestrator | Build state machine that chains guardrail and multi-agent stages, records checkpoints, and supports resume/skip flags. | T3,T4,T5,T6,T7,T8 | Code | Done |
| T12 | Consensus diff reviewer | Implement MCP tool that compares agent outputs and enforces consensus acknowledgment before stage completion. | T3 | Gemini Ultra | Backlog |
| T13 | Telemetry schema enforcement | Add validation to fail `/spec-auto` when JSON evidence is missing or malformed. | T2 | Code | Backlog |
| T14 | Documentation updates | Refresh `docs/slash-commands.md`, AGENTS.md, onboarding materials to reflect new commands and workflows. | T1-T8 | Code | In Progress |
| T15 | Nightly sync check | Create script/job that reconciles local-memory entries with evidence logs to detect drift. | T10 | Code | Backlog |
