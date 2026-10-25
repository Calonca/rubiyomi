// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::sync::Mutex;

use rubiyomi_core::platform::PlatformEvent;

#[cfg(target_os = "android")]
pub mod android;
#[cfg(not(target_os = "android"))]
pub mod desktop;

#[derive(Default)]
pub struct EventQueue {
    events: Mutex<Vec<PlatformEvent>>,
    waker: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
}

impl EventQueue {
    pub fn push(&self, event: PlatformEvent) {
        self.events.lock().unwrap_or_else(|e| e.into_inner()).push(event);
        if let Some(wake) = &*self.waker.lock().unwrap_or_else(|e| e.into_inner()) {
            wake();
        }
    }

    pub fn drain(&self) -> Vec<PlatformEvent> {
        std::mem::take(&mut *self.events.lock().unwrap_or_else(|e| e.into_inner()))
    }

    pub fn set_waker(&self, wake: Box<dyn Fn() + Send + Sync>) {
        *self.waker.lock().unwrap_or_else(|e| e.into_inner()) = Some(wake);
    }
}
