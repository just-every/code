# PRD: Webhook Notification System for Task Completion Events (SPEC-KIT-065-add-webhook-notification-system-for)

**SPEC-ID**: SPEC-KIT-065-add-webhook-notification-system-for
**Status**: Draft
**Created**: 2025-10-14
**Author**: Multi-agent consensus (gemini, claude, code)

---

## Problem Statement

Spec-Kit orchestrates multi-agent development through the `/spec-auto` pipeline (Plan → Tasks → Implement → Validate → Audit → Unlock) and records evidence under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/`. Today the framework offers no push-based signal when stages complete, pipelines succeed or fail, or tracked tasks enter `Done`. Teams poll `/spec-status` or scrape telemetry folders, delaying follow-up actions such as ChatOps notifications, CI/CD gating, or budget controls. This polling friction slows the 40–60 minute pipeline loop, hides failures until humans recheck, and blocks deeper integration with observability stacks.

---

## Target Users & Use Cases

### Primary User: DevOps / Platform Engineers

- Profile: Own CI/CD automation, observability tooling, and integrations around Spec-Kit.
- Current Workflow: Trigger `/spec-auto` and run ad-hoc scripts to poll evidence directories until a terminal state appears.
- Pain Points: Brittle polling logic, delayed downstream actions, no standard event schema.
- Desired Outcome: Configure webhooks once in `config.toml` and receive signed HTTP POST notifications with rich context for every subscribed event.

### Secondary User: Engineering Leads & TPMs

- Profile: Monitor feature progress, cost, and quality across teams.
- Use Case: Receive Slack or dashboard updates when stages finish, consensus conflicts emerge, or budgets breach thresholds.
- Desired Outcome: Near real-time visibility without manual status checks; actionable payloads that link directly to evidence.

### Tertiary User: Feature Developers

- Profile: Kick off `/spec-auto` runs and context-switch to other work.
- Use Case: Get notified when their SPEC completes or stalls so they can respond immediately.
- Desired Outcome: Personal or channel alerts with failure summaries and evidence paths.

---

## Goals

### Primary Goals

1. **Reliable push notifications for pipeline milestones**  
   Success Metric: ≥99% of subscribed events emit a webhook within 5 seconds of telemetry availability; ≥99.5% delivered within retry policy.

2. **Secure, verifiable payloads with sufficient context**  
   Success Metric: 100% of deliveries signed via HMAC-SHA256 and include telemetry v1 core fields, event envelope, evidence links, and consensus metadata when available.

3. **Operator-friendly configuration, testing, and redelivery**  
   Success Metric: Configure and test a webhook in ≤5 minutes; failed deliveries replayable via CLI/TUI without re-running pipelines.

### Secondary Goals

- Align webhook schema with telemetry-driven alerts (telemetry-tasks.md Task 4.3) so anomaly notifications can share infrastructure.
- Capture delivery evidence under the existing Spec-Ops tree for auditability and postmortems.

---

## Non-Goals

- Building a hosted notification service or UI dashboard—delivery remains repo-local and CLI/TUI driven.
- Inbound webhooks or external triggers that start `/spec-auto` runs.
- Advanced auth schemes beyond shared-secret HMAC (e.g., OAuth, mTLS) for v1.
- Real-time streaming of every agent step; scope is stage completion, pipeline status, and tracked task transitions.

---

## Scope & Assumptions

### In Scope

- Outbound HTTPS POST webhooks for:
  - Stage completions (success, failure, halt) across Plan/Tasks/Implement/Validate/Audit/Unlock.
  - Pipeline completion (success or failure after automated retries).
  - Tracked task transitions to `Done` in `SPEC.md` or per-SPEC `tasks.md`.
  - Optional alerts (consensus conflicts, cost threshold breaches) reusing telemetry anomaly signals.
- Per-subscription configuration: filters, secrets, retry policy, dry-run mode, enable/disable toggle.
- Durable spool with exponential backoff, jitter, and idempotency keys; asynchronous workers so guardrails never block on network I/O.
- Evidence logging for every attempt under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/<SPEC-ID>/webhooks/`.
- CLI/TUI helpers (`/spec-notify`) for testing, listing, and replaying deliveries.

### Assumptions

- Branch `feat/spec-auto-telemetry` continues to emit telemetry v1 artifacts for every stage and exposes consensus metadata.
- Outbound network is permitted when explicitly enabled by operators; defaults remain safe for offline or air-gapped use.
- Secrets supplied through environment variables (no plaintext in git) and available to guardrail scripts via `scripts/env_run.sh`.

### Constraints

- Delivery subsystem must remain optional and default-off to respect restricted environments.
- Spool size and concurrency bound to avoid starving guardrails or exceeding evidence footprint budgets.
- Schema changes remain backward-compatible; payloads carry `schemaVersion` for future evolution.

---

## Functional Requirements

| ID | Requirement | Acceptance Criteria | Priority |
|----|-------------|---------------------|----------|
| FR1 | Event emission coverage | Stage, pipeline, and task completion events enqueue webhook jobs with event envelope and telemetry snapshot. | P1 |
| FR2 | Subscription management | `config.toml` supports N subscriptions with URL, secret env var, event/stage/status filters, enable flag, retry policy, and dry-run toggle. | P1 |
| FR3 | Secure delivery | Payloads signed with HMAC-SHA256 using per-subscription secret; headers include timestamp and signature. Verification example documented. | P1 |
| FR4 | Asynchronous spool & retries | Delivery executed via background workers with exponential backoff (configurable attempts, jitter) and per-attempt timeout; guardrails record success/failure without blocking. | P1 |
| FR5 | Idempotency & logging | Each delivery carries unique ID and idempotency key; attempts logged under evidence tree with request/response metadata. | P1 |
| FR6 | CLI/TUI tooling | `/spec-notify --test|--list|--replay` surfaces configuration validation, queued deliveries, and targeted replays without pipeline reruns. | P2 |
| FR7 | Telemetry integration | Payload embeds telemetry v1 fields plus consensus metadata (`model`, `model_release`, `reasoning_mode`, `consensus.status`); optional anomaly signals appended when triggered. | P1 |
| FR8 | Failure tolerance | Exhausted retries move payload to durable dead-letter folder with actionable error message; pipeline execution proceeds. | P1 |
| FR9 | Config validation | Startup validation fails fast on missing secrets, malformed URLs, or invalid filters; dry-run mode logs payload without network call. | P2 |

---

## Non-Functional Requirements

| ID | Requirement | Target Metric | Validation Method |
|----|-------------|---------------|-------------------|
| NFR1 | Emit latency | ≤5s p95 from telemetry availability to first delivery attempt; ≤30s p99 including retries. | Synthetic SPEC run with 50 staged events, measured via timestamps. |
| NFR2 | Delivery reliability | ≥99.5% successful delivery to reachable endpoints within policy; remaining events present in dead-letter with replay instructions. | Fault-injection against mock endpoint with intermittent failures. |
| NFR3 | Guardrail overhead | <200 ms additional wall time per stage when webhooks enabled (async spool). | Benchmark `/spec-ops-plan` with feature toggled on/off. |
| NFR4 | Security & secrecy | Zero secrets persisted to logs or repo; signature validation passes unit + integration tests. | Static scanning + unit tests verifying header computation. |
| NFR5 | Evidence footprint | Webhook logs stay under 5 MB per SPEC with automatic rotation (FIFO) once threshold reached. | Evidence audit script asserting size cap. |
| NFR6 | Concurrency safety | Worker pool obeys configurable `max_concurrency`; no dropped events under 10 parallel SPEC runs. | Stress test with concurrent pipelines. |

---

## User Experience

### Workflow 1: Configure and Dry-Run a Webhook
1. Operator adds subscription to `config.toml` (see Configuration) and exports secret env var.  
2. Run `/spec-notify --test SPEC-KIT-065 --subscription chatops-slack` to emit a signed sample payload without touching pipelines.  
3. Receiver verifies signature; CLI reports success or validation errors.

### Workflow 2: Receive Stage & Pipeline Updates
1. Run `/spec-auto SPEC-KIT-065` with webhooks enabled.  
2. On each stage completion the spool queues payloads asynchronously.  
3. ChatOps bot posts structured summary (stage, status, evidence links).  
4. On pipeline completion a consolidated payload publishes outcome, total cost/time, and outstanding follow-ups.

### Workflow 3: Investigate and Replay Failures
1. Developer inspects `/spec-notify --list SPEC-KIT-065` to review deliveries and failure reasons.  
2. They run `/spec-notify --replay SPEC-KIT-065 --delivery-id <uuid>` after fixing endpoint.  
3. Successful replay updates evidence logs and removes the item from dead-letter queue.  
4. Persistent failures raise warning banners in TUI until resolved.

**Error Paths**  
- Endpoint unreachable → retries follow policy, then payload shunted to dead-letter with mitigation hint.  
- Signature mismatch at receiver → documentation instructs secret rotation and replay.  
- Spool saturation → system emits warning, drops oldest pending after configurable threshold while preserving telemetry.

---

## Configuration

```toml
[spec_ops.webhooks]
enabled = true
max_concurrency = 4
spool_dir = "docs/SPEC-OPS-004-integrated-coder-hooks/evidence/webhooks"
# Implementation creates per-SPEC subdirectories: webhooks/SPEC-KIT-065-.../, webhooks/SPEC-KIT-070-.../, etc.

[[spec_ops.webhooks.subscriptions]]
id = "chatops-slack"
url = "https://hooks.slack.com/services/T000/B000/XXXX"
secret_env = "SPECKIT_WEBHOOK_SECRET_SLACK"
active = true
dry_run = false
include_events = ["spec-auto.stage.completed", "spec-auto.pipeline.completed"]
include_stages = ["Plan", "Tasks", "Implement", "Validate", "Audit", "Unlock"]
include_status = ["success", "failure", "halt"]
max_attempts = 10
initial_backoff_ms = 1000
backoff_factor = 2.0
jitter_ms = 250
per_request_timeout_ms = 5000

[[spec_ops.webhooks.subscriptions]]
id = "task-tracker"
url = "https://tracker.example/api/spec-kit/webhooks"
secret_env = "SPECKIT_WEBHOOK_SECRET_TRACKER"
active = true
dry_run = true
include_events = ["spec-kit.task.completed"]
max_attempts = 5
per_request_timeout_ms = 3000
```

Environment overrides:

```bash
export SPECKIT_WEBHOOKS_ENABLED=1
export SPECKIT_WEBHOOK_SECRET_SLACK="..."
```

---

## Payload Schema

Headers:
- `X-SpecKit-Event` — event type (e.g., `spec-auto.stage.completed`).
- `X-SpecKit-Id` — UUID v4 delivery identifier.
- `X-SpecKit-Request-Timestamp` — seconds since epoch.
- `X-SpecKit-Signature` — `sha256=<hex(hmac(secret, body))>`.
- `Content-Type` — `application/json`.

Body example:

```json
{
  "schemaVersion": "1.0",
  "event": {
    "type": "spec-auto.stage.completed",
    "status": "success",
    "stage": "Plan",
    "idempotency_key": "SPEC-KIT-065:plan:attempt-1"
  },
  "spec": {
    "id": "SPEC-KIT-065-add-webhook-notification-system-for",
    "branch": "feat/spec-auto-telemetry"
  },
  "pipeline": {
    "attempt": 1,
    "maxAttempts": 3,
    "startedAt": "2025-10-14T10:00:00Z",
    "completedAt": "2025-10-14T10:05:06Z"
  },
  "telemetry": {
    "command": "spec-plan",
    "specId": "SPEC-KIT-065-add-webhook-notification-system-for",
    "sessionId": "abcd-1234",
    "timestamp": "2025-10-14T10:05:06Z",
    "artifacts": [
      { "path": "docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-065-add-webhook-notification-system-for/plan_2025-10-14.log" }
    ]
  },
  "consensus": {
    "status": "ok",
    "agents": [
      { "agent": "gemini", "model": "gemini-2.5-pro", "reasoning_mode": "research" },
      { "agent": "claude", "model": "claude-4.5-sonnet", "reasoning_mode": "synthesis" },
      { "agent": "code", "model": "gpt-5-codex", "reasoning_mode": "implementation" }
    ]
  },
  "task": {
    "id": "T61",
    "title": "Webhook notification system for task completion",
    "status": "Done"
  },
  "evidence": {
    "paths": [
      "docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-065-add-webhook-notification-system-for/plan_2025-10-14.log"
    ]
  }
}
```

Signature verification helper:

```python
expected = "sha256=" + hmac.new(secret, request.data, hashlib.sha256).hexdigest()
if not hmac.compare_digest(expected, request.headers["X-SpecKit-Signature"]):
    raise ValueError("invalid signature")
```

---

## Delivery Semantics

- **Idempotency**: Consumers deduplicate on `X-SpecKit-Id` and `event.idempotency_key`.
- **Retries**: Exponential backoff with jitter until `max_attempts`; HTTP 2xx stops retries, 4xx (except 429) recorded as terminal failure.
- **Timeouts**: Abort requests exceeding configured timeout; log latency for observability.
- **Ordering**: Best-effort per subscription; document expectation that downstream systems order by timestamp.
- **Dead-letter handling**: Failed payloads persisted with metadata (status codes, error) for manual replay.

---

## Dependencies

### Technical
- Telemetry artifacts and schema v1 definitions under `docs/SPEC-OPS-004-integrated-coder-hooks/`.
- Guardrail command scripts in `scripts/spec_ops_004/commands/` for stage lifecycle hooks.
- TUI components (`codex-rs/tui/src/chatwidget.rs`, `spec_status.rs`) for surfacing delivery status.
- Rust/Bash/Python helpers (`reqwest` or `curl`, `sha2`/`openssl`) for HTTP and HMAC support.

### Organizational
- Secret management owner defined (rotate env vars, maintain allowlists).
- Documentation updates in `docs/` and `SPEC.md` guidance for enabling webhooks.

### Data
- No PII introduced; payload limited to SPEC metadata and telemetry references.
- Evidence storage monitored by existing footprint tooling to prevent repository bloat.

---

## Risks & Mitigations

| Risk | Impact | Probability | Mitigation | Owner |
|------|--------|-------------|------------|-------|
| Endpoint outages cause backlog growth | High | Medium | Bounded spool size, circuit breaker warning, manual replay tooling. | Platform |
| Secret leakage via misconfigured logging | High | Low | Redact sensitive headers, lint for `secret_env` usage, security review before release. | Security |
| Payload schema drift vs telemetry updates | Medium | Medium | Version payloads (`schemaVersion`), document additive-change policy, add contract tests. | Spec Ops |
| Network-disabled environments break guardrails | Medium | Medium | Feature default-off, dry-run mode writes payloads locally without HTTP call. | Platform |
| Performance regression under concurrent SPEC runs | Medium | Medium | Concurrency limits, load testing prior to launch, monitor via telemetry dashboards. | Platform |

---

## Success Metrics

- p95 notification latency ≤5 s; p99 ≤30 s including retries.
- ≥90% of teams enabling webhooks adopt at least one downstream automation within 30 days.
- 100% of deliveries produce evidence artifacts (attempt history, final status).
- Dead-letter replay success rate ≥95% within 24 hours of operator action.

---

## Validation Plan

### Testing Strategy
- **Unit tests**: signature generation, config parsing, retry scheduler, idempotency key builder.
- **Integration tests**: loopback HTTP server verifying headers, dry-run mode, failure escalations.
- **E2E tests**: demo SPEC run exercising success, failure, and halt paths with webhook assertions.
- **Performance tests**: generate 100 staged events with 10 concurrent subscriptions; verify latency and spool limits.
- **Security tests**: static analysis ensuring secrets never logged; negative tests for signature tampering.

### Review Process
- PRD review with Spec-Kit maintainers, platform, and security stakeholders.
- Design review focusing on spool architecture, retry policy, TUI/CLI exposure, and telemetry alignment.
- Code review requiring at least two approvals with evidence of passing tests.
- Security review of secret handling, signature verification, and logging redaction.

---

## Multi-Agent Consensus

- **Agreement**: All agents prioritized outbound HTTPS webhooks, HMAC signatures, asynchronous delivery, and reuse of telemetry evidence paths.
- **Resolved Differences**: Gemini proposed fire-and-forget delivery; Claude and Code advocated retries and durable spooling. Consensus adopted retries with dead-letter support as baseline while keeping delivery optional. Claude suggested optional anomaly events (cost alerts, consensus conflicts) which are included as configurable extensions. Code proposed CLI replay tooling and evidence logging; consensus accepted to ensure auditability.
- **Open Considerations**: Future iterations may add UI management and advanced auth once baseline adoption measured.

---

## Implementation Notes (Non-Normative)

- Introduce `spec_ops/webhooks.rs` (or Bash/Python helper) invoked post-telemetry write; prefer Rust integration if existing guardrail hooks can call into compiled binary without blocking.
- Extend `/spec-status` to surface most recent webhook deliveries and outstanding failures.
- Document best practices for secret rotation and endpoint testing in `docs/SPEC-KIT-065-add-webhook-notification-system-for/tasks.md` once `/tasks` stage runs.
- Coordinate with telemetry schema updates to ensure optional anomaly fields map cleanly to webhook payload extensions.
