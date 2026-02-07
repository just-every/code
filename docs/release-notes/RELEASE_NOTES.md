## @just-every/code v0.6.60
This release tightens version compatibility handling for more predictable upgrades.

### Changes
- Core/Version: enforce minimum wire-compatible version and derive it from the announcement tip.
- Core/Version: strip punctuation when scanning minimum semver for wire compatibility.
- Core/Provider: clamp the OpenAI version header for stability.

### Install
```bash
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.59...v0.6.60
