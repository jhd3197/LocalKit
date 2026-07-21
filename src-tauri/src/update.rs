//! In-app update awareness (plan 25).
//!
//! Releases are unsigned, and `tauri-plugin-updater` requires signed artifacts,
//! so this is deliberately a *checker*, not an updater: ask GitHub for the
//! latest release tag and compare it to the compiled-in version. It never
//! downloads or installs anything — the UI just links to the release page.
//! If releases become signed later, this is the seam to swap for the real
//! updater behind the same Settings row.

use serde::{Deserialize, Serialize};

/// The version this build reports as — the crate version, which the release
/// workflow tags as `vX.Y.Z`.
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

const OWNER_REPO: &str = "jhd3197/LocalKit";
const USER_AGENT: &str = concat!("LocalKit/", env!("CARGO_PKG_VERSION"));

/// Result of an update check, shared by the GUI command and `lk doctor`.
#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    /// This build's version (no leading `v`).
    pub current: String,
    /// The latest published release's version (no leading `v`).
    pub latest: String,
    /// The GitHub release page for `latest`, to open in a browser.
    pub url: String,
    /// Whether `latest` is strictly newer than `current`.
    pub update_available: bool,
}

#[derive(Deserialize)]
struct GhRelease {
    #[serde(default)]
    tag_name: String,
    #[serde(default)]
    html_url: String,
}

/// Ask GitHub for the newest published release and compare it to this build.
///
/// The `/releases/latest` endpoint already excludes drafts and pre-releases, so
/// a checker never nags about an in-progress draft the release workflow left
/// behind. Any network/parse failure is an `Err` the caller treats as "couldn't
/// check" — never as "up to date" and never as a hard failure.
pub async fn check() -> Result<UpdateInfo, String> {
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("failed to build HTTP client: {e}"))?;
    let url = format!("https://api.github.com/repos/{OWNER_REPO}/releases/latest");
    let resp = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("could not reach GitHub to check for updates: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!(
            "GitHub returned HTTP {} when checking for updates.",
            resp.status()
        ));
    }
    let release: GhRelease = resp
        .json()
        .await
        .map_err(|e| format!("could not parse the GitHub release response: {e}"))?;

    let tag = release.tag_name.trim();
    if tag.is_empty() {
        return Err("GitHub did not report a latest release tag.".into());
    }
    let page = if release.html_url.trim().is_empty() {
        format!("https://github.com/{OWNER_REPO}/releases/latest")
    } else {
        release.html_url
    };
    Ok(UpdateInfo {
        current: normalize(CURRENT_VERSION),
        latest: normalize(tag),
        url: page,
        update_available: is_newer(tag, CURRENT_VERSION),
    })
}

/// Strip a leading `v`/`V` so `v0.2.0` and `0.2.0` compare equal.
fn normalize(tag: &str) -> String {
    tag.trim().trim_start_matches(['v', 'V']).to_string()
}

/// Numeric version components, dropping any pre-release suffix (`-rc1`). A part
/// that isn't a number reads as 0, so an unparseable tag degrades to "not
/// newer" rather than a false alarm.
fn parts(v: &str) -> Vec<u64> {
    normalize(v)
        .split('-')
        .next()
        .unwrap_or("")
        .split('.')
        .map(|p| p.parse::<u64>().unwrap_or(0))
        .collect()
}

/// Is `latest` strictly newer than `current`? Compares numeric components left
/// to right, zero-padding the shorter one.
pub fn is_newer(latest: &str, current: &str) -> bool {
    let a = parts(latest);
    let b = parts(current);
    for i in 0..a.len().max(b.len()) {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        if x != y {
            return x > y;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_versions_are_detected() {
        assert!(is_newer("0.2.0", "0.1.1"));
        assert!(is_newer("v0.1.2", "0.1.1"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(is_newer("0.1.10", "0.1.9")); // numeric, not lexical
    }

    #[test]
    fn equal_or_older_is_not_an_update() {
        assert!(!is_newer("0.1.1", "0.1.1"));
        assert!(!is_newer("v0.1.1", "0.1.1")); // the leading v is not a difference
        assert!(!is_newer("0.1.0", "0.1.1"));
        assert!(!is_newer("0.0.9", "0.1.0"));
    }

    #[test]
    fn a_prerelease_suffix_is_ignored_and_junk_never_false_alarms() {
        assert!(!is_newer("0.1.1-rc1", "0.1.1"));
        assert!(is_newer("0.2.0-rc1", "0.1.1"));
        // Totally unparseable tags must never read as an available update.
        assert!(!is_newer("nightly", "0.1.1"));
        assert!(!is_newer("", "0.1.1"));
    }

    #[test]
    fn mismatched_component_counts_zero_pad() {
        assert!(is_newer("0.2", "0.1.9"));
        assert!(!is_newer("0.1", "0.1.0"));
        assert!(is_newer("1", "0.9.9"));
    }
}
