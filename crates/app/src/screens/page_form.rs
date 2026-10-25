// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use egui::{CornerRadius, Sense, Stroke, vec2};
use rubiyomi_core::settings::{Furigana, PageSettings, PageTheme, Typeface, WordColours};
use rubiyomi_core::theme::Scheme;

use crate::ui::m3::{self, c, font, text};
use crate::ui::page;

// The page settings, in Settings and in the reader's "Aa" sheet. Says whether anything changed.
pub fn ui(ui: &mut egui::Ui, s: &Scheme, p: &mut PageSettings, colours_matter: bool) -> bool {
    let mut changed = false;
    m3::section_header(ui, s, "Theme");
    changed |= themes(ui, s, &mut p.theme);
    m3::section_header(ui, s, "Text");
    changed |= m3::slider_row(ui, s, "Text size", &format!("{:.0}", p.font_size), &mut p.font_size, 14.0..=44.0, 1.0);
    changed |= m3::slider_row(ui, s, "Line spacing", &format!("{:.1}", p.line_height), &mut p.line_height, 1.3..=2.6, 0.1);
    changed |= m3::slider_row(ui, s, "Margins", &format!("{:.0}", p.margin), &mut p.margin, 8.0..=72.0, 4.0);
    let mut chars = p.max_chars_per_line as f32;
    let shown = if p.max_chars_per_line == 0 { "No limit".to_owned() } else { p.max_chars_per_line.to_string() };
    if m3::slider_row(ui, s, "Characters per line", &shown, &mut chars, 0.0..=60.0, 1.0) {
        // Under ten a line is too short to read; the slider's start means no limit.
        p.max_chars_per_line = if chars < 10.0 { 0 } else { chars as u32 };
        changed = true;
    }
    changed |= choice(ui, s, "Typeface", &["Serif", "Sans"], &[Typeface::Serif, Typeface::Sans], &mut p.typeface);
    changed |= choice(ui, s, "Furigana", &["Show", "Hide", "Faint"], &[Furigana::Show, Furigana::Hide, Furigana::Blur], &mut p.furigana);
    if colours_matter {
        changed |= choice(
            ui,
            s,
            "Word colours",
            &["Off", "Underline", "Text"],
            &[WordColours::Off, WordColours::Underline, WordColours::Text],
            &mut p.colours,
        );
    }
    changed |= m3::switch_row(ui, s, "Vertical text", "Columns read from right to left", &mut p.vertical);
    m3::section_header(ui, s, "Reading");
    changed |= m3::switch_row(ui, s, "Tap the edges to turn", "Otherwise swipe, or use the keys", &mut p.tap_to_turn);
    changed |= m3::switch_row(ui, s, "Show how far you are", "A percentage under the page", &mut p.show_percent);
    changed |= m3::switch_row(ui, s, "Keep the screen on", "While a book is open", &mut p.keep_awake);
    changed
}

fn choice<T: Copy + PartialEq>(ui: &mut egui::Ui, s: &Scheme, label: &str, names: &[&str], values: &[T], value: &mut T) -> bool {
    let mut i = values.iter().position(|v| v == value).unwrap_or(0);
    let mut changed = false;
    egui::Frame::new().inner_margin(egui::Margin { left: 16, right: 16, top: 8, bottom: 4 }).show(ui, |ui| {
        ui.label(egui::RichText::new(label).font(font(text::BODY_LARGE)).color(c(s.on_surface)));
        if m3::segmented(ui, s, names, &mut i) {
            *value = values[i];
            changed = true;
        }
    });
    changed
}

fn themes(ui: &mut egui::Ui, s: &Scheme, theme: &mut PageTheme) -> bool {
    let mut changed = false;
    egui::Frame::new().inner_margin(egui::Margin::symmetric(16, 4)).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            for t in PageTheme::ALL {
                let (paper, ink) = page::swatch(t, s);
                let (rect, response) = ui.allocate_exact_size(vec2(72.0, 64.0), Sense::click());
                let on = *theme == t;
                response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, on, t.label()));
                let p = ui.painter();
                p.rect_filled(rect, CornerRadius::same(12), c(paper));
                let edge = if on { Stroke::new(3.0, c(s.primary)) } else { Stroke::new(1.0, c(s.outline_variant)) };
                p.rect_stroke(rect, CornerRadius::same(12), edge, egui::StrokeKind::Inside);
                p.text(rect.center() - vec2(0.0, 8.0), egui::Align2::CENTER_CENTER, "あ", font(text::TITLE_LARGE), c(ink));
                p.text(rect.center() + vec2(0.0, 18.0), egui::Align2::CENTER_CENTER, t.label(), font(text::LABEL_MEDIUM), c(ink));
                if response.clicked() && !on {
                    *theme = t;
                    changed = true;
                }
            }
        });
    });
    changed
}
