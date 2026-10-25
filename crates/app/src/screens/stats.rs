// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use egui::{Align2, CornerRadius, RichText, Sense, pos2, vec2};
use egui_material_icons::icons::ICON_INSIGHTS;
use rubiyomi_core::stats::{Day, Stats, today};
use rubiyomi_core::theme::Scheme;

use crate::ui::m3::{self, c, font, text};

pub fn duration(seconds: u64) -> String {
    let minutes = seconds / 60;
    match minutes {
        0 if seconds > 0 => "under a minute".to_owned(),
        m if m < 60 => format!("{m} min"),
        m => format!("{} h {} min", m / 60, m % 60),
    }
}

pub fn count(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

pub fn ui(ui: &mut egui::Ui, s: &Scheme, stats: &Stats) {
    m3::top_app_bar(ui, s, "Stats", false);
    egui::CentralPanel::default().frame(super::surface(s)).show(ui, |ui| {
        let total = stats.total();
        if total == Day::default() {
            m3::empty_state(ui, s, ICON_INSIGHTS, "Nothing read yet", "Reading time and characters read will show up here, day by day.");
            return;
        }
        let on = today();
        let days = stats.recent(on, 14);
        let week = days[7..].iter().fold(Day::default(), |mut a, (_, d)| {
            a.seconds += d.seconds;
            a.chars += d.chars;
            a.lookups += d.lookups;
            a
        });
        let day = stats.day(on);
        let streak = stats.streak(on);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            egui::Frame::new().inner_margin(egui::Margin::symmetric(16, 8)).show(ui, |ui| {
                ui.set_width(ui.available_width().min(640.0));
                let tiles = [
                    ("Today", duration(day.seconds), format!("{} characters · {} looked up", count(day.chars), day.lookups)),
                    ("Streak", if streak == 1 { "1 day".into() } else { format!("{streak} days") }, "in a row, a minute or more".into()),
                    ("Last 7 days", duration(week.seconds), format!("{} characters · {} looked up", count(week.chars), week.lookups)),
                    ("All time", duration(total.seconds), format!("{} characters · {} looked up", count(total.chars), total.lookups)),
                ];
                let columns = if ui.available_width() > 480.0 { 2 } else { 1 };
                egui::Grid::new("stat-tiles").num_columns(columns).spacing(vec2(12.0, 12.0)).show(ui, |ui| {
                    let width = (ui.available_width() - 12.0 * (columns - 1) as f32) / columns as f32;
                    for (i, (title, big, small)) in tiles.iter().enumerate() {
                        tile(ui, s, width, title, big, small);
                        if (i + 1) % columns == 0 {
                            ui.end_row();
                        }
                    }
                });
                if total.seconds >= 600 && total.chars > 0 {
                    ui.add_space(12.0);
                    let speed = total.chars * 3600 / total.seconds;
                    ui.label(
                        RichText::new(format!("You read about {} characters an hour.", count(speed)))
                            .font(font(text::BODY_MEDIUM))
                            .color(c(s.on_surface_variant)),
                    );
                }
                ui.add_space(20.0);
                ui.label(RichText::new("Minutes read, last 14 days").font(font(text::TITLE_MEDIUM)).color(c(s.on_surface)));
                ui.add_space(8.0);
                chart(ui, s, &days);
            });
        });
    });
}

fn tile(ui: &mut egui::Ui, s: &Scheme, width: f32, title: &str, big: &str, small: &str) {
    egui::Frame::new().fill(c(s.surface_container)).corner_radius(16).inner_margin(egui::Margin::same(16)).show(ui, |ui| {
        ui.set_width(width - 32.0);
        ui.spacing_mut().item_spacing.y = 4.0;
        ui.label(RichText::new(title).font(font(text::LABEL_LARGE)).color(c(s.on_surface_variant)));
        ui.label(RichText::new(big).font(font(text::HEADLINE_SMALL)).color(c(s.on_surface)));
        ui.label(RichText::new(small).font(font(text::BODY_SMALL)).color(c(s.on_surface_variant)));
    });
}

fn chart(ui: &mut egui::Ui, s: &Scheme, days: &[(rubiyomi_core::stats::NaiveDate, Day)]) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 180.0), Sense::hover());
    let p = ui.painter();
    let top = days.iter().map(|(_, d)| d.seconds).max().unwrap_or(0).max(60) as f32 / 60.0;
    let plot = egui::Rect::from_min_max(rect.min + vec2(0.0, 18.0), rect.max - vec2(0.0, 22.0));
    let slot = plot.width() / days.len().max(1) as f32;
    let bar = (slot * 0.6).min(28.0);
    p.hline(plot.x_range(), plot.bottom(), egui::Stroke::new(1.0, c(s.outline_variant)));
    let last = days.len().saturating_sub(1);
    for (i, (date, d)) in days.iter().enumerate() {
        let minutes = d.seconds as f32 / 60.0;
        let x = plot.left() + slot * (i as f32 + 0.5);
        let h = (minutes / top * plot.height()).max(if d.seconds > 0 { 2.0 } else { 0.0 });
        let colour = if i == last { c(s.primary) } else { m3::alpha(c(s.primary), 0.55) };
        let r = egui::Rect::from_min_max(pos2(x - bar / 2.0, plot.bottom() - h), pos2(x + bar / 2.0, plot.bottom()));
        p.rect_filled(r, CornerRadius { nw: 4, ne: 4, sw: 0, se: 0 }, colour);
        if d.seconds >= 60 {
            p.text(pos2(x, r.top() - 2.0), Align2::CENTER_BOTTOM, format!("{:.0}", minutes.floor()), font(10.0), c(s.on_surface_variant));
        }
        let ink = if i == last { c(s.primary) } else { c(s.on_surface_variant) };
        p.text(pos2(x, plot.bottom() + 4.0), Align2::CENTER_TOP, date.format("%-d").to_string(), font(10.0), ink);
    }
}
