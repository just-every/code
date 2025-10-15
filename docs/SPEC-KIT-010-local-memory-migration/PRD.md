# PRD: Local-memory Migration (T10)

## Overview
- Spec Kit workflows still lean on Byterover for historical context and telemetry despite local-memory being the mandated source of truth.
- Missing automation leads to drift between the two stores, forcing manual reconciliation and blocking guardrails such as the nightly drift detector.
- This PRD captures the functional expectations for migrating data and rewiring workflows so local-memory becomes authoritative.

## Goals
- Deliver an automated migration that copies all relevant Byterover memories into local-memory with accurate domains, tags, and metadata.
- Ensure runtime prompts, slash commands, and consensus storage use local-memory first and automatically persist any Byterover fallbacks.
- Provide operators with documentation and evidence so they can rerun the migration, audit results, and troubleshoot conflicts.

## Non-Goals
- Replacing Byterover infrastructure or its MCP APIs.
- Providing a GUI for browsing or editing local-memory entries.
- Covering non-Spec Kit knowledge domains beyond governance, spec-tracker, impl-notes, infra-ci, and docs-ops.

## User Stories
- As an operator, I can run a migration command that imports all Byterover memories into local-memory and review a summary report of what changed.
- As an engineer invoking `/spec-plan` or `/spec-tasks`, I receive context from local-memory without waiting on Byterover, and any fallbacks automatically populate local-memory for the next run.
- As an auditor, I can inspect evidence logs showing migration results, counts per domain, and any conflicts resolved during sync.

## Requirements
1. Provide a CLI command or script with dry-run and apply modes that fetches Byterover entries, normalises them, and writes to local-memory.
2. Generate machine-readable (JSON) and human-readable summaries under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-010/` each time the migration runs.
3. Update Codex CLI/TUI flows (slash commands, consensus verdict storage, Spec Ops hooks) to read from local-memory, invoking Byterover only when local results are missing and persisting those fallbacks.
4. Document the execution workflow (prerequisites, commands, verification, rollback) and ensure SPEC tracker + lint are updated.

## Success Metrics
- 100% of Byterover memories in the target domains exist in local-memory after migration, with zero skipped entries.
- Slash command context hydration operates without Byterover access when local-memory already has the required entries.
- Nightly drift detector (T15) reports clean state after migration.

## Open Questions
- How should conflicts be handled if local-memory already contains an entry with the same slug/ID but divergent content?
- What credentials or configuration are required to run the Byterover MCP client in CI vs local development?
- Should the migration script support selective domain/spec filters for partial re-runs?
