//! Shared on-disk candidate set for managed daemon restarts.

use std::collections::BTreeSet;
use std::io;
use std::path::Path;

use codex_core::path_utils::write_atomically;
pub fn read_candidates(path: &Path) -> io::Result<BTreeSet<String>> {
    match std::fs::read(path) {
        Ok(contents) => serde_json::from_slice(&contents).map_err(io::Error::other),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(BTreeSet::new()),
        Err(err) => Err(err),
    }
}

pub fn write_candidates(path: &Path, candidates: &BTreeSet<String>) -> io::Result<()> {
    write_atomically(
        path,
        &serde_json::to_string(candidates).map_err(io::Error::other)?,
    )
}
