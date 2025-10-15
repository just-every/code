use std::fs;
use std::path::PathBuf;

/// Build a reqwest Client with optional extra root certificates loaded from
/// common environment variables (SSL_CERT_FILE, REQUESTS_CA_BUNDLE,
/// NODE_EXTRA_CA_CERTS). This helps environments using corporate/mitm proxies
/// whose CAs are distributed via system config or user-provided files.
pub fn build_http_client() -> reqwest::Client {
    let mut builder = reqwest::Client::builder();

    // Helper to load a PEM or DER certificate file if it exists and is readable.
    fn load_cert(path: PathBuf) -> Option<reqwest::Certificate> {
        if !path.exists() || !path.is_file() {
            return None;
        }
        let bytes = fs::read(&path).ok()?;
        reqwest::Certificate::from_pem(&bytes)
            .or_else(|_| reqwest::Certificate::from_der(&bytes))
            .ok()
    }

    // Single-file variables (common across ecosystems)
    for var in ["SSL_CERT_FILE", "REQUESTS_CA_BUNDLE", "NODE_EXTRA_CA_CERTS"] {
        if let Ok(val) = std::env::var(var) {
            if !val.trim().is_empty() {
                if let Some(cert) = load_cert(PathBuf::from(val)) {
                    builder = builder.add_root_certificate(cert);
                }
            }
        }
    }

    // Directory of certs (PEM/CRT). Iterate best-effort.
    if let Ok(dir) = std::env::var("SSL_CERT_DIR") {
        let path = PathBuf::from(dir);
        if path.is_dir() {
            if let Ok(rd) = fs::read_dir(path) {
                for entry in rd.flatten() {
                    let p = entry.path();
                    if p.extension()
                        .and_then(|s| s.to_str())
                        .map(|s| matches!(s, "crt" | "pem" | "der"))
                        .unwrap_or(false)
                    {
                        if let Some(cert) = load_cert(p) {
                            builder = builder.add_root_certificate(cert);
                        }
                    }
                }
            }
        }
    }

    builder.build().unwrap_or_else(|_| reqwest::Client::new())
}
