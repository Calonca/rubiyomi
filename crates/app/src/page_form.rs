// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::rc::Rc;
use std::sync::Arc;

use nsh_core::prelude::*;
use rubiyomi_core::grade::Grader;
use rubiyomi_core::settings::{Furigana, PageSettings, PageTheme, Typeface, WordColours};

use crate::colours_page;
use crate::ui::{self, Ripples, c, glyph, page_scheme, scheme};

fn themes(page: Signal<PageSettings>) -> AnyElement {
    let mut row = view().flex_row().flex_wrap().gap(px(8.0)).px_4().py_1();
    for theme in PageTheme::ALL {
        row = row.child(
            view()
                .w(px(72.0))
                .h(px(64.0))
                .rounded(px(12.0))
                .items_center()
                .justify_center()
                .border_2()
                .label(theme.label())
                .bg(move || c(page_scheme(theme).surface))
                .border_color(
                    move || {
                        if page.with(|p| p.theme == theme) { c(scheme().primary) } else { c(page_scheme(theme).outline_variant) }
                    },
                )
                .ripples()
                .on_press(move || page.update(|p| p.theme = theme))
                .child(text("\u{3042}").text_size(px(22.0)).text_color(move || c(page_scheme(theme).on_surface)))
                .child(text(theme.label()).text_size(px(11.0)).text_color(move || c(page_scheme(theme).on_surface))),
        );
    }
    row.into_element()
}

macro_rules! flag {
    ($title:expr, $detail:expr, $page:expr, $field:ident) => {
        ui::switch_row($title, $detail, move || $page.with(|p| p.$field), move |on| $page.update(|p| p.$field = on))
    };
}

macro_rules! amount {
    ($title:expr, $range:expr, $step:expr, $page:expr, $dragging:expr, $field:ident, $digits:expr) => {
        ui::slider_row(
            $title,
            $range,
            $step,
            $page.get_untracked().$field,
            |v| format!("{:.*}", $digits, v),
            move |v| $page.update(|p| p.$field = v),
            $dragging,
        )
    };
}

// The page settings, in Settings and in the reader's "Aa" sheet.
pub fn form(
    page: Signal<PageSettings>,
    dragging: Signal<bool>,
    grader: Signal<Option<Arc<dyn Grader>>>,
    to_colours: impl Fn() + 'static,
) -> AnyElement {
    let to_colours = Rc::new(to_colours);
    let palette = computed(move || page.with(|p| p.palette));
    view()
        .child(ui::section_header("Theme"))
        .child(themes(page))
        .child(ui::section_header("Text"))
        .child(amount!("Text size", 14.0..=44.0, 1.0, page, dragging, font_size, 0))
        .child(amount!("Line spacing", 1.3..=2.6, 0.1, page, dragging, line_height, 1))
        .child(amount!("Margins", 8.0..=72.0, 4.0, page, dragging, margin, 0))
        .child({
            // Under ten a line is too short to read; the slider's start means no limit.
            let value = page.get_untracked().max_chars_per_line as f32;
            ui::slider_row(
                "Characters per line",
                0.0..=60.0,
                1.0,
                value,
                |v| if v < 10.0 { "No limit".to_owned() } else { format!("{v:.0}") },
                move |v| page.update(|p| p.max_chars_per_line = if v < 10.0 { 0 } else { v as u32 }),
                dragging,
            )
        })
        .child(ui::segmented(
            Some("Typeface"),
            &["Serif", "Sans"],
            &[Typeface::Serif, Typeface::Sans],
            move || page.with(|p| p.typeface),
            move |v| page.update(|p| p.typeface = v),
        ))
        .child(ui::segmented(
            Some("Furigana"),
            &["Show", "Hide"],
            &[Furigana::Show, Furigana::Hide],
            // A saved "blur" (the old app's Faint) shows the readings, so it reads as Show.
            move || page.with(|p| if p.furigana == Furigana::Hide { Furigana::Hide } else { Furigana::Show }),
            move |v| page.update(|p| p.furigana = v),
        ))
        .child(dynamic(move || {
            if grader.with(|g| g.is_none()) && !crate::hooks::fake_states_on() {
                return view().into_element();
            }
            let to_colours = to_colours.clone();
            view()
                .child(ui::segmented(
                    Some("Word colours"),
                    &["Off", "Underline", "Text"],
                    &[WordColours::Off, WordColours::Underline, WordColours::Text],
                    move || page.with(|p| p.colours),
                    move |v| page.update(|p| p.colours = v),
                ))
                .child(dynamic(move || {
                    let to_colours = to_colours.clone();
                    ui::icon_row(glyph::PALETTE, "Palette", palette.get().label(), move || to_colours()).into_element()
                }))
                .into_element()
        }))
        .child(flag!("Vertical text", "Columns read from right to left", page, vertical))
        .child(ui::section_header("Reading"))
        .child(flag!("Tap the edges to turn", "Otherwise swipe, or use the keys", page, tap_to_turn))
        .child(flag!("Page number and percentage", "Under the page", page, show_position))
        .child(flag!("Keep the screen on", "While a book is open", page, keep_awake))
        .child(flag!("Reopen the last book", "On launch, where you left it", page, open_last_on_launch))
        .into_element()
}

// What the reader's page sheet shows. The word colours open in the sheet, with the page in sight.
#[derive(Clone, Copy, PartialEq)]
enum Shown {
    Form,
    Colours,
    Custom,
}

impl Shown {
    fn title(self) -> &'static str {
        match self {
            Shown::Form => "Page",
            Shown::Colours => "Word colours",
            Shown::Custom => "Custom colours",
        }
    }

    fn parent(self) -> Shown {
        if self == Shown::Custom { Shown::Colours } else { Shown::Form }
    }
}

fn sheet_title(shown: Signal<Shown>) -> AnyElement {
    dynamic(move || {
        let at = shown.get();
        view()
            .flex_row()
            .items_center()
            .child((at != Shown::Form).then(|| {
                view()
                    .w(px(40.0))
                    .h(px(40.0))
                    .items_center()
                    .justify_center()
                    .label("Back")
                    .ripples()
                    .on_press(move || shown.set(at.parent()))
                    .child(ui::icon(glyph::ARROW_BACK, 24.0, move || c(scheme().on_surface)))
            }))
            .child(text(at.title()).text_size(px(22.0)).text_color(move || c(scheme().on_surface)))
            .into_element()
    })
    .into_element()
}

fn sheet_body(
    shown: Signal<Shown>,
    page: Signal<PageSettings>,
    dragging: Signal<bool>,
    grader: Signal<Option<Arc<dyn Grader>>>,
) -> AnyElement {
    dynamic(move || {
        let at = shown.get();
        if at != Shown::Form {
            on_back_press(move || {
                shown.set(at.parent());
                true
            });
        }
        match at {
            Shown::Form => form(page, dragging, grader, move || shown.set(Shown::Colours)),
            Shown::Colours => colours_page::choose(page, move || shown.set(Shown::Custom)),
            Shown::Custom => colours_page::custom(page),
        }
    })
    .into_element()
}

// The reader's page sheet. Changing a setting lays the chapter out again where the reader is.
pub fn sheet(
    open: Signal<bool>,
    page: Signal<PageSettings>,
    dragging: Signal<bool>,
    grader: Signal<Option<Arc<dyn Grader>>>,
    size: Signal<(f32, f32)>,
) -> AnyElement {
    when(
        move || open.get(),
        move || {
            let height = size.get_untracked().1 * 0.72;
            // A debug hook for the headless runs: `RUBIYOMI_SHEET_PAGE=colours` starts there.
            let hooked = crate::hooks::var("RUBIYOMI_SHEET_PAGE").is_some();
            let shown = signal(if hooked { Shown::Colours } else { Shown::Form });
            ui::scrim().justify_end().on_press(move || open.set(false)).child(
                view()
                    .w_full()
                    .max_w(px(ui::CONTENT_W))
                    .self_center()
                    .h(px(height))
                    .rounded_t(px(28.0))
                    .overflow_hidden()
                    .bg(move || c(scheme().surface_container_high))
                    .on_press(|| {})
                    .child(
                        view()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .px_4()
                            .pt_4()
                            .child(sheet_title(shown))
                            .child(ui::tonal_button("Done", move || open.set(false))),
                    )
                    .child(scroll().flex_1().child(sheet_body(shown, page, dragging, grader))),
            )
        },
    )
    .into_element()
}
