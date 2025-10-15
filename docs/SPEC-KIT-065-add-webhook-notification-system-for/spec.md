**SPEC-ID**: SPEC-KIT-065-add-webhook-notification-system-for
**Feature**: Webhook Notification System for Task Completion Events
**Status**: Backlog
**Created**: 2025-10-14
**Branch**: feat/spec-auto-telemetry
**Owner**: Code

**Context**: `/spec-auto` pipelines generate telemetry and evidence but lack push notifications. Teams poll `/spec-status` or evidence folders to learn when stages finish or tasks complete, delaying reactions and limiting integrations. This SPEC delivers signed, configurable webhooks that fire on stage, pipeline, and task completion events with retry, logging, and replay tooling.

---

## User Scenarios

### P1: DevOps engineer subscribes to pipeline events

**Story**: As a DevOps engineer, I want Spec-Kit to POST signed webhooks when `/spec-auto` stages finish so my CI/CD jobs can continue automatically.

**Priority Rationale**: Eliminates manual polling that currently delays deployments by 20–30 minutes per run.

**Testability**: Configure webhook in `config.toml`, run demo SPEC, assert POST arrives within 5 seconds and signature validates.

**Acceptance Scenarios**:
- Given a subscription listening for `spec-auto.pipeline.completed`, when `/spec-auto SPEC-KIT-065` succeeds, then a webhook with status `success` arrives and contains evidence links.
- Given the same subscription, when the pipeline fails validation, then the webhook payload reports `status: failure` and references the failing stage telemetry.
- Given the endpoint is unreachable, when retries are exhausted, then the dead-letter log contains the payload and `/spec-notify --list` reports failure.

### P2: Engineering lead tracks consensus conflicts

**Story**: As an engineering lead, I want webhook alerts when consensus disagreements exceed the configured threshold so I can intervene quickly.

**Priority Rationale**: Enables proactive response to risky SPECs without monitoring dashboards continuously.

**Testability**: Simulate a consensus conflict during `/spec-auto`, ensure webhook payload includes `consensus.status = conflict` and is routed only to subscriptions that opt into anomaly events.

**Acceptance Scenarios**:
- Given a subscription with `include_events` containing `spec-auto.anomaly.consensus_conflict`, when the arbiter marks a stage as conflicted, then the webhook payload includes disagreement summary and mitigation guidance.
- Given a subscription without anomaly events, when the conflict occurs, then no webhook is delivered.

### P3: Developer replays failed notifications

**Story**: As a feature developer, I want to replay failed webhook deliveries after fixing an endpoint so stakeholders receive updates without rerunning `/spec-auto`.

**Priority Rationale**: Reduces wasted compute spend and accelerates communication.

**Testability**: Force a delivery failure, run `/spec-notify --replay` with the delivery ID, confirm the webhook arrives and evidence logs append replay metadata.

**Acceptance Scenarios**:
- Given a dead-lettered delivery, when `/spec-notify --replay` is invoked, then the payload is resent with the same `X-SpecKit-Id` and success recorded.
- Given a replay attempt that still fails, when retries exhaust, then the dead-letter log records both attempts and surfaces warning in TUI.

---

## Edge Cases

- Webhook endpoint returns 429 throttling response.
- Multiple SPEC runs finish simultaneously targeting the same subscription (ensure queue ordering, no dropped events).
- Secrets rotated during pipeline execution (next delivery uses updated secret).
- Dry-run mode enabled (payload logged locally, no HTTP call).
- Telemetry artifact missing or malformed (payload falls back to minimal fields but still delivers).

---

## Requirements

### Functional Requirements

- **FR1**: Emit configurable HTTPS webhooks for stage, pipeline, task completion, and optional anomaly events with signed payloads.
- **FR2**: Maintain asynchronous spool with exponential backoff, idempotency keys, and dead-letter storage scoped to each SPEC's evidence directory without blocking guardrails.
- **FR3**: Provide CLI/TUI commands to test, list, and replay deliveries, reflecting state in evidence logs.

### Non-Functional Requirements

- **Performance**: Additional per-stage latency ≤200 ms, p95 webhook delivery ≤5 s.
- **Security**: HMAC-SHA256 signatures, secrets only via env vars, no plaintext logging.
- **Scalability**: Support ≥10 subscriptions per event with bounded spool (<5 MB per SPEC-specific evidence directory).
- **Reliability**: ≥99.5% successful delivery to healthy endpoints within retry policy; failures visible and replayable.

---

## Success Criteria

- Sample SPEC demonstrates end-to-end notifications (success + failure paths) with passing unit/integration tests.
- Evidence tree records every delivery attempt, replay, and outcome for SPEC-KIT-065 under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-065-add-webhook-notification-system-for/webhooks/`.
- Documentation updated with configuration, payload schema, and replay workflow.
- `/spec-notify` tooling validated in automation and smoke tests.

---

## Evidence & Validation

**Acceptance Tests**: Defined in forthcoming `docs/SPEC-KIT-065-add-webhook-notification-system-for/tasks.md` during `/tasks` stage.

**Telemetry Path**: `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-065-add-webhook-notification-system-for/`

**Consensus Evidence**: `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-065-add-webhook-notification-system-for/`

**Validation Commands**:
```bash
# Run individual stages once implemented (current command names; `/speckit.*` will replace them post-migration)
/spec-plan SPEC-KIT-065-add-webhook-notification-system-for
/spec-tasks SPEC-KIT-065-add-webhook-notification-system-for
/spec-ops-implement SPEC-KIT-065-add-webhook-notification-system-for

# Full pipeline
/spec-auto SPEC-KIT-065-add-webhook-notification-system-for

# Status & evidence review
/spec-status SPEC-KIT-065-add-webhook-notification-system-for
```

---

## Clarifications

### 2025-10-14 - Initial Spec Creation

**Clarification needed**: Confirm preferred language/tooling (Rust vs. Python helper) for webhook worker implementation.

**Resolution**: Pending design review; PRD allows either provided guardrail hooks remain non-blocking.

**Updated sections**: n/a

### 2025-10-15 - Worker implementation decision

**Clarification needed**: Finalize runtime ownership and evidence storage boundaries for the webhook worker.

**Resolution**: Webhook worker will be a Rust long-lived service embedded in the Codex CLI runtime. Guardrail scripts trigger it asynchronously, while each SPEC stores spool and dead-letter artifacts beneath its own evidence directory (`docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/<SPEC-ID>/webhooks/`).

**Updated sections**: Requirements, Success Criteria, Configuration

---

## Dependencies

- Telemetry schema alignment (`telemetry-tasks.md` Task 4.3) for anomaly signals.
- Secret management guidance in documentation and environment provisioning scripts.
- Potential updates to TUI (`codex-rs/tui/src`) to surface webhook status.

---

## Notes

- Feature ships behind `SPECKIT_WEBHOOKS_ENABLED`; default-off until documentation and validation complete.
- Coordinate with SPEC operations team before enabling in CI to ensure network policies allow outbound HTTPS.
