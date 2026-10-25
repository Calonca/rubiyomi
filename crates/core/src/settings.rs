// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::grade::anki::AnkiMapping;
use crate::library::fsutil::{read_json_or_default, write_json};
use crate::platform::Platform;

const JPDB_KEY: &str = "jpdb-key";
const ANKI_KEY: &str = "anki-key";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReviewSource {
    #[default]
    None,
    Jpdb,
    Anki,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub reviews: ReviewSource,
    #[serde(skip)]
    pub jpdb_key: String,
    pub jpdb_deck: String,
    pub jpdb_deck_name: String,
    pub anki_url: String,
    #[serde(skip)]
    pub anki_key: String,
    pub anki: AnkiMapping,
}

impl Settings {
    fn path(data_dir: &Path) -> PathBuf {
        data_dir.join("settings.json")
    }

    pub fn load(platform: &dyn Platform) -> Self {
        let mut me: Settings = read_json_or_default(&Self::path(&platform.data_dir()));
        me.jpdb_key = platform.secret(JPDB_KEY).unwrap_or_default();
        me.anki_key = platform.secret(ANKI_KEY).unwrap_or_default();
        me
    }

    pub fn save(&self, platform: &dyn Platform) -> Result<(), String> {
        write_json(&Self::path(&platform.data_dir()), self).map_err(|e| e.to_string())?;
        platform.set_secret(JPDB_KEY, self.jpdb_key.trim())?;
        platform.set_secret(ANKI_KEY, self.anki_key.trim())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_never_reach_the_file() {
        let s = Settings { jpdb_key: "secret-jpdb".into(), anki_key: "secret-anki".into(), jpdb_deck: "5".into(), ..Settings::default() };
        let json = serde_json::to_string(&s).unwrap();
        assert!(!json.contains("secret"), "{json}");
        assert!(json.contains("\"5\""));
    }
}
