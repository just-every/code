# Ensemble Validation Run Checklist

Goal: execute `/spec-implement` with the GPT-5-Codex ⊕ Claude 4.5 ensemble and capture full evidence that all four agents (Gemini, Claude, GPT-5-Codex, GPT-5) participated.

## Preconditions
- SPEC has Plan/Tasks consensus with prompt versions recorded in local-memory.
- Guardrail baseline (`scripts/spec_ops_004/spec_auto.sh <SPEC-ID> --from plan`) has been executed successfully.
- Local-memory contains stage entries for prior runs; Byterover fallback disabled.

## Steps
1. Trigger `/spec-ops-implement <SPEC-ID>` to lock the workspace and collect telemetry.
2. Run `/spec-implement <SPEC-ID> [goal]` in the TUI, ensuring the composer prompt shows the expected `PROMPT_VERSION` strings.
3. Confirm Gemini, Claude, GPT-5-Codex, and GPT-5 outputs land in local-memory with matching prompt versions.
4. After consensus, verify `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/<SPEC-ID>/` contains the new JSON verdict with the correct `prompt_version` and agent metadata.
5. Capture local-memory export (`code local-memory export --output tmp/memories.jsonl`) for auditing.
6. Update SPEC.md task row with evidence references.

## Artifacts
- Guardrail telemetry JSON/log (implement stage).
- Consensus verdict JSON with SHA-256 recorded in local-memory summary.
- Local-memory export snippet showing all agent responses and prompt versions.
