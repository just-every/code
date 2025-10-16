# Spec-Kit Module Architecture

**Version:** 1.0
**Date:** 2025-10-15
**Status:** Production

See REFACTORING_COMPLETE_SUMMARY.md for complete architectural documentation.

## Quick Reference

**Location:** `tui/src/chatwidget/spec_kit/`

**Modules:**
- `consensus.rs` (953 lines) - Consensus checking
- `guardrail.rs` (424 lines) - Validation
- `handler.rs` (582 lines) - Command handlers  
- `state.rs` (244 lines) - State types
- `mod.rs` (32 lines) - Exports

**Pattern:** Friend module + free functions to avoid borrow checker conflicts

**Documentation:** See REFACTORING_COMPLETE_SUMMARY.md for full details
