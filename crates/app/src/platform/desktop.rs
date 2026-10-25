// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use keyring_core::{Entry, Error as KeyError};

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

const SERVICE: &str = "Rubiyomi";

fn keyring() -> bool {
    static READY: OnceLock<bool> = OnceLock::new();
    *READY.get_or_init(|| match zbus_secret_service_keyring_store::Store::new() {
        Ok(store) => {
            keyring_core::set_default_store(store);
            true
        }
        Err(e) => {
            log::info!("no Secret Service ({e}); keys go to a file only this user can read");
            false
        }
    })
}

fn secrets_file(dir: &Path) -> PathBuf {
    dir.join("secrets.json")
}

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    std::fs::rename(&tmp, path)
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

    fn bundled(&self, name: &str) -> Option<Box<dyn Read + Send>> {
        let mut dirs = Vec::new();
        if let Some(env) = std::env::var_os("RUBIYOMI_DICT_DIR") {
            dirs.push(PathBuf::from(env));
        }
        if let Ok(exe) = std::env::current_exe()
            && let Some(dir) = exe.parent()
        {
            dirs.push(dir.join("dict"));
            dirs.push(dir.join("../share/rubiyomi/dict"));
        }
        dirs.push(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../dist/dict")));
        dirs.into_iter().find_map(|d| std::fs::File::open(d.join(name)).ok()).map(|f| Box::new(f) as Box<dyn Read + Send>)
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

    fn open_url(&self, url: &str) -> bool {
        for command in [&["xdg-open"][..], &["flatpak-spawn", "--host", "xdg-open"][..]] {
            let status = std::process::Command::new(command[0]).args(&command[1..]).arg(url).status();
            if status.is_ok_and(|s| s.success()) {
                return true;
            }
        }
        false
    }

    fn to_background(&self) -> bool {
        false
    }

    fn secret(&self, name: &str) -> Option<String> {
        if !keyring() {
            let all: HashMap<String, String> = serde_json::from_slice(&std::fs::read(secrets_file(&self.data_dir())).ok()?).ok()?;
            return all.get(name).cloned();
        }
        match Entry::new(SERVICE, name).and_then(|e| e.get_password()) {
            Ok(v) => Some(v),
            Err(KeyError::NoEntry) => None,
            Err(e) => {
                log::warn!("reading {name} from the keyring: {e}");
                None
            }
        }
    }

    fn set_secret(&self, name: &str, value: &str) -> Result<(), String> {
        if !keyring() {
            let path = secrets_file(&self.data_dir());
            let mut all: HashMap<String, String> =
                std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
            if value.is_empty() {
                all.remove(name);
            } else {
                all.insert(name.to_owned(), value.to_owned());
            }
            return write_private(&path, &serde_json::to_vec(&all).map_err(|e| e.to_string())?).map_err(|e| e.to_string());
        }
        let entry = Entry::new(SERVICE, name).map_err(|e| e.to_string())?;
        match if value.is_empty() { entry.delete_credential() } else { entry.set_password(value) } {
            Ok(()) | Err(KeyError::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}
