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
    pub setup_done: bool,
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PageTheme {
    #[default]
    System,
    Light,
    Sepia,
    Grey,
    Dark,
    Black,
    Ocean,
}

impl PageTheme {
    pub const ALL: [PageTheme; 7] =
        [PageTheme::System, PageTheme::Light, PageTheme::Sepia, PageTheme::Grey, PageTheme::Dark, PageTheme::Black, PageTheme::Ocean];

    pub fn label(self) -> &'static str {
        match self {
            PageTheme::System => "System",
            PageTheme::Light => "Light",
            PageTheme::Sepia => "Sepia",
            PageTheme::Grey => "Grey",
            PageTheme::Dark => "Dark",
            PageTheme::Black => "Black",
            PageTheme::Ocean => "Ocean",
        }
    }
}

// Furigana drawn always, never (taking no room), or faint until the word is looked up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Furigana {
    #[default]
    Show,
    Hide,
    Blur,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WordColours {
    Off,
    Underline,
    #[default]
    Text,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Typeface {
    Serif,
    #[default]
    Sans,
}

// How the page looks. Kept apart from the reviews settings, which hold keys:
// these change with every slider step.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PageSettings {
    pub theme: PageTheme,
    pub vertical: bool,
    pub font_size: f32,
    pub line_height: f32,
    pub margin: f32,
    // 0 lets a line fill the page.
    pub max_chars_per_line: u32,
    pub typeface: Typeface,
    pub furigana: Furigana,
    pub colours: WordColours,
    pub tap_to_turn: bool,
    pub keep_awake: bool,
    pub show_percent: bool,
}

impl Default for PageSettings {
    fn default() -> Self {
        PageSettings {
            theme: PageTheme::System,
            vertical: false,
            font_size: 28.0,
            line_height: 1.8,
            margin: 24.0,
            max_chars_per_line: 0,
            typeface: Typeface::Sans,
            furigana: Furigana::Show,
            colours: WordColours::Text,
            tap_to_turn: false,
            keep_awake: false,
            show_percent: false,
        }
    }
}

impl PageSettings {
    fn path(data_dir: &Path) -> PathBuf {
        data_dir.join("page.json")
    }

    pub fn load(data_dir: &Path) -> Self {
        read_json_or_default(&Self::path(data_dir))
    }

    pub fn save(&self, data_dir: &Path) -> std::io::Result<()> {
        write_json(&Self::path(data_dir), self)
    }
}
