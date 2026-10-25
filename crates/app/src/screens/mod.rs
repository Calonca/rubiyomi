// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use rubiyomi_core::finder::Finder;
use rubiyomi_core::library::Library;
use rubiyomi_core::platform::Platform;
use rubiyomi_core::theme::Scheme;

use crate::ui::m3::Snackbar;

pub mod library;
pub mod reader;
pub mod settings;
pub mod stats;
pub mod words;

pub struct Ctx<'a> {
    pub scheme: &'a Scheme,
    pub platform: &'a dyn Platform,
    pub library: &'a Library,
    pub snackbar: &'a mut Snackbar,
    pub finder: Result<&'a Finder, &'a str>,
}

fn surface(s: &Scheme) -> egui::Frame {
    egui::Frame::new().fill(crate::ui::m3::c(s.surface))
}
