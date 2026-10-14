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
/// `(version, page URL)`. A pre-release build (`1.0.0-beta.1`) also sees newer
/// pre-releases; a release build only sees releases.
pub fn newer_release(repo: &str, current: &str) -> Result<Option<(String, String)>, String> {
    let pre = current.contains('-');
    let api = if pre {
        format!("https://api.github.com/repos/{repo}/releases?per_page=20")
    } else {
        format!("https://api.github.com/repos/{repo}/releases/latest")
    };
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
    let releases = match v {
        serde_json::Value::Array(a) => a,
        one => vec![one],
    };
    Ok(newest(&releases, current))
}

/// The newest published release in a GitHub API list that is newer than
/// `current`.
fn newest(releases: &[serde_json::Value], current: &str) -> Option<(String, String)> {
    releases
        .iter()
        .filter(|r| !r.get("draft").and_then(|d| d.as_bool()).unwrap_or(false))
        .filter_map(|r| {
            let tag = r.get("tag_name")?.as_str()?;
            let url = r
                .get("html_url")
                .and_then(|u| u.as_str())
                .unwrap_or_default();
            is_newer(tag, current).then(|| (tag.to_owned(), url.to_owned()))
        })
        .reduce(|a, b| if is_newer(&b.0, &a.0) { b } else { a })
        .map(|(tag, url)| (tag.trim_start_matches('v').to_owned(), url))
}

/// Whether version `a` (`v1.2.3`, `1.2`, `1.0.0-beta.2`) is newer than `b`, in
/// SemVer order: a pre-release comes before its release, and pre-release
/// identifiers compare numerically when both are numbers.
pub fn is_newer(a: &str, b: &str) -> bool {
    let split = |s: &str| -> (Vec<u64>, Option<String>) {
        let s = s.trim().trim_start_matches('v');
        let s = s.split('+').next().unwrap_or("");
        let (core, pre) = match s.split_once('-') {
            Some((c, p)) => (c, Some(p.to_owned())),
            None => (s, None),
        };
        (
            core.split('.').map(|p| p.parse().unwrap_or(0)).collect(),
            pre,
        )
    };
    let ((x, xp), (y, yp)) = (split(a), split(b));
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
    match (xp, yp) {
        (None, Some(_)) => true,
        (Some(_), None) | (None, None) => false,
        (Some(p), Some(q)) => pre_cmp(&p, &q).is_gt(),
    }
}

/// SemVer pre-release precedence (`beta.2` < `beta.10` < `rc.1`).
fn pre_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let (mut x, mut y) = (a.split('.'), b.split('.'));
    loop {
        match (x.next(), y.next()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(p), Some(q)) => {
                let o = match (p.parse::<u64>(), q.parse::<u64>()) {
                    (Ok(m), Ok(n)) => m.cmp(&n),
                    (Ok(_), Err(_)) => std::cmp::Ordering::Less,
                    (Err(_), Ok(_)) => std::cmp::Ordering::Greater,
                    (Err(_), Err(_)) => p.cmp(q),
                };
                if o.is_ne() {
                    return o;
                }
            }
        }
    }
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

    #[test]
    fn pre_releases_in_semver_order() {
        assert!(is_newer("v1.0.0-beta.2", "1.0.0-beta.1"));
        assert!(is_newer("v1.0.0-beta.10", "1.0.0-beta.9"));
        assert!(is_newer("v1.0.0-rc.1", "1.0.0-beta.3"));
        assert!(is_newer("v1.0.0", "1.0.0-beta.1"));
        assert!(!is_newer("v1.0.0-beta.1", "1.0.0"));
        assert!(!is_newer("v1.0.0-beta.1", "1.0.0-beta.1"));
        assert!(is_newer("v1.0.0-beta.1", "0.1.0"));
        let list = serde_json::json!([
            {"tag_name": "v1.0.0-beta.3", "html_url": "u3", "draft": true},
            {"tag_name": "v1.0.0-beta.2", "html_url": "u2"},
            {"tag_name": "v1.0.0-beta.1", "html_url": "u1"},
        ]);
        let list = list.as_array().unwrap();
        assert_eq!(
            newest(list, "1.0.0-beta.1"),
            Some(("1.0.0-beta.2".into(), "u2".into()))
        );
        assert_eq!(newest(list, "1.0.0-beta.2"), None);
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
