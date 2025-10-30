## Summary
- preserve `PATH` (and `NVM_DIR` when present) across shell environment filtering so workspace commands like `npm` remain available
- continue to respect `use_profile` so commands run through the user's login shell when configured
- add unit coverage for the environment builder and an integration-style npm smoke test (skips automatically if npm is unavailable)

## Testing
- ./build-fast.sh
- cargo test -p code-core --test npm_command *(fails: local cargo registry copy of `cc` 1.2.41 is missing generated modules; clear/update the registry and rerun)*

Closes #5925.
