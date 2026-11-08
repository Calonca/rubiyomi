// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

// No console window beside the app on Windows.
#![windows_subsystem = "windows"]

#[cfg(target_os = "linux")]
use nsh_platform_linux as platform;
#[cfg(windows)]
use nsh_platform_windows as platform;

// `RUBIYOMI_WINDOW=WxH` opens the window at that size; the headless runs use it.
fn window_size() -> (i32, i32) {
    rubiyomi::hooks::var("RUBIYOMI_WINDOW")
        .and_then(|v| {
            let (w, h) = v.split_once('x')?;
            Some((w.parse().ok()?, h.parse().ok()?))
        })
        .unwrap_or((1100, 800))
}

fn main() -> impl std::process::Termination {
    env_logger::init();
    #[cfg(feature = "update-check")]
    {
        let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
        if let [flag, old, ..] = args.as_slice()
            && flag == rubiyomi::updates::REPLACES
        {
            rubiyomi::updates::finish_replacing(old.into());
        }
    }
    let size = window_size();
    let window = format!(r#"{{"defaultSize":{{"width":{},"height":{}}},"minSize":{{"width":360,"height":480}}}}"#, size.0, size.1);
    platform::run_app(
        platform::AppSource::Rust(platform::Root::of::<rubiyomi::App>().main_window_json(&window)),
        platform::AppOptions { title: "Rubiyomi".to_owned(), app_id: "dev.calonca.rubiyomi".to_owned(), size, ..Default::default() },
    )
}
