// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use nsh_base::{ColorChannel, ColorPickerEvent, color_area, color_field, color_picker, color_slider};
use nsh_core::prelude::*;
use rubiyomi_core::cards::CardState;
use rubiyomi_core::grade::{anki, jpdb, wanikani};
use rubiyomi_core::settings::{CustomColours, PageSettings, PageTheme, ReviewSource, WordColours, WordPalette};
use rubiyomi_core::theme::{Color, Scheme};

use crate::app::shared;
use crate::reader::words::{StateColours, full_colour, state_colour};
use crate::ui::{self, Ripples, c, glyph, page_scheme, scheme};

// The custom grid, for a light page and for a dark one: a page can change between the two, so
// a choice is stored as its pair, and both of a pair read on every page theme.
pub const SWATCHES: [[u32; 2]; 15] = [
    [0xc62828, 0xe06c6c],
    [0xb35309, 0xef9a55],
    [0x9a6a00, 0xe7b95a],
    [0x5f7a00, 0xb5d65a],
    [0x2e7d32, 0x69c67a],
    [0x00695c, 0x4db6ac],
    [0x00788c, 0x4fc3d9],
    [0x1565c0, 0x6ba8e8],
    [0x3f51b5, 0x9fa8f0],
    [0x5e35b1, 0x9b8ec4],
    [0x9c27b0, 0xd98ae6],
    [0xc2185b, 0xf48fb1],
    [0x795548, 0xc9a38f],
    [0x607080, 0xaab2bd],
    [0x37474f, 0xcfd8dc],
];

// The same contrast the swatches hold to.
const READABLE: f64 = 3.0;

const SAMPLES: [CardState; 6] =
    [CardState::Known, CardState::New, CardState::Due, CardState::Failed, CardState::NotInDeck, CardState::Blacklisted];

const SERVICES: [(ReviewSource, &str, &[CardState]); 3] = [
    (ReviewSource::Jpdb, "jpdb.io", jpdb::STATES),
    (ReviewSource::Anki, "Anki", anki::STATES),
    (ReviewSource::WaniKani, "WaniKani", wanikani::STATES),
];

fn services_for(connected: ReviewSource) -> Vec<(ReviewSource, &'static str, &'static [CardState])> {
    let mut services = SERVICES.to_vec();
    services.sort_by_key(|(source, ..)| *source != connected);
    services
}

pub fn set_custom(page: Signal<PageSettings>, state: CardState, colour: Option<[u32; 2]>) {
    page.update(|p| {
        p.custom_colours.insert(state, colour);
    });
}

#[derive(Clone, PartialEq)]
struct Look {
    palette: WordPalette,
    custom: CustomColours,
    theme: PageTheme,
}

fn look(page: Signal<PageSettings>) -> Memo<Look> {
    computed(move || page.with(|p| Look { palette: p.palette, custom: p.custom_colours.clone(), theme: p.theme }))
}

fn ink(colour: Option<Color>, on: Scheme) -> Rgba {
    c(colour.unwrap_or(on.on_surface))
}

fn card(option: WordPalette, look: &Look, on: Scheme, choose: impl Fn() + 'static) -> AnyElement {
    let colours = StateColours::of(option, &look.custom, on.dark);
    let chosen = look.palette == option;
    let mut card = view()
        .flex_1()
        .min_w(px(128.0))
        .gap(px(2.0))
        .p_3()
        .rounded(px(12.0))
        .border_2()
        .label(option.label())
        .bg(c(on.surface))
        .border_color(move || if chosen { c(scheme().primary) } else { c(on.outline_variant) })
        .ripples()
        .on_press(choose)
        .child(text(option.label()).text_size(px(15.0)).pb_1().text_color(c(on.on_surface)));
    for state in SAMPLES {
        card = card.child(text(state.label().to_lowercase()).text_size(px(14.0)).text_color(ink(colours.get(state), on)));
    }
    card.into_element()
}

fn dot(colour: Option<Color>, on: Scheme) -> AnyElement {
    let dot = view().size(px(10.0)).rounded(px(5.0));
    match colour {
        Some(colour) => dot.bg(c(colour)).into_element(),
        None => dot.border_1().border_color(c(on.on_surface_variant)).into_element(),
    }
}

fn service(title: &'static str, states: &[CardState], colours: StateColours, on: Scheme) -> AnyElement {
    let mut names = view()
        .flex_row()
        .flex_wrap()
        .mx_4()
        .my_1()
        .px_3()
        .py_2()
        .rounded(px(12.0))
        .border_1()
        .border_color(c(on.outline_variant))
        .bg(c(on.surface));
    for state in states {
        let colour = colours.get(*state);
        names = names.child(
            view()
                .w(px(132.0))
                .flex_row()
                .items_center()
                .gap(px(8.0))
                .py_1()
                .child(dot(colour, on))
                .child(text(state.label()).text_size(px(14.0)).text_color(ink(colour, on))),
        );
    }
    view().child(ui::section_header(title)).child(names).into_element()
}

fn about(palette: WordPalette) -> &'static str {
    match palette {
        WordPalette::Full => "Every state has its own colour.",
        WordPalette::Minimal => "Only what you can act on is coloured: new, learning, due and failed words, and words not in a deck.",
        WordPalette::Custom => "Your own colours, starting as a copy of Full. Tap Custom to change them.",
    }
}

// Choosing a palette: its cards, and what each service's states look like in it.
pub fn choose(page: Signal<PageSettings>, to_custom: impl Fn() + Clone + 'static) -> AnyElement {
    let look = look(page);
    let mode = computed(move || page.with(|p| p.colours));
    let settings = shared().settings;
    let connected = computed(move || settings.with(|s| s.reviews));
    view()
        .pb_4()
        .child(dynamic(move || match mode.get() {
            WordColours::Off => ui::hint("Word colours are off for now. Turn them on in Settings \u{2192} Page.").into_element(),
            _ => view().into_element(),
        }))
        .child(dynamic(move || {
            let now = look.get();
            let on = page_scheme(now.theme);
            let mut cards = view().flex_row().flex_wrap().gap(px(8.0)).px_4().py_2();
            for option in WordPalette::ALL {
                let to_custom = to_custom.clone();
                cards = cards.child(card(option, &now, on, move || {
                    page.update(|p| p.palette = option);
                    if option == WordPalette::Custom {
                        to_custom();
                    }
                }));
            }
            let colours = StateColours::of(now.palette, &now.custom, on.dark);
            let mut body = view().child(cards).child(ui::hint(about(now.palette)));
            for (_, title, states) in services_for(connected.get()) {
                body = body.child(service(title, states, colours, on));
            }
            body.into_element()
        }))
        .into_element()
}

// The grid, and every colour of the built-in palettes, faint as some of those are on purpose.
fn swatches() -> Vec<[u32; 2]> {
    let plain = CustomColours::new();
    let mut all = SWATCHES.to_vec();
    for palette in [WordPalette::Full, WordPalette::Minimal] {
        for state in CardState::ALL {
            let shade = |dark| state_colour(state, dark, palette, &plain).map(Color::to_rgb);
            if let (Some(light), Some(dark)) = (shade(false), shade(true))
                && !all.contains(&[light, dark])
            {
                all.push([light, dark]);
            }
        }
    }
    all
}

fn page_surfaces(dark: bool) -> Vec<Color> {
    let themes = PageTheme::ALL.into_iter().filter(|t| *t != PageTheme::System).map(page_scheme);
    themes.filter(|s| s.dark == dark).map(|s| s.surface).chain([ui::app_scheme(dark).surface]).collect()
}

// The picked colour, taken towards black or white only as far as every page of that kind needs.
fn readable(rgb: u32, dark: bool) -> u32 {
    let surfaces = page_surfaces(dark);
    let picked = Color::from_rgb(rgb);
    let target = if dark { 255.0 } else { 0.0 };
    let towards = |channel: u8, part: f32| (channel as f32 + (target - channel as f32) * part).round() as u8;
    (0..=40)
        .map(|step| {
            let part = step as f32 / 40.0;
            Color { r: towards(picked.r, part), g: towards(picked.g, part), b: towards(picked.b, part) }
        })
        .find(|colour| surfaces.iter().all(|surface| colour.contrast(*surface) >= READABLE))
        .map_or(if dark { 0xffffff } else { 0 }, Color::to_rgb)
}

fn shades(rgb: u32) -> [u32; 2] {
    [readable(rgb, false), readable(rgb, true)]
}

fn to_rgb(colour: Hsla) -> u32 {
    u32::from(Rgba::from(colour)) >> 8
}

fn sample(rgb: u32, background: Color) -> AnyElement {
    view()
        .flex_1()
        .items_center()
        .py_2()
        .rounded(px(8.0))
        .bg(c(background))
        .child(text("word").text_size(px(16.0)).text_color(c(Color::from_rgb(rgb))))
        .into_element()
}

// Any colour: the shades it gets are shown, as those are what is saved.
fn picker(state: CardState, start: u32, page: Signal<PageSettings>, open: Signal<Option<CardState>>, on: Scheme) -> AnyElement {
    let colour = signal(Hsla::from(rgb(start)));
    let (light_page, dark_page) = (page_scheme(PageTheme::Light).surface, page_scheme(PageTheme::Dark).surface);
    view()
        .gap(px(10.0))
        .py_2()
        .child(
            color_picker()
                .value(move || Some(colour.get()))
                .on_change(move |e: &ColorPickerEvent| {
                    if let ColorPickerEvent::Change(value, _) = e {
                        colour.set(*value);
                    }
                })
                .child(
                    color_area()
                        .accessibility_label("Colour")
                        .w_full()
                        .h(px(140.0))
                        .thumb_class("size-5 rounded-full border-2 border-white"),
                )
                .child(
                    color_slider()
                        .channel(ColorChannel::Hue)
                        .accessibility_label("Hue")
                        .w_full()
                        .h(px(36.0))
                        .justify_center()
                        .track_style(|s| s.h(px(12.0)))
                        .thumb_class("size-5 rounded-full border-2 border-white"),
                )
                .child(
                    color_field()
                        .accessibility_label("Colour as hex")
                        .w(px(120.0))
                        .h(px(40.0))
                        .px_2()
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(c(on.outline_variant))
                        .text_color(c(on.on_surface)),
                ),
        )
        .child(dynamic(move || {
            let [light, dark] = shades(to_rgb(colour.get()));
            view().flex_row().gap(px(8.0)).child(sample(light, light_page)).child(sample(dark, dark_page)).into_element()
        }))
        .child(ui::button_row("Use this colour", move || {
            set_custom(page, state, Some(shades(to_rgb(colour.get_untracked()))));
            open.set(None);
        }))
        .into_element()
}

fn swatch(colour: Option<Color>, chosen: bool, name: String, on: Scheme) -> AnyElement {
    let ring = view()
        .size(px(36.0))
        .rounded(px(18.0))
        .items_center()
        .justify_center()
        .border_2()
        .label(name)
        .border_color(move || if chosen { c(on.on_surface) } else { rgba(0) });
    let face = view().size(px(26.0)).rounded(px(13.0)).items_center().justify_center();
    match colour {
        Some(colour) => ring.child(face.bg(c(colour))).into_element(),
        None => ring
            .child(face.border_1().border_color(c(on.outline_variant)).child(ui::icon(glyph::BLOCK, 18.0, c(on.on_surface_variant))))
            .into_element(),
    }
}

// What the state's stored colour is, as the grid names it.
fn stored(state: CardState, custom: &CustomColours) -> Option<[u32; 2]> {
    match custom.get(&state) {
        Some(pair) => *pair,
        None => full_colour(state, false).zip(full_colour(state, true)).map(|(l, d)| [l.to_rgb(), d.to_rgb()]),
    }
}

fn grid(state: CardState, page: Signal<PageSettings>, open: Signal<Option<CardState>>, now: &Look, on: Scheme) -> AnyElement {
    let current = stored(state, &now.custom);
    let mut cells = view().flex_row().flex_wrap().gap(px(2.0)).pb_2();
    let mut pairs = swatches();
    // A picked colour stays on offer.
    if let Some(pair) = current.filter(|pair| !pairs.contains(pair)) {
        pairs.push(pair);
    }
    for pair in std::iter::once(None).chain(pairs.into_iter().map(Some)) {
        let colour = pair.map(|p| Color::from_rgb(p[on.dark as usize]));
        let name = pair.map_or("None".to_owned(), |p| format!("#{:06x}", p[on.dark as usize]));
        let choose = move || {
            set_custom(page, state, pair);
            open.set(None);
        };
        cells = cells.child(view().ripples().on_press(choose).child(swatch(colour, pair == current, name, on)));
    }
    let start = current.map_or(0x808080, |pair| pair[on.dark as usize]);
    view().child(cells).child(picker(state, start, page, open, on)).into_element()
}

fn row(state: CardState, page: Signal<PageSettings>, open: Signal<Option<CardState>>, now: &Look, on: Scheme) -> AnyElement {
    let colour = state_colour(state, on.dark, WordPalette::Custom, &now.custom);
    let is_open = open.get() == Some(state);
    view()
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(px(12.0))
                .py_1()
                .label(state.label())
                .ripples()
                .on_press(move || open.set(if open.get_untracked() == Some(state) { None } else { Some(state) }))
                .child(view().flex_1().child(text(state.label()).text_size(px(15.0)).text_color(c(on.on_surface))))
                .child(text("word").text_size(px(16.0)).text_color(ink(colour, on)))
                .child(swatch(colour, is_open, format!("Colour of {}", state.label()), on)),
        )
        .child(is_open.then(|| grid(state, page, open, now, on)))
        .into_element()
}

// A colour for each state, on the page's own background.
pub fn custom(page: Signal<PageSettings>) -> AnyElement {
    let look = look(page);
    // A debug hook for the headless runs: `RUBIYOMI_COLOUR_GRID=known` opens that state's grid.
    let open = signal(crate::hooks::var("RUBIYOMI_COLOUR_GRID").map(|w| CardState::from_wire(&w)));
    view()
        .pb_4()
        .child(ui::hint(
            "Every colour has a shade for light pages and one for dark pages, so it reads on both. None leaves the word in plain ink. A picked colour gets its two shades the same way.",
        ))
        .child(dynamic(move || {
            let now = look.get();
            let on = page_scheme(now.theme);
            let mut rows =
                view().mx_4().my_2().px_3().py_1().rounded(px(12.0)).border_1().border_color(c(on.outline_variant)).bg(c(on.surface));
            for state in CardState::ALL {
                rows = rows.child(row(state, page, open, &now, on));
            }
            rows.into_element()
        }))
        .child(ui::button_row("Start again from Full", move || page.update(|p| p.custom_colours.clear())))
        .into_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_connected_service_comes_first() {
        let order = |source| services_for(source).iter().map(|(_, name, _)| *name).collect::<Vec<_>>();
        assert_eq!(order(ReviewSource::None), ["jpdb.io", "Anki", "WaniKani"]);
        assert_eq!(order(ReviewSource::WaniKani), ["WaniKani", "jpdb.io", "Anki"]);
    }

    #[test]
    fn every_built_in_colour_can_be_chosen() {
        let plain = CustomColours::new();
        let all = swatches();
        for palette in [WordPalette::Full, WordPalette::Minimal] {
            for state in CardState::ALL {
                let shade = |dark| state_colour(state, dark, palette, &plain).map(Color::to_rgb);
                if let (Some(light), Some(dark)) = (shade(false), shade(true)) {
                    assert!(all.contains(&[light, dark]), "{palette:?} {state:?}");
                }
            }
        }
    }

    #[test]
    fn a_picked_colour_reads_on_every_page_theme() {
        let picks = [0x000000, 0xffffff, 0x808080, 0xffff00, 0x00ffff, 0x0000ff, 0xff0000, 0x123456, 0xfedcba];
        for pick in picks {
            let pair = shades(pick);
            for theme in PageTheme::ALL.into_iter().filter(|t| *t != PageTheme::System) {
                let s = page_scheme(theme);
                let contrast = Color::from_rgb(pair[s.dark as usize]).contrast(s.surface);
                assert!(contrast >= READABLE, "{pick:06x} on {theme:?}: {contrast:.2}");
            }
        }
    }

    #[test]
    fn a_readable_colour_is_left_alone() {
        assert_eq!(shades(SWATCHES[0][0])[0], SWATCHES[0][0]);
        assert_eq!(shades(SWATCHES[0][1])[1], SWATCHES[0][1]);
    }

    #[test]
    fn a_picked_colour_comes_back_as_it_went_in() {
        assert_eq!(to_rgb(Hsla::from(rgb(0x3b82f6))), 0x3b82f6);
    }

    #[test]
    fn every_swatch_reads_on_every_page_theme() {
        for theme in PageTheme::ALL.into_iter().filter(|t| *t != PageTheme::System) {
            let s = page_scheme(theme);
            for pair in SWATCHES {
                let colour = Color::from_rgb(pair[s.dark as usize]);
                let contrast = colour.contrast(s.surface);
                assert!(contrast >= 3.0, "{colour:?} on {theme:?}: {contrast:.2}");
            }
        }
    }
}
