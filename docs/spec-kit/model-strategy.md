# Spec Kit Model Strategy

This document records the canonical mapping between Spec Kit command stages and
the language models they invoke. Update this file whenever models change so the
consensus guardrails and offline fallbacks stay aligned.

## Active Model Lineup (October 2025)

| Stage / Command | Research Agent | Synthesiser | Arbiter / Executor | Guardrails |
| --- | --- | --- | --- | --- |
| `/spec-plan` | `gemini-2.5-pro` (breadth + tool use; Flash only for quick pre-scan) | `claude-4.5-sonnet` | `gpt-5` (escalate with `--reasoning high` if consensus conflicts) | Layered: `gpt-5-codex` prefilter → `gpt-5` policy gate |
| `/spec-tasks` | `gemini-2.5-pro` | `claude-4.5-sonnet` | `gpt-5` (`--reasoning high` when task slices disagree) | `gpt-5-codex` → `gpt-5` |
| `/spec-implement` | `gemini-2.5-pro` (retrieve refs, APIs, prior art) | Code ensemble: `gpt-5-codex` ⊕ `claude-4.5-sonnet`; prose stays `claude-4.5-sonnet` | `gpt-5` (`--reasoning high`) signs off merges | `gpt-5-codex` → `gpt-5` |
| `/spec-validate` | `gemini-2.5-pro` (collect tests, logs, benchmarks) | `claude-4.5-sonnet` (tight, low-temp) | `gpt-5` (`--reasoning high`) issues pass/fail with evidence cites | `gpt-5-codex` → `gpt-5` |
| `/spec-audit` | `gemini-2.5-pro` (source crawl, citation mapping) | `claude-4.5-sonnet` | `gpt-5` (`--reasoning high`) enforces ≥80 % claim-to-source coverage | `gpt-5-codex` → `gpt-5` |
| `/spec-unlock` | `gemini-2.5-pro` | `claude-4.5-sonnet` (divergent options) | `gpt-5` (`--reasoning high`) | `gpt-5-codex` → `gpt-5` |
| `/spec-ops-*` guardrails | n/a | n/a | n/a | `gpt-5-codex` fails fast; `gpt-5` adjudicates policy |
| `/spec-consensus` / `/spec-auto` gating | consumes all three artefacts | — | `gpt-5` (retry with `--reasoning high`) | Guardrail stack reused |

### October 2025 Update

- Claude Sonnet 4.5 is now the default synthesiser across `/spec-*`, offering stronger coding/agent performance and longer autonomous sessions than earlier Claude releases.
- `/spec-implement` runs a two-vote code ensemble (`gpt-5-codex` alongside Claude Sonnet 4.5) before the arbiter signs off, combining OpenAI and Anthropic tool stacks for diffs.
- `gpt-5` remains the universal arbiter; when more depth is required we simply rerun the stage with `--reasoning high` instead of swapping to a different model.
- Guardrails execute as a layered pass: `gpt-5-codex` provides the fast prefilter and `gpt-5` performs the final policy/claims adjudication.

### Escalation Rules

- **Consensus degraded** (`missing_agents` or conflicts) ⇒ rerun stage with
  `gemini-2.5-pro` (thinking budget 0.6) and reissue the arbiter call with `gpt-5 --reasoning high`.
- **Thinking budget exhausted** ⇒ promote `gemini-2.5-flash` to Pro and log the
  retry in consensus metadata.
- **Guardrail parsing failure** ⇒ retry with `gpt-5-codex`; if still failing,
  escalate to `gpt-5` (low effort) and tag verdict with `guardrail_escalated=true`.
- **Offline mode** ⇒ use on-prem fallbacks documented in operational runbooks; record `"offline": true` in consensus metadata when invoked.

### Prompt Metadata Requirements

Every agent response used for consensus must include:

```json
{
  "model": "<provider-model-id>",
  "model_release": "YYYY-MM-DD",
  "prompt_version": "YYYYMMDD-stage-suffix",
  "reasoning_mode": "fast|thinking|auto",
  "consensus": { "agreements": [], "conflicts": [] }
}
```

The consensus checker rejects artefacts missing these fields or reporting a
model outside the table above.

Prompt versions follow a `YYYYMMDD-stage-suffix` convention (for example
`20251002-plan-a`) and live in `docs/spec-kit/prompts.json`. Update the version
string whenever a prompt changes in a way that affects downstream consensus or
evidence interpretation.

### Validation Checklist

- Integration tests cover degraded consensus, thinking-budget retries, and
  guardrail parsing parity (`gpt-5-codex` vs `gpt-5`).
- `/spec-auto` run summaries include the chosen model IDs so ops can audit cost
  and reliability.
- Cost alerts fire when `gpt-5 --reasoning high` or `gemini-2.5-pro` thinking budgets exceed
  the configured daily threshold.
