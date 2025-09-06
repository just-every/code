//! Private library utilities that are not exposed to clients since we don't
//! want to make semver guarantees about them

use std::path::Path;

use walkdir::WalkDir;

/// Private helper to walk a dir and also follow symbolic links.
pub fn walk_dir<P: AsRef<Path>>(folder: P) -> WalkDir {
    WalkDir::new(folder).follow_links(true)
}
