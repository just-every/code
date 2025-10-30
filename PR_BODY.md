## Summary
- promote `/exit` to a first-class slash command (aliasing `/quit`) and keep dispatch wiring intact
- normalize the parser so `/exit` preserves its spelling, while `/quit` remains supported
- add parser and ChatWidget harness coverage and document the new command in `docs/slash-commands.md`

## Testing
- ./build-fast.sh
- cargo test -p code-tui slash_exit_and_quit_dispatch_exit_command *(fails: local cargo registry copy of `cc` 1.2.41 is missing generated modules; clear/update the crate and rerun)*

Closes #5932.
