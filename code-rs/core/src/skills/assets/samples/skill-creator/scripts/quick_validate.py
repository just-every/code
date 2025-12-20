#!/usr/bin/env python3
"""
Quick validation script for skills - minimal version
"""

import sys
from pathlib import Path

import yaml


def validate_skill(skill_path):
    """Basic validation of a skill"""
    skill_path = Path(skill_path)

    # Check SKILL.md exists
    skill_md = skill_path / "SKILL.md"
    if not skill_md.exists():
        return False, f"SKILL.md not found in {skill_path}"

    # Read and parse frontmatter
    content = skill_md.read_text()
    if not content.startswith("---"):
        return False, "SKILL.md must start with YAML frontmatter (---)"

    try:
        parts = content.split("---", 2)
        if len(parts) < 3:
            return False, "SKILL.md frontmatter must be closed with ---"
        frontmatter = yaml.safe_load(parts[1])
    except yaml.YAMLError as e:
        return False, f"Invalid YAML in frontmatter: {e}"

    # Validate required fields
    if not frontmatter or "name" not in frontmatter:
        return False, "Missing required field: name"
    if "description" not in frontmatter:
        return False, "Missing required field: description"

    name = frontmatter.get("name", "")
    if not isinstance(name, str) or not name.strip():
        return False, "Name must be a non-empty string"
    if not all(c.islower() or c.isdigit() or c == "-" for c in name):
        return False, "Name must be lowercase with only letters, digits, and hyphens"
    if name.startswith("-") or name.endswith("-") or "--" in name:
        return (
            False,
            f"Name '{name}' cannot start/end with hyphen or contain consecutive hyphens",
        )
    if len(name) > 64:
        return False, f"Name is too long ({len(name)} characters). Maximum is 64 characters."

    description = frontmatter.get("description", "")
    if not isinstance(description, str):
        return False, "Description must be a string"
    if not description.strip():
        return False, "Description must be non-empty"
    if len(description) > 1024:
        return (
            False,
            f"Description is too long ({len(description)} characters). Maximum is 1024 characters.",
        )

    return True, "Skill validation passed"


def main():
    if len(sys.argv) < 2:
        print("Usage: python quick_validate.py <skill_directory>")
        sys.exit(1)

    valid, message = validate_skill(sys.argv[1])
    if valid:
        print(f"✅ {message}")
        sys.exit(0)
    else:
        print(f"❌ {message}")
        sys.exit(1)


if __name__ == "__main__":
    main()
