## @just-every/code v0.6.102

This release improves compatibility with strict OpenAI-compatible chat providers.

### Changes

- Core: prevent strict providers from rejecting developer or custom chat roles.
- Models: apply chat-role normalization only to providers that require it.

### Install

```bash
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.101...v0.6.102
