## @just-every/code v0.5.2
This patch improves agent defaults and keeps reasoning effort within provider-supported bounds.

### Changes
- Agents: default automation flows to gpt-5.1-codex-max and add gemini-3-pro as an option for higher-capacity runs.
- Models: clamp reasoning effort to supported bands so prompts no longer fail with invalid request errors.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.5.1...v0.5.2
