# PRD: Webhook Notification System for Task Completion Events (SPEC-KIT-075-add-webhook-notification-system-for)

**SPEC-ID**: SPEC-KIT-075-add-webhook-notification-system-for
**Status**: Draft
**Created**: 2025-10-15
**Author**: Multi-agent consensus (gemini-2.5-pro, claude-4.5-sonnet, gpt-5-codex)

---

## Problem Statement

**Current State**: The Spec Kit automation platform orchestrates multi-agent pipelines via `/spec-auto` (Plan → Tasks → Implement → Validate → Audit → Unlock) and emits telemetry v1 artifacts under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/`. While earlier work laid groundwork for stage and pipeline notifications, there is still no focused, reliable push signal tied to task completion transitions recorded in `SPEC.md` and per-SPEC `tasks.md` files.

**Pain Points**:
- Downstream automations (ChatOps, deployment gates, follow-up validations) wait 20–30 minutes for humans to poll the Tasks table or TUI.
- Operators maintain brittle polling scripts around `/spec-status` and evidence folders, leading to missed or duplicated updates.
- Payloads lack consensus metadata and telemetry pointers, making it hard to correlate a task completion with pipeline evidence.

**Impact**: Lack of proactive, task-centric notifications increases lead time for dependent work, introduces operational toil, and weakens integration with observability/CI systems. A standardized, signed webhook centered on task transitions will reduce MTTR, improve coordination, and align with SPEC-OPS-004 guardrails and the `feat/spec-auto-telemetry` branch outputs.

According to Byterover memory layer, SPEC-KIT-065 delivered a broader webhook prototype; this new spec narrows scope to task lifecycle durability while reusing telemetry contracts.

---

## Target Users & Use Cases

### Primary User: DevOps / Platform Engineers

**Profile**: Own CI/CD automation, observability hooks, and guardrail operations.

**Current Workflow**: Trigger `/spec-auto`, then poll Tasks table or evidence folders until high-priority tasks reach `Done`.

**Pain Points**: Manual polling, delayed awareness of regressions, and lack of replay tooling when endpoints fail.

**Desired Outcome**: Configure subscriptions once in `config.toml` and receive signed HTTP POST notifications with telemetry links for subscribed task events.

### Secondary User: Engineering Leads & TPMs

**Profile**: Monitor task burn-down and quality for active features.

**Use Case**: Receive Slack/task tracker updates when blocking tasks reach `In Review` or `Done`, including consensus status to assess residual risk.

### Tertiary User: Feature Developers

**Profile**: Run `/spec-auto` and context-switch until notified of completion.

**Use Case**: Receive timely alerts when their SPEC tasks finish or fail, with replayable audit trails to debug or re-trigger dependent work.

---

## Goals

### Primary Goals

1. **Task-centric push notifications**: Deliver signed webhooks for task lifecycle transitions (focus on `Done`, configurable for `In Review`).
   **Success Metric**: ≥99% of eligible task transitions emit a webhook within 5 seconds of telemetry availability; ≥99.5% delivered within retry policy.

2. **Security & authenticity**: Ensure receivers can verify payload integrity without exposing secrets.
   **Success Metric**: 100% of deliveries signed with HMAC-SHA256 using per-subscription secrets; zero secrets stored in repo or plain-text logs.

3. **Operator-friendly configuration & replay**: Allow teams to enable, dry-run, and replay deliveries without rerunning pipelines.
   **Success Metric**: Configure and dry-run a subscription in ≤5 minutes; ≥95% of dead-lettered deliveries replay successfully within 24 hours of fix.

### Secondary Goals

- Align payload schema with telemetry v1 and consensus metadata so notifications can be correlated with `/spec-auto` evidence.
- Maintain compatibility with SPEC-OPS-004 guardrails and prior webhook infrastructure while emphasizing task completion coverage.

---

## Non-Goals

**Explicitly Out of Scope**:
- Hosted notification UI; management remains CLI/TUI driven.
- Inbound webhooks or external triggers that start `/spec-auto`.
- Advanced auth beyond shared-secret HMAC in v1 (mTLS/OAuth evaluated later).
- Real-time streaming of every agent step; scope is task transitions and optional pipeline summaries.

**Rationale**: Keep v1 simple, secure, and compatible with air-gapped environments while maximizing integration readiness.

---

## Scope & Assumptions

**In Scope**:
- Outbound HTTPS POST webhooks for task transitions observed in `SPEC.md` Tasks table and per-spec `tasks.md` (focus on `Done`; optional `In Review`).
- Optional events: `spec-auto.pipeline.completed` and `spec-auto.stage.completed` to preserve continuity with SPEC-KIT-065.
- Durable spool with exponential backoff, jitter, and idempotency keys; asynchronous workers so guardrails remain non-blocking.
- Evidence logging for each attempt under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/<SPEC-ID>/webhooks/`.
- CLI/TUI helpers to test (`--test`), list, and replay deliveries.

**Assumptions**:
- Branch `feat/spec-auto-telemetry` continues to emit telemetry v1 artifacts and consensus metadata.
- Secrets provided via environment variables and surfaced through `scripts/env_run.sh`; no plaintext secrets in git.
- Outbound network access is explicitly enabled; feature defaults to off in locked-down environments.

**Constraints**:
- Maintain ≤200 ms synchronous overhead per guardrail stage.
- Small evidence footprint (≤5 MB per SPEC) with rotation/retention policies.
- Payload schema versioned to avoid breaking downstream integrations.

---

## Functional Requirements

| ID | Requirement | Acceptance Criteria | Priority |
|----|-------------|---------------------|----------|
| FR1 | Emit task transition events | When a tracked task transitions to `Done` (and optionally `In Review`), enqueue a webhook job with telemetry snapshot, consensus metadata, and evidence references. | P1 |
| FR2 | Subscription filters & config | `config.toml` supports per-subscription filters (events, statuses, SPEC IDs, task IDs/prefixes), enable flag, secrets, retry policy, and dry-run mode. | P1 |
| FR3 | Secure payload signing | Payloads signed via HMAC-SHA256 and include timestamp, delivery ID, schema version, and signature headers; verification snippet documented. | P1 |
| FR4 | Async delivery & retries | Background workers deliver with exponential backoff + jitter, per-request timeout, bounded concurrency, and dead-letter queue after `max_attempts`. | P1 |
| FR5 | Operational tooling | `/spec-notify --test|--list|--replay` (CLI/TUI) validates config, surfaces queue/dead-letter status, and replays delivery IDs without rerunning pipelines. | P2 |

---

## Non-Functional Requirements

| ID | Requirement | Target Metric | Validation Method |
|----|-------------|---------------|-------------------|
| NFR1 | Delivery latency | ≤5 s p95 from event detection to first delivery attempt; ≤30 s p99 including retries. | Synthetic SPEC with 50 task transitions; compare timestamps. |
| NFR2 | Reliability | ≥99.5% delivery success to reachable endpoints; remaining events recorded in dead-letter with replay guidance. | Fault-injection against mock endpoints in integration tests. |
| NFR3 | Guardrail overhead | <200 ms additional synchronous time per stage; guardrails never block on network I/O. | Compare `/spec-ops-plan` timings with feature toggled. |
| NFR4 | Security | Zero secret leakage to logs/evidence; signature verification tests cover happy/attack paths. | Static analysis + unit/integration tests. |
| NFR5 | Evidence footprint | Webhook evidence <5 MB per SPEC with FIFO rotation. | Evidence audit script across staged runs. |

---

## User Experience

**Key Workflows**:

### Workflow 1: Configure and Dry-Run

**Steps**:
1. Add subscription to `config.toml` with event filters, target URL, and secret env var.
2. Run `/spec-notify --test SPEC-KIT-075 --subscription chatops`; when `dry_run=true`, payload logged only.
3. Receiver verifies signature using provided snippet.

**Success Path**: Config validation passes; sample payload logged to evidence; receiver signature check succeeds.

**Error Paths**:
- Missing secret → config validation fails with actionable error.
- Invalid URL → subscription disabled with warning surfaced in TUI/CLI.

### Workflow 2: Receive Task Completion Updates

**Steps**:
1. Run `/spec-auto SPEC-KIT-075` or update tasks via `/spec-tasks`.
2. Guardrail detects task transition and enqueues webhook job.
3. ChatOps endpoint receives signed payload with telemetry links.
4. Operator optionally acknowledges via TUI or downstream system.

**Error Paths**:
- Endpoint unreachable → retries escalate to dead-letter with replay guidance.
- Signature mismatch → rotate secret, replay delivery via CLI.

### Workflow 3: Replay Failed Deliveries

**Steps**:
1. Inspect dead-letter queue with `/spec-notify --list --failed`.
2. Resolve endpoint issue and replay targeted delivery IDs.
3. Confirm evidence logs capture replay metadata and success status.

**Error Paths**:
- Replay fails repeatedly → escalate to guardrail warning and prompt manual export.

---

## Dependencies

**Technical**:
- Telemetry schema v1 artifacts in `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/`.
- Guardrail scripts under `scripts/spec_ops_004/commands/` to hook emissions after telemetry writes.
- Rust crates (`tokio`, `reqwest`, `hmac`, `sha2`, `serde_json`, `sled` or equivalent persistence) for async delivery.
- Existing TUI/CLI surfaces for guardrail warnings.

**Organizational**:
- Secret management owner to provision/rotate webhook secrets.
- Coordination with observability/ChatOps teams consuming payloads.

**Data**:
- No PII; payloads include SPEC/task metadata, consensus summary, and evidence references.

---

## Risks & Mitigations

| Risk | Impact | Probability | Mitigation | Owner |
|------|--------|-------------|------------|-------|
| Endpoint outages create backlog | High | Medium | Bounded spool, retries, dead-letter queue, replay tooling. | Platform |
| Schema drift vs telemetry | Medium | Medium | Versioned payload; contract tests against telemetry snapshots. | Spec Ops |
| Secret leakage through logs | High | Low | Redact sensitive headers, env-only secrets, security review before launch. | Security |
| Concurrency regressions under parallel SPECs | Medium | Medium | Limit worker concurrency; load-test with ≥10 concurrent SPEC runs. | Platform |

---

## Success Metrics

**Launch Criteria**:
- Task completion webhooks validated end-to-end across success, failure, and replay scenarios.
- `/spec-notify --test` gated behind config validation and signature checks.
- Evidence directory captures attempt/replay metadata with bounded footprint.
- Security review confirms secret handling and signature strength.

**Post-Launch Metrics** (30-day window):
- p95 delivery time ≤5 s for production subscriptions.
- ≥2 downstream automations adopt the webhook feed.
- ≤1% of deliveries require replay across all SPECs.
- User satisfaction ≥4/5 via internal survey.

---

## Validation Plan

### Testing Strategy

1. **Unit Tests**: Cover payload construction, signature generation/verification, config parsing, retry policies.
2. **Integration Tests**: Run against mock HTTP servers covering success, timeout, and failure modes; assert evidence output.
3. **E2E Tests**: Execute demo SPEC with real spool delivering to local test endpoint; verify tasks transitioning trigger payloads.
4. **Performance Tests**: Stress test 50 task transitions + 10 concurrent SPEC runs; ensure latency and concurrency targets hold.

### Review Process

1. **PRD Review**: Platform, Spec Ops, Security stakeholders.
2. **Design Review**: Rust orchestrator maintainers + telemetry owners.
3. **Code Review**: Standard multi-reviewer workflow with guardrail CI.
4. **Security Review**: Confirm secret, signature, and replay handling before rollout.

---

## Multi-Agent Consensus

### PRD Quality Assessment

**Completeness**: Gemini emphasized coverage for task transitions and reliability metrics; Claude focused on operator tooling and configuration clarity; Code highlighted guardrail integration and evidence hygiene. Combined output meets template requirements with actionable scope.

**Clarity**: All requirements are measurable and map to telemetry artifacts; payload schema, retry behavior, and tooling responsibilities are explicit.

**Testability**: Acceptance criteria reference concrete commands (`/spec-notify`, `/spec-auto`) and measurable latency/reliability goals.

### Conflicts Resolved

- **Optional pipeline events**: Gemini proposed keeping stage/pipeline notifications; Claude preferred task-only scope. Resolution keeps pipeline events optional per subscription to maintain backwards compatibility while prioritizing task transitions.
- **Replay tooling placement**: Code advocated CLI-first replay, Gemini suggested API; consensus delivers CLI/TUI first with API as future enhancement.

Consensus achieved without outstanding blockers.

---

## Evidence & Telemetry

- PRD Creation Evidence: `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-075-add-webhook-notification-system-for/prd-consensus.json`
- Agent Outputs: `.code/agents/{gemini,claude,code}/SPEC-KIT-075/`
- Validation: Use `/speckit.analyze SPEC-KIT-075-add-webhook-notification-system-for` to confirm PRD ↔ spec alignment once available.

---

## Open Questions

1. **Dead-letter retention policy**: How long should failed deliveries remain replayable? **Impact**: Medium. **Blocker**: No — default to 7 days unless security requests shorter.
2. **Multi-tenant secrets**: Should different environments support distinct secrets per subscription? **Impact**: Medium. **Resolution Path**: Evaluate with security + devops during design review.

---

## Changelog

### 2025-10-15 - Initial PRD
- Created by multi-agent consensus.
- Refined scope to task-centric notifications, preserving optional pipeline events.
- Captured latency, reliability, and security targets aligned with telemetry v1.

