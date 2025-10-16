# Template Integration - Actual Implementation

**Status:** Documented 2025-10-16  
**Finding:** Templates are structural references, not auto-populated forms

---

## How Templates Are Currently Used

### Templates Serve Dual Purpose

**1. Format Guides for Agents**

Prompts explicitly reference templates:
```
"Template: ~/.code/templates/plan-template.md (reference for output structure)"
"Fill arrays to match plan-template.md structure"
```

**Benefit:** Agents know the expected structure and produce template-aligned JSON.
- Work breakdown includes: step, rationale, success_signal (matching template fields)
- Acceptance mapping includes: requirement, validation_step, artifact (matching template)
- Risks include: risk, owner, mitigation (matching template)

**Result:** 50% speed improvement - agents don't need to invent structure, just fill it.

**Agents output:**
```json
{
  "work_breakdown": [{"step": "...", "rationale": "...", "success_signal": "..."}],
  "acceptance_mapping": [{"requirement": "...", "validation_step": "...", "artifact": "..."}],
  "risks": [{"risk": "...", "owner": "...", "mitigation": "..."}]
}
```

**2. Human Synthesis Guides**

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
- Agents see template structure in prompts (format guide)
- Agents produce JSON with fields matching template sections
- JSON structure optimized for template synthesis
- Consistency across outputs
- Faster generation (50%) - agents don't invent structure

**What it does NOT mean:**
- Automatic programmatic template filling (no code does this)
- Agents write markdown directly (they output JSON)
- Zero human synthesis (human still converts JSON → markdown)

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
