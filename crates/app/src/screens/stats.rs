// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use egui::{Align2, Color32, CornerRadius, Pos2, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use egui_material_icons::icons::ICON_INSIGHTS;
use rubiyomi_core::stats::{Day, Granularity, Period, Stats, today};
use rubiyomi_core::theme::Scheme;

use crate::ui::m3::{self, c, font, text};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Metric {
    Chars,
    Lookups,
    Speed,
    Chapters,
    Unknown,
}

impl Metric {
    const ALL: [Metric; 5] = [Metric::Chars, Metric::Lookups, Metric::Speed, Metric::Chapters, Metric::Unknown];

    fn title(self) -> &'static str {
        match self {
            Metric::Chars => "Characters read",
            Metric::Lookups => "Words looked up",
            Metric::Speed => "Reading speed",
            Metric::Chapters => "Chapters finished",
            Metric::Unknown => "Unknown words",
        }
    }

    fn unit(self) -> &'static str {
        match self {
            Metric::Chars => "chars",
            Metric::Lookups => "words",
            Metric::Speed => "chars/min",
            Metric::Chapters => "chapters",
            Metric::Unknown => "",
        }
    }

    // A count is drawn as bars, a rate as a line.
    fn is_rate(self) -> bool {
        matches!(self, Metric::Speed | Metric::Unknown)
    }

    fn lower_is_better(self) -> bool {
        self == Metric::Unknown
    }

    fn of(self, d: &Day) -> Option<f64> {
        match self {
            Metric::Chars => Some(d.chars as f64),
            Metric::Lookups => Some(d.lookups as f64),
            Metric::Speed => d.chars_per_minute(),
            Metric::Chapters => Some(d.chapters as f64),
            Metric::Unknown => d.unknown_rate(),
        }
    }

    fn format(self, v: Option<f64>) -> String {
        let Some(v) = v else { return "–".into() };
        match self {
            Metric::Chars | Metric::Lookups | Metric::Chapters | Metric::Speed => grouped(v.round() as i64),
            Metric::Unknown => format!("{}%", (v * 100.0).round()),
        }
    }

    // The period against the one before, as a short figure.
    fn delta(self, now: Option<f64>, before: Option<f64>) -> Option<String> {
        let (now, before) = (now?, before?);
        let diff = now - before;
        if diff == 0.0 {
            return Some("±0".into());
        }
        let sign = if diff > 0.0 { "+" } else { "−" };
        Some(match self {
            Metric::Unknown => format!("{sign}{} pt", (diff.abs() * 100.0).round()),
            Metric::Speed => format!("{sign}{}", grouped(diff.abs().round() as i64)),
            _ if before == 0.0 => format!("{sign}{}", grouped(diff.abs().round() as i64)),
            _ => format!("{sign}{}%", (diff.abs() / before * 100.0).round()),
        })
    }
}

fn grouped(n: i64) -> String {
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    if n < 0 { format!("−{out}") } else { out }
}

fn period_title(g: Granularity) -> &'static str {
    match g {
        Granularity::Day => "Today",
        Granularity::Week => "This week",
        Granularity::Month => "This month",
    }
}

fn previous_name(g: Granularity) -> &'static str {
    match g {
        Granularity::Day => "yesterday",
        Granularity::Week => "last week",
        Granularity::Month => "last month",
    }
}

fn period_range(p: &Period, g: Granularity) -> String {
    let last = p.end.pred_opt().unwrap_or(p.end);
    match g {
        Granularity::Day => p.start.format("%b %-d").to_string(),
        Granularity::Week => format!("{} – {}", p.start.format("%-m/%-d"), last.format("%-m/%-d")),
        Granularity::Month => p.start.format("%b %Y").to_string(),
    }
}

fn tick(p: &Period, g: Granularity) -> String {
    match g {
        Granularity::Day => p.start.format("%-d").to_string(),
        Granularity::Week => p.start.format("%-m/%-d").to_string(),
        Granularity::Month => p.start.format("%b").to_string(),
    }
}

pub struct StatsScreen {
    granularity: Granularity,
    // The bar under the finger, per chart, if one is.
    picked: [Option<usize>; 5],
    // A finger is held on a chart: the page doesn't scroll under it.
    holding: bool,
}

impl Default for StatsScreen {
    fn default() -> Self {
        StatsScreen { granularity: Granularity::Day, picked: [None; 5], holding: false }
    }
}

// How long a finger rests on a chart before it picks instead of scrolling.
const HOLD: f64 = 0.3;

#[derive(Clone, Copy, PartialEq)]
enum Gesture {
    Waiting { since: f64, at: Pos2 },
    Picking,
    Scrolling,
}

impl StatsScreen {
    fn count(&self) -> usize {
        match self.granularity {
            Granularity::Day => 14,
            Granularity::Week | Granularity::Month => 12,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, s: &Scheme, stats: &Stats) {
        m3::top_app_bar(ui, s, "Stats", false);
        egui::CentralPanel::default().frame(super::surface(s)).show(ui, |ui| {
            if stats.is_empty() {
                m3::empty_state(
                    ui,
                    s,
                    ICON_INSIGHTS,
                    "Nothing read yet",
                    "Characters read, words looked up and reading speed will show up here, by day, week and month.",
                );
                return;
            }
            let source = if self.holding {
                egui::scroll_area::ScrollSource { drag: egui::scroll_area::DragScroll::Never, ..Default::default() }
            } else {
                egui::scroll_area::ScrollSource::default()
            };
            let mut holding = false;
            m3::scroll().auto_shrink([false, false]).scroll_source(source).show(ui, |ui| {
                egui::Frame::new().inner_margin(egui::Margin { left: 16, right: 16, top: 8, bottom: 32 }).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let all = [Granularity::Day, Granularity::Week, Granularity::Month];
                    let mut i = all.iter().position(|g| *g == self.granularity).unwrap_or(0);
                    if m3::segmented(ui, s, &["Day", "Week", "Month"], &mut i) {
                        self.granularity = all[i];
                        self.picked = [None; 5];
                    }
                    let periods = stats.periods(self.granularity, self.count(), today());
                    ui.add_space(16.0);
                    let current = &periods[periods.len() - 1];
                    let previous = &periods[periods.len() - 2];
                    ui.label(RichText::new(period_title(self.granularity)).font(font(20.0)).color(c(s.on_surface)));
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(format!(
                            "{}, compared with {}",
                            period_range(current, self.granularity),
                            previous_name(self.granularity)
                        ))
                        .font(font(text::BODY_SMALL))
                        .color(c(s.on_surface_variant)),
                    );
                    ui.add_space(12.0);
                    tile_grid(ui, s, &current.stats, &previous.stats);
                    ui.add_space(24.0);
                    for (i, m) in Metric::ALL.into_iter().enumerate() {
                        holding |= chart_card(ui, s, m, &periods, self.granularity, &mut self.picked[i]);
                        ui.add_space(16.0);
                    }
                });
            });
            self.holding = holding;
        });
    }
}

fn tile_grid(ui: &mut Ui, s: &Scheme, current: &Day, previous: &Day) {
    let width = ui.available_width();
    let columns = if width >= 560.0 { 3 } else { 2 };
    let gap = 10.0;
    let w = (width - gap * (columns as f32 - 1.0)) / columns as f32;
    for row in Metric::ALL.chunks(columns) {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for &m in row {
                ui.allocate_ui_with_layout(vec2(w, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(w);
                    stat_tile(ui, s, m, m.of(current), m.of(previous));
                });
            }
        });
        ui.add_space(gap);
    }
}

fn stat_tile(ui: &mut Ui, s: &Scheme, m: Metric, now: Option<f64>, before: Option<f64>) {
    let (ink, dim) = (c(s.on_surface), c(s.on_surface_variant));
    egui::Frame::new()
        .fill(c(s.surface_container))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(m.title()).font(font(12.0)).color(dim));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let value = ui.painter().layout_no_wrap(m.format(now), font(26.0), ink);
                let (rect, _) = ui.allocate_exact_size(value.size(), Sense::hover());
                let baseline =
                    rect.top() + value.rows[0].row.glyphs.first().map_or(value.size().y * 0.8, |g| g.pos.y + value.rows[0].pos.y);
                ui.painter().galley(rect.min, value, ink);
                if !m.unit().is_empty() {
                    let unit = ui.painter().layout_no_wrap(m.unit().to_string(), font(12.0), dim);
                    let ub = unit.rows[0].row.glyphs.first().map_or(unit.size().y * 0.8, |g| g.pos.y + unit.rows[0].pos.y);
                    let (r, _) = ui.allocate_exact_size(unit.size(), Sense::hover());
                    ui.painter().galley(pos2(r.left(), baseline - ub), unit, dim);
                }
            });
            ui.add_space(6.0);
            ui.label(RichText::new(m.delta(now, before).unwrap_or_else(|| "Nothing to compare".into())).font(font(12.0)).color(dim));
        });
}

// Says whether a finger is holding the chart.
fn chart_card(ui: &mut Ui, s: &Scheme, m: Metric, periods: &[Period], g: Granularity, picked: &mut Option<usize>) -> bool {
    let mut holding = false;
    let values: Vec<Option<f64>> = periods.iter().map(|pd| m.of(&pd.stats)).collect();
    let shown = picked.unwrap_or(periods.len() - 1);
    let caption = match picked {
        None => series_caption(m, &values, periods, g),
        Some(i) => format!(
            "{}: {}{}",
            period_range(&periods[*i], g),
            m.format(values[*i]),
            if m.unit().is_empty() { String::new() } else { format!(" {}", m.unit()) }
        ),
    };
    egui::Frame::new()
        .fill(c(s.surface_container))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin { left: 14, right: 14, top: 12, bottom: 10 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(m.title()).font(font(14.0)).color(c(s.on_surface)));
            ui.add_space(2.0);
            ui.label(RichText::new(caption).font(font(12.0)).color(c(s.on_surface_variant)));
            ui.add_space(10.0);
            // Held still for a moment, a finger picks the period under it and can slide
            // sideways to pick others; moving first, it scrolls the page instead.
            let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 140.0), Sense::hover());
            let n = periods.len();
            let id = response.id.with("gesture");
            let mut gesture: Option<Gesture> = ui.data(|d| d.get_temp(id)).unwrap_or(None);
            let (pressed, down, origin, at, time) = ui
                .input(|i| (i.pointer.any_pressed(), i.pointer.primary_down(), i.pointer.press_origin(), i.pointer.interact_pos(), i.time));
            if pressed && let Some(o) = origin.filter(|o| rect.contains(*o)) {
                gesture = Some(Gesture::Waiting { since: time, at: o });
            }
            if !down {
                gesture = None;
            }
            if let (Some(Gesture::Waiting { since, at: start }), Some(now)) = (gesture, at) {
                if (now - start).length() > 8.0 {
                    gesture = Some(Gesture::Scrolling);
                } else if time - since >= HOLD {
                    gesture = Some(Gesture::Picking);
                } else {
                    ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(HOLD - (time - since)));
                }
            }
            match (gesture, at) {
                (Some(Gesture::Picking), Some(at)) => {
                    let i = (((at.x - rect.left()) / rect.width() * n as f32).floor() as isize).clamp(0, n as isize - 1) as usize;
                    *picked = Some(i);
                }
                (Some(Gesture::Waiting { .. }), _) => {}
                _ => *picked = None,
            }
            ui.data_mut(|d| d.insert_temp(id, gesture));
            holding = gesture == Some(Gesture::Picking);
            let labels: Vec<String> = periods.iter().map(|pd| tick(pd, g)).collect();
            paint_series(ui, rect, s, m, &values, &labels, shown);
        });
    holding
}

// The whole chart in one line: the best period, or which way a rate went.
fn series_caption(m: Metric, values: &[Option<f64>], periods: &[Period], g: Granularity) -> String {
    let present: Vec<usize> = (0..values.len()).filter(|&i| values[i].is_some_and(|v| v > 0.0 || m.is_rate())).collect();
    if present.len() < 2 {
        return "Not enough reading yet to compare".into();
    }
    if !m.is_rate() {
        let mut best = present[0];
        for &i in &present {
            if values[i] > values[best] {
                best = i;
            }
        }
        return format!("Best: {} ({} {})", period_range(&periods[best], g), m.format(values[best]), m.unit());
    }
    let (first, last) = (values[present[0]].unwrap_or(0.0), values[present[present.len() - 1]].unwrap_or(0.0));
    if last == first {
        return "No change".into();
    }
    let better = if m.lower_is_better() { last < first } else { last > first };
    format!(
        "{} from {} ({}) to {}",
        if better { "Improved" } else { "Went" },
        m.format(Some(first)),
        period_range(&periods[present[0]], g),
        m.format(Some(last))
    )
}

// A round number at or above `v`, for the top of the scale.
fn nice_ceiling(v: f64) -> f64 {
    if v <= 0.0 {
        return 1.0;
    }
    let exp = 10f64.powf(v.log10().floor());
    let m = v / exp;
    let nice = if m <= 1.0 {
        1.0
    } else if m <= 2.0 {
        2.0
    } else if m <= 2.5 {
        2.5
    } else if m <= 5.0 {
        5.0
    } else {
        10.0
    };
    nice * exp
}

fn paint_series(ui: &Ui, rect: Rect, s: &Scheme, m: Metric, values: &[Option<f64>], labels: &[String], picked: usize) {
    const LABEL_BAND: f32 = 18.0;
    const TOP_BAND: f32 = 14.0;
    let painter = ui.painter();
    let n = values.len();
    if n == 0 {
        return;
    }
    let bottom = rect.bottom() - LABEL_BAND;
    let plot_h = bottom - (rect.top() + TOP_BAND);
    let slot = rect.width() / n as f32;
    let mut max = values.iter().flatten().fold(0.0f64, |a, &b| a.max(b));
    if m == Metric::Unknown {
        max = max.max(0.05);
    }
    let scale = if max > 0.0 { nice_ceiling(max) } else { 1.0 };
    let (accent, ink, grid) = (c(s.primary), c(s.on_surface_variant), c(s.outline_variant));
    let small = font(10.0);

    for f in [1.0, 0.5] {
        painter.hline(rect.x_range(), bottom - plot_h * f, Stroke::new(1.0, grid));
    }
    painter.text(pos2(rect.right(), rect.top() + TOP_BAND - 3.0), Align2::RIGHT_BOTTOM, m.format(Some(scale)), small.clone(), ink);
    painter.hline(rect.x_range(), bottom, Stroke::new(1.0, m3::alpha(ink, 0.5)));
    let y_of = |v: f64| bottom - plot_h * (v / scale).clamp(0.0, 1.0) as f32;

    if m.is_rate() {
        let points: Vec<(usize, Pos2)> =
            (0..n).filter_map(|i| values[i].map(|v| (i, pos2(rect.left() + slot * i as f32 + slot / 2.0, y_of(v))))).collect();
        if points.len() > 1 {
            painter.add(egui::Shape::line(points.iter().map(|(_, p)| *p).collect(), Stroke::new(2.0, accent)));
        }
        // A period without reading breaks the line; a dot marks each one with some, so a lone one shows.
        for (i, pt) in &points {
            painter.circle_filled(*pt, if *i == picked { 4.5 } else { 3.0 }, accent);
        }
        if let Some((_, pt)) = points.iter().find(|(i, _)| *i == picked) {
            painter.text(*pt - vec2(0.0, 7.0), Align2::CENTER_BOTTOM, m.format(values[picked]), small.clone(), ink);
        }
    } else {
        let gap = (slot * 0.3).max(2.0);
        let width = slot - gap;
        for (i, value) in values.iter().enumerate() {
            let Some(v) = value.filter(|v| *v > 0.0) else { continue };
            let left = rect.left() + slot * i as f32 + gap / 2.0;
            let top = y_of(v).min(bottom - 2.0);
            let colour = if i == picked { accent } else { m3::alpha(accent, 0.55) };
            painter.rect_filled(
                Rect::from_min_max(pos2(left, top), pos2(left + width, bottom)),
                CornerRadius { nw: 3, ne: 3, sw: 0, se: 0 },
                colour,
            );
            if i == picked {
                painter.text(pos2(left + width / 2.0, top - 3.0), Align2::CENTER_BOTTOM, m.format(Some(v)), small.clone(), ink);
            }
        }
    }

    // Every label if they fit, else every other, and always the last.
    let every = if slot >= 30.0 { 1 } else { 2 };
    let mut i = n as isize - 1;
    while i >= 0 {
        let iu = i as usize;
        let colour: Color32 = if iu == picked { accent } else { ink };
        painter.text(
            pos2(rect.left() + slot * iu as f32 + slot / 2.0, bottom + 4.0),
            Align2::CENTER_TOP,
            &labels[iu],
            small.clone(),
            colour,
        );
        i -= every;
    }
}
