// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::{Arc, Weak};
use std::time::Duration;

use jni::JNIEnv;
use jni::objects::{JIntArray, JObject, JString};
use rubiyomi_core::grade::GradeError;
use rubiyomi_core::grade::anki::{AnkiBackend, Note};
use rubiyomi_core::platform::{Insets, Platform, PlatformEvent};
use serde_json::{Value, json};
use winit::platform::android::activity::AndroidApp;

use super::EventQueue;

pub struct Android {
    app: AndroidApp,
    queue: Arc<EventQueue>,
    me: Weak<Android>,
}

impl Android {
    pub fn new(app: AndroidApp) -> Arc<Self> {
        let me = Arc::new_cyclic(|me| Android { app, queue: Arc::new(EventQueue::default()), me: me.clone() });
        me.watch_events();
        me
    }

    fn anki_call(&self, method: &str, args: &Value) -> Result<Value, GradeError> {
        let answer = self
            .with_activity("ankiCall", |env, a| {
                let m = env.new_string(method)?;
                let j = env.new_string(args.to_string())?;
                let v = env
                    .call_method(a, "ankiCall", "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;", &[(&m).into(), (&j).into()])?
                    .l()?;
                let s: String = env.get_string(&JString::from(v))?.into();
                Ok(s)
            })
            .ok_or_else(|| GradeError::Other("AnkiDroid couldn't be asked".into()))?;
        let json: Value = serde_json::from_str(&answer).map_err(|_| GradeError::Other("AnkiDroid's answer wasn't JSON".into()))?;
        match json.get("error").and_then(Value::as_str) {
            Some("permission") => Err(GradeError::PermissionDenied),
            Some("missing") => Err(GradeError::NotFound("AnkiDroid isn't installed".into())),
            Some(_) => Err(GradeError::Other(format!("AnkiDroid: {}", json.get("message").and_then(Value::as_str).unwrap_or("failed")))),
            None => Ok(json.get("ok").cloned().unwrap_or(Value::Null)),
        }
    }

    fn with_activity<T>(&self, what: &str, f: impl FnOnce(&mut JNIEnv<'_>, &JObject<'_>) -> jni::errors::Result<T>) -> Option<T> {
        // SAFETY: the VM pointer comes from the running NativeActivity and lives as long as the process.
        let vm = unsafe { jni::JavaVM::from_raw(self.app.vm_as_ptr().cast()) }.ok()?;
        let mut env = vm.attach_current_thread().ok()?;
        // SAFETY: android-activity's global reference to the Activity, valid while it runs; JObject has no Drop.
        let activity = unsafe { JObject::from_raw(self.app.activity_as_ptr().cast()) };
        let result = env.with_local_frame(16, |env| f(env, &activity));
        match result {
            Ok(v) => Some(v),
            Err(e) => {
                let _ = env.exception_describe();
                let _ = env.exception_clear();
                log::warn!("{what}: {e}");
                None
            }
        }
    }

    fn watch_events(self: &Arc<Self>) {
        let me = Arc::downgrade(self);
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_millis(300));
                let Some(me) = me.upgrade() else { return };
                while let Some(line) = me
                    .with_activity("pollEvent", |env, a| {
                        let v = env.call_method(a, "pollEvent", "()Ljava/lang/String;", &[])?.l()?;
                        if v.is_null() {
                            return Ok(None);
                        }
                        let s: String = env.get_string(&JString::from(v))?.into();
                        Ok(Some(s))
                    })
                    .flatten()
                {
                    if let Some(event) = parse_event(&line) {
                        me.queue.push(event);
                    }
                }
            }
        });
    }

    pub fn exit_when_finishing(self: &Arc<Self>) {
        let me = Arc::downgrade(self);
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_millis(500));
                let Some(me) = me.upgrade() else { return };
                if me.with_activity("isFinishing", |env, a| env.call_method(a, "isFinishing", "()Z", &[])?.z()) == Some(true) {
                    log::info!("the activity is finishing; leaving so the next launch starts fresh");
                    std::process::exit(0);
                }
            }
        });
    }
}

fn parse_event(line: &str) -> Option<PlatformEvent> {
    let mut fields = line.split('\t');
    match fields.next()? {
        "imported" => {
            let path = PathBuf::from(fields.next()?);
            let name = fields.next().unwrap_or("").to_owned();
            Some(PlatformEvent::Imported { path, name, temporary: true })
        }
        "import-failed" => Some(PlatformEvent::ImportFailed(fields.next().unwrap_or("unknown error").to_owned())),
        "permission" => Some(PlatformEvent::Permission { granted: fields.next() == Some("granted") }),
        other => {
            log::warn!("unknown event from Java: {other}");
            None
        }
    }
}

impl Platform for Android {
    fn data_dir(&self) -> PathBuf {
        let dir = self.app.internal_data_path().unwrap_or_else(std::env::temp_dir);
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    fn bundled(&self, name: &str) -> Option<Box<dyn Read + Send>> {
        let path = std::ffi::CString::new(format!("dict/{name}")).ok()?;
        let mut asset = self.app.asset_manager().open(&path)?;
        // Only assets stored uncompressed (the .xz ones) have a file descriptor.
        let Ok(fd) = asset.open_file_descriptor() else {
            let mut bytes = Vec::new();
            asset.read_to_end(&mut bytes).ok()?;
            return Some(Box::new(std::io::Cursor::new(bytes)));
        };
        let mut file = std::fs::File::from(fd.fd);
        file.seek(SeekFrom::Start(fd.offset as u64)).ok()?;
        Some(Box::new(file.take(fd.size as u64)))
    }

    fn dark_mode(&self) -> Option<bool> {
        self.with_activity("darkMode", |env, a| env.call_method(a, "darkMode", "()Z", &[])?.z())
    }

    fn accent_color(&self) -> Option<u32> {
        let c = self.with_activity("accentColor", |env, a| env.call_method(a, "accentColor", "()I", &[])?.i())?;
        (c >= 0).then_some(c as u32)
    }

    fn pick_epub(&self) {
        self.with_activity("pickEpub", |env, a| env.call_method(a, "pickEpub", "()V", &[]).map(|_| ()));
    }

    fn poll_events(&self) -> Vec<PlatformEvent> {
        self.queue.drain()
    }

    fn set_waker(&self, wake: Box<dyn Fn() + Send + Sync>) {
        self.queue.set_waker(wake);
    }

    fn insets(&self) -> Insets {
        self.with_activity("insets", |env, a| {
            let array = JIntArray::from(env.call_method(a, "insets", "()[I", &[])?.l()?);
            let mut v = [0i32; 4];
            env.get_int_array_region(&array, 0, &mut v)?;
            Ok(v.map(|x| x as f32))
        })
        .unwrap_or([0.0; 4])
    }

    fn open_url(&self, url: &str) -> bool {
        self.with_activity("openUrl", |env, a| {
            let url = env.new_string(url)?;
            env.call_method(a, "openUrl", "(Ljava/lang/String;)Z", &[(&url).into()])?.z()
        })
        .unwrap_or(false)
    }

    fn to_background(&self) -> bool {
        self.with_activity("toBackground", |env, a| env.call_method(a, "toBackground", "()Z", &[])?.z()).unwrap_or(false)
    }

    fn anki_droid(&self) -> Option<Box<dyn AnkiBackend>> {
        Some(Box::new(AnkiDroid { android: self.me.upgrade()? }))
    }

    fn anki_droid_status(&self) -> Option<(bool, bool)> {
        let s = self.anki_call("status", &json!({})).ok()?;
        Some((s["installed"].as_bool().unwrap_or(false), s["permission"].as_bool().unwrap_or(false)))
    }

    fn request_anki_permission(&self) {
        self.with_activity("requestAnkiPermission", |env, a| env.call_method(a, "requestAnkiPermission", "()V", &[]).map(|_| ()));
    }

    fn keyboard(&self) -> f32 {
        self.with_activity("keyboardHeight", |env, a| env.call_method(a, "keyboardHeight", "()I", &[])?.i()).unwrap_or(0) as f32
    }

    fn can_paste(&self) -> bool {
        true
    }

    fn clipboard_text(&self) -> Option<String> {
        self.with_activity("clipboardText", |env, a| {
            let v = env.call_method(a, "clipboardText", "()Ljava/lang/String;", &[])?.l()?;
            if v.is_null() {
                return Ok(None);
            }
            Ok(Some(env.get_string(&JString::from(v))?.into()))
        })
        .flatten()
    }

    fn copy_text(&self, text: &str) -> bool {
        self.with_activity("copyText", |env, a| {
            let text = env.new_string(text)?;
            env.call_method(a, "copyText", "(Ljava/lang/String;)V", &[(&text).into()]).map(|_| ())
        })
        .is_some()
    }

    fn keep_screen_on(&self, on: bool) {
        self.with_activity("keepScreenOn", |env, a| env.call_method(a, "keepScreenOn", "(Z)V", &[on.into()]).map(|_| ()));
    }

    fn secret(&self, name: &str) -> Option<String> {
        self.with_activity("secret", |env, a| {
            let name = env.new_string(name)?;
            let v = env.call_method(a, "secret", "(Ljava/lang/String;)Ljava/lang/String;", &[(&name).into()])?.l()?;
            if v.is_null() {
                return Ok(None);
            }
            Ok(Some(env.get_string(&JString::from(v))?.into()))
        })
        .flatten()
    }

    fn set_secret(&self, name: &str, value: &str) -> Result<(), String> {
        let stored = self.with_activity("setSecret", |env, a| {
            let (name, value) = (env.new_string(name)?, env.new_string(value)?);
            env.call_method(a, "setSecret", "(Ljava/lang/String;Ljava/lang/String;)Z", &[(&name).into(), (&value).into()])?.z()
        });
        if stored == Some(true) { Ok(()) } else { Err("the Android Keystore refused it".into()) }
    }
}

struct AnkiDroid {
    android: Arc<Android>,
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_owned)).collect()).unwrap_or_default()
}

impl AnkiBackend for AnkiDroid {
    fn decks(&self) -> Result<Vec<String>, GradeError> {
        Ok(strings(&self.android.anki_call("decks", &json!({}))?))
    }

    fn note_types(&self) -> Result<Vec<String>, GradeError> {
        Ok(strings(&self.android.anki_call("noteTypes", &json!({}))?))
    }

    fn fields(&self, note_type: &str) -> Result<Vec<String>, GradeError> {
        Ok(strings(&self.android.anki_call("fields", &json!({ "noteType": note_type }))?))
    }

    fn find_notes(&self, query: &str) -> Result<Vec<u64>, GradeError> {
        let v = self.android.anki_call("findNotes", &json!({ "query": query }))?;
        Ok(v.as_array().map(|a| a.iter().filter_map(Value::as_u64).collect()).unwrap_or_default())
    }

    fn notes(&self, ids: &[u64]) -> Result<Vec<Note>, GradeError> {
        let v = self.android.anki_call("notes", &json!({ "ids": ids }))?;
        Ok(v.as_array().into_iter().flatten().filter_map(|n| Some(Note { id: n["id"].as_u64()?, fields: strings(&n["fields"]) })).collect())
    }

    fn suspend(&self, note: u64) -> Result<(), GradeError> {
        self.android.anki_call("suspend", &json!({ "note": note }))?;
        Ok(())
    }

    fn answer(&self, note: u64, ease: u8) -> Result<(), GradeError> {
        let changed = self.android.anki_call("answer", &json!({ "note": note, "ease": ease }))?;
        // AnkiDroid ignores a grade for a card that isn't due.
        if changed.as_u64() == Some(0) {
            return Err(GradeError::Other("AnkiDroid didn't take the grade (is the card due?)".into()));
        }
        Ok(())
    }

    fn add_note(&self, deck: &str, note_type: &str, fields: &[(String, String)], tags: &[&str]) -> Result<u64, GradeError> {
        let map: serde_json::Map<String, Value> = fields.iter().map(|(k, v)| (k.clone(), v.clone().into())).collect();
        let v =
            self.android.anki_call("addNote", &json!({ "deck": deck, "noteType": note_type, "fields": map, "tags": tags.join(" ") }))?;
        v.as_u64().ok_or_else(|| GradeError::Other("AnkiDroid didn't add the note".into()))
    }
}
