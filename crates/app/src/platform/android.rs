// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::sync::Arc;
use std::time::Duration;

use jni::JNIEnv;
use jni::objects::{JIntArray, JObject};
use rubiyomi_core::platform::{Insets, Platform};
use winit::platform::android::activity::AndroidApp;

pub struct Android {
    app: AndroidApp,
}

impl Android {
    pub fn new(app: AndroidApp) -> Arc<Self> {
        Arc::new(Android { app })
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

impl Platform for Android {
    fn dark_mode(&self) -> Option<bool> {
        self.with_activity("darkMode", |env, a| env.call_method(a, "darkMode", "()Z", &[])?.z())
    }

    fn accent_color(&self) -> Option<u32> {
        let c = self.with_activity("accentColor", |env, a| env.call_method(a, "accentColor", "()I", &[])?.i())?;
        (c >= 0).then_some(c as u32)
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

    fn to_background(&self) -> bool {
        self.with_activity("toBackground", |env, a| env.call_method(a, "toBackground", "()Z", &[])?.z()).unwrap_or(false)
    }
}
