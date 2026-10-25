// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

fn main() -> eframe::Result<()> {
    env_logger::init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Rubiyomi")
            .with_app_id("dev.calonca.rubiyomi")
            .with_inner_size([1100.0, 800.0])
            .with_min_inner_size([360.0, 480.0]),
        ..Default::default()
    };
    let desktop = rubiyomi::platform::desktop::Desktop::with_files(std::env::args_os().skip(1).map(Into::into));
    rubiyomi::run(options, std::sync::Arc::new(desktop))
}
