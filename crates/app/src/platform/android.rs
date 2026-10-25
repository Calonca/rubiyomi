// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use jni::JNIEnv;
use jni::objects::{JIntArray, JObject, JString};
use rubiyomi_core::platform::{Insets, Platform, PlatformEvent};
use winit::platform::android::activity::AndroidApp;

use super::EventQueue;

pub struct Android {
    app: AndroidApp,
    queue: Arc<EventQueue>,
}

impl Android {
    pub fn new(app: AndroidApp) -> Arc<Self> {
        let me = Arc::new(Android { app, queue: Arc::new(EventQueue::default()) });
        me.watch_events();
        me
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
}
