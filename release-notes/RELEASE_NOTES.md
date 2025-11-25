## @just-every/code v0.5.5
Fresh automation and UX polish to make prompt management, streaming visibility, and model picks smoother for this release.

### Changes
- Auto Drive: restore 600-char CLI prompts, enforce sane bounds, add fallback to the current binary, and append test guidance to each goal for smoother automation handoffs.
- TUI/Prompts: add a full management section with save/reload, slash access, and alias autocomplete so custom prompts stay at your fingertips.
- Streaming: show reconnecting spinners, log retry causes, and classify more transient errors so network hiccups stay visible without noise.
- Agents: retier frontline options, upgrade opus/gemini defaults, and tighten descriptions to highlight the recommended models.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.5.4...v0.5.5
