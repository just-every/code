## Summary
- add explicit phase context to initialize/list/call MCP failures so logs show where the request died
- retry the Streamable HTTP initialize handshake once on obvious transient network errors before surfacing failure
- cover the new helpers with unit tests for phase labeling and retry gating

## Testing
- cargo test -p code-rmcp-client
- ./build-fast.sh
