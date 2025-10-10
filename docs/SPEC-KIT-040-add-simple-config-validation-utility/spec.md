# Spec: Simple Config Validation Utility (T48)

## Context
Created via /new-spec to test full pipeline execution. Provides CLI command to validate Codex config files.

## Objectives
1. Implement `codex config validate` command
2. Check TOML syntax, structure, required keys
3. Validate enum values
4. Report errors with severity levels

## Acceptance Criteria
- Command validates config.toml files
- Exits 0 for valid, 1 for errors
- JSON output mode available
- Integration tests pass

## Tasks
See tasks.md for breakdown.
