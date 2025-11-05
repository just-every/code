use env_flags::env_flags;

env_flags! {
    /// Fixture path for offline tests (see client.rs).
    pub CODEX_RS_SSE_FIXTURE: Option<&str> = None;

    /// Enable context timeline delta tracking (Phase 2).
    pub CTX_DELTAS: bool = false;

    /// Enable context timeline snapshot storage (Phase 2).
    pub CTX_SNAPSHOTS: bool = false;

    /// Enable context timeline UI features (future phases).
    pub CTX_UI: bool = false;
}
