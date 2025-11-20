#!/usr/bin/env bash
set -euo pipefail

# highlight-critical-changes.sh: Identify changes affecting prompts/API/executor
#
# This script searches for changes in critical subsystems that affect:
# - System prompts and tool definitions
# - API surface (protocol, event types)
# - Execution flow (codex.rs, exec, apply-patch)
#
# Usage:
#   ./scripts/upstream-merge/highlight-critical-changes.sh [crate-name]
#   ./scripts/upstream-merge/highlight-critical-changes.sh --all
#
# Examples:
#   ./scripts/upstream-merge/highlight-critical-changes.sh core
#   ./scripts/upstream-merge/highlight-critical-changes.sh --all

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." >/dev/null 2>&1 && pwd)"
cd "$ROOT_DIR"

OUTPUT_DIR=".github/auto/upstream-diffs"
CRITICAL_DIR="${OUTPUT_DIR}/critical-changes"
mkdir -p "$CRITICAL_DIR"

# Critical files and patterns to watch
CRITICAL_PATTERN_KEYS=(
    "prompts"
    "openai_tools"
    "agent_tool"
    "protocol"
    "app_server_protocol"
    "mcp_types"
    "codex_main"
    "exec"
    "apply_patch"
    "acp"
    "config"
)

CRITICAL_PATTERN_REGEXES=(
    'prompts/.*\.md'
    'codex-rs/core/src/openai_tools\.rs|code-rs/core/src/openai_tools\.rs'
    'codex-rs/core/src/agent_tool\.rs|code-rs/core/src/agent_tool\.rs'
    'codex-rs/core/src/protocol\.rs|code-rs/core/src/protocol\.rs'
    'codex-rs/app-server-protocol/src/.*|code-rs/app-server-protocol/src/.*'
    'codex-rs/mcp-types/src/.*|code-rs/mcp-types/src/.*'
    'codex-rs/core/src/codex\.rs|code-rs/core/src/codex\.rs'
    'codex-rs/exec/src/.*|code-rs/exec/src/.*'
    'codex-rs/apply-patch/src/.*|code-rs/apply-patch/src/.*'
    'codex-rs/core/src/acp\.rs|code-rs/core/src/acp\.rs'
    'codex-rs/core/src/config.*\.rs|code-rs/core/src/config.*\.rs'
)

pattern_for_category() {
    local key="$1"
    local idx
    for ((idx = 0; idx < ${#CRITICAL_PATTERN_KEYS[@]}; idx++)); do
        if [[ "${CRITICAL_PATTERN_KEYS[$idx]}" == "$key" ]]; then
            printf '%s' "${CRITICAL_PATTERN_REGEXES[$idx]}"
            return 0
        fi
    done
    return 1
}

# Color codes for output
RED='\033[0;31m'
YELLOW='\033[1;33m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Function to analyze a diff file for critical changes
analyze_diff() {
    local crate_name="$1"
    local diff_file="${OUTPUT_DIR}/${crate_name}.diff"

    if [[ ! -f "$diff_file" ]]; then
        echo -e "${GREEN}✅ ${crate_name}: No differences${NC}"
        return 0
    fi

    echo -e "${BLUE}🔍 Analyzing ${crate_name}...${NC}"

    local critical_file="${CRITICAL_DIR}/${crate_name}-critical.md"
    local has_critical=0

    # Initialize critical changes file
    cat > "$critical_file" <<HEADER
# Critical Changes in ${crate_name}

Generated: $(date -u +%Y-%m-%d\ %H:%M:%S\ UTC)

HEADER

    # Check for each critical pattern
    for category in "${CRITICAL_PATTERN_KEYS[@]}"; do
        local pattern
        pattern=$(pattern_for_category "$category") || continue

        # Extract relevant sections from diff
        if grep -E "^\+\+\+ |^--- " "$diff_file" | grep -E "$pattern" > /dev/null 2>&1; then
            echo "## ${category}" >> "$critical_file"
            echo "" >> "$critical_file"

            # Extract the diff sections for files matching this pattern
            awk -v pattern="$pattern" '
                /^\+\+\+ |^--- / {
                    if ($0 ~ pattern) {
                        in_section=1
                        print $0
                    } else {
                        in_section=0
                    }
                    next
                }
                in_section {
                    print
                    if (/^diff --git/) in_section=0
                }
            ' "$diff_file" >> "$critical_file"

            echo "" >> "$critical_file"
            has_critical=1
        fi
    done

    # Check for specific high-impact changes
    echo "## High-Impact Changes" >> "$critical_file"
    echo "" >> "$critical_file"

    # Function signature changes
    if grep -E "^\+.*fn |^-.*fn " "$diff_file" | head -20 >> "$critical_file" 2>/dev/null; then
        echo -e "${YELLOW}⚠️  Function signature changes detected${NC}"
        has_critical=1
    fi

    # Enum/struct changes
    if grep -E "^\+.*enum |^-.*enum |^\+.*struct |^-.*struct " "$diff_file" | head -20 >> "$critical_file" 2>/dev/null; then
        echo -e "${YELLOW}⚠️  Type definition changes detected${NC}"
        has_critical=1
    fi

    # Protocol message changes
    if grep -E "^\+.*EventMsg|^-.*EventMsg|^\+.*Op::|^-.*Op::" "$diff_file" | head -20 >> "$critical_file" 2>/dev/null; then
        echo -e "${YELLOW}⚠️  Protocol message changes detected${NC}"
        has_critical=1
    fi

    if [[ $has_critical -eq 1 ]]; then
        echo -e "${RED}🚨 Critical changes found - see ${critical_file}${NC}"
        return 1
    else
        rm "$critical_file"
        echo -e "${GREEN}✅ No critical changes detected${NC}"
        return 0
    fi
}

# Function to generate critical changes summary
generate_critical_summary() {
    echo "📋 Generating critical changes summary..."
    local summary_file="${CRITICAL_DIR}/CRITICAL-SUMMARY.md"

    cat > "$summary_file" <<'HEADER'
# Critical Changes Summary

Generated: $(date -u +%Y-%m-%d\ %H:%M:%S\ UTC)

This report highlights changes in critical subsystems that affect:
- **Prompts**: System prompts and tool definitions
- **API**: Protocol definitions and event types
- **Executor**: Core execution logic and tool handlers

## Critical Crates

HEADER

    local critical_count=0

    for crate_file in "$CRITICAL_DIR"/*-critical.md; do
        if [[ -f "$crate_file" ]]; then
            local crate_name=$(basename "$crate_file" -critical.md)
            echo "### ${crate_name}" >> "$summary_file"
            echo "" >> "$summary_file"
            echo "See [\`${crate_name}-critical.md\`](./${crate_name}-critical.md) for details." >> "$summary_file"
            echo "" >> "$summary_file"
            ((critical_count++))
        fi
    done

    if [[ $critical_count -eq 0 ]]; then
        echo "✅ No critical changes detected across all crates." >> "$summary_file"
    else
        echo "" >> "$summary_file"
        echo "## Action Items" >> "$summary_file"
        echo "" >> "$summary_file"
        echo "1. Review each critical change file for merge impact" >> "$summary_file"
        echo "2. Identify which changes are upstream improvements vs. fork regressions" >> "$summary_file"
        echo "3. Plan integration strategy (adopt, adapt, or preserve fork behavior)" >> "$summary_file"
        echo "4. Update \`docs/code-crate-parity-tracker.md\` with findings" >> "$summary_file"
    fi

    echo "✅ Critical summary written to ${summary_file}"
    cat "$summary_file"
}

# Main logic
main() {
    # Check if diff files exist
    if [[ ! -d "$OUTPUT_DIR" ]] || [[ -z "$(ls -A $OUTPUT_DIR/*.diff 2>/dev/null)" ]]; then
        echo "⚠️  No diff files found. Run diff-crates.sh first:"
        echo "   ./scripts/upstream-merge/diff-crates.sh --all"
        exit 1
    fi

    if [[ $# -eq 0 ]]; then
        echo "Usage: $0 [crate-name | --all]"
        echo ""
        echo "Available options:"
        echo "  --all         Analyze all crate diffs"
        echo "  <crate-name>  Analyze specific crate"
        exit 1
    fi

    case "$1" in
        --all)
            echo "🔍 Analyzing all crate diffs for critical changes..."
            for diff_file in "$OUTPUT_DIR"/*.diff; do
                if [[ -f "$diff_file" ]]; then
                    local crate_name=$(basename "$diff_file" .diff)
                    analyze_diff "$crate_name" || true
                fi
            done
            generate_critical_summary
            ;;
        *)
            analyze_diff "$1"
            ;;
    esac
}

main "$@"
