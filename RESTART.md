# Restart Plan: Spec Kit Multi-Agent Pipeline

## Context Recap
- Branch: `spec-auto-telemetry`
- New commands: `/spec-plan`, `/spec-tasks`, `/spec-implement`, `/spec-validate`, `/spec-review`, `/spec-unlock`, `/spec-auto`.
- Guardrail wrappers `/spec-ops-*` remain and now mention the follow-up multi-agent stage.
- `docs/spec-kit/prompts.json` provides stage prompts for Gemini Ultra, Claude MAX, and GPT Pro. Loader lives in `codex-rs/tui/src/spec_prompts.rs`.
- `/spec-auto` currently chains guardrail commands and loads prompts, but the evidence workflow/self-healing logic introduced in `chatwidget.rs` still needs to be completed (compilation failing due to borrow issues).

## TODO Summary
1. Resolve Rust borrow/lifetime errors around the spec-auto state machine in `codex-rs/tui/src/chatwidget.rs` (~lines 15500-15690).
   - Refactor to avoid borrowing `self` mutably while calling helper methods that also take `&mut self`.
   - Potential approach: build the next action from the state first, then invoke helper functions with the necessary data once the borrow ends.
2. Ensure telemetry parsing uses `SystemTime` import if needed.
3. Re-run `cargo test -p codex-tui spec_prompts` until it passes.
4. Plan local-memory write-backs for guardrail outcomes once compilation succeeds.

## Local Memory References
- Prompt addition: mem `8d7681a2-ebe9-43da-bbad-04d79aa01578`
- /spec-auto telemetry summary: mem `a13178aa-d909-4c14-b94a-3cf11e5f2c4a`
- Local memory guardrail instructions: mem `f12d5830-942d-4236-a2e1-6cba91c294a1`

## Next Steps after Fixes
- Integrate MCP/local-memory logging of guardrail telemetry for `/spec-auto`.
- Decide whether prompts should auto-run agents or continue to require user confirmation.
- Run the broader test suite once the slash-command flow compiles cleanly.
