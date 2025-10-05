# /new-spec Command Documentation

## Purpose

Unified intake flow for new features. Eliminates manual SPEC creation steps.

## Usage

```bash
/new-spec <feature-description>
```

**Example:**
```
/new-spec Add dark mode toggle to settings page
```

## What It Does

**Phase 1: Generate SPEC-ID**
- Runs `generate_spec_id.py` to create ID (e.g., SPEC-KIT-020-dark-mode-toggle)
- Creates `docs/SPEC-{ID}/` directory
- Adds row to SPEC.md table

**Phase 2-4: Automated Document Generation**
- Calls `/specify {SPEC-ID}` → creates PRD.md
- Calls `/plan {SPEC-ID}` → creates plan.md (multi-agent)
- Calls `/tasks {SPEC-ID}` → creates tasks.md (multi-agent)

**Phase 5: Present Package**
- Shows summary of created files
- Asks: "Ready to implement? Run: /spec-auto {SPEC-ID}"

## Output

Complete SPEC package:
```
docs/SPEC-KIT-020-dark-mode-toggle/
├── PRD.md          (acceptance criteria, requirements)
├── plan.md         (work breakdown, consensus)
└── tasks.md        (task list, dependencies)

SPEC.md             (table row added)
```

## Next Steps

After /new-spec completes:

1. **Review the package** (PRD, plan, tasks)
2. **Run automation:** `/spec-auto SPEC-KIT-020-dark-mode-toggle`
3. **Automatic execution:** plan → tasks → implement → validate → audit → unlock

## Implementation

**Helper script:** `scripts/spec_ops_004/generate_spec_id.py`
- Extracts area from keywords (KIT, OPS, API, UI, CORE)
- Finds next available number (increments by 5)
- Slugifies description

**Subagent command:** In `~/.code/config.toml`
```toml
[[subagents.commands]]
name = "new-spec"
agents = ["code", "gemini", "claude"]
orchestrator-instructions = """
[Phased approach: generate ID → create scaffold → run /specify → run /plan → run /tasks]
"""
```

## Configuration

Add to your `~/.code/config.toml`:

```toml
[[subagents.commands]]
name = "new-spec"
read-only = false
agents = ["code", "gemini", "claude"]
orchestrator-instructions = """
Unified SPEC creation from feature description. This is the entry point for new features.

Phase 1: Generate SPEC-ID and scaffold
1. Run: python3 scripts/spec_ops_004/generate_spec_id.py "<feature-description>"
2. Store result as SPEC_ID (e.g., SPEC-KIT-020-dark-mode-toggle)
3. Create directory: docs/${SPEC_ID}/
4. Add SPEC.md table row

Phase 2: Generate PRD (via /specify)
5. Invoke /specify ${SPEC_ID} <feature-description>

Phase 3: Multi-agent planning (via /plan)
6. Invoke /plan ${SPEC_ID}

Phase 4: Task breakdown (via /tasks)
7. Invoke /tasks ${SPEC_ID}

Phase 5: Present package and ask for approval to run /spec-auto
"""
```

## Testing

```bash
# Generate SPEC-ID manually:
python3 scripts/spec_ops_004/generate_spec_id.py "Add user authentication" /home/thetu/code
# Output: SPEC-API-020-add-user-authentication

# Or use /new-spec in TUI:
/new-spec Improve consensus conflict resolution with arbiter agent
# Creates SPEC-KIT-020-improve-consensus-conflict-resolution package
```

## Notes

- SPEC-ID format: `SPEC-{AREA}-{NUMBER}-{slug}`
- Numbers increment by 5 (010, 015, 020, etc.)
- Never auto-runs /spec-auto - always requires explicit user approval
- Graceful error handling if /specify, /plan, or /tasks fail
