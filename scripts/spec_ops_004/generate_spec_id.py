#!/usr/bin/env python3
"""
Generate next SPEC-ID from feature description.
Format: SPEC-{AREA}-{NUMBER}-{slug}

Example: "Add dark mode toggle" → SPEC-KIT-035-dark-mode-toggle
"""

import re
import sys
from pathlib import Path

def slugify(text: str) -> str:
    """Convert text to URL-safe slug."""
    text = text.lower()
    text = re.sub(r'[^\w\s-]', '', text)
    text = re.sub(r'[-\s]+', '-', text)
    return text.strip('-')

def extract_area(description: str, existing_specs: list[str]) -> str:
    """
    Determine SPEC area from description or existing patterns.

    Default areas:
    - KIT: Spec-kit tooling/automation
    - OPS: Operational/infrastructure
    - CORE: Core Kavedarr functionality
    - API: API endpoints
    - UI: User interface
    """
    desc_lower = description.lower()

    # Keyword mapping
    if any(kw in desc_lower for kw in ['spec', 'guardrail', 'telemetry', 'consensus', 'automation', 'pipeline']):
        return 'KIT'
    elif any(kw in desc_lower for kw in ['deploy', 'ci', 'infra', 'docker', 'monitor']):
        return 'OPS'
    elif any(kw in desc_lower for kw in ['api', 'endpoint', 'route', 'rest', 'graphql']):
        return 'API'
    elif any(kw in desc_lower for kw in ['ui', 'button', 'page', 'view', 'component', 'style']):
        return 'UI'

    # Default to most common area in existing specs
    area_counts = {}
    for spec in existing_specs:
        match = re.match(r'SPEC-([A-Z]+)-', spec)
        if match:
            area = match.group(1)
            area_counts[area] = area_counts.get(area, 0) + 1

    if area_counts:
        return max(area_counts, key=area_counts.get)

    return 'KIT'  # Final fallback

def get_next_number(area: str, existing_specs: list[str]) -> int:
    """Find next available number for given area."""
    max_num = 0
    pattern = re.compile(rf'SPEC-{area}-(\d+)')

    for spec in existing_specs:
        match = pattern.match(spec)
        if match:
            num = int(match.group(1))
            max_num = max(max_num, num)

    # Use increments of 5 for spacing (convention in codebase)
    next_num = ((max_num // 5) + 1) * 5
    return next_num

def find_existing_specs(repo_root: Path) -> list[str]:
    """Find all SPEC directories."""
    docs_dir = repo_root / "docs"
    if not docs_dir.exists():
        return []

    specs = []
    for path in docs_dir.glob("SPEC-*"):
        if path.is_dir():
            specs.append(path.name)

    return specs

def generate_spec_id(description: str, repo_root: Path) -> str:
    """Generate SPEC-ID from description."""
    existing_specs = find_existing_specs(repo_root)

    area = extract_area(description, existing_specs)
    number = get_next_number(area, existing_specs)
    slug = slugify(description)

    # Limit slug length
    if len(slug) > 50:
        words = slug.split('-')
        slug = '-'.join(words[:5])  # First 5 words max

    return f"SPEC-{area}-{number:03d}-{slug}"

def main():
    if len(sys.argv) < 2:
        print("Usage: generate_spec_id.py <feature-description> [repo-root]", file=sys.stderr)
        sys.exit(1)

    description = sys.argv[1]
    repo_root = Path(sys.argv[2]) if len(sys.argv) > 2 else Path.cwd()

    spec_id = generate_spec_id(description, repo_root)
    print(spec_id)

if __name__ == "__main__":
    main()
