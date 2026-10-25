// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use egui_material_icons::icons::ICON_DICTIONARY;
use rubiyomi_core::theme::Scheme;

use crate::ui::m3;

pub fn ui(ui: &mut egui::Ui, s: &Scheme) {
    m3::top_app_bar(ui, s, "Words", false);
    egui::CentralPanel::default().frame(super::surface(s)).show(ui, |ui| {
        m3::empty_state(ui, s, ICON_DICTIONARY, "No words yet", "Words you look up while reading will be collected here.");
    });
}
