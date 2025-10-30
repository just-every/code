## Summary
- allow Windows AltGr combos (Control+Alt) to insert printable characters without swallowing them, while keeping Ctrl+Alt shortcuts intact
- add Windows-only unit tests around `TextArea` and an integration-style ComposerInput test to cover `/`, `@`, and Ctrl+Alt+H
- document the fix via regression tests so future Windows keyboard regressions are caught early

## Testing
- ./build-fast.sh
- cargo test -p code-tui --test windows_altgr -- --ignored *(fails: local cargo registry copy of `cc` 1.2.41 is missing generated modules; clear/update the registry and rerun on Windows)*

Closes #5922.
