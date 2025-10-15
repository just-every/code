**SPEC-ID**: SPEC-KIT-075-add-webhook-notification-system-for
**Feature**: Webhook Notification System for Task Completion Events (v2)
**Status**: Backlog
**Created**: 2025-10-15
**Branch**: feat/spec-auto-telemetry
**Owner**: Code

**Context**: `/spec-auto` pipelines emit telemetry but still rely on polling to learn when tracked tasks reach `Done`. Prior SPEC-KIT-065 introduced pipeline-level webhooks; this iteration focuses on durable task transition notifications with signed payloads, replay tooling, and bounded guardrail overhead aligned with telemetry schema v1.

---

## User Scenarios

### P1: DevOps engineer receives task completion webhooks

**Story**: As a DevOps engineer, I want Spec Kit to POST signed webhooks when high-priority tasks reach `Done` so downstream automation (deployments, ChatOps) can proceed without manual polling.

**Priority Rationale**: Unblocks automation that currently waits 20–30 minutes for manual confirmation.

**Testability**: Configure subscription in `config.toml`, run demo SPEC, assert webhook arrives within 5 s with valid HMAC signature and telemetry links.

**Acceptance Scenarios**:
- Given a subscription filtering `status = Done`, when `/spec-auto SPEC-KIT-075` completes a task, then a signed webhook posts to the endpoint with telemetry summary and evidence links.
- Given the endpoint returns HTTP 500 twice, when the spool retries, then delivery succeeds on third attempt and evidence logs show retry metadata.
- Given signature verification fails, when receiver rejects payload, then replay after secret rotation succeeds with new signature.

### P2: Engineering lead monitors review progress

**Story**: As an engineering lead, I want optional alerts when tasks enter `In Review` so I can prioritize approvals.

**Priority Rationale**: Provides visibility while allowing per-subscription opt-in.

**Testability**: Enable `include_status = ["In Review"]`, promote a task, ensure webhook includes consensus status and reviewer checklist link.

**Acceptance Scenarios**:
- Given `include_status` contains `In Review`, when `/spec-tasks` moves a task to `In Review`, then webhook arrives with consensus metadata and pending reviewers.

### P3: Operator replays failed deliveries

**Story**: As an operator, I want to replay failed task notifications so downstream systems stay in sync after outages.

**Priority Rationale**: Keeps evidence accurate without rerunning the full pipeline.

**Testability**: Force endpoint outage, verify dead-letter entry, replay via `/spec-notify --replay`, confirm payload delivered and evidence annotated.

**Acceptance Scenarios**:
- Given a delivery is dead-lettered, when operator runs `/spec-notify --replay <id>`, then payload succeeds and evidence captures replay timestamp/user.
- Given replay exceeds retry budget, when operator replays again, then CLI surfaces warning to export payload manually.

---

## Edge Cases

- Secret rotation mid-run; ensure subsequent retries use updated secret.
- Duplicate task transitions (e.g., toggling `Done` ↔ `In Progress`) handled idempotently via delivery IDs.
- Network partition causing prolonged retries; bounded spool prevents guardrail starvation.
- Concurrent SPEC runs emitting identical tasks; worker pool isolates per SPEC.
- Telemetry schema version bump; payloads include schema version for compatibility checks.

---

## Requirements

### Functional Requirements

- **FR1**: Detect task status transitions (`Done`, optional `In Review`) and enqueue webhook jobs with telemetry + consensus metadata.
- **FR2**: Support per-subscription filtering, enable flag, secret reference, retry policy, dry-run, and optional pipeline events through `config.toml`.
- **FR3**: Generate HMAC-SHA256 signatures with timestamp + idempotency headers; provide verification snippets.
- **FR4**: Deliver asynchronously with exponential backoff, bounded concurrency, and dead-letter queue after `max_attempts`.
- **FR5**: Provide CLI/TUI tooling (`/spec-notify`) to validate, list, replay, and surface health status.

### Non-Functional Requirements

- **Performance**: ≤5 s p95 to first delivery; ≤200 ms synchronous guardrail overhead.
- **Security**: Secrets never written to disk; payload redaction for sensitive fields.
- **Scalability**: Handle ≥10 concurrent SPEC pipelines without queue saturation.
- **Reliability**: ≥99.5% successful delivery to reachable endpoints; dead-letter entries preserve replay context.

---

## Success Criteria

- Integration tests cover success, failure, and replay flows with evidence assertions.
- `/spec-notify --test` validates configuration and signatures prior to live mode.
- Telemetry + consensus metadata embedded in payload for all deliveries.
- Evidence footprint constrained (<5 MB per SPEC) with rotation policies documented.

---

## Evidence & Validation

**Acceptance Tests**: Detailed mapping to be produced in `docs/SPEC-KIT-075-add-webhook-notification-system-for/tasks.md` during `/tasks` stage; must include unit, integration, and replay scenarios.

**Telemetry Path**: `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-075-add-webhook-notification-system-for/`

**Consensus Evidence**: `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-075-add-webhook-notification-system-for/`

**Validation Commands**:
```bash
/speckit.plan SPEC-KIT-075-add-webhook-notification-system-for
/speckit.tasks SPEC-KIT-075-add-webhook-notification-system-for
/speckit.implement SPEC-KIT-075-add-webhook-notification-system-for
/speckit.auto SPEC-KIT-075-add-webhook-notification-system-for
/speckit.status SPEC-KIT-075-add-webhook-notification-system-for
/spec-notify --test SPEC-KIT-075-add-webhook-notification-system-for
```

---

## Clarifications

### 2025-10-15 - Initial Spec Creation

**Clarification needed**: Finalize dead-letter retention window and operator escalation path.

**Resolution**: Pending design review with platform + security (default assumption 7 days retention).

**Updated sections**: None yet — track outcome in future revisions.

---

## Dependencies

- Telemetry schema v1 artifacts and consensus metadata emitted by `/spec-auto`.
- Guardrail command hooks in `scripts/spec_ops_004/` for integrating spool triggers.
- Rust crates for async HTTP, signing (`tokio`, `reqwest`, `hmac`, `sha2`, `serde_json`, persistence layer).
- Configuration management for secrets via environment variables (`scripts/env_run.sh`).

---

## Notes

- Multi-agent synthesis combined Gemini reliability targets, Claude operator workflows, and Code guardrail constraints; pipeline events remain optional per subscription.
- Ensure SPEC-KIT-065 documentation references this SPEC once implementation supersedes the earlier prototype.

