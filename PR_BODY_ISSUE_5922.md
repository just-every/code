## Summary
- treat Windows AltGr (Control+Alt) key chords as printable characters so `/`, `@`, and other symbols insert correctly in the composer and terminal cells
- keep Ctrl+Alt shortcuts (e.g., Ctrl+Alt+H delete word) by excluding ASCII letters from the AltGr path and add Windows-only regression tests
- cover the change with new `TextArea` unit tests and a ComposerInput integration-style test to prevent future regressions

## Testing
- ./build-fast.sh
- cargo test -p code-tui --test windows_altgr -- --ignored *(fails: local cargo registry copy of `cc` 1.2.41 is missing generated modules; clear/update the registry and rerun on Windows)*

Closes #5922.
