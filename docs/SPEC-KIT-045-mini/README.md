# SPEC-KIT-045 Mini Fixture (<=100 KB)

This directory holds the synthetic SPEC bundle used for fast rehearsals while developing the systematic testing framework. The goal is to keep the entire tree under **100 KB** so `/guardrail.*` stages and `/speckit.auto --from <stage>` can be iterated on in minutes instead of hours.

## Contents
- `spec.md` – trimmed requirements covering spawn audit, telemetry checks, and HAL toggle expectations.
- `plan.md` – single-page work breakdown with acceptance mapping for the rehearsal flow.
- `tasks.md` – five actionable rehearsal steps tied to concrete validation commands.
- `telemetry/sample-validate.json` – 1 KB dummy payload that mimics schema v1 fields the scripts expect.

## Regeneration Checklist
1. Capture fresh guardrail output with the full SPEC (`SPEC-KIT-DEMO` or target SPEC) and copy only the minimal artifacts needed for rehearsal.
2. Update the docs in this directory to reference the latest evidence bundle IDs and note any skip rationales (HAL mock/live).
3. Verify the size stays below the budget:
   ```bash
   du -chs docs/SPEC-KIT-045-mini
   ```
4. Record new hashes for traceability:
   ```bash
   find docs/SPEC-KIT-045-mini -type f -print0 | sort -z | xargs -0 sha256sum > docs/SPEC-KIT-045-mini/checksums.sha256
   ```
5. Update `docs/SPEC-KIT-045-design-systematic-testing-framework-for/plan.md` with the latest bundle timestamp and checksum note.

## Size Tracking
- Generated: 2025-10-12
- Size budget check (run locally): `du -chs docs/SPEC-KIT-045-mini`
- `checksums.sha256` is regenerated alongside any fixture refresh and committed with the docs update.
