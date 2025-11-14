#!/bin/bash
# migrate-crate.sh - Helper script to migrate a crate from codex-rs to code-rs
# Usage: ./migrate-crate.sh <crate-name>
# Example: ./migrate-crate.sh linux-sandbox

set -e

if [ $# -ne 1 ]; then
    echo "Usage: $0 <crate-name>"
    echo "Example: $0 linux-sandbox"
    exit 1
fi

CRATE_NAME="$1"
CODEX_CRATE_PATH="../codex-rs/${CRATE_NAME}"
CODE_CRATE_PATH="${CRATE_NAME}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}=== Migrating ${CRATE_NAME} from codex-rs to code-rs ===${NC}"

# Step 1: Verify source crate exists
if [ ! -d "$CODEX_CRATE_PATH" ]; then
    echo -e "${RED}Error: Source crate not found at $CODEX_CRATE_PATH${NC}"
    exit 1
fi

# Step 2: Verify destination crate exists (should be a wrapper)
if [ ! -d "$CODE_CRATE_PATH" ]; then
    echo -e "${RED}Error: Destination crate not found at $CODE_CRATE_PATH${NC}"
    exit 1
fi

# Step 3: Backup the wrapper's lib.rs
echo -e "${YELLOW}Backing up current wrapper implementation...${NC}"
cp "${CODE_CRATE_PATH}/src/lib.rs" "${CODE_CRATE_PATH}/src/lib.rs.backup"

# Step 4: Copy source files
echo -e "${YELLOW}Copying source files from codex-rs...${NC}"
# Copy all .rs files except main.rs and lib.rs (we'll handle those specially)
find "${CODEX_CRATE_PATH}/src" -name "*.rs" -not -name "main.rs" -not -name "lib.rs" -exec cp -v {} "${CODE_CRATE_PATH}/src/" \;

# Step 5: Copy tests if they exist
if [ -d "${CODEX_CRATE_PATH}/tests" ]; then
    echo -e "${YELLOW}Copying test files...${NC}"
    mkdir -p "${CODE_CRATE_PATH}/tests"
    cp -rv "${CODEX_CRATE_PATH}/tests/"* "${CODE_CRATE_PATH}/tests/"
fi

# Step 6: Display the original Cargo.toml for reference
echo -e "${YELLOW}=== Original codex-rs Cargo.toml dependencies ===${NC}"
echo "You need to manually update ${CODE_CRATE_PATH}/Cargo.toml with these dependencies:"
echo "(replacing codex-* with code-* where applicable)"
echo ""
grep -A 100 "^\[dependencies\]" "${CODEX_CRATE_PATH}/Cargo.toml" | grep -B 100 "^\[" | head -n -1 || true
echo ""
grep -A 100 "^\[dev-dependencies\]" "${CODEX_CRATE_PATH}/Cargo.toml" | grep -B 100 "^\[" | head -n -1 || true
echo ""
grep -A 100 "^\[target\." "${CODEX_CRATE_PATH}/Cargo.toml" || true

echo -e "${GREEN}Files copied successfully!${NC}"
echo ""
echo -e "${YELLOW}=== Manual steps required ===${NC}"
echo "1. Update ${CODE_CRATE_PATH}/Cargo.toml:"
echo "   - Replace wrapper dependencies with actual dependencies from codex-rs"
echo "   - Change 'codex-*' dependencies to 'code-*' where those crates exist"
echo ""
echo "2. Update ${CODE_CRATE_PATH}/src/lib.rs:"
echo "   - Remove the 'pub use codex_*::*;' wrapper line"
echo "   - Copy the module structure from codex-rs/lib.rs"
echo ""
echo "3. Update ${CODE_CRATE_PATH}/src/main.rs (if binary):"
echo "   - Change 'codex_*::' to 'code_*::'"
echo ""
echo "4. Update all source files to use code_* instead of codex_*:"
echo "   find ${CODE_CRATE_PATH}/src -name '*.rs' -exec sed -i 's/codex_core/code_core/g' {} +"
echo "   find ${CODE_CRATE_PATH}/src -name '*.rs' -exec sed -i 's/codex_${CRATE_NAME//-/_}/code_${CRATE_NAME//-/_}/g' {} +"
echo ""
echo "5. Update workspace Cargo.toml:"
echo "   - Remove 'codex-${CRATE_NAME} = { path = \"../codex-rs/${CRATE_NAME}\" }'"
echo ""
echo "6. Build and verify:"
echo "   cargo build -p code-${CRATE_NAME}"
echo "   cargo test -p code-${CRATE_NAME}"
echo ""
echo -e "${GREEN}Migration setup complete! Follow the manual steps above.${NC}"
