// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

//! Rubiyomi on native views. Rust only: no JS engine is linked.

#[cfg(all(feature = "dev-hooks", target_os = "android"))]
compile_error!("dev-hooks reads the environment and keeps keys in a file: it is never on Android");

#[cfg(target_os = "android")]
mod android;
mod app;
mod colours_page;
pub mod hooks;
mod library;
mod page_form;
mod platform;
mod reader;
mod reviews;
mod reviews_form;
#[cfg(not(target_os = "android"))]
mod secrets;
mod settings;
mod setup;
mod stats;
mod ui;
mod words;

pub use app::App;
use nsh_core::prelude::*;

#[native_module("rubiyomi/native")]
fn native(registry: &mut Registry) {
    registry.register::<App>();
}
