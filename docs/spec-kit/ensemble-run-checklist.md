# Ensemble Validation Run Checklist

**Last Updated:** 2025-10-15 (Phase 3 standardization)
**Status:** ✅ Operational (Tier 3 validated via SPEC-KIT-045-mini)

---

## Goal

Execute `/speckit.implement` with the GPT-5-Codex ⊕ Claude 4.5 ensemble (Tier 3: 4 agents) and capture full evidence that all four agents (Gemini, Claude, GPT-5-Codex, GPT-5) participated.

---

## Preconditions

- SPEC has Plan/Tasks consensus with prompt versions recorded in local-memory
- Guardrail baseline (`/spec-ops-auto <SPEC-ID> --from plan`) executed successfully
- Local-memory contains stage entries for prior runs
- All 5 agent types configured in `~/.code/config.toml` (gemini, claude, gpt_pro, gpt_codex, code)

---

## Steps

1. **Trigger guardrail:** `/spec-ops-implement <SPEC-ID>` to lock workspace and collect telemetry
2. **Run multi-agent:** `/speckit.implement <SPEC-ID> [goal]` in TUI, confirm composer prompt shows expected `PROMPT_VERSION` strings
3. **Verify agents:** Confirm Gemini, Claude, GPT-5-Codex, and GPT-5 outputs land in local-memory with matching prompt versions
4. **Check consensus:** Verify `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/<SPEC-ID>/` contains new JSON verdict with correct `prompt_version` and agent metadata
5. **Export evidence:** Capture local-memory export (`code local-memory export --output tmp/memories.jsonl`) for auditing
6. **Update tracker:** Update SPEC.md task row with evidence references

---

## Artifacts

**Guardrail telemetry:**
- `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/<SPEC-ID>/spec-ops-implement_*.json`

**Consensus evidence:**
- `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/<SPEC-ID>/implement_*_gemini.json`
- `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/<SPEC-ID>/implement_*_claude.json`
- `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/<SPEC-ID>/implement_*_gpt_codex.json`
- `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/<SPEC-ID>/implement_*_gpt_pro.json`
- `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/<SPEC-ID>/implement_*_synthesis.json`

**Local-memory:**
- Export snippet showing all agent responses and prompt versions
- SHA-256 recorded in local-memory summary

---

## Tier 3 Validation (Phase 3)

**✅ Validated via SPEC-KIT-045-mini:**
- All 4 agents participated (Gemini, Claude, GPT-5-Codex, GPT-5)
- Code ensemble produced two-vote system (GPT-5-Codex + Claude)
- Arbiter (GPT-5) signed off on merge
- Evidence captured correctly
- Prompt versions matched across all agents
- Synthesis status: "ok" (no conflicts)

---

**Document Version:** 2.0 (Phase 3 Tier 3 validated)
**Last Updated:** 2025-10-15
**Owner:** @just-every/automation
