// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::io::Read;
use std::path::PathBuf;

use crate::grade::anki::AnkiBackend;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlatformEvent {
    Imported { path: PathBuf, name: String, temporary: bool },
    ImportFailed(String),
    Permission { granted: bool },
    // The system's back gesture, where it doesn't come as a key.
    Back,
}

pub trait Platform: Send + Sync {
    fn data_dir(&self) -> PathBuf;

    fn bundled(&self, name: &str) -> Option<Box<dyn Read + Send>>;

    fn accent_color(&self) -> Option<u32>;

    fn pick_epub(&self);

    fn poll_events(&self) -> Vec<PlatformEvent>;

    fn open_url(&self, url: &str) -> bool;

    fn anki_droid(&self) -> Option<Box<dyn AnkiBackend>> {
        None
    }

    // Installed, and allowed: None where there is no AnkiDroid to ask.
    fn anki_droid_status(&self) -> Option<(bool, bool)> {
        None
    }

    fn request_anki_permission(&self) {}

    // While a book is open, if the reader asked for it.
    fn keep_screen_on(&self, _on: bool) {}

    // Keys go to the system's own store, never into our files. An empty value forgets one.
    fn secret(&self, name: &str) -> Option<String> {
        let _ = name;
        None
    }

    fn set_secret(&self, name: &str, value: &str) -> Result<(), String> {
        let _ = (name, value);
        Err("there is no safe place for keys here".into())
    }
}
