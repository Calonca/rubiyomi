// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::collections::HashMap;
use std::io::{Read, Write};
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
    *READY.get_or_init(|| match native_store() {
        Ok(()) => true,
        Err(e) => {
            log::info!("no keyring ({e}); keys go to a file only this user can read");
            false
        }
    })
}

#[cfg(target_os = "linux")]
fn native_store() -> Result<(), String> {
    keyring_core::set_default_store(zbus_secret_service_keyring_store::Store::new().map_err(|e| e.to_string())?);
    Ok(())
}

#[cfg(windows)]
fn native_store() -> Result<(), String> {
    keyring_core::set_default_store(windows_native_keyring_store::Store::new().map_err(|e| e.to_string())?);
    Ok(())
}

#[cfg(not(any(target_os = "linux", windows)))]
fn native_store() -> Result<(), String> {
    Err("no keyring support on this system".into())
}

fn secrets_file(dir: &Path) -> PathBuf {
    dir.join("secrets.json")
}

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut f = options.open(&tmp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    std::fs::rename(&tmp, path)
}

#[cfg(feature = "embed-dict")]
fn embedded(name: &str) -> Option<&'static [u8]> {
    macro_rules! dict {
        ($file:literal) => {
            include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../dist/dict/", $file))
        };
    }
    Some(match name {
        "jmdict.sqlite.xz" => dict!("jmdict.sqlite.xz"),
        "ipadic.tar.xz" => dict!("ipadic.tar.xz"),
        "version.json" => dict!("version.json"),
        "DATA-LICENCE.txt" => dict!("DATA-LICENCE.txt"),
        _ => return None,
    })
}

fn imported(path: PathBuf) -> PlatformEvent {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    PlatformEvent::Imported { path, name, temporary: false }
}

impl Platform for Desktop {
    fn data_dir(&self) -> PathBuf {
        let home = || std::env::var_os("HOME").map(PathBuf::from);
        let base = if cfg!(windows) {
            std::env::var_os("APPDATA").map(PathBuf::from)
        } else if cfg!(target_os = "macos") {
            home().map(|h| h.join("Library/Application Support"))
        } else {
            std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .or_else(|| home().map(|h| h.join(".local/share")))
        };
        let dir = base.unwrap_or_else(std::env::temp_dir).join(if cfg!(windows) { "Rubiyomi" } else { "rubiyomi" });
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    fn bundled(&self, name: &str) -> Option<Box<dyn Read + Send>> {
        #[cfg(feature = "embed-dict")]
        if let Some(bytes) = embedded(name) {
            return Some(Box::new(std::io::Cursor::new(bytes)));
        }
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
        let openers: &[&[&str]] = if cfg!(windows) {
            &[&["rundll32", "url.dll,FileProtocolHandler"]]
        } else if cfg!(target_os = "macos") {
            &[&["open"]]
        } else {
            &[&["xdg-open"], &["flatpak-spawn", "--host", "xdg-open"]]
        };
        for command in openers {
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
