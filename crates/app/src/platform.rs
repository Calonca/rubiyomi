// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::collections::HashSet;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use nsh_core::file_dialog::FileFilter;
use nsh_core::picker::FilePickRequest;
use nsh_core::prelude::*;
#[cfg(target_os = "android")]
use rubiyomi_core::grade::anki::AnkiBackend;
use rubiyomi_core::platform::{Platform, PlatformEvent};

// Picks that overlap must not share a file name.
static PICKED: AtomicU64 = AtomicU64::new(0);

pub struct NshPlatform {
    data_dir: PathBuf,
    events: Arc<Mutex<Vec<PlatformEvent>>>,
    apk: Option<PathBuf>,
    #[cfg(not(target_os = "android"))]
    secrets: crate::secrets::Secrets,
    unread: Unread,
}

// The keys whose read failed. They load as empty, and every save writes all keys back: that
// empty value must not forget a key that is there, in a keyring that was only locked.
#[derive(Default)]
struct Unread(Mutex<HashSet<String>>);

impl Unread {
    fn names(&self) -> MutexGuard<'_, HashSet<String>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn read(&self, name: &str, read: Result<Option<String>, String>) -> Option<String> {
        match read {
            Ok(key) => {
                self.names().remove(name);
                key
            }
            Err(e) => {
                log::warn!("reading {name}: {e}");
                self.names().insert(name.to_owned());
                None
            }
        }
    }

    // A key typed in over an unread one is a new key; an empty one says nothing about it.
    fn leaves(&self, name: &str, value: &str) -> bool {
        value.is_empty() && self.names().contains(name)
    }

    fn stored(&self, name: &str, stored: Result<(), String>) -> Result<(), String> {
        if stored.is_ok() {
            self.names().remove(name);
        }
        stored
    }
}

impl NshPlatform {
    pub fn new() -> Self {
        let android = cfg!(target_os = "android");
        let data_dir = if android {
            // Android sets TMPDIR to <app data>/cache before the app starts.
            std::env::var_os("TMPDIR").map(|t| android_data_dir(Path::new(&t))).unwrap_or_else(|| PathBuf::from("files"))
        } else {
            desktop_data_dir()
        };
        let _ = std::fs::create_dir_all(&data_dir);
        let apk = android.then(|| std::fs::read_to_string("/proc/self/maps").ok().and_then(|m| apk_in_maps(&m))).flatten();
        Self::on(data_dir, apk)
    }

    fn on(data_dir: PathBuf, apk: Option<PathBuf>) -> Self {
        NshPlatform {
            #[cfg(not(target_os = "android"))]
            secrets: crate::secrets::Secrets::new(&data_dir),
            data_dir,
            events: Arc::default(),
            apk,
            unread: Unread::default(),
        }
    }

    // A platform on a directory of its own, for tests.
    #[cfg(test)]
    pub fn at(data_dir: PathBuf) -> Self {
        NshPlatform { secrets: crate::secrets::Secrets::in_file(&data_dir), ..Self::on(data_dir, None) }
    }

    fn push(events: &Mutex<Vec<PlatformEvent>>, event: PlatformEvent) {
        events.lock().unwrap_or_else(|e| e.into_inner()).push(event);
    }
}

impl Default for NshPlatform {
    fn default() -> Self {
        Self::new()
    }
}

fn desktop_data_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(std::env::temp_dir)
        .join("rubiyomi")
}

fn android_data_dir(tmpdir: &Path) -> PathBuf {
    tmpdir.with_file_name("files")
}

// The APK is mapped for its native library; the line ends in the file's path.
fn apk_in_maps(maps: &str) -> Option<PathBuf> {
    maps.lines().find(|l| l.ends_with("base.apk")).and_then(|l| l.split_whitespace().last()).map(PathBuf::from)
}

fn read_asset(zip_path: &Path, name: &str) -> Option<Vec<u8>> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(zip_path).ok()?).ok()?;
    let mut entry = zip.by_name(name).ok()?;
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry.read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

fn desktop_dict(name: &str) -> Option<std::fs::File> {
    let mut dirs = Vec::new();
    if let Some(env) = std::env::var_os("RUBIYOMI_DICT_DIR") {
        dirs.push(PathBuf::from(env));
    }
    dirs.push(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../dist/dict")));
    dirs.into_iter().find_map(|d| std::fs::File::open(d.join(name)).ok())
}

impl Platform for NshPlatform {
    fn data_dir(&self) -> PathBuf {
        self.data_dir.clone()
    }

    fn bundled(&self, name: &str) -> Option<Box<dyn Read + Send>> {
        if let Some(apk) = &self.apk {
            let bytes = read_asset(apk, &format!("assets/dict/{name}"))?;
            return Some(Box::new(Cursor::new(bytes)));
        }
        desktop_dict(name).map(|f| Box::new(f) as Box<dyn Read + Send>)
    }

    fn accent_color(&self) -> Option<u32> {
        #[cfg(target_os = "android")]
        return crate::android::accent_color();
        #[cfg(not(target_os = "android"))]
        None
    }

    fn pick_epub(&self) {
        let (events, dir) = (self.events.clone(), self.data_dir.join("imports"));
        spawn(async move {
            let picked = pick_files(FilePickRequest::new(true, 0).filter(FileFilter::new("EPUB", ["epub"]))).await;
            let _ = std::fs::create_dir_all(&dir);
            for file in picked {
                let path = dir.join(format!("{}.epub", PICKED.fetch_add(1, Ordering::Relaxed)));
                let event = match std::fs::write(&path, file.data.bytes()) {
                    Ok(()) => PlatformEvent::Imported { path, name: file.name, temporary: true },
                    Err(e) => PlatformEvent::ImportFailed(e.to_string()),
                };
                Self::push(&events, event);
            }
        });
    }

    fn poll_events(&self) -> Vec<PlatformEvent> {
        #[cfg(target_os = "android")]
        if let Some(granted) = crate::android::permission_answer() {
            Self::push(&self.events, PlatformEvent::Permission { granted });
        }
        std::mem::take(&mut *self.events.lock().unwrap_or_else(|e| e.into_inner()))
    }

    #[cfg(target_os = "android")]
    fn open_url(&self, url: &str) -> bool {
        crate::android::open_url(url)
    }

    #[cfg(not(target_os = "android"))]
    fn open_url(&self, url: &str) -> bool {
        [&["xdg-open"][..], &["flatpak-spawn", "--host", "xdg-open"]]
            .iter()
            .any(|c| std::process::Command::new(c[0]).args(&c[1..]).arg(url).status().is_ok_and(|s| s.success()))
    }

    #[cfg(target_os = "android")]
    fn anki_droid(&self) -> Option<Box<dyn AnkiBackend>> {
        Some(Box::new(crate::android::AnkiDroid))
    }

    #[cfg(target_os = "android")]
    fn anki_droid_status(&self) -> Option<(bool, bool)> {
        crate::android::anki_status()
    }

    #[cfg(target_os = "android")]
    fn request_anki_permission(&self) {
        crate::android::request_anki_permission();
    }

    #[cfg(target_os = "android")]
    fn keep_screen_on(&self, on: bool) {
        crate::android::keep_screen_on(on);
    }

    fn secret(&self, name: &str) -> Option<String> {
        #[cfg(target_os = "android")]
        let read = crate::android::secret(name);
        #[cfg(not(target_os = "android"))]
        let read = self.secrets.get(name);
        self.unread.read(name, read)
    }

    fn set_secret(&self, name: &str, value: &str) -> Result<(), String> {
        if self.unread.leaves(name, value) {
            return Ok(());
        }
        #[cfg(target_os = "android")]
        let stored = crate::android::set_secret(name, value);
        #[cfg(not(target_os = "android"))]
        let stored = self.secrets.set(name, value);
        self.unread.stored(name, stored)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use rubiyomi_core::settings::Settings;

    use super::*;

    fn key_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rubiyomi-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn saving_after_a_failed_read_keeps_the_key() {
        let dir = key_dir("unread");
        let file = dir.join("secrets.json");
        let saved = br#"{"jpdb-key":"made-up-key"}"#;
        // As a locked keyring does: there is a key, and reading it fails.
        std::fs::write(&file, b"locked").unwrap();
        let platform = NshPlatform::at(dir);
        let settings = Settings::load(&platform);
        assert_eq!(settings.jpdb_key, "");
        settings.save(&platform).unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"locked");

        std::fs::write(&file, saved).unwrap();
        assert_eq!(Settings::load(&platform).jpdb_key, "made-up-key");
    }

    #[test]
    fn a_key_read_and_then_cleared_is_forgotten() {
        let dir = key_dir("cleared");
        let platform = NshPlatform::at(dir);
        let mut settings = Settings { jpdb_key: "made-up-key".into(), ..Settings::default() };
        settings.save(&platform).unwrap();
        settings = Settings::load(&platform);
        settings.jpdb_key.clear();
        settings.save(&platform).unwrap();
        assert_eq!(platform.secret("jpdb-key"), None);
    }

    #[test]
    fn bundled_reads_a_stored_zip_entry() {
        let dir = std::env::temp_dir().join(format!("rubiyomi-zip-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("base.apk");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        zip.start_file(
            "assets/dict/version.json",
            zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
        )
        .unwrap();
        zip.write_all(b"{\"format\":1}").unwrap();
        zip.finish().unwrap();

        assert_eq!(read_asset(&path, "assets/dict/version.json").as_deref(), Some(&b"{\"format\":1}"[..]));
        assert_eq!(read_asset(&path, "assets/dict/missing"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn data_dir_is_under_the_tmpdir_parent() {
        assert_eq!(
            android_data_dir(std::path::Path::new("/data/user/0/dev.calonca.rubiyomi/cache")),
            PathBuf::from("/data/user/0/dev.calonca.rubiyomi/files")
        );
    }

    #[test]
    fn the_apk_is_the_mapping_that_ends_in_base_apk() {
        let maps = "7f00-7f10 r--p 00000000 fd:01 12 /system/lib64/libc.so\n7f20-7f30 r--p 00001000 fd:01 99 /data/app/~~x==/dev.calonca.rubiyomi-y==/base.apk\n";
        assert_eq!(apk_in_maps(maps), Some(PathBuf::from("/data/app/~~x==/dev.calonca.rubiyomi-y==/base.apk")));
        assert_eq!(apk_in_maps("7f00-7f10 r--p 0 fd:01 12 /system/lib64/libc.so\n"), None);
    }
}
