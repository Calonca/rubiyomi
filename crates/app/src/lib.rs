// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::sync::Arc;

use rubiyomi_core::platform::Platform;

pub mod app;
pub mod platform;
pub mod reviews;
pub mod screens;
pub mod task;
pub mod ui;

pub fn run(options: eframe::NativeOptions, platform: Arc<dyn Platform>) -> eframe::Result<()> {
    eframe::run_native("Rubiyomi", options, Box::new(move |cc| Ok(Box::new(app::App::new(&cc.egui_ctx, platform)))))
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag("rubiyomi"));
    let platform = platform::android::Android::new(app.clone());
    platform.exit_when_finishing();
    let options = eframe::NativeOptions { android_app: Some(app), ..Default::default() };
    if let Err(e) = run(options, platform) {
        log::error!("Rubiyomi stopped: {e}");
    }
    // Android keeps the process for the next launch, and winit allows one event loop per process.
    std::process::exit(0);
}
