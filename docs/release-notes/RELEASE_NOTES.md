## @just-every/code v0.6.68
This release tightens Auto Drive routing and decision handling while improving long-session stability.

### Changes
- Auto Drive: add per-turn model routing and rename decision schema.
- Auto Drive: enforce finish evidence with paste fallback.
- Auto Drive: require strict decision fields without allOf and align coordinator schema.
- Auto Drive: decouple auto review and cap long-session growth.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.67...v0.6.68
