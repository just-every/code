# Spec: Automated Conflict Resolution Arbiter (T36)

## Context
- Consensus runner currently halts on conflicts and requires manual review before `/spec-auto` continues, increasing turnaround time and leaving telemetry without final verdicts.
- Guardrails already capture multi-agent artefacts (Gemini research, Claude synthesis, consensus synthesis) under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/<SPEC-ID>/`, but there is no automated arbiter step.
- Model strategy mandates GPT-5 (high reasoning) as the arbiter for consensus stages; automation must respect telemetry schema v1 and existing guardrail layering.

## Objectives
1. Automatically invoke a GPT-5 arbiter when consensus metadata reports conflicts or ties, using high reasoning mode and full artefact context.
2. Extend consensus artefacts and telemetry records with arbiter verdict metadata while remaining schema v1 compliant.
3. Provide operability hooks: feature flag, manual override with audit trail, degraded-run signalling, and cost telemetry visibility.

## Approach
- Extend consensus runner to inspect stage outputs (`consensus.conflicts`, `ties_pending`) and trigger a new arbiter call.
- Construct arbiter prompt bundle containing: Gemini research notes, Claude synthesis, previous arbiter verdicts, guardrail diagnostics, and current stage metadata.
- Persist arbiter response inside consensus synthesis (new `arbiter` block) and telemetry JSON (new `arbiter` object) with model metadata, rationale digest, escalation flags, and evidence pointers.
- Introduce retry budget (default 1 automatic retry) before marking run as degraded; surface CLI messaging summarizing outcome and evidence paths.
- Add manual override command/flag that records operator name, reason, and chosen verdict inside evidence tree and updates SPEC.md notes.
- Update documentation (CLAUDE.md, slash-commands) describing automation, feature flag (`SPEC_KIT_AUTOMATED_ARBITER`), and override process.

## Telemetry & Evidence
- Telemetry schema v1 remains authoritative; add `arbiter` object with required fields (`model`, `model_release`, `reasoning_mode`, `verdict`, `rationale_digest`, `escalated`, `retry_count`).
- Evidence directory per run stores: `arbiter_input_manifest.json`, `arbiter_verdict.json`, updated `consensus_synthesis.json`, and optional `override.json`.
- HAL telemetry summary should include arbiter outcome and retry counts to support governance reporting.

## Risks & Mitigations
- **Cost spikes:** GPT-5 high reasoning calls add cost; mitigate via feature flag rollout, telemetry dashboards, and per-run budget warnings.
- **Incomplete artefacts:** Missing inputs could degrade verdict quality; add checksum validation and fail fast with degraded status.
- **Telemetry drift:** Schema changes must be backwards compatible; coordinate with telemetry guard (T13) to update validators concurrently.
- **Operator trust:** Automated verdict must still allow manual override and transparent logging to maintain confidence.

## Open Questions
- Final retry budget and backoff strategy before marking `degraded`.
- Whether manual overrides require dual authorisation in production paths.
- How to expose arbiter cost metrics in existing governance dashboards.
