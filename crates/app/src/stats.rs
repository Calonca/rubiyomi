// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use nsh_core::prelude::*;
use rubiyomi_core::stats::{Day, Granularity, Period, Stats, today};

use crate::app::shared;
use crate::ui::{self, Ripples, c, scheme};

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

    fn unit(self, of: Option<f64>) -> &'static str {
        let one = of == Some(1.0);
        match self {
            Metric::Chars if one => "char",
            Metric::Chars => "chars",
            Metric::Lookups if one => "word",
            Metric::Lookups => "words",
            Metric::Speed => "chars/min",
            Metric::Chapters if one => "chapter",
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
        return format!("Best: {} ({} {})", period_range(&periods[best], g), m.format(values[best]), m.unit(values[best]));
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

// The chart: the scale's top band, the plot, and the label band under it.
const TOP_BAND: f32 = 14.0;
const PLOT: f32 = 118.0;
const LABEL_BAND: f32 = 18.0;

fn count(g: Granularity) -> usize {
    match g {
        Granularity::Day => 14,
        Granularity::Week | Granularity::Month => 12,
    }
}

fn stat_tile(m: Metric, now: Option<f64>, before: Option<f64>) -> impl IntoElement {
    view()
        .gap(px(6.0))
        .px_4()
        .py_3()
        .rounded(px(10.0))
        .bg(move || c(scheme().surface_container))
        .child(text(m.title()).text_size(px(12.0)).text_color(move || c(scheme().on_surface_variant)))
        .child(
            view()
                .flex_row()
                .items_baseline()
                .gap(px(4.0))
                .child(text(m.format(now)).text_size(px(26.0)).text_color(move || c(scheme().on_surface)))
                .child(
                    (!m.unit(now).is_empty())
                        .then(|| text(m.unit(now)).text_size(px(12.0)).text_color(move || c(scheme().on_surface_variant))),
                ),
        )
        .child(
            text(m.delta(now, before).unwrap_or_else(|| "Nothing to compare".into()))
                .text_size(px(12.0))
                .text_color(move || c(scheme().on_surface_variant)),
        )
}

fn tiles(current: &Day, previous: &Day) -> AnyElement {
    let mut grid = view().grid().grid_cols(|| if ui::window_width().min(ui::CONTENT_W) >= 560.0 { 3u16 } else { 2 }).gap(px(10.0));
    for m in Metric::ALL {
        grid = grid.child(stat_tile(m, m.of(current), m.of(previous)));
    }
    grid.into_element()
}

fn line(values: &[Option<f64>], scale: f64, width: f32) -> Painter {
    let n = values.len();
    let slot = width / n as f32;
    let at = |i: usize, v: f64| (slot * i as f32 + slot / 2.0, TOP_BAND + PLOT * (1.0 - (v / scale).clamp(0.0, 1.0) as f32));
    let mut p = Painter::new().stroke_color(c(scheme().primary)).line_width(2.0f32);
    // A period without reading breaks the line.
    let mut open = false;
    for (i, v) in values.iter().enumerate() {
        match v {
            Some(v) => {
                let (x, y) = at(i, *v);
                p = if open { p.line_to(x, y) } else { p.begin_path().move_to(x, y) };
                open = true;
            }
            None => {
                if open {
                    p = p.stroke();
                }
                open = false;
            }
        }
    }
    if open {
        p = p.stroke();
    }
    p
}

// Debug hook for the headless runs: `RUBIYOMI_STATS=week|month,charts,rates,pick3` picks the granularity, hides
// the tiles, hides the bar charts, and picks a bar in every chart.
fn hook(word: &str) -> bool {
    crate::hooks::var("RUBIYOMI_STATS").is_some_and(|v| v.split(',').any(|w| w == word))
}

fn chart(m: Metric, periods: &[Period], g: Granularity) -> AnyElement {
    let picked: Signal<Option<usize>> = signal(if hook("pick3") { Some(3) } else { None });
    let values: Vec<Option<f64>> = periods.iter().map(|pd| m.of(&pd.stats)).collect();
    let n = values.len();
    let mut max = values.iter().flatten().fold(0.0f64, |a, &b| a.max(b));
    if m == Metric::Unknown {
        max = max.max(0.05);
    }
    let scale = if max > 0.0 { nice_ceiling(max) } else { 1.0 };
    let periods: Vec<Period> = periods.to_vec();
    let values = std::rc::Rc::new(values);
    let width = signal(0.0f32);

    let caption = {
        let (values, periods) = (values.clone(), periods.clone());
        text(move || match picked.get() {
            None => series_caption(m, &values, &periods, g),
            Some(i) => format!(
                "{}: {}{}",
                period_range(&periods[i], g),
                m.format(values[i]),
                if m.unit(values[i]).is_empty() { String::new() } else { format!(" {}", m.unit(values[i])) }
            ),
        })
        .text_size(px(12.0))
        .text_color(move || c(scheme().on_surface_variant))
    };

    let rule = |y: f32| view().absolute().left(px(0.0)).right(px(0.0)).top(px(y)).h(px(1.0)).bg(move || c(scheme().outline_variant));
    let columns = {
        let (values, periods) = (values.clone(), periods.clone());
        dynamic(move || {
            let shown = picked.get();
            let chosen = shown.unwrap_or(n - 1);
            let every = if width.get() / n as f32 >= 30.0 { 1 } else { 2 };
            let mut row = view().flex_row().absolute().left(px(0.0)).right(px(0.0)).top(px(0.0)).h(px(TOP_BAND + PLOT + LABEL_BAND));
            for i in 0..n {
                let label = if (n - 1 - i).is_multiple_of(every) { tick(&periods[i], g) } else { String::new() };
                let mark = values[i].filter(|v| *v > 0.0 || m.is_rate());
                let mut plot = view().h(px(TOP_BAND + PLOT)).w_full().justify_end().items_center();
                if let Some(v) = mark {
                    let h = ((v / scale).clamp(0.0, 1.0) as f32 * PLOT).max(2.0);
                    if m.is_rate() {
                        let r = if i == chosen { 4.5 } else { 3.0 };
                        let y = TOP_BAND + PLOT - h - r;
                        plot = view().h(px(TOP_BAND + PLOT)).w_full().child(
                            view()
                                .absolute()
                                .left(px(0.0))
                                .right(px(0.0))
                                .top(px(y))
                                .items_center()
                                .child(view().size(px(r * 2.0)).rounded(px(r)).bg(move || c(scheme().primary))),
                        );
                    } else {
                        let on = i == chosen;
                        plot = plot.child(
                            view().w_full().px(px(3.0)).h(px(h)).child(
                                view()
                                    .w_full()
                                    .h_full()
                                    .rounded_t(px(3.0))
                                    .bg(move || if on { c(scheme().primary) } else { c(scheme().primary).alpha(0.55) }),
                            ),
                        );
                    }
                }
                row = row.child(
                    view()
                        .flex_1()
                        .ripples()
                        .on_press(move || picked.set(if picked.get_untracked() == Some(i) { None } else { Some(i) }))
                        .child(plot)
                        .child(
                            view().h(px(LABEL_BAND)).items_center().pt(px(4.0)).child(text(label).text_size(px(10.0)).text_color(
                                move || {
                                    if i == chosen { c(scheme().primary) } else { c(scheme().on_surface_variant) }
                                },
                            )),
                        ),
                );
            }
            row.into_element()
        })
    };
    let lines = m.is_rate().then(|| {
        let values = values.clone();
        canvas(move || line(&values, scale, width.get()))
            .absolute()
            .left(px(0.0))
            .top(px(0.0))
            .size_full()
            .pointer_events(PointerEvents::None)
    });
    let top_label = text(m.format(Some(scale))).text_size(px(10.0)).text_color(move || c(scheme().on_surface_variant));

    view()
        .gap(px(2.0))
        .px_4()
        .pt_3()
        .pb_3()
        .rounded(px(10.0))
        .bg(move || c(scheme().surface_container))
        .child(text(m.title()).text_size(px(14.0)).text_color(move || c(scheme().on_surface)))
        .child(caption)
        .child(
            view()
                .mt(px(8.0))
                .h(px(TOP_BAND + PLOT + LABEL_BAND))
                .on_layout(move |b: &Bounds<Points>| width.set(b.size.width.0))
                .child(rule(TOP_BAND))
                .child(rule(TOP_BAND + PLOT / 2.0))
                .child(rule(TOP_BAND + PLOT))
                .child(view().absolute().right(px(0.0)).top(px(0.0)).child(top_label))
                .child(lines)
                .child(columns),
        )
        .into_element()
}

fn report(g: Granularity, stats: &Stats) -> AnyElement {
    let periods = stats.periods(g, count(g), today());
    let current = &periods[periods.len() - 1];
    let previous = &periods[periods.len() - 2];
    let mut body = view()
        .gap(px(12.0))
        .px_4()
        .pb_6()
        .child(
            view().child(text(period_title(g)).text_size(px(20.0)).text_color(move || c(scheme().on_surface))).child(
                text(format!("{}, compared with {}", period_range(current, g), previous_name(g)))
                    .text_size(px(12.0))
                    .text_color(move || c(scheme().on_surface_variant)),
            ),
        )
        .child((!hook("charts")).then(|| tiles(&current.stats, &previous.stats)));
    for m in Metric::ALL.into_iter().filter(|m| !hook("rates") || m.is_rate()) {
        body = body.child(chart(m, &periods, g));
    }
    body.into_element()
}

pub fn screen() -> impl IntoElement {
    let shared = shared();
    let page = view().flex_1().child(ui::page_bar("Stats"));
    if shared.stats.borrow().is_empty() {
        return page.child(ui::empty_state(
            ui::glyph::INSIGHTS,
            "Nothing read yet",
            "Characters read, words looked up and reading speed will show up here, by day, week and month.",
        ));
    }
    let first = if hook("week") {
        Granularity::Week
    } else if hook("month") {
        Granularity::Month
    } else {
        Granularity::Day
    };
    let granularity = signal(first);
    let stats = shared.stats.clone();
    page.child(
        scroll().flex_1().child(
            view()
                .child(ui::segmented(
                    None,
                    &["Day", "Week", "Month"],
                    &[Granularity::Day, Granularity::Week, Granularity::Month],
                    move || granularity.get(),
                    move |g| granularity.set(g),
                ))
                .child(dynamic(move || report(granularity.get(), &stats.borrow()))),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_of_a_thing_is_singular() {
        assert_eq!(Metric::Chapters.unit(Some(1.0)), "chapter");
        assert_eq!(Metric::Chapters.unit(Some(2.0)), "chapters");
        assert_eq!(Metric::Lookups.unit(Some(1.0)), "word");
        assert_eq!(Metric::Chars.unit(Some(1.0)), "char");
        assert_eq!(Metric::Chars.unit(None), "chars");
    }
}
