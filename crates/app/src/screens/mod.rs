// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::sync::Arc;

use rubiyomi_core::finder::Finder;
use rubiyomi_core::grade::Grader;
use rubiyomi_core::history::History;
use rubiyomi_core::library::Library;
use rubiyomi_core::platform::Platform;
use rubiyomi_core::settings::PageSettings;
use rubiyomi_core::stats::Stats;
use rubiyomi_core::theme::Scheme;

use crate::ui::m3::Snackbar;

pub mod library;
pub mod page_form;
pub mod reader;
pub mod reviews_form;
pub mod settings;
pub mod setup;
pub mod stats;
pub mod words;

pub struct Ctx<'a> {
    pub scheme: &'a Scheme,
    pub platform: &'a dyn Platform,
    pub library: &'a Library,
    pub snackbar: &'a mut Snackbar,
    pub finder: Result<&'a Arc<Finder>, &'a str>,
    pub grader: Option<&'a Arc<dyn Grader>>,
    pub generation: u64,
    pub page: &'a mut PageSettings,
    pub history: &'a mut History,
    pub stats: &'a mut Stats,
}

fn surface(s: &Scheme) -> egui::Frame {
    egui::Frame::new().fill(crate::ui::m3::c(s.surface))
}
