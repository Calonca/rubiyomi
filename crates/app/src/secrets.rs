// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

// Where keys are kept on the desktop: the Secret Service, and nowhere else. Without one a key
// isn't kept, and is asked again. On Android they go to the Keystore, through android.rs.

#[cfg(any(test, feature = "dev-hooks"))]
use std::collections::HashMap;
#[cfg(any(test, feature = "dev-hooks"))]
use std::io::Write;
use std::path::Path;
#[cfg(any(test, feature = "dev-hooks"))]
use std::path::PathBuf;

#[cfg(feature = "desktop")]
mod keyring {
    use std::sync::OnceLock;

    use keyring_core::{Entry, Error};

    const SERVICE: &str = "Rubiyomi";

    pub fn available() -> bool {
        static READY: OnceLock<bool> = OnceLock::new();
        *READY.get_or_init(|| match zbus_secret_service_keyring_store::Store::new() {
            Ok(store) => {
                keyring_core::set_default_store(store);
                true
            }
            Err(e) => {
                log::info!("no keyring ({e}); keys won't be kept");
                false
            }
        })
    }

    pub fn get(name: &str) -> Result<Option<String>, String> {
        match Entry::new(SERVICE, name).and_then(|e| e.get_password()) {
            Ok(v) => Ok(Some(v)),
            Err(Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn set(name: &str, value: &str) -> Result<(), String> {
        let entry = Entry::new(SERVICE, name).map_err(|e| e.to_string())?;
        match if value.is_empty() { entry.delete_credential() } else { entry.set_password(value) } {
            Ok(()) | Err(Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

enum Store {
    #[cfg(feature = "desktop")]
    Keyring,
    // For the headless runs and the tests only: the app says keys are never in its files.
    #[cfg(any(test, feature = "dev-hooks"))]
    File(PathBuf),
    Nowhere,
}

pub struct Secrets(Store);

impl Secrets {
    pub fn new(dir: &Path) -> Self {
        // tool/headless.sh sets it: a headless run shares the user's session bus, and
        // saving settings with no key would delete the real one.
        #[cfg(feature = "dev-hooks")]
        if crate::hooks::var("RUBIYOMI_SECRETS_FILE").is_some() {
            return Self::in_file(dir);
        }
        let _ = dir;
        #[cfg(feature = "desktop")]
        if keyring::available() {
            return Secrets(Store::Keyring);
        }
        Secrets(Store::Nowhere)
    }

    #[cfg(any(test, feature = "dev-hooks"))]
    pub fn in_file(dir: &Path) -> Self {
        Secrets(Store::File(dir.join("secrets.json")))
    }

    #[cfg_attr(not(any(feature = "desktop", feature = "dev-hooks")), allow(unused_variables))]
    // None when there is no such key, an error when there may be one that can't be read.
    pub fn get(&self, name: &str) -> Result<Option<String>, String> {
        match &self.0 {
            #[cfg(feature = "desktop")]
            Store::Keyring => keyring::get(name),
            #[cfg(any(test, feature = "dev-hooks"))]
            Store::File(path) => Ok(all(path)?.remove(name)),
            Store::Nowhere => Ok(None),
        }
    }

    #[cfg_attr(not(any(feature = "desktop", feature = "dev-hooks")), allow(unused_variables))]
    pub fn set(&self, name: &str, value: &str) -> Result<(), String> {
        match &self.0 {
            #[cfg(feature = "desktop")]
            Store::Keyring => keyring::set(name, value),
            #[cfg(any(test, feature = "dev-hooks"))]
            Store::File(path) => {
                let mut all = all(path)?;
                if value.is_empty() {
                    all.remove(name);
                } else {
                    all.insert(name.to_owned(), value.to_owned());
                }
                write_private(path, &serde_json::to_vec(&all).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
            }
            Store::Nowhere => Err("this system has no keyring".to_owned()),
        }
    }
}

#[cfg(any(test, feature = "dev-hooks"))]
fn all(path: &Path) -> Result<HashMap<String, String>, String> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| e.to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(HashMap::new()),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(any(test, feature = "dev-hooks"))]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_a_keyring_a_key_is_refused_and_no_file_is_written() {
        let dir = std::env::temp_dir().join(format!("rubiyomi-secrets-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let secrets = Secrets(Store::Nowhere);

        assert!(secrets.set("jpdb-key", "made-up-key").is_err());
        assert_eq!(secrets.get("jpdb-key"), Ok(None));
        assert!(!dir.join("secrets.json").exists());
    }
}
