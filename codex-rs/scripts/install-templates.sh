#!/usr/bin/env bash
# Install spec-kit templates to ~/.code/templates/

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TEMPLATES_SRC="$REPO_ROOT/templates"
TEMPLATES_DEST="$HOME/.code/templates"

echo "Installing spec-kit templates to $TEMPLATES_DEST..."

# Create ~/.code/templates/ if it doesn't exist
mkdir -p "$TEMPLATES_DEST"

# Check if source templates exist
if [ ! -d "$TEMPLATES_SRC" ]; then
    echo "ERROR: Source templates not found at $TEMPLATES_SRC"
    exit 1
fi

# Copy all template files
count=0
for template in "$TEMPLATES_SRC"/*.md; do
    if [ -f "$template" ]; then
        filename=$(basename "$template")
        cp "$template" "$TEMPLATES_DEST/$filename"
        echo "  ✓ Installed $filename"
        ((count++))
    fi
done

if [ $count -eq 0 ]; then
    echo "WARNING: No template files found in $TEMPLATES_SRC"
    exit 1
fi

echo ""
echo "Template installation complete! Installed $count templates."
echo "Location: $TEMPLATES_DEST"
