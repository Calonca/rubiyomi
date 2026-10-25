// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::path::PathBuf;
use std::sync::Arc;

use rubiyomi_core::platform::{Insets, Platform, PlatformEvent};

use super::EventQueue;

#[derive(Default)]
pub struct Desktop {
    queue: Arc<EventQueue>,
}

impl Desktop {
    pub fn with_files(paths: impl IntoIterator<Item = PathBuf>) -> Self {
        let desktop = Desktop::default();
        for path in paths {
            desktop.queue.push(imported(path));
        }
        desktop
    }
}

fn imported(path: PathBuf) -> PlatformEvent {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    PlatformEvent::Imported { path, name, temporary: false }
}

impl Platform for Desktop {
    fn data_dir(&self) -> PathBuf {
        let base = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
            .unwrap_or_else(std::env::temp_dir);
        let dir = base.join("rubiyomi");
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    fn accent_color(&self) -> Option<u32> {
        None
    }

    fn pick_epub(&self) {
        let queue = self.queue.clone();
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new().set_title("Add EPUB").add_filter("EPUB", &["epub"]).pick_files();
            for path in picked.unwrap_or_default() {
                queue.push(imported(path));
            }
        });
    }

    fn poll_events(&self) -> Vec<PlatformEvent> {
        self.queue.drain()
    }

    fn set_waker(&self, wake: Box<dyn Fn() + Send + Sync>) {
        self.queue.set_waker(wake);
    }

    fn insets(&self) -> Insets {
        [0.0; 4]
    }

    fn to_background(&self) -> bool {
        false
    }
}
