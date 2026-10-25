// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::sync::Arc;

use egui_material_icons::icons::{ICON_DICTIONARY, ICON_INSIGHTS, ICON_LOCAL_LIBRARY, ICON_SETTINGS};
use rubiyomi_core::platform::Platform;
use rubiyomi_core::theme::{self, Scheme};

use crate::screens;
use crate::ui::m3;

pub struct App {
    platform: Arc<dyn Platform>,
    schemes: [Scheme; 2],
    section: usize,
}

impl App {
    pub fn new(ctx: &egui::Context, platform: Arc<dyn Platform>) -> Self {
        egui_material_icons::initialize(ctx);
        let seed = platform.accent_color().unwrap_or(theme::DEFAULT_SEED);
        let schemes = [false, true].map(|dark| theme::scheme_from_seed(seed, dark));
        for s in &schemes {
            m3::apply(ctx, s);
        }
        App { platform, schemes, section: 0 }
    }

    fn follow_system(&mut self, ctx: &egui::Context) {
        let ppp = ctx.pixels_per_point();
        let [l, t, r, b] = self.platform.insets().map(|px| (px / ppp).round().clamp(0.0, 127.0) as i8);
        m3::set_insets(ctx, egui::Margin { left: l, top: t, right: r, bottom: b });
        if let Some(dark) = self.platform.dark_mode() {
            let want = if dark { egui::Theme::Dark } else { egui::Theme::Light };
            if ctx.theme() != want {
                ctx.set_theme(want);
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.follow_system(&ctx);
        let s = self.schemes[(ctx.theme() == egui::Theme::Dark) as usize];
        let sections = [(ICON_LOCAL_LIBRARY, "Library"), (ICON_DICTIONARY, "Words"), (ICON_INSIGHTS, "Stats"), (ICON_SETTINGS, "Settings")];
        m3::navigation_bar(ui, &s, &sections, &mut self.section);
        match self.section {
            0 => screens::library::ui(ui, &s),
            1 => screens::words::ui(ui, &s),
            2 => screens::stats::ui(ui, &s),
            _ => screens::settings::ui(ui, &s),
        }
        if ctx.input(|i| i.key_pressed(egui::Key::BrowserBack)) {
            if self.section != 0 {
                self.section = 0;
            } else if !self.platform.to_background() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}
