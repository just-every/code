use chrono::{DateTime, Utc};
use code_protocol::openai_models::ModelInfo;
use serde::{Deserialize, Serialize};
use std::io;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ModelsCache {
    pub(crate) fetched_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) etag: Option<String>,
    pub(crate) models: Vec<ModelInfo>,
}

pub(crate) fn is_fresh(fetched_at: DateTime<Utc>, ttl: Duration) -> bool {
    if ttl.is_zero() {
        return false;
    }
    let Ok(ttl_duration) = chrono::Duration::from_std(ttl) else {
        return false;
    };
    let age = Utc::now().signed_duration_since(fetched_at);
    age <= ttl_duration
}

pub(crate) fn load_cache(path: &Path) -> io::Result<Option<ModelsCache>> {
    match std::fs::read(path) {
        Ok(contents) => {
            let cache = serde_json::from_slice(&contents)
                .map_err(|err| io::Error::new(ErrorKind::InvalidData, err.to_string()))?;
            Ok(Some(cache))
        }
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

pub(crate) fn save_cache(path: &Path, cache: &ModelsCache) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let json = serde_json::to_vec_pretty(cache)
        .map_err(|err| io::Error::new(ErrorKind::InvalidData, err.to_string()))?;

    let tmp_path = tmp_path_for(path);
    std::fs::write(&tmp_path, json)?;
    std::fs::rename(&tmp_path, path)
}

fn tmp_path_for(path: &Path) -> PathBuf {
    let mut tmp = path.as_os_str().to_os_string();
    tmp.push(".tmp");
    PathBuf::from(tmp)
}

