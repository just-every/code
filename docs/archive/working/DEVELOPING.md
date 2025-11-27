# Development Notes

## Testing / Linting

- `./build-fast.sh`
- `pnpm install && pnpm lint`

### Build cache layout and sccache

The fast build now chooses a cache bucket per Git worktree or branch so
concurrent builds no longer fight over `target`. Each bucket name includes a
hash of the raw ref to keep slashes and similarly named branches distinct. You
can override the default bucket with `BUILD_FAST_CACHE_KEY=my-feature` when you
need a stable directory for long lived experiments. The active bucket is echoed
at the start of each run.

Rust compiler outputs are still shared through `sccache` when it is available.
For best results in a multi-worktree setup, configure a single `sccache`
back-end (local disk or remote Redis/S3) and mount each checkout at a consistent
absolute path so cache keys line up. Environment variables like
`SCCACHE_ENDPOINT`, `SCCACHE_BUCKET`, or `SCCACHE_DIR` are respected if you need
to point at a central cache server.

## Auto Drive Fault Injection (development only)

Some retry/backoff paths are easier to exercise with injectable failures. Build
the TUI with the `dev-faults` feature and set the appropriate environment
variables before launching:

```bash
cd code-rs
cargo run --bin code-tui --features dev-faults
```

Available knobs (all optional):

- `CODEX_FAULTS_SCOPE`: currently only `auto_drive` is supported. Faults are
  ignored for other scopes.
- `CODEX_FAULTS`: comma-delimited list of `kind:count`. Supported kinds:
  - `disconnect` – simulate a transient “stream disconnected” error.
  - `429` – simulate a rate-limit response.
  Example: `CODEX_FAULTS="disconnect:2,429:1"` will fire two disconnects
  followed by one rate-limit.
- `CODEX_FAULTS_429_RESET`: optional reset hint for the 429 fault. Accepted
  values:
  - integer seconds (`120`)
  - `now+90s`
  - RFC3339 timestamp (`2025-09-29T12:00:00Z`)

When a fault fires, the CLI logs a `[faults] …` warning including the remaining
count. Without the feature flag (default builds), all hooks compile out and
production behaviour is unaffected.

## Auto Drive review preconditions

- The Auto Drive coordinator now emits a `review_commit` payload alongside
  `turn_descriptor` when it wants to enter review mode. The field identifies the
  artifact under review:
  - `{ "source": "staged" }` -> review the currently staged diff.
  - `{ "source": "commit", "sha": "…" }` -> review the specified commit.
- The TUI rejects review turns unless the referenced commit exists or staged
  changes are present. When the check fails, Auto Drive asks the CLI to stage or
  commit the outstanding work before retrying.
