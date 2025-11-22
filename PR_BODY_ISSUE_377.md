## Summary
- wrap model stream connect errors with contextual guidance so DNS/connection failures no longer surface as opaque reqwest messages
- treat timeouts and DNS resolution failures as `CodexErr::Stream` with actionable hints (e.g., check `/etc/resolv.conf`)
- add unit coverage for the DNS hint helper

## Testing
- cargo test -p code-core chat_completions::tests::dns_hint_matches_common_messages
- ./build-fast.sh

Fixes #377.
