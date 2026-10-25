// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

pub mod library;
pub mod settings;
pub mod stats;
pub mod words;

fn surface(s: &rubiyomi_core::theme::Scheme) -> egui::Frame {
    egui::Frame::new().fill(crate::ui::m3::c(s.surface))
}
