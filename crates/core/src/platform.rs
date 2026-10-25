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
}

pub type Insets = [f32; 4];

pub trait Platform: Send + Sync {
    fn data_dir(&self) -> PathBuf;

    fn bundled(&self, name: &str) -> Option<Box<dyn Read + Send>>;

    fn accent_color(&self) -> Option<u32>;

    fn dark_mode(&self) -> Option<bool> {
        None
    }

    fn pick_epub(&self);

    fn poll_events(&self) -> Vec<PlatformEvent>;

    fn set_waker(&self, wake: Box<dyn Fn() + Send + Sync>);

    fn insets(&self) -> Insets;

    fn open_url(&self, url: &str) -> bool;

    fn to_background(&self) -> bool;

    fn anki_droid(&self) -> Option<Box<dyn AnkiBackend>> {
        None
    }

    // Installed, and allowed: None where there is no AnkiDroid to ask.
    fn anki_droid_status(&self) -> Option<(bool, bool)> {
        None
    }

    fn request_anki_permission(&self) {}

    // Software keyboard height in pixels; 0 when hidden or not known.
    fn keyboard(&self) -> f32 {
        0.0
    }

    // Where egui can't reach the clipboard itself (Android).
    fn can_paste(&self) -> bool {
        false
    }

    fn clipboard_text(&self) -> Option<String> {
        None
    }

    // False where egui's own clipboard works.
    fn copy_text(&self, text: &str) -> bool {
        let _ = text;
        false
    }

    // Keys go to the system's own store, never into our files. An empty value forgets one.
    // While a book is open, if the reader asked for it.
    fn keep_screen_on(&self, _on: bool) {}

    fn secret(&self, name: &str) -> Option<String> {
        let _ = name;
        None
    }

    fn set_secret(&self, name: &str, value: &str) -> Result<(), String> {
        let _ = (name, value);
        Err("there is no safe place for keys here".into())
    }
}
