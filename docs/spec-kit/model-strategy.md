# Spec Kit Model Strategy

This document records the canonical mapping between Spec Kit command stages and
the language models they invoke. Update this file whenever models change so the
consensus guardrails and offline fallbacks stay aligned.

## Active Model Lineup (September 2025)

| Stage / Command | Research Agent | Synthesiser | Arbiter / Executor | Guardrail Default | Offline Fallback |
| --- | --- | --- | --- | --- | --- |
| `/spec-plan` | `gemini-2.5-pro` (thinking budget 0.3) | `claude-opus-4-1-20250805` | `gpt-5` (auto router) | `gpt-5-mini` | `rstar-l3` |
| `/spec-tasks` | `gemini-2.5-flash` → escalate to `gemini-2.5-pro` on `--deep-research` | `claude-opus-4-1` → fallback `claude-sonnet-4-20250514` | `gpt-5` | `gpt-5-mini` | `rstar-l3` |
| `/spec-implement` | `gemini-2.5-pro` | `claude-opus-4-1` | `gpt-5-pro` (Thinking mode) | `o4-mini-high` | `qwen2.5-coder-32b` |
| `/spec-validate` | `gemini-2.5-pro` | `claude-opus-4-1` | `gpt-5` (auto Thinking) | `o4-mini-high` | `rstar-l3` |
| `/spec-audit` | `gemini-2.5-pro` | `claude-opus-4-1` | `gpt-5` (escalate to `gpt-5-pro` on degraded verdict) | `gpt-5-mini` | `rstar-l3` |
| `/spec-unlock` | `gemini-2.5-flash` | `claude-sonnet-4-20250514` | `gpt-5-mini` | `o4-mini-high` | `rstar-l3` |
| `/spec-ops-*` guardrails | n/a | n/a | `o4-mini-high` | `gpt-5-mini` | `rstar-l3` |
| `/spec-consensus` / `/spec-auto` gating | consumes all three artefacts | — | `gpt-5` → `gpt-5-pro` on retries | — | `rstar-l3` |

### Escalation Rules

- **Consensus degraded** (`missing_agents` or conflicts) ⇒ rerun stage with
  `gemini-2.5-pro` (thinking budget 0.6) and `gpt-5-pro`.
- **Thinking budget exhausted** ⇒ promote `gemini-2.5-flash` to Pro and log the
  retry in consensus metadata.
- **Guardrail parsing failure** ⇒ retry with `o4-mini-high`; if still failing,
  escalate to `gpt-5-mini` and tag verdict with `guardrail_escalated=true`.
- **Offline mode** ⇒ switch research/synth/arbiter to `rstar-l3` and
  implementation tasks to `qwen2.5-coder-32b`; mark verdict JSON with
  `"offline": true`.

### Prompt Metadata Requirements

Every agent response used for consensus must include:

```json
{
  "model": "<provider-model-id>",
  "model_release": "YYYY-MM-DD",
  "reasoning_mode": "fast|thinking|auto",
  "consensus": { "agreements": [], "conflicts": [] }
}
```

The consensus checker rejects artefacts missing these fields or reporting a
model outside the table above.

### Validation Checklist

- Integration tests cover degraded consensus, thinking-budget retries, and
  guardrail parsing parity (`o4-mini-high` vs `gpt-5-mini`).
- `/spec-auto` run summaries include the chosen model IDs so ops can audit cost
  and reliability.
- Cost alerts fire when `gpt-5-pro` or `gemini-2.5-pro` Thinking mode exceeds
  the configured daily budget.

