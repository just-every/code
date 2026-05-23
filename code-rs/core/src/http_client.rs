use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::LazyLock;

use reqwest::cookie::CookieStore;
use reqwest::cookie::Jar;
use reqwest::header::HeaderValue;

pub const CODEX_CA_CERTIFICATE_ENV_VAR: &str = "CODEX_CA_CERTIFICATE";

static SHARED_CHATGPT_CLOUDFLARE_COOKIE_STORE: LazyLock<Arc<ChatGptCloudflareCookieStore>> =
    LazyLock::new(|| Arc::new(ChatGptCloudflareCookieStore::default()));

#[derive(Debug, Default)]
struct ChatGptCloudflareCookieStore {
    jar: Jar,
}

impl CookieStore for ChatGptCloudflareCookieStore {
    fn set_cookies(
        &self,
        cookie_headers: &mut dyn Iterator<Item = &HeaderValue>,
        url: &reqwest::Url,
    ) {
        if !is_chatgpt_cookie_url(url) {
            return;
        }

        let mut cloudflare_cookie_headers =
            cookie_headers.filter(|header| is_allowed_cloudflare_set_cookie_header(header));
        self.jar.set_cookies(&mut cloudflare_cookie_headers, url);
    }

    fn cookies(&self, url: &reqwest::Url) -> Option<HeaderValue> {
        if is_chatgpt_cookie_url(url) {
            self.jar.cookies(url).and_then(only_cloudflare_cookies)
        } else {
            None
        }
    }
}

pub fn is_allowed_chatgpt_host(host: &str) -> bool {
    const EXACT_HOSTS: &[&str] = &["chatgpt.com", "chat.openai.com", "chatgpt-staging.com"];
    const SUBDOMAIN_SUFFIXES: &[&str] = &[".chatgpt.com", ".chatgpt-staging.com"];

    EXACT_HOSTS.contains(&host)
        || SUBDOMAIN_SUFFIXES
            .iter()
            .any(|suffix| host.ends_with(suffix))
}

pub fn with_chatgpt_cloudflare_cookie_store(
    builder: reqwest::ClientBuilder,
) -> reqwest::ClientBuilder {
    builder.cookie_provider(Arc::clone(&SHARED_CHATGPT_CLOUDFLARE_COOKIE_STORE))
}

fn is_chatgpt_cookie_url(url: &reqwest::Url) -> bool {
    if url.scheme() != "https" {
        return false;
    }

    let Some(host) = url.host_str() else {
        return false;
    };

    is_allowed_chatgpt_host(host)
}

fn is_allowed_cloudflare_set_cookie_header(header: &HeaderValue) -> bool {
    header
        .to_str()
        .ok()
        .and_then(set_cookie_name)
        .is_some_and(is_allowed_cloudflare_cookie_name)
}

fn set_cookie_name(header: &str) -> Option<&str> {
    let (name, _) = header.split_once('=')?;
    let name = name.trim();
    (!name.is_empty()).then_some(name)
}

fn only_cloudflare_cookies(header: HeaderValue) -> Option<HeaderValue> {
    let header = header.to_str().ok()?;
    let cookies = header
        .split(';')
        .filter_map(|cookie| {
            let cookie = cookie.trim();
            let name = cookie.split_once('=')?.0.trim();
            is_allowed_cloudflare_cookie_name(name).then_some(cookie)
        })
        .collect::<Vec<_>>()
        .join("; ");

    if cookies.is_empty() {
        None
    } else {
        HeaderValue::from_str(&cookies).ok()
    }
}

fn is_allowed_cloudflare_cookie_name(name: &str) -> bool {
    matches!(
        name,
        "__cf_bm"
            | "__cflb"
            | "__cfruid"
            | "__cfseq"
            | "__cfwaitingroom"
            | "_cfuvid"
            | "cf_clearance"
            | "cf_ob_info"
            | "cf_use_ob"
    ) || name.starts_with("cf_chl_")
}

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
    apply_extra_root_certificates(with_chatgpt_cloudflare_cookie_store(
        reqwest::Client::builder(),
    ))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}
