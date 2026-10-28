// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

// The update check of the GitHub builds (AppImage, .exe, APK): the one call to the internet
// the reader didn't set up, made when the app starts, and one they can turn off.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::library::fsutil::{read_json_or_default, write_json};

const LATEST: &str = "https://api.github.com/repos/Calonca/rubiyomi/releases/latest";
pub const RELEASES: &str = "https://github.com/Calonca/rubiyomi/releases";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateSettings {
    pub check_on_start: bool,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        UpdateSettings { check_on_start: true }
    }
}

impl UpdateSettings {
    fn path(data_dir: &Path) -> PathBuf {
        data_dir.join("updates.json")
    }

    pub fn load(data_dir: &Path) -> Self {
        read_json_or_default(&Self::path(data_dir))
    }

    pub fn save(&self, data_dir: &Path) -> std::io::Result<()> {
        write_json(&Self::path(data_dir), self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Asset {
    pub name: String,
    #[serde(rename = "browser_download_url")]
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Release {
    #[serde(rename = "tag_name")]
    pub tag: String,
    #[serde(rename = "html_url")]
    pub page: String,
    #[serde(default, rename = "body")]
    pub notes: Option<String>,
    pub assets: Vec<Asset>,
}

impl Release {
    pub fn version(&self) -> &str {
        self.tag.trim_start_matches('v')
    }

    // The file for this kind of build, by the end of its name ("-x86_64.AppImage").
    pub fn asset(&self, suffix: &str) -> Option<&Asset> {
        self.assets.iter().find(|a| a.name.ends_with(suffix))
    }
}

// Whether `latest` is a later release than `current`. Both are plain x.y.z; one that isn't
// never counts as newer, so a tag we can't read offers nothing.
pub fn newer(latest: &str, current: &str) -> bool {
    fn parse(v: &str) -> Option<Vec<u64>> {
        let v = v.trim().trim_start_matches('v');
        let parts: Option<Vec<u64>> = v.split('.').map(|p| p.parse().ok()).collect();
        parts.filter(|p| !p.is_empty() && p.len() <= 4)
    }
    match (parse(latest), parse(current)) {
        (Some(mut l), Some(mut c)) => {
            let n = l.len().max(c.len());
            l.resize(n, 0);
            c.resize(n, 0);
            l > c
        }
        _ => false,
    }
}

fn hex(digest: &[u8]) -> String {
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

// Checks a SHA-256 against a checksum file as `sha256sum` writes it: "<hex>  <name>", or the hex alone.
pub fn verify(actual: &str, checksum_file: &str) -> Result<(), String> {
    let expected = checksum_file.split_whitespace().next().unwrap_or_default().to_ascii_lowercase();
    if expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("the checksum file holds no SHA-256".into());
    }
    if actual != expected {
        return Err("the download doesn't match its checksum".into());
    }
    Ok(())
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_global(Some(Duration::from_secs(900)))
        .user_agent(concat!("Rubiyomi/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

// `feed` replaces GitHub only to try the update flow, with tool/update-feed.py.
pub fn latest(feed: Option<&str>) -> Result<Release, String> {
    let mut response = agent()
        .get(feed.unwrap_or(LATEST))
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("couldn't reach GitHub: {e}"))?;
    let body = response.body_mut().read_to_string().map_err(|e| format!("GitHub's answer: {e}"))?;
    serde_json::from_str(&body).map_err(|e| format!("GitHub's answer: {e}"))
}

// Bytes so far and the whole size (0 until known), for a progress bar.
#[derive(Clone, Default)]
pub struct Progress(Arc<(AtomicU64, AtomicU64)>);

impl Progress {
    pub fn fraction(&self) -> Option<f32> {
        let (done, total) = (self.0.0.load(Ordering::Relaxed), self.0.1.load(Ordering::Relaxed));
        (total > 0).then(|| done as f32 / total as f32)
    }
}

// Saves the release's file ending in `suffix` to `to`, once it matches its `.sha256`;
// on any failure nothing is left there.
pub fn download(release: &Release, suffix: &str, to: &Path, progress: &Progress) -> Result<(), String> {
    let asset = release.asset(suffix).ok_or_else(|| format!("{} has no {suffix} file", release.tag))?;
    let checksum = release.asset(&format!("{}.sha256", asset.name)).ok_or_else(|| format!("{} has no checksum", asset.name))?;
    let agent = agent();
    let expected = agent
        .get(&checksum.url)
        .call()
        .and_then(|mut r| r.body_mut().with_config().limit(4096).read_to_string())
        .map_err(|e| format!("downloading the checksum: {e}"))?;
    let mut response = agent.get(&asset.url).call().map_err(|e| format!("downloading {}: {e}", asset.name))?;
    let total = response.body().content_length().unwrap_or(0);
    progress.0.1.store(total, Ordering::Relaxed);
    let part = to.with_extension("part");
    let result = (|| {
        let mut file = std::fs::File::create(&part).map_err(|e| format!("writing {}: {e}", part.display()))?;
        let mut reader = response.body_mut().with_config().limit(1 << 30).reader();
        let mut hasher = Sha256::new();
        let mut buffer = vec![0; 256 * 1024];
        let mut done = 0;
        loop {
            let n = reader.read(&mut buffer).map_err(|e| format!("downloading {}: {e}", asset.name))?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
            file.write_all(&buffer[..n]).map_err(|e| format!("writing {}: {e}", part.display()))?;
            done += n as u64;
            progress.0.0.store(done, Ordering::Relaxed);
        }
        file.sync_all().map_err(|e| e.to_string())?;
        verify(&hex(&hasher.finalize()), &expected)?;
        std::fs::rename(&part, to).map_err(|e| format!("writing {}: {e}", to.display()))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert!(newer("v0.6.0", "0.5.1"));
        assert!(newer("0.5.10", "0.5.9"));
        assert!(newer("1.0", "0.9.9"));
        assert!(newer("0.5.1.1", "0.5.1"));
        assert!(!newer("v0.5.1", "0.5.1"));
        assert!(!newer("0.5.0", "0.5.1"));
        assert!(!newer("0.5.1", "0.5.1.0"));
        assert!(!newer("v0.6.0-beta", "0.5.1"));
        assert!(!newer("", "0.5.1"));
    }

    #[test]
    fn checksums() {
        let actual = hex(&Sha256::digest(b"rubiyomi"));
        assert_eq!(verify(&actual, &format!("{actual}  Rubiyomi-0.6.0-x86_64.AppImage\n")), Ok(()));
        assert_eq!(verify(&actual, &format!("{}\n", actual.to_uppercase())), Ok(()));
        assert!(verify(&hex(&Sha256::digest(b"rubiyomi!")), &actual).is_err());
        assert!(verify(&actual, "").is_err());
        assert!(verify(&actual, "<!DOCTYPE html>").is_err());
    }
}
