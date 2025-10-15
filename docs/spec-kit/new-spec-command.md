# /speckit.new Command Documentation

**Last Updated:** 2025-10-15 (Phase 3 standardization)
**Command:** `/speckit.new` (formerly `/new-spec`)
**Tier:** 2 (Triple agent: gemini, claude, code)

---

## Purpose

Unified intake flow for new features. Eliminates manual SPEC creation steps using GitHub-inspired templates.

**Phase 3 Enhancements:**
- Template-based generation (55% faster than baseline)
- Multi-agent PRD consensus
- Consistent P1/P2/P3 scenario structure
- Automatic SPEC-ID generation

---

## Usage

```bash
/speckit.new <feature-description>
```

**Example:**
```bash
/speckit.new Add dark mode toggle to settings page
```

**Legacy Command (Still Works):**
```bash
/new-spec Add dark mode toggle to settings page
```

---

## What It Does

**Phase 1: Generate SPEC-ID**
- Runs `generate_spec_id.py` to create ID (e.g., SPEC-KIT-020-dark-mode-toggle)
- Creates `docs/SPEC-{ID}/` directory
- Adds row to SPEC.md table

**Phase 2: Multi-Agent PRD Generation**
- Uses `templates/PRD-template.md` for consistent structure
- 3 agents (gemini, claude, code) draft PRD.md with consensus
- Structured requirements with testability criteria
- P1/P2/P3 user scenarios

**Phase 3: Multi-Agent Planning**
- Uses `templates/plan-template.md`
- 3 agents (gemini, claude, gpt_pro) create work breakdown
- Consensus on approach, risks, acceptance mapping
- GitHub-style plan structure

**Phase 4: Task Decomposition**
- Uses `templates/tasks-template.md`
- 3 agents (gemini, claude, gpt_pro) break down tasks
- Checkbox task lists with dependencies
- Updates SPEC.md Tasks table

**Phase 5: Present Package**
- Shows summary of created files
- Evidence paths for multi-agent consensus
- Suggests: "Ready to implement? Run: /speckit.auto {SPEC-ID}"

---

## Performance

**Tier 2 (Triple Agent):**
- Agents: gemini-2.5-pro, claude-4.5-sonnet, code
- Duration: ~13 minutes (55% faster than pre-template baseline of 30 min)
- Cost: ~$0.60
- Mode: Parallel agent spawning → Consensus synthesis

**Template Benefits:**
- Consistent structure across all SPECs
- Pre-defined P1/P2/P3 scenarios
- Acceptance criteria mapping
- 55% speed improvement validated (SPEC-KIT-060)

---

## Output

Complete SPEC package:
```
docs/SPEC-KIT-020-dark-mode-toggle/
├── PRD.md          (acceptance criteria, P1/P2/P3 scenarios, requirements)
├── plan.md         (work breakdown, consensus, acceptance mapping)
└── tasks.md        (checkbox task list, dependencies, validation)

SPEC.md             (table row added with SPEC-ID, status, PRD path)
```

**Evidence:**
```
docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-020-*/
├── specify_*_gemini.json
├── specify_*_claude.json
├── specify_*_code.json
├── specify_*_synthesis.json
├── plan_*_gemini.json
├── plan_*_claude.json
├── plan_*_gpt_pro.json
├── plan_*_synthesis.json
└── tasks_*_synthesis.json
```

---

## Next Steps

After `/speckit.new` completes:

**Option 1: Quality Checks (Recommended)**
```bash
# Proactive quality validation
/speckit.clarify SPEC-KIT-020-dark-mode-toggle
/speckit.analyze SPEC-KIT-020-dark-mode-toggle
/speckit.checklist SPEC-KIT-020-dark-mode-toggle
```

**Option 2: Full Automation**
```bash
# Run 6-stage pipeline
/speckit.auto SPEC-KIT-020-dark-mode-toggle
# Automatic execution: plan → tasks → implement → validate → audit → unlock
```

**Option 3: Manual Stages**
```bash
# Step through individual stages
/speckit.implement SPEC-KIT-020-dark-mode-toggle
/speckit.validate SPEC-KIT-020-dark-mode-toggle
# etc.
```

---

## Implementation

**Helper script:** `scripts/spec_ops_004/generate_spec_id.py`
- Extracts area from keywords (KIT, OPS, API, UI, CORE)
- Finds next available number (increments by 5)
- Slugifies description
- Creates directory structure

**Subagent command:** In `~/.code/config.toml`
```toml
[[subagents.commands]]
name = "speckit.new"
agents = ["gemini", "claude", "code"]
orchestrator-instructions = """
[Phased approach: generate ID → create scaffold → run /speckit.specify → run /speckit.plan → run /speckit.tasks]
Uses templates for consistent structure (55% faster).
"""
```

---

## Configuration

Add to your `~/.code/config.toml`:

```toml
[[subagents.commands]]
name = "speckit.new"
read-only = false
agents = ["gemini", "claude", "code"]
orchestrator-instructions = """
Unified SPEC creation from feature description using templates. Entry point for new features.

Phase 1: Generate SPEC-ID and scaffold
1. Run: python3 scripts/spec_ops_004/generate_spec_id.py "<feature-description>"
2. Store result as SPEC_ID (e.g., SPEC-KIT-020-dark-mode-toggle)
3. Create directory: docs/${SPEC_ID}/
4. Add SPEC.md table row

Phase 2: Generate PRD (via /speckit.specify with templates)
5. Use templates/PRD-template.md for structure
6. Invoke /speckit.specify ${SPEC_ID} <feature-description>
7. Multi-agent consensus (gemini, claude, code)

Phase 3: Multi-agent planning (via /speckit.plan with templates)
8. Use templates/plan-template.md
9. Invoke /speckit.plan ${SPEC_ID}
10. Consensus across gemini, claude, gpt_pro

Phase 4: Task breakdown (via /speckit.tasks with templates)
11. Use templates/tasks-template.md
12. Invoke /speckit.tasks ${SPEC_ID}
13. Update SPEC.md Tasks table

Phase 5: Present package
14. Show created files and evidence paths
15. Suggest quality checks (/speckit.clarify, /speckit.analyze, /speckit.checklist)
16. Suggest automation (/speckit.auto ${SPEC_ID})

NEVER auto-run /speckit.auto - always require explicit user approval.
Graceful error handling if stages fail (continue with degraded consensus).
"""
```

**Backward Compatibility:**
```toml
# Legacy command (maps to speckit.new internally)
[[subagents.commands]]
name = "new-spec"
agents = ["gemini", "claude", "code"]
# ... same instructions, routes to /speckit.new
```

---

## Testing

```bash
# Generate SPEC-ID manually:
python3 scripts/spec_ops_004/generate_spec_id.py "Add user authentication" /home/thetu/code
# Output: SPEC-API-020-add-user-authentication

# Use /speckit.new in TUI:
/speckit.new Improve consensus conflict resolution with arbiter agent
# Creates SPEC-KIT-020-improve-consensus-conflict-resolution package

# Legacy command still works:
/new-spec Add webhook notifications for task completion
# Routes to /speckit.new internally
```

---

## Template Structure

**PRD Template** (`templates/PRD-template.md`):
- P1 scenarios (critical user journeys)
- P2 scenarios (important but not critical)
- P3 scenarios (nice to have)
- Acceptance criteria with testability
- Success metrics

**Plan Template** (`templates/plan-template.md`):
- Work breakdown (numbered steps)
- Acceptance mapping table
- Risks & unknowns
- Consensus & risks (multi-AI)
- Exit criteria

**Tasks Template** (`templates/tasks-template.md`):
- Checkbox task lists
- Dependencies
- Validation steps
- Evidence references

---

## Multi-Agent Consensus

**Agent Roles:**
- **Gemini 2.5 Pro:** Research, breadth, exploration
- **Claude 4.5 Sonnet:** Synthesis, precision, analysis
- **Code (Claude Code):** Orchestration, validation

**Consensus Process:**
1. All 3 agents generate PRD independently
2. Synthesis identifies agreements and conflicts
3. Conflicts resolved via arbiter (gpt-5) if needed (<5% of runs)
4. Final PRD combines best elements from all agents

**Evidence:**
- Per-agent outputs saved
- Synthesis summary with consensus metadata
- Local-memory stores for `/spec-consensus` retrieval

---

## Notes

- SPEC-ID format: `SPEC-{AREA}-{NUMBER}-{slug}`
- Numbers increment by 5 (010, 015, 020, etc.)
- **Never auto-runs /speckit.auto** - always requires explicit user approval
- Graceful error handling if /speckit.specify, /speckit.plan, or /speckit.tasks fail
- Template validation ensures consistency (55% faster, validated SPEC-KIT-060)
- Quality commands (/speckit.clarify, analyze, checklist) recommended before automation

---

## Migration from /new-spec

**Old workflow:**
```bash
/new-spec Add feature
# → Creates SPEC, slow (30 min), inconsistent structure
```

**New workflow:**
```bash
/speckit.new Add feature
# → Creates SPEC with templates, fast (13 min), consistent P1/P2/P3 structure
# → Quality checks available
# → Backward compatible (/new-spec still works)
```

**Benefits:**
- 55% faster (templates)
- Consistent structure
- Quality commands available
- Same agents, better output

---

**Document Version:** 2.0 (Phase 3 template-based)
**Last Updated:** 2025-10-15
**Status:** Fully operational with templates
**Owner:** @just-every/automation
