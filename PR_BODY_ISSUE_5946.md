## Summary
- abort PTY reader tasks when a shell session is force-killed (timeout or user interrupt) so we don't wait forever for EOF from orphaned grandchildren
- join stdout/stderr readers with a short timeout during normal completion to avoid indefinite hangs when a child leaves pipes open
- add an exec regression test that kills a long-running python loop and ensures `kill_all()` unblocks pending exec requests

## Testing
- ./build-fast.sh
- cargo test -p code-core --test dotnet_build_hang *(fails: upstream `cc` 1.2.41 crate is missing generated modules in the local cargo registry; clear/update the registry and rerun)*

Closes #5946.
