# Code Spec-Kit Constitution

## Core Principles

### Evidence-Driven Templates
- Every update must keep acceptance criteria, task mappings, and guardrail documentation in sync across SPEC.md, plan/tasks templates, and slash-command guidance.

### Cross-Repo Separation
- Shared tooling and templates live in this repository; project-specific configurations, telemetry, and evidence stay inside their respective product repositories.

### Tooling Discipline
- Data access and automation must flow through MCP/LLM tooling; avoid bespoke shell scripts for runtime evidence or API calls unless MCP cannot satisfy the requirement.

## Governance & Workflow
- SPEC.md is the canonical tracker; keep one `In Progress` entry per active thread and update notes with dated evidence references.
- Template changes require accompanying documentation updates (RESTART.md, docs/slash-commands.md, etc.) and passing `cargo test -p codex-tui spec_auto`.
- Guardrail scripts and prompts must remain agent-friendly: record model metadata, surface telemetry artifacts, and never rely on local state that agents cannot reproduce.

**Version**: 1.1 | **Ratified**: 2025-09-28 | **Last Amended**: 2025-09-28
