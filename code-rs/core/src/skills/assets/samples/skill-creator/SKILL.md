---
name: skill-creator
description: Guide for creating effective skills. This skill should be used when users want to create a new skill (or update an existing skill) that extends Codex's capabilities with specialized knowledge, workflows, or tool integrations.
metadata:
  short-description: Create or update a skill
---

# Skill Creator

This skill provides guidance for creating effective skills.

## What is a skill?

A skill is a bundle of instructions and resources that Codex can load on demand to complete a specific task. Skills are stored on disk and referenced in project documentation, allowing Codex to access specialized workflows without bloating the global prompt.

Every skill consists of a required SKILL.md file and optional bundled resources:

```
skill-name/
  SKILL.md
  scripts/     (optional)
  references/  (optional)
  assets/      (optional)
```

## Design principles

- **Clarity:** The `name` and `description` fields in SKILL.md frontmatter are the only fields used for discovery and matching. They must clearly describe when this skill should be used.
- **Focus:** Keep the skill narrowly scoped to a specific set of tasks.
- **Progressive disclosure:** SKILL.md should contain only the minimal instructions needed to get started. Link to detailed files in `references/` as needed.
- **Reusability:** Structure scripts and assets so they can be reused across similar tasks.
- **Maintainability:** Avoid duplicating information across multiple files.

## When to create a new skill

- The user wants Codex to perform a specialized or recurring workflow.
- The task requires detailed domain knowledge or step-by-step instructions.
- The workflow involves reusable scripts or templates.

## Skill file structure

SKILL.md contains:

1. **YAML frontmatter** (required)
   - `name`: The skill's identifier (lowercase, hyphenated)
   - `description`: A clear sentence describing when to use the skill
2. **Body** (optional but recommended)
   - Instructions, workflow, examples, and links to supporting files

### YAML frontmatter example

```
---
name: my-skill
description: Use when the user requests XYZ or mentions ABC workflows.
---
```

## SKILL.md body patterns

Choose a structure that fits the skill's purpose. Common patterns:

**1. Workflow-Based** (best for sequential processes)
- Works well when there are clear step-by-step procedures
- Example: DOCX skill with "Workflow Decision Tree" → "Reading" → "Creating" → "Editing"
- Structure: ## Overview → ## Workflow Decision Tree → ## Step 1 → ## Step 2...

**2. Task-Based** (best for tool collections)
- Works well when the skill offers different operations/capabilities
- Example: PDF skill with "Quick Start" → "Merge PDFs" → "Split PDFs" → "Extract Text"
- Structure: ## Overview → ## Quick Start → ## Task Category 1 → ## Task Category 2...

**3. Reference/Guidelines** (best for standards or specifications)
- Works well for brand guidelines, coding standards, or requirements
- Example: Brand styling with "Brand Guidelines" → "Colors" → "Typography" → "Features"
- Structure: ## Overview → ## Guidelines → ## Specifications → ## Usage...

**4. Capabilities-Based** (best for integrated systems)
- Works well when the skill provides multiple interrelated features
- Example: Product Management with "Core Capabilities" → numbered capability list
- Structure: ## Overview → ## Core Capabilities → ### 1. Feature → ### 2. Feature...

Patterns can be mixed and matched as needed. Most skills combine patterns (e.g., start with task-based, add workflow for complex operations).

## Resource directories

Use resource directories to store supporting files. Only create directories that are actually needed.

### scripts/
Executable code (Python/Bash/etc.) that can be run directly to perform specific operations.

### references/
Detailed documentation, schemas, or reference material that doesn't belong in SKILL.md.

### assets/
Templates, example data, or other static files.

## Avoid these mistakes

- **Overly broad descriptions:** Skills should be specific. Avoid vague descriptions like "Handles all tasks".
- **Verbose SKILL.md:** Keep the core instructions concise; move detailed info to references.
- **Duplicated content:** Don't copy large blocks of reference text into SKILL.md.

## Skill creation workflow

1. Understand the use case and examples.
2. Design the skill structure and required resources.
3. Initialize the skill using the provided script.
4. Edit SKILL.md and add resource files.
5. Validate and package the skill if needed.

### Step 1: Gather examples

Ask the user for concrete examples of how the skill will be used. This informs both the description and the workflow.

### Step 2: Plan the contents

Decide:

- What the skill needs to do
- Which scripts or reference files are required
- The best structure for SKILL.md

### Step 3: Initialize the skill

Use the helper script to create the initial structure.

```bash
scripts/init_skill.py <skill-name> --path <output-directory>
```

The script:

- Creates the skill directory at the specified path
- Generates a SKILL.md template with proper frontmatter and TODO placeholders
- Creates example resource directories: `scripts/`, `references/`, and `assets/`
- Adds example files in each directory that can be customized or deleted

After initialization, customize or remove the generated SKILL.md and example files as needed.

### Step 4: Edit the Skill

- Fill out the frontmatter description carefully.
- Write clear, step-by-step instructions.
- Reference any scripts or external files.

### Step 5: Validate and package

Use `scripts/quick_validate.py` to check that SKILL.md is valid.

```bash
python scripts/quick_validate.py <path-to-skill-dir>
```

Package a skill into a `.skill` file (optional):

```bash
python scripts/package_skill.py <path-to-skill-dir> [output-directory]
```

## Skill naming guidelines

### Skill Naming

- Use lowercase letters, digits, and hyphens only; normalize user-provided titles to hyphen-case (e.g., "Plan Mode" -> `plan-mode`).
- Prefer short, verb-led phrases that describe the action.
- Namespace by tool when it improves clarity or triggering (e.g., `gh-address-comments`, `linear-address-issue`).
- Name the skill folder exactly after the skill name.

### How to choose names

- Keep it short and descriptive.
- Use the verb form if the skill is an action.
- Avoid generic names like `helper` or `tools`.

### Bad names

- `general-tools`
- `utility`
- `code`

### Good names

- `pdf-processing`
- `sql-migration`
- `azure-deploy`

## Example skill snippet

```markdown
---
name: pdf-processing
description: Extract text and tables from PDFs; use when PDFs, forms, or document extraction are mentioned.
---

# PDF Processing
- Use pdfplumber to extract text.
- For form filling, see FORMS.md.
```

## Step-by-step checklists

### Step 1: Understand the skill with concrete examples

If the user is not sure how the skill will be used, prompt them with a few example scenarios:

- "What tasks should this skill handle?"
- "Can you provide example inputs or files?"
- "Is this skill used in a specific context (e.g., data science, infra)?"

### Step 2: Plan reusable skill contents

- Identify reusable scripts
- Identify reference docs
- Identify templates or assets

### Step 3: Initialize the skill

Run:

```bash
scripts/init_skill.py my-skill --path skills/public
```

### Step 4: Write SKILL.md

- Keep it concise
- Include clear instructions
- Reference scripts and files

### Step 5: Package or distribute (optional)

Use the packager if needed:

```bash
python scripts/package_skill.py skills/public/my-skill
```

## Additional guidance

### Maintainability

- Avoid overloading a single skill with multiple unrelated workflows.
- Prefer multiple small skills over one large skill.

### Reusability

- Use scripts to automate repetitive steps.
- Store stable references in separate files.

### Documentation discipline

- Don't include internal reasoning or drafting notes in SKILL.md.
- Keep content concise and actionable.

## Final checklist

- [ ] SKILL.md has valid frontmatter
- [ ] Description clearly states when the skill should be used
- [ ] Skill name matches folder name
- [ ] Resource files are present if referenced
- [ ] Scripts are tested
