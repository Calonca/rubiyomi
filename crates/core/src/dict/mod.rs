// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

pub mod jmdict;

use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::library::fsutil::{read_json, write_json};
use crate::platform::Platform;

pub const IPADIC_ASSET: &str = "ipadic.tar.xz";
pub const JMDICT_ASSET: &str = "jmdict.sqlite.xz";
pub const VERSION_ASSET: &str = "version.json";

pub const FORMAT: u32 = 1;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DictVersion {
    pub format: u32,
    pub jmdict: String,
    #[serde(default)]
    pub jmdict_date: String,
    pub ipadic: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DictState {
    Missing,
    Unpacking(&'static str),
    Ready(DictVersion),
    Failed(String),
}

#[derive(Clone)]
pub struct DictStore {
    dir: PathBuf,
    state: Arc<Mutex<DictState>>,
}

impl DictStore {
    pub fn new(data_dir: &Path) -> Self {
        let dir = data_dir.join("dict");
        let state = match read_json::<DictVersion>(&dir.join(VERSION_ASSET)) {
            Some(v) if v.format == FORMAT && dir.join("ipadic").is_dir() && dir.join("jmdict.sqlite").is_file() => DictState::Ready(v),
            _ => DictState::Missing,
        };
        DictStore { dir, state: Arc::new(Mutex::new(state)) }
    }

    pub fn state(&self) -> DictState {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn set(&self, s: DictState) {
        *self.state.lock().unwrap_or_else(|e| e.into_inner()) = s;
    }

    pub fn ipadic_dir(&self) -> PathBuf {
        self.dir.join("ipadic")
    }

    pub fn jmdict_path(&self) -> PathBuf {
        self.dir.join("jmdict.sqlite")
    }

    pub fn installed(&self) -> Option<DictVersion> {
        match self.state() {
            DictState::Ready(v) => Some(v),
            _ => None,
        }
    }

    pub fn prepare(&self, platform: &dyn Platform) {
        let bundled = platform.bundled(VERSION_ASSET).and_then(|mut r| {
            let mut s = String::new();
            r.read_to_string(&mut s).ok()?;
            serde_json::from_str::<DictVersion>(&s).ok()
        });
        let Some(bundled) = bundled else {
            if self.installed().is_none() {
                self.set(DictState::Failed("This build has no dictionaries: run cargo xtask dict.".into()));
            }
            return;
        };
        if self.installed().is_some_and(|have| have.ipadic == bundled.ipadic && !newer(&bundled.jmdict, &have.jmdict)) {
            return;
        }
        if let Err(e) = self.unpack(platform, &bundled) {
            log::warn!("unpacking the dictionaries: {e}");
            self.set(DictState::Failed(e.to_string()));
        }
    }

    fn unpack(&self, platform: &dyn Platform, version: &DictVersion) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let missing = || std::io::Error::other("a dictionary is missing from the app");
        self.set(DictState::Unpacking("Preparing the word finder"));
        let tar = self.dir.join("ipadic.tar.tmp");
        {
            let source = platform.bundled(IPADIC_ASSET).ok_or_else(missing)?;
            let mut out = std::io::BufWriter::new(std::fs::File::create(&tar)?);
            xz(source, &mut out)?;
        }
        let staging = self.dir.join("ipadic.tmp");
        let _ = std::fs::remove_dir_all(&staging);
        tar::Archive::new(std::fs::File::open(&tar)?).unpack(&staging)?;
        let _ = std::fs::remove_file(&tar);
        let _ = std::fs::remove_dir_all(self.ipadic_dir());
        std::fs::rename(&staging, self.ipadic_dir())?;

        self.set(DictState::Unpacking("Preparing the dictionary"));
        let tmp = self.dir.join("jmdict.sqlite.tmp");
        {
            let source = platform.bundled(JMDICT_ASSET).ok_or_else(missing)?;
            let mut out = std::io::BufWriter::new(std::fs::File::create(&tmp)?);
            xz(source, &mut out)?;
        }
        std::fs::rename(&tmp, self.jmdict_path())?;
        write_json(&self.dir.join(VERSION_ASSET), version)?;
        self.set(DictState::Ready(version.clone()));
        Ok(())
    }
}

pub fn newer(a: &str, b: &str) -> bool {
    let parts = |v: &str| v.split(['.', '+', '-']).map(|p| p.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    parts(a) > parts(b)
}

fn xz(source: Box<dyn Read + Send>, out: &mut impl std::io::Write) -> std::io::Result<()> {
    lzma_rs::xz_decompress(&mut BufReader::with_capacity(1 << 16, source), out).map_err(|e| std::io::Error::other(format!("{e:?}")))
}

#[cfg(test)]
mod tests {
    use super::newer;

    #[test]
    fn versions_compare_by_number() {
        assert!(newer("3.6.10+20270101000000", "3.6.9+20261012204142"));
        assert!(!newer("3.6.2+20261019180022", "3.6.2+20261019180022"));
        assert!(!newer("3.6.2", "3.7.0"));
    }
}
