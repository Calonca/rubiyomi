// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

// No console window beside the app on Windows.
#![windows_subsystem = "windows"]

fn main() -> eframe::Result<()> {
    env_logger::init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Rubiyomi")
            .with_app_id("dev.calonca.rubiyomi")
            .with_inner_size([1100.0, 800.0])
            .with_min_inner_size([360.0, 480.0])
            .with_icon(eframe::icon_data::from_png_bytes(include_bytes!("../../../art/icon-512.png")).unwrap_or_default()),
        ..Default::default()
    };
    let mut args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    if args.len() >= 2 && args[0] == rubiyomi::updates::REPLACES {
        rubiyomi::updates::finish_replacing(args.drain(..2).nth(1).unwrap_or_default().into());
    }
    let desktop = rubiyomi::platform::desktop::Desktop::with_files(args.into_iter().map(Into::into));
    rubiyomi::run(options, std::sync::Arc::new(desktop))
}
