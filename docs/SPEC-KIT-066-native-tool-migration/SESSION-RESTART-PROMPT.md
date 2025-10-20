# Session Restart Prompt: SPEC-KIT-066 Native Tool Migration

**Created**: 2025-10-20
**Purpose**: Resume work on migrating spec-kit orchestrator commands from bash/python to native Codex tools
**Prior Session**: 2025-10-19/20 (11 hours, 15 commits, routing bug fixed)

---

## System Prompt (CLEARFRAME Mode - REQUIRED)

```
[ROLE]
You are an analytic peer, not a service persona.

[PRIME DIRECTIVES]
1) Truthfulness over likeability; correctness over completeness; internal consistency over style.
2) Anti‑sycophancy: do not agree to please. If my claim conflicts with facts or logic, say so directly.
3) Challenge protocol: when input is underspecified, contradictory, or shows a reasoning flaw, interrupt, name the flaw, and propose a fix.
4) Calibration: avoid performative hedges. When uncertainty materially affects a claim or action, append one tag:
   [Confidence: 0.00–1.00 | Key driver: <single cause>].
5) Safety & legality: follow platform safety rules and applicable law; if blocked, explain the constraint and suggest safe alternatives.

[DEFAULT ANSWER STRUCTURE]
1) Claims — numbered, each a single sentence.
2) Evidence — verifiable support (citations, code lines, data). Keep quotes minimal.
3) Counter‑check — what would falsify/stress‑test the claims and how to test it.
4) Action — the 1–3 highest‑leverage next steps (brief, concrete).

[LANGUAGE]
- Be precise and concrete. No filler or performance of empathy.
- Flag lazy/clichéd phrasing inline as: ⚠ lazy: "<phrase>" → suggestion: "<rewrite>"
- Prefer active voice, specific nouns/verbs, and bounded quantities.

[ASSUMPTIONS & SCOPE]
- If required inputs are missing, proceed with the best‑effort answer:
  - State minimal assumptions explicitly (bullet list).
  - Continue unless a single crisp question is strictly necessary to proceed.

[ERROR & CONFLICT HANDLING]
- If instructions conflict, surface the conflict, propose a resolution, and continue with the most conservative, verifiable path.
- Do not invent access, results, or references. If access is unavailable, state that and propose the safest viable alternative.
```

---

## PROJECT CONTEXT

**Repository**: https://github.com/theturtlecsz/code (FORK of just-every/code)
**Working Directory**: /home/thetu/code/codex-rs
**Branch**: main (15 commits ahead)
**Binary**: /home/thetu/code/codex-rs/target/dev-fast/code (in PATH)

**Current State** (2025-10-20):
- ✅ 604 tests @ 100% pass rate
- ✅ 42-48% test coverage (exceeds 40% target)
- ✅ Routing bug FIXED (routing.rs now passes config)
- ⚠️ Orchestrator config uses bash/python (THIS TASK fixes it)
- 📁 SPEC-066 created, documented, ready for work

**Build Command**: `./build-fast.sh` (from /home/thetu/code)

---

## MANDATORY: Query Local-Memory FIRST

**Before starting any work**:

```
Use mcp__local-memory__search:
  query: "SPEC-066 routing orchestrator native tools 2025-10-20"
  limit: 15
  use_ai: true
  session_filter_mode: all
```

**Then retrieve specific memory IDs**:
```
Use mcp__local-memory__get_memory_by_id:
  - 908a7170-fa0d-4dcc-8438-1206fb87d22e (orchestrator config issue)
  - a0a5c0a7-9632-4770-99c1-a983326838cd (routing bug fix)
  - 9ce77450-b081-41c3-ae82-5de8383df22c (native tool capabilities)
  - 06c4c7df-8941-4f3a-b726-b37b69c587d1 (session summary)
```

**Why**: These contain complete context about bugs discovered, fixes applied, and migration strategy.

---

## SPEC-KIT-066: Task Overview

**Goal**: Migrate orchestrator instructions from bash/python to native Codex tools

**Problem**:
- ~/.code/config.toml orchestrator-instructions reference "Run: python3..." and "bash scripts/..."
- Orchestrator interprets these as advisory descriptions, not actionable commands
- Result: Creates plan documents instead of executing tools
- Example: /speckit.new created "Search Command Plan" instead of SPEC directory

**Solution**: Rewrite instructions to imperatively use Glob, Read, Write, Edit tools

---

## Phase 1: Research & Inventory (1-2 hours)

**Objective**: Understand current state of all subagent commands

**Steps**:

1. **Load current config**:
   ```
   Read: ~/.code/config.toml
   Focus on lines 214-523 ([[subagents.commands]] section)
   ```

2. **Inventory all 9 commands**:
   - speckit.new (line 222)
   - speckit.specify (line 272)
   - speckit.plan (line 284)
   - speckit.tasks (line 291)
   - speckit.implement (line 302)
   - speckit.auto (line 314)
   - speckit.clarify (line 401)
   - speckit.analyze (line 423)
   - speckit.checklist (line 446)

3. **For each command, identify**:
   - Bash/python script references
   - What the script does
   - Native tool replacement (Glob/Read/Write/Edit or keep bash)

4. **Create inventory table**:
   | Command | Line # | Script Refs | Complexity | Native Replacement | Priority |
   |---------|--------|-------------|------------|-------------------|----------|
   | speckit.new | 222 | python3 generate_spec_id.py | SIMPLE | Glob+parse | P0 |
   | ... | | | | | |

**Deliverable**: Markdown table showing migration strategy

---

## Phase 2: Migrate speckit.new (2-3 hours)

**Objective**: Make /speckit.new work without Python scripts

**Current Instructions** (lines 225-269):
```
1. Run: python3 scripts/spec_ops_004/generate_spec_id.py
2. Store as SPEC_ID
3. mkdir -p docs/${SPEC_ID}/
4. Add SPEC.md entry
...
```

**New Instructions** (use these as template):
```
Create SPEC from feature description. EXECUTE these steps using your tools:

1. Generate SPEC-ID:
   - Use Glob tool: pattern="SPEC-KIT-*" in docs/ directory
   - Parse numbers from directory names (extract digits after SPEC-KIT-)
   - Find maximum number (e.g., 060)
   - Increment by 1 (e.g., 061)
   - Create slug: lowercase feature description, replace spaces with hyphens, remove special chars
   - Format: SPEC-KIT-{number}-{slug}

2. Create directory structure:
   - Use Write tool to create: docs/SPEC-KIT-{number}-{slug}/PRD.md
   - Parent directories created automatically by Write tool

3. Generate PRD:
   - Use Read tool: ~/.code/templates/PRD-template.md (if exists)
   - Fill placeholders: [FEATURE_NAME], [PROBLEM_STATEMENT], [SOLUTION], [ACCEPTANCE_CRITERIA]
   - OR generate comprehensive PRD from scratch using feature description
   - Use Write tool: docs/SPEC-KIT-{number}-{slug}/PRD.md

4. Create spec.md:
   - Use Write tool: docs/SPEC-KIT-{number}-{slug}/spec.md
   - Initial spec referencing PRD, placeholder for requirements

5. Update SPEC.md tracker:
   - Use Read tool: SPEC.md (find "## Active Tasks" or "### Production Readiness")
   - Use Edit tool: Add new table row
   - Format: | Order | SPEC-KIT-{number} | {title} | Backlog | Code | PRD.md | | | {date} | | |

6. Report completion:
   - Display: "✅ SPEC created: SPEC-KIT-{number}"
   - Display: "📁 Location: docs/SPEC-KIT-{number}-{slug}/"
   - Display: "Next: /speckit.auto SPEC-KIT-{number} to begin development"

CRITICAL: Actually USE your tools (Glob, Read, Write, Edit). Do NOT just describe steps.
Do NOT invoke other slash commands (/specify, /plan) - those are separate stages.
Do NOT use bash/python - use native Codex tools only.
```

**Steps to Apply**:

1. **Edit ~/.code/config.toml**:
   ```
   Use Edit tool:
   - old_string: Lines 225-269 (current orchestrator-instructions)
   - new_string: The imperative instructions above
   ```

2. **Rebuild**:
   ```
   Run: ./build-fast.sh
   ```

3. **Test**:
   ```
   In TUI: /speckit.new Add /search command for conversation history
   ```

4. **Verify**:
   ```
   Check: ls docs/SPEC-KIT-*/
   Should see new SPEC-KIT-{number}-search-command/ directory

   Check: tail -20 SPEC.md
   Should see new table row
   ```

---

## Phase 3: Migrate Other Commands (2-4 hours)

**After speckit.new works**, migrate in priority order:

1. **speckit.specify** (line 272): Already mostly native, verify
2. **speckit.clarify** (line 401): Check for script refs
3. **speckit.analyze** (line 423): Check for script refs
4. **speckit.checklist** (line 446): Check for script refs
5. **speckit.plan/tasks** (lines 284, 291): Verify agent_run usage
6. **speckit.implement** (line 302): Likely needs bash for cargo/clippy
7. **speckit.auto** (line 314): **KEEP bash guardrails** (legitimate complexity)

**For each**: Apply same pattern (imperative tool usage, no script references)

---

## Phase 4: Real-World Validation (1 hour)

**After migrations complete**:

1. **Create real feature**:
   ```
   /speckit.new Add /search command to find text in conversation history
   ```
   Expected: SPEC-KIT-067 (or next number) directory created

2. **Run full pipeline**:
   ```
   /speckit.auto SPEC-KIT-{number}
   ```
   Expected: 6 stages execute, /search command implemented

3. **Validate feature**:
   ```
   Test /search in TUI
   ```
   Expected: Searches conversation and shows results

---

## Success Criteria

**Phase 1 Complete**:
- ✅ Inventory table created showing all 9 commands
- ✅ Native replacement strategy for each
- ✅ Priority order determined

**Phase 2 Complete**:
- ✅ ~/.code/config.toml updated (speckit.new uses native tools)
- ✅ /speckit.new creates SPEC directory + files
- ✅ SPEC.md updated with new entry
- ✅ No Python/bash scripts invoked

**Phase 3 Complete**:
- ✅ All non-guardrail commands migrated
- ✅ Only speckit.auto keeps bash (for guardrails)

**Phase 4 Complete**:
- ✅ Real feature SPEC created via /speckit.new
- ✅ /speckit.auto executes full pipeline
- ✅ Feature works in TUI
- ✅ Framework proven with non-meta work

---

## Critical Context

**From Local-Memory** (retrieve these IDs):

**908a7170**: Orchestrator config issue - instructions reference bash/python that aren't executed

**a0a5c0a7**: Routing bug fixed - routing.rs now passes widget.config to format_subagent_command

**9ce77450**: Native tool capabilities - Glob (find files), Read (read), Write (create with parent dirs), Edit (modify)

**33fe6bdf**: Config architecture - ~/.code/config.toml is PRIMARY (not .github version)

**03f4ef51**: Session summary - epic sprint, 604 tests, discoveries, commits

---

## Files to Reference

**Essential**:
- docs/SPEC-KIT-066-native-tool-migration/PRD.md
- docs/SPEC-KIT-066-native-tool-migration/spec.md
- ~/.code/config.toml (THE config, not .github version)
- codex-rs/tui/src/chatwidget/spec_kit/routing.rs (routing fix)
- SPEC.md (SPEC-066 entry in Production Readiness section)

**Background**:
- codex-rs/MEMORY-POLICY.md (local-memory usage policy)
- CLAUDE.md (operating guide, now includes mandatory local-memory workflow)
- docs/INDEX.md (documentation navigation)

---

## Execution Checklist

**Session Start**:
- [ ] Query local-memory for SPEC-066 context
- [ ] Retrieve 5 specific memory IDs listed above
- [ ] Read SPEC-066 PRD and spec
- [ ] Read ~/.code/config.toml

**Phase 1**:
- [ ] Create inventory of 9 subagent commands
- [ ] Identify bash/python references
- [ ] Design native replacements
- [ ] Store findings in local-memory

**Phase 2**:
- [ ] Edit ~/.code/config.toml (speckit.new instructions)
- [ ] Rebuild: ./build-fast.sh
- [ ] Test: /speckit.new in TUI
- [ ] Verify: SPEC directory created
- [ ] Store success in local-memory

**Phase 3**:
- [ ] Migrate remaining commands
- [ ] Test each one
- [ ] Store progress in local-memory

**Phase 4**:
- [ ] Real feature test (create /search command SPEC)
- [ ] Run /speckit.auto end-to-end
- [ ] Validate feature works
- [ ] Store validation results in local-memory

---

## Expected Effort

**Total**: 5-9 hours
**Breakdown**:
- Phase 1: 1-2 hours (inventory)
- Phase 2: 2-3 hours (speckit.new migration + testing)
- Phase 3: 2-4 hours (other commands)
- Phase 4: 1 hour (real-world validation)

---

## Copy-Paste Restart Command

**Paste this into your next TUI session**:

```
Resume SPEC-KIT-066: Native Tool Migration

MANDATORY FIRST STEPS:

1. Query local-memory:
   Search: "SPEC-066 routing orchestrator native tools 2025-10-20"
   Retrieve IDs: 908a7170, a0a5c0a7, 9ce77450, 33fe6bdf, 03f4ef51

2. Load context:
   Read: docs/SPEC-KIT-066-native-tool-migration/PRD.md
   Read: docs/SPEC-KIT-066-native-tool-migration/spec.md
   Read: ~/.code/config.toml (lines 214-523)

3. Verify routing fix:
   Read: codex-rs/tui/src/chatwidget/spec_kit/routing.rs (lines 58-80)
   Confirm: format_subagent_command gets Some(&widget.config.agents)

TASK: Migrate orchestrator commands from bash/python to native Codex tools.

Start with Phase 1: Create inventory of all 9 [[subagents.commands]] entries in ~/.code/config.toml, identify bash/python references, design native replacements.

After inventory, proceed to Phase 2: Migrate speckit.new orchestrator-instructions to use Glob/Read/Write/Edit tools.

SUCCESS = /speckit.new creates SPEC directory without Python scripts.

Store ALL findings and decisions in local-memory as you work (importance ≥7).
```

---

**Session restart prompt ready!**

Use this to begin your next session with full context from local-memory.
