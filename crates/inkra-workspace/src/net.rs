//! HTTP for the few network features SPEC allows (NF-004): remote images the
//! document refers to and the update check. TLS uses the system's trust store.

use std::sync::OnceLock;
use std::time::Duration;

/// Largest response accepted (images).
pub const MAX_BYTES: u64 = 32 * 1024 * 1024;

fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        let tls = ureq::tls::TlsConfig::builder()
            .root_certs(ureq::tls::RootCerts::PlatformVerifier)
            .build();
        ureq::Agent::config_builder()
            .tls_config(tls)
            .timeout_global(Some(Duration::from_secs(20)))
            .user_agent(concat!("Inkra/", env!("CARGO_PKG_VERSION")))
            .max_redirects(5)
            .build()
            .into()
    })
}

/// Downloads `url` (http or https only).
pub fn get(url: &str) -> Result<Vec<u8>, String> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(format!("unsupported URL: {url}"));
    }
    let mut r = agent().get(url).call().map_err(|e| e.to_string())?;
    r.body_mut()
        .with_config()
        .limit(MAX_BYTES)
        .read_to_vec()
        .map_err(|e| e.to_string())
}

/// A release newer than the running version, if GitHub reports one (PR-020):
/// `(version, page URL)`.
pub fn newer_release(repo: &str, current: &str) -> Result<Option<(String, String)>, String> {
    let api = format!("https://api.github.com/repos/{repo}/releases/latest");
    let mut r = agent()
        .get(&api)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| e.to_string())?;
    let body = r
        .body_mut()
        .with_config()
        .limit(1024 * 1024)
        .read_to_string()
        .map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    let tag = v
        .get("tag_name")
        .and_then(|t| t.as_str())
        .unwrap_or_default();
    let url = v
        .get("html_url")
        .and_then(|t| t.as_str())
        .unwrap_or_default()
        .to_owned();
    Ok(is_newer(tag, current).then(|| (tag.trim_start_matches('v').to_owned(), url)))
}

/// Whether version `a` (`v1.2.3`, `1.2`) is newer than `b`.
pub fn is_newer(a: &str, b: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> {
        s.trim()
            .trim_start_matches('v')
            .split(['-', '+'])
            .next()
            .unwrap_or("")
            .split('.')
            .map(|p| p.parse().unwrap_or(0))
            .collect()
    };
    let (x, y) = (parse(a), parse(b));
    if x.iter().all(|n| *n == 0) {
        return false;
    }
    for k in 0..x.len().max(y.len()) {
        let (p, q) = (
            x.get(k).copied().unwrap_or(0),
            y.get(k).copied().unwrap_or(0),
        );
        if p != q {
            return p > q;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_and_schemes() {
        assert!(is_newer("v1.0.0", "0.9.9"));
        assert!(is_newer("0.10.0", "0.9.0"));
        assert!(!is_newer("v0.1.0", "0.1.0"));
        assert!(!is_newer("0.1", "0.1.0"));
        assert!(!is_newer("garbage", "0.1.0"));
        assert!(get("file:///etc/passwd").is_err());
    }
}

#[cfg(test)]
mod online {
    /// Needs network access; run with `INKRA_ONLINE_TESTS=1`.
    #[test]
    fn fetches_over_https() {
        if std::env::var("INKRA_ONLINE_TESTS").is_err() {
            return;
        }
        let b = super::get("https://www.rust-lang.org/static/images/rust-logo-blk.svg").unwrap();
        assert!(String::from_utf8_lossy(&b).contains("<svg"));
    }
}
