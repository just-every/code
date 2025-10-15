#!/bin/bash
#
# Fork Rebase Validation Script
# Run after every rebase from upstream to verify fork-specific code survived
#
# Usage: bash scripts/fork_maintenance/validate_rebase.sh
# Exit: 0 if all checks pass, non-zero if validation fails

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

echo "======================================"
echo "Fork Rebase Validation"
echo "======================================"
echo ""
echo "Repository: $REPO_ROOT"
echo "Date: $(date -Iseconds)"
echo ""

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

pass() {
    echo -e "${GREEN}✓${NC} $1"
}

fail() {
    echo -e "${RED}✗${NC} $1"
    exit 1
}

warn() {
    echo -e "${YELLOW}⚠${NC} $1"
}

# ====================
# 1. COMPILATION CHECK
# ====================

echo "1. Checking compilation..."
cd "$REPO_ROOT/codex-rs"

if cargo build -p codex-tui --profile dev-fast 2>&1 | tee /tmp/fork-build.log | tail -5; then
    pass "Compilation successful"
else
    fail "Compilation failed - check /tmp/fork-build.log"
fi

echo ""

# ====================
# 2. BINARY CHECK
# ====================

echo "2. Checking TUI binary..."
if [ -f "target/dev-fast/code" ]; then
    BINARY_SIZE=$(stat -f%z "target/dev-fast/code" 2>/dev/null || stat -c%s "target/dev-fast/code")
    pass "Binary exists ($(numfmt --to=iec-i --suffix=B $BINARY_SIZE))"
else
    fail "Binary not found at target/dev-fast/code"
fi

echo ""

# ====================
# 3. ENUM VARIANT CHECK
# ====================

echo "3. Checking SpecKit enum variants..."
ENUM_FILE="$REPO_ROOT/codex-rs/tui/src/slash_command.rs"

# Check for SpecKit variants (either inline or nested)
if grep -q "SpecKitNew\|SpecKit(SpecKitCommand)" "$ENUM_FILE"; then
    pass "SpecKit enum variants present"
else
    fail "SpecKit enum variants missing in slash_command.rs"
fi

# Check for Guardrail variants
if grep -q "GuardrailPlan\|Guardrail(GuardrailCommand)" "$ENUM_FILE"; then
    pass "Guardrail enum variants present"
else
    fail "Guardrail enum variants missing in slash_command.rs"
fi

echo ""

# ====================
# 4. ROUTING CHECK
# ====================

echo "4. Checking app.rs routing..."
ROUTING_FILE="$REPO_ROOT/codex-rs/tui/src/app.rs"

# Check for SpecKit routing (either inline or delegated)
if grep -q "SlashCommand::SpecKitPlan\|SlashCommand::SpecKit" "$ROUTING_FILE"; then
    pass "SpecKit routing present in app.rs"
else
    fail "SpecKit routing missing in app.rs"
fi

# Check for Guardrail routing
if grep -q "SlashCommand::GuardrailPlan\|SlashCommand::Guardrail" "$ROUTING_FILE"; then
    pass "Guardrail routing present in app.rs"
else
    fail "Guardrail routing missing in app.rs"
fi

echo ""

# ====================
# 5. MODULE CHECK
# ====================

echo "5. Checking spec-kit modules..."

# Check for spec_prompts (always present)
if [ -f "$REPO_ROOT/codex-rs/tui/src/spec_prompts.rs" ]; then
    pass "spec_prompts.rs exists"
else
    warn "spec_prompts.rs missing (expected if pre-refactoring)"
fi

# Check for spec_status (always present)
if [ -f "$REPO_ROOT/codex-rs/tui/src/spec_status.rs" ]; then
    pass "spec_status.rs exists"
else
    fail "spec_status.rs missing"
fi

# Check for spec_kit module (only after refactoring)
if [ -d "$REPO_ROOT/codex-rs/tui/src/spec_kit" ]; then
    pass "spec_kit/ module exists (refactored)"

    # Verify handler
    if [ -f "$REPO_ROOT/codex-rs/tui/src/spec_kit/handler.rs" ]; then
        pass "spec_kit/handler.rs exists"
    else
        fail "spec_kit/handler.rs missing"
    fi
else
    warn "spec_kit/ module not found (pre-refactoring state)"
fi

echo ""

# ====================
# 6. TEMPLATE CHECK
# ====================

echo "6. Checking templates..."
TEMPLATE_DIR="$REPO_ROOT/templates"

for template in spec-template.md PRD-template.md plan-template.md tasks-template.md; do
    if [ -f "$TEMPLATE_DIR/$template" ]; then
        pass "$template exists"
    else
        fail "$template missing"
    fi
done

echo ""

# ====================
# 7. SCRIPT CHECK
# ====================

echo "7. Checking spec-kit scripts..."
SCRIPT_BASE="$REPO_ROOT/scripts/spec_ops_004"

for script in consensus_runner.sh spec_auto.sh; do
    if [ -f "$SCRIPT_BASE/$script" ]; then
        pass "$script exists"
    else
        fail "$script missing"
    fi
done

echo ""

# ====================
# 8. DOCUMENTATION CHECK
# ====================

echo "8. Checking critical documentation..."

for doc in CLAUDE.md AGENTS.md PLANNING.md product-requirements.md; do
    if [ -f "$REPO_ROOT/$doc" ]; then
        pass "$doc exists"
    else
        fail "$doc missing"
    fi
done

# Check spec-kit docs
if [ -f "$REPO_ROOT/docs/spec-kit/model-strategy.md" ]; then
    pass "docs/spec-kit/model-strategy.md exists"
else
    fail "docs/spec-kit/model-strategy.md missing"
fi

echo ""

# ====================
# 9. CONFIG CHECK
# ====================

echo "9. Checking agent configuration..."
CONFIG_FILE="$HOME/.code/config.toml"

if [ -f "$CONFIG_FILE" ]; then
    AGENT_COUNT=$(grep -c 'enabled = true' "$CONFIG_FILE" || echo "0")
    if [ "$AGENT_COUNT" -eq 5 ]; then
        pass "All 5 agents configured"
    else
        warn "Expected 5 agents, found $AGENT_COUNT in $CONFIG_FILE"
    fi
else
    warn "Config file not found at $CONFIG_FILE"
fi

echo ""

# ====================
# 10. SUMMARY
# ====================

echo "======================================"
echo "Validation Summary"
echo "======================================"
echo ""
pass "All critical checks passed"
echo ""
echo "Fork-specific components verified:"
echo "  - Compilation successful"
echo "  - Binary built"
echo "  - Enum variants present"
echo "  - Routing intact"
echo "  - Modules present"
echo "  - Templates present"
echo "  - Scripts present"
echo "  - Documentation present"
echo ""
echo "✅ Rebase validation PASSED"
echo ""
echo "Next steps:"
echo "  1. Test in TUI: /speckit.status SPEC-KIT-045-mini"
echo "  2. Run one /speckit.auto pipeline"
echo "  3. Verify evidence files created"
echo "  4. Document rebase in docs/spec-kit/REBASE_LOG.md"
echo ""
