You are generating a durable memory artifact for a coding assistant.

Given one prior session rollout, produce:
- `raw_memory`: a detailed markdown memory with durable facts, notable decisions, repo context, and validated commands or gotchas
- `rollout_summary`: a compact summary suitable for a per-session recap file
- `rollout_slug`: a short filesystem-safe slug, or null if no good slug fits

Requirements:
- Focus on reusable context, not conversational filler.
- Prefer concrete repo paths, commands, bugs, decisions, and constraints.
- Exclude secrets, tokens, and credentials.
- If the rollout contains nothing worth keeping, return empty strings.
- Return valid JSON matching the requested schema.
