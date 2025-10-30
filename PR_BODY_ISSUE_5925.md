## Summary
- always preserve `PATH` (and `NVM_DIR`, if present) through `ShellEnvironmentPolicy` filtering so npm remains discoverable
- continue to wrap commands in the user shell when `use_profile` is enabled, ensuring profile-managed Node installations work
- add unit coverage for the environment builder and integration-style npm smoke tests (skipped automatically when npm is absent)

## Testing
- ./build-fast.sh
- cargo test -p code-core --test npm_command *(fails: local cargo registry copy of `cc` 1.2.41 is missing generated modules; clear/update the crate cache and rerun)*

Closes #5925.
