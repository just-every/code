# Template Integration - Actual Implementation

**Status:** Documented 2025-10-16  
**Finding:** Templates are structural references, not auto-populated forms

---

## How Templates Are Currently Used

### Agent Workflow

**Prompts tell agents:**
```
"Template: ~/.code/templates/plan-template.md (reference for output structure)"
"Fill arrays to match plan-template.md structure"
```

**Agents output:**
```json
{
  "work_breakdown": [...],
  "acceptance_mapping": [...],
  "risks": [...]
}
```

### Conversion Process

**Current:** Manual synthesis
1. Agents produce JSON
2. Human (or orchestrator) reads JSON
3. Human writes plan.md using template structure as guide
4. plan.md sections match template but content is synthesized

**Templates provide:**
- Section headings (## Work Breakdown, ## Risks, etc.)
- Expected structure (tables, lists, hierarchies)
- Placeholder examples ([FEATURE_NAME], [STEP_1], etc.)

**Templates do NOT provide:**
- Automatic JSON → markdown conversion
- Fill-in-the-blank automation
- Agent-driven template population

### Evidence

Examined `docs/SPEC-KIT-045-mini/plan.md`:
- Has template structure (Inputs, Work Breakdown, Acceptance Mapping, Consensus)
- Content is human-written prose referencing specific evidence files
- Not auto-generated from JSON

---

## Implications

**What "template-aware" means:**
- Agents know expected output structure
- JSON fields align with template sections
- Consistency across outputs

**What it does NOT mean:**
- Automatic template filling
- Agents write markdown directly
- No human synthesis needed

**Value Delivered:**
- Faster generation (50%) because agents know structure
- Consistent output format
- Easier human synthesis (clear target structure)

**Gap:**
- JSON → markdown conversion is manual
- No tooling to auto-populate templates from JSON
- "Template awareness" is structural, not generative

---

## Future Enhancement Opportunity

**Could implement:**
```rust
fn synthesize_plan_from_json(
    json: &Value,
    template: &str
) -> Result<String, Error> {
    // Parse template
    // Extract JSON fields
    // Populate placeholders
    // Return filled markdown
}
```

**Benefit:** Fully automated template population

**Effort:** 10-15 hours

**Priority:** Low (current manual process works)

---

## Recommendation

**Accept current design:** Templates as structural guides, not auto-fill forms.

**Document clearly:** Avoid claiming "agents fill templates" - more accurate: "agents produce template-aligned JSON for human synthesis"

**Future:** Consider automation if synthesis becomes bottleneck.
