use std::fs;
use std::path::PathBuf;

pub const CODEX_CA_CERTIFICATE_ENV_VAR: &str = "CODEX_CA_CERTIFICATE";

pub fn apply_extra_root_certificates(
    mut builder: reqwest::ClientBuilder,
) -> reqwest::ClientBuilder {
    fn load_cert(path: PathBuf) -> Option<reqwest::Certificate> {
        if !path.exists() || !path.is_file() {
            return None;
        }
        let bytes = fs::read(&path).ok()?;
        reqwest::Certificate::from_pem(&bytes)
            .or_else(|_| reqwest::Certificate::from_der(&bytes))
            .ok()
    }

    let codex_ca_certificate = std::env::var(CODEX_CA_CERTIFICATE_ENV_VAR)
        .ok()
        .filter(|value| !value.trim().is_empty());
    let ssl_cert_file = std::env::var("SSL_CERT_FILE")
        .ok()
        .filter(|value| !value.trim().is_empty());

    // Let the Codex-specific override win over general SSL_CERT_FILE while
    // still honoring other common CA-bundle env vars for existing workflows.
    let mut single_file_candidates = Vec::new();
    if let Some(path) = codex_ca_certificate {
        single_file_candidates.push(path);
    } else if let Some(path) = ssl_cert_file {
        single_file_candidates.push(path);
    }
    if let Ok(path) = std::env::var("REQUESTS_CA_BUNDLE")
        && !path.trim().is_empty()
    {
        single_file_candidates.push(path);
    }
    if let Ok(path) = std::env::var("NODE_EXTRA_CA_CERTS")
        && !path.trim().is_empty()
    {
        single_file_candidates.push(path);
    }

    for path in single_file_candidates {
        if let Some(cert) = load_cert(PathBuf::from(path)) {
            builder = builder.add_root_certificate(cert);
        }
    }

    if let Ok(dir) = std::env::var("SSL_CERT_DIR") {
        let path = PathBuf::from(dir);
        if path.is_dir() && let Ok(rd) = fs::read_dir(path) {
            for entry in rd.flatten() {
                let p = entry.path();
                if p
                    .extension()
                    .and_then(|s| s.to_str())
                    .map(|s| matches!(s, "crt" | "pem" | "der"))
                    .unwrap_or(false)
                    && let Some(cert) = load_cert(p)
                {
                    builder = builder.add_root_certificate(cert);
                }
            }
        }
    }

    builder
}

/// Build a reqwest Client with optional extra root certificates loaded from
/// common environment variables. `CODEX_CA_CERTIFICATE` takes precedence over
/// `SSL_CERT_FILE`, and other ecosystem-standard CA bundle variables continue
/// to work as fallbacks.
pub fn build_http_client() -> reqwest::Client {
    apply_extra_root_certificates(reqwest::Client::builder())
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}
