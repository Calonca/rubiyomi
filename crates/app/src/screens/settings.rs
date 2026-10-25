// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use egui_material_icons::icons::{ICON_AUTO_STORIES, ICON_DICTIONARY, ICON_INFO, ICON_PALETTE, ICON_SCHOOL, ICON_TRANSLATE};
use rubiyomi_core::theme::Scheme;

use crate::ui::m3;

pub fn ui(ui: &mut egui::Ui, s: &Scheme) {
    m3::top_app_bar(ui, s, "Settings");
    egui::CentralPanel::default().frame(super::surface(s)).show(ui, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            m3::section_header(ui, s, "Reading");
            m3::list_item(ui, s, ICON_AUTO_STORIES, "Page", "Font size, line spacing, vertical or horizontal text");
            m3::list_item(ui, s, ICON_TRANSLATE, "Parsing", "How words are found in the text");
            m3::section_header(ui, s, "Words");
            m3::list_item(ui, s, ICON_DICTIONARY, "Dictionary", "The dictionary used for lookups");
            m3::list_item(ui, s, ICON_SCHOOL, "Reviews", "Send words you look up to jpdb.io or Anki");
            m3::section_header(ui, s, "App");
            m3::list_item(ui, s, ICON_PALETTE, "Appearance", "Colours and theme");
            m3::list_item(ui, s, ICON_INFO, "About", concat!("Rubiyomi ", env!("CARGO_PKG_VERSION"), " · MIT licence"));
        });
    });
}
