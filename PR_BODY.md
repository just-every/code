## Summary
- abort PTY reader tasks after forced termination so long-running commands (e.g., `dotnet build`) stop hanging on EOF
- join stdout/stderr readers with a short timeout during normal shutdown to guard against pipes left open by orphaned grandchildren
- add an integration regression test that spawns a noisy python loop, calls `kill_all()`, and asserts the exec request completes promptly

## Testing
- ./build-fast.sh
- cargo test -p code-core --test dotnet_build_hang *(fails: upstream `cc` 1.2.41 crate is missing generated modules; clear/update the registry and rerun)*

Closes #5946.
