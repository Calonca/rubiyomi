// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use nsh_base::{SliderValue, slider, switch};
use nsh_core::Ripple;
use nsh_core::prelude::*;
use rubiyomi_core::settings::PageTheme;
use rubiyomi_core::theme::{Color, DEFAULT_SEED, Scheme, scheme_from_seed};

thread_local! {
    #[cfg_attr(target_os = "android", allow(clippy::missing_const_for_thread_local))]
    static SCHEMES: RefCell<Option<[Scheme; 2]>> = const { RefCell::new(None) };
    #[cfg_attr(target_os = "android", allow(clippy::missing_const_for_thread_local))]
    static PAGE_SCHEMES: RefCell<Vec<(PageTheme, Scheme)>> = const { RefCell::new(Vec::new()) };
}

pub fn init(seed: u32) {
    SCHEMES.with(|s| *s.borrow_mut() = Some([false, true].map(|dark| scheme_from_seed(seed, dark))));
}

// Reading it subscribes to the system's appearance.
pub fn scheme() -> Scheme {
    app_scheme(dark())
}

// The app's scheme for light or dark, whichever the system is on now.
pub fn app_scheme(dark: bool) -> Scheme {
    SCHEMES.with(|s| s.borrow().as_ref().map(|both| both[dark as usize])).unwrap_or_else(|| scheme_from_seed(DEFAULT_SEED, dark))
}

static ICON_FONT: &[u8] = include_bytes!("../assets/MaterialSymbolsRounded-subset.ttf");

// The code points of the font's icons; tool/icon-font.py lists all it keeps.
pub mod glyph {
    pub const ADD: &str = "\u{e145}";
    pub const ALL_INCLUSIVE: &str = "\u{eb3d}";
    pub const ARROW_BACK: &str = "\u{e5c4}";
    pub const AUTO_STORIES: &str = "\u{e666}";
    pub const BLOCK: &str = "\u{f08c}";
    pub const CLOSE: &str = "\u{e5cd}";
    pub const CONTENT_PASTE: &str = "\u{e14f}";
    pub const DESCRIPTION: &str = "\u{e873}";
    pub const DICTIONARY: &str = "\u{f539}";
    pub const GAVEL: &str = "\u{e90e}";
    pub const INFO: &str = "\u{e88e}";
    pub const INSIGHTS: &str = "\u{f092}";
    pub const LIST: &str = "\u{e896}";
    pub const LOCAL_LIBRARY: &str = "\u{e54b}";
    pub const MORE_HORIZ: &str = "\u{e5d3}";
    pub const OPEN_IN_NEW: &str = "\u{e89e}";
    pub const PALETTE: &str = "\u{e40a}";
    pub const PAUSE: &str = "\u{e034}";
    pub const PRIVACY_TIP: &str = "\u{f0dc}";
    pub const SCHOOL: &str = "\u{e80c}";
    pub const SEGMENT: &str = "\u{e94b}";
    pub const SETTINGS: &str = "\u{e8b8}";
    pub const TEXT_FIELDS: &str = "\u{e262}";
    #[cfg(feature = "update-check")]
    pub const UPDATE: &str = "\u{e923}";
}

// The family the icon font was registered under, once it has been.
#[derive(Clone, Default, PartialEq)]
struct IconFont(Option<String>);

pub fn load_icons() {
    let font = global::<IconFont>();
    spawn(async move {
        match register_font(ICON_FONT).await {
            Ok(family) => font.set(IconFont(Some(family))),
            Err(e) => log::warn!("icon font: {e}"),
        }
    });
}

// Empty until the font is in: before that the glyph would show as a box.
pub fn icon(glyph: &'static str, size: f32, colour: impl IntoReactive<AdaptiveColor>) -> Text {
    let font = global::<IconFont>();
    text(move || if font.with(|f| f.0.is_some()) { glyph } else { "" })
        .font_family(move || SharedString::from(font.with(|f| f.0.clone().unwrap_or_default())))
        .text_size(px(size))
        .line_height(px(size))
        .w(px(size))
        .h(px(size))
        .text_center()
        .number_of_lines(1)
        .allow_font_scaling(false)
        .select_none()
        .text_color(colour)
}

struct Palette {
    dark: bool,
    background: u32,
    surface: u32,
    text: u32,
    dim: u32,
    accent: u32,
    border: u32,
}

fn palette(theme: PageTheme) -> Option<Palette> {
    let p = |dark, background, surface, text, dim, accent, border| Palette { dark, background, surface, text, dim, accent, border };
    Some(match theme {
        PageTheme::System => return None,
        PageTheme::Light => p(false, 0xfdfdfd, 0xf2f2f3, 0x1b1b1d, 0x65676b, 0x2f6fed, 0xdfe0e3),
        PageTheme::Sepia => p(false, 0xf6efe0, 0xeee4d1, 0x3b332a, 0x7d6f5d, 0x9a6b3f, 0xddd0b8),
        PageTheme::Grey => p(true, 0x3b3d40, 0x45484c, 0xe3e4e6, 0xa9acb1, 0x8ab4f8, 0x54585d),
        PageTheme::Dark => p(true, 0x16181c, 0x1f2227, 0xdfe2e6, 0x9aa0a8, 0x7aa2f7, 0x2b2f36),
        PageTheme::Black => p(true, 0x000000, 0x0d0d0f, 0xcfd2d6, 0x85898f, 0x6f9bf0, 0x1c1d20),
        PageTheme::Ocean => p(true, 0x0f2b33, 0x14363f, 0xd7e8e6, 0x8fb0ad, 0x54c1b5, 0x1d4650),
    })
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color { r: m(a.r, b.r), g: m(a.g, b.g), b: m(a.b, b.b) }
}

// The colours of the page and its bars. Reading it subscribes to the system's appearance.
// Generating a scheme is slow and this runs for every character of every line drawn.
pub fn page_scheme(theme: PageTheme) -> Scheme {
    let Some(p) = palette(theme) else { return scheme() };
    PAGE_SCHEMES.with(|schemes| {
        let mut schemes = schemes.borrow_mut();
        match schemes.iter().find(|(made_for, _)| *made_for == theme) {
            Some((_, s)) => *s,
            None => {
                let s = make_page_scheme(p);
                schemes.push((theme, s));
                s
            }
        }
    })
}

fn make_page_scheme(p: Palette) -> Scheme {
    let mut s = scheme_from_seed(p.accent, p.dark);
    let (background, surface, text) = (Color::from_rgb(p.background), Color::from_rgb(p.surface), Color::from_rgb(p.text));
    s.primary = Color::from_rgb(p.accent);
    s.surface = background;
    s.surface_container_low = mix(background, surface, 0.5);
    s.surface_container = surface;
    s.surface_container_high = mix(surface, text, 0.05);
    s.surface_container_highest = mix(surface, text, 0.1);
    s.on_surface = text;
    s.on_surface_variant = Color::from_rgb(p.dim);
    s.outline_variant = Color::from_rgb(p.border);
    s
}

// Forms, lists and text rows stop at this width and sit in the middle of a wider window.
pub const CONTENT_W: f32 = 640.0;

// The window's width in points; reading it follows a resize.
pub fn window_width() -> f32 {
    window().width.0
}

// A column of at most `CONTENT_W`, centred, as tall as its parent.
pub fn content(body: impl IntoElement) -> AnyElement {
    view().flex_1().w_full().max_w(px(CONTENT_W)).self_center().child(body).into_element()
}

// Inherited by all text: Latin stays in the system's font, Japanese gets a Japanese face.
pub const DESKTOP_FAMILY: &str = "system-ui, Noto Sans CJK JP";

// A view laid over its parent, edge to edge.
pub fn fill_parent() -> View {
    view().absolute().top(px(0.0)).bottom(px(0.0)).left(px(0.0)).right(px(0.0))
}

// What dims the screen behind a dialog or a sheet.
pub fn scrim() -> View {
    fill_parent().bg(rgba(0x00000099))
}

pub fn c(color: Color) -> Rgba {
    rgb(color.to_rgb())
}

pub fn page_bar(title: impl Into<String>) -> impl IntoElement {
    view().px_4().py_4().child(text(title.into()).text_size(px(24.0)).text_color(move || c(scheme().on_surface)))
}

pub trait Ripples: Interactive {
    // Mid grey, as the theme's highlight is dark and pages can be too. Over the content, which can have its own background.
    fn ripples(self) -> Self {
        self.ripple(Ripple { color: Some(rgba(0x8080_8066).into()), foreground: true, ..Ripple::default() })
    }
}

impl<T: Interactive> Ripples for T {}

pub fn primary_button(label: impl Into<String>, on_press: impl Fn() + 'static) -> impl IntoElement {
    view()
        .px_6()
        .py_3()
        .rounded(px(24.0))
        .items_center()
        .bg(move || c(scheme().primary))
        .ripples()
        .on_press(on_press)
        .child(text(label.into()).text_color(move || c(scheme().on_primary)))
}

pub fn icon_row(
    glyph: &'static str,
    title: impl Into<String>,
    detail: impl Into<String>,
    on_press: impl Fn() + 'static,
) -> impl IntoElement {
    view()
        .flex_row()
        .items_center()
        .gap(px(16.0))
        .px_4()
        .py_3()
        .ripples()
        .on_press(on_press)
        .child(icon(glyph, 24.0, move || c(scheme().on_surface_variant)))
        .child(
            view()
                .flex_1()
                .child(text(title.into()).text_size(px(16.0)).text_color(move || c(scheme().on_surface)))
                .child(text(detail.into()).text_size(px(13.0)).text_color(move || c(scheme().on_surface_variant))),
        )
}

#[derive(Clone, Default, PartialEq)]
struct Snack {
    message: Option<String>,
    shown: u64,
}

// Shows a message for a few seconds in the host that `snackbar_host` draws.
pub fn snackbar(message: impl Into<String>) {
    let message = message.into();
    global::<Snack>().update(|s| {
        s.message = Some(message);
        s.shown += 1;
    });
}

pub fn snackbar_host() -> impl IntoElement {
    let state = global::<Snack>();
    // The host keeps the time: whoever called `snackbar` may be gone by then, its timer with it.
    effect(move || {
        let shown = state.with(|s| s.message.is_some().then_some(s.shown));
        if let Some(shown) = shown {
            set_timeout(Duration::from_secs(4), move || {
                if state.with_untracked(|s| s.shown == shown) {
                    state.update(|s| s.message = None);
                }
            });
        }
    });
    dynamic(move || match state.get().message {
        Some(message) => view()
            .absolute()
            .bottom(px(88.0))
            .left(px(16.0))
            .right(px(16.0))
            .pointer_events(PointerEvents::BoxNone)
            .child(
                view()
                    .w_full()
                    .max_w(px(CONTENT_W))
                    .self_center()
                    .px_4()
                    .py_3()
                    .rounded(px(8.0))
                    .bg(move || c(scheme().inverse_surface))
                    .ripples()
                    .on_press(move || state.update(|s| s.message = None))
                    .child(text(message).text_color(move || c(scheme().inverse_on_surface))),
            )
            .into_element(),
        None => view().into_element(),
    })
}

pub fn section_header(title: &'static str) -> impl IntoElement {
    view().px_4().pt_4().pb_1().child(text(title).text_size(px(13.0)).text_color(move || c(scheme().primary)))
}

pub fn back_bar(title: impl Into<String>, back: impl Fn() + 'static) -> impl IntoElement {
    view()
        .flex_row()
        .items_center()
        .px_1()
        .child(view().w(px(48.0)).h(px(48.0)).items_center().justify_center().label("Back").ripples().on_press(back).child(icon(
            glyph::ARROW_BACK,
            24.0,
            move || c(scheme().on_surface),
        )))
        .child(text(title.into()).text_size(px(22.0)).text_color(move || c(scheme().on_surface)))
}

// A row of a form: the label and, under it, what it does.
fn caption(title: &'static str, detail: &'static str) -> impl IntoElement {
    view()
        .flex_1()
        .child(text(title).text_size(px(16.0)).text_color(move || c(scheme().on_surface)))
        .child((!detail.is_empty()).then(|| text(detail).text_size(px(13.0)).text_color(move || c(scheme().on_surface_variant))))
}

pub fn switch_row(title: &'static str, detail: &'static str, on: impl Fn() -> bool + 'static, set: impl Fn(bool) + 'static) -> AnyElement {
    let (on, set) = (Rc::new(on), Rc::new(set));
    dynamic(move || {
        let (on, set) = (on.clone(), set.clone());
        let s = scheme();
        let (track_off, track_on, thumb_off, thumb_on) =
            (class_colour(s.surface_container_highest), class_colour(s.primary), class_colour(s.outline), class_colour(s.on_primary));
        view()
            .flex_row()
            .items_center()
            .gap(px(16.0))
            .px_4()
            .py_3()
            .child(caption(title, detail))
            .child(
                switch()
                    .accessibility_label(title)
                    .checked(move || on())
                    .track_class(format!("w-[52px] h-8 p-[4px] justify-center rounded-full {track_off} checked:{track_on}"))
                    .thumb_class(format!("size-6 rounded-full {thumb_off} checked:translate-x-5 checked:{thumb_on}"))
                    .on_change(move |v: &bool| set(*v)),
            )
            .into_element()
    })
    .into_element()
}

// How long a value offered to `Newest` waits for a newer one.
const NEWEST_WAIT: Duration = Duration::from_millis(30);

// Runs `apply` on the newest value only: a drag can report steps faster than they can be applied.
pub struct Newest<T> {
    value: Rc<Cell<Option<T>>>,
    apply: Rc<dyn Fn(T)>,
}

impl<T> Clone for Newest<T> {
    fn clone(&self) -> Self {
        Newest { value: self.value.clone(), apply: self.apply.clone() }
    }
}

impl<T: Copy + 'static> Newest<T> {
    pub fn new(apply: impl Fn(T) + 'static) -> Self {
        Newest { value: Rc::default(), apply: Rc::new(apply) }
    }

    pub fn offer(&self, value: T) {
        if self.value.replace(Some(value)).is_none() {
            let me = self.clone();
            set_timeout(NEWEST_WAIT, move || {
                if let Some(value) = me.value.take() {
                    (me.apply)(value);
                }
            });
        }
    }

    // The last value: applied now, and whatever waits is dropped.
    pub fn finish(&self, value: T) {
        self.value.set(None);
        (self.apply)(value);
    }
}

// The page follows the thumb; `dragging` stays set until the drag ends, so saving can wait for that.
pub fn slider_row(
    title: &'static str,
    range: std::ops::RangeInclusive<f32>,
    step: f32,
    value: f32,
    show: impl Fn(f32) -> String + 'static,
    set: impl Fn(f32) + 'static,
    dragging: Signal<bool>,
) -> AnyElement {
    let held = signal(value);
    let (show, live) = (Rc::new(show), Newest::new(set));
    // A slider taken away under the pointer never reports the drag's end.
    cleanup(move || dragging.set(false));
    dynamic(move || {
        let (show, live, last) = (show.clone(), live.clone(), live.clone());
        let s = scheme();
        let (rail, fill) = (class_colour(s.surface_container_highest), class_colour(s.primary));
        view()
            .px_4()
            .py_2()
            .child(
                view()
                    .flex_row()
                    .justify_between()
                    .child(text(title).text_size(px(16.0)).text_color(move || c(scheme().on_surface)))
                    .child(text(move || show(held.get())).text_size(px(14.0)).text_color(move || c(scheme().on_surface_variant))),
            )
            .child(
                slider()
                    .accessibility_label(title)
                    .min(*range.start() as f64)
                    .max(*range.end() as f64)
                    .step(step as f64)
                    .value(move || SliderValue::Single(held.get() as f64))
                    .track_class("h-8 justify-center px-[10px]")
                    .indicator_class(format!("h-1 rounded-full {rail}"))
                    .range_class(format!("rounded-full {fill}"))
                    .thumb_class(format!("size-5 rounded-full {fill}"))
                    .on_change(move |v: &SliderValue| {
                        held.set(v.start() as f32);
                        dragging.set(true);
                        live.offer(v.start() as f32);
                    })
                    .on_change_end(move |v: &SliderValue| {
                        batch(|| {
                            last.finish(v.start() as f32);
                            dragging.set(false);
                        })
                    }),
            )
            .into_element()
    })
    .into_element()
}

pub fn class_colour(color: Color) -> String {
    format!("bg-[#{:06x}]", color.to_rgb())
}

// Options side by side, the chosen one filled.
pub fn segmented<T: Copy + PartialEq + 'static>(
    title: Option<&'static str>,
    names: &'static [&'static str],
    values: &'static [T],
    get: impl Fn() -> T + 'static,
    set: impl Fn(T) + 'static,
) -> AnyElement {
    let (get, set) = (Rc::new(get), Rc::new(set));
    let mut options = view().flex_row().gap(px(4.0));
    for (name, value) in names.iter().zip(values.iter().copied()) {
        let (get, set, mark) = (get.clone(), set.clone(), get.clone());
        let chosen = move || mark() == value;
        let ink = chosen.clone();
        options = options.child(
            view()
                .flex_1()
                .items_center()
                .py_2()
                .rounded(px(20.0))
                .bg(move || if chosen() { c(scheme().secondary_container) } else { c(scheme().surface_container) })
                .ripples()
                .on_press(move || {
                    if get() != value {
                        set(value);
                    }
                })
                .child(
                    text(*name)
                        .text_size(px(14.0))
                        .text_color(move || if ink() { c(scheme().on_secondary_container) } else { c(scheme().on_surface_variant) }),
                ),
        );
    }
    view()
        .px_4()
        .py_2()
        .gap(px(6.0))
        .child(title.map(|t| text(t).text_size(px(16.0)).text_color(move || c(scheme().on_surface))))
        .child(options)
        .into_element()
}

// An error with the means to ask again: a press bumps `tries`, which the request follows.
pub fn retry(message: impl Into<String>, tries: Signal<u32>) -> AnyElement {
    view().child(error(message)).child(button_row("Try again", move || tries.update(|n| *n += 1))).into_element()
}

// The middle of a page that has nothing to list yet.
pub fn empty_state(glyph: &'static str, title: &'static str, detail: &'static str) -> AnyElement {
    view()
        .flex_1()
        .items_center()
        .justify_center()
        .gap(px(4.0))
        .px_6()
        .child(icon(glyph, 64.0, move || c(scheme().on_surface_variant)).mb(px(12.0)))
        .child(text(title).text_size(px(20.0)).text_color(move || c(scheme().on_surface)))
        .child(text(detail).text_center().text_color(move || c(scheme().on_surface_variant)))
        .into_element()
}

pub fn prose(body: impl Into<String>) -> impl IntoElement {
    view().px_4().py_2().child(text(body.into()).text_size(px(15.0)).text_color(move || c(scheme().on_surface)))
}

pub fn hint(body: impl Into<String>) -> impl IntoElement {
    view().px_4().py_1().child(text(body.into()).text_size(px(13.0)).text_color(move || c(scheme().on_surface_variant)))
}

pub fn error(body: impl Into<String>) -> impl IntoElement {
    view().px_4().py_1().child(text(body.into()).text_size(px(13.0)).text_color(move || c(scheme().error)))
}

// What a field or a choice under it is for.
pub fn label(title: &'static str) -> impl IntoElement {
    view().px_4().pt_3().pb_1().child(text(title).text_size(px(14.0)).text_color(move || c(scheme().on_surface)))
}

// A button that reads as one: filled, in the colours of `colours`.
pub fn tonal_button_in(label: impl Into<String>, colours: impl Fn() -> Scheme + Copy + 'static, on_press: impl Fn() + 'static) -> View {
    view()
        .px_4()
        .py_2()
        .rounded(px(20.0))
        .bg(move || c(colours().secondary_container))
        .ripples()
        .on_press(on_press)
        .child(text(label.into()).text_size(px(14.0)).text_color(move || c(colours().on_secondary_container)))
}

pub fn tonal_button(label: impl Into<String>, on_press: impl Fn() + 'static) -> View {
    tonal_button_in(label, scheme, on_press)
}

// A tonal button on a line of its own, at its own width.
pub fn button_row(label: &'static str, on_press: impl Fn() + 'static) -> AnyElement {
    view().flex_row().px_4().py_1().child(tonal_button(label, on_press)).into_element()
}

pub fn field_box(field: impl IntoElement) -> impl IntoElement {
    view().mx_4().my_1().px_3().py_2().rounded(px(8.0)).bg(move || c(scheme().surface_container)).child(field)
}

// What is chosen; a press lists `options`, as (value, label), in place. Inline rather than a
// dialog: deck lists run long, and a system dialog on the desktop doesn't scroll.
pub fn choice(shown: String, options: Vec<(String, String)>, set: impl Fn(String) + 'static) -> AnyElement {
    let open = signal(false);
    let set = Rc::new(set);
    view()
        .mx_4()
        .my_1()
        .rounded(px(8.0))
        .bg(move || c(scheme().surface_container))
        .child(
            view()
                .flex_row()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .px_3()
                .py_3()
                .ripples()
                .on_press(move || open.update(|o| *o = !*o))
                .child(view().flex_1().child(text(shown).text_size(px(16.0)).text_color(move || c(scheme().on_surface))))
                .child(text(move || if open.get() { "\u{25b4}" } else { "\u{25be}" }).text_color(move || c(scheme().on_surface_variant))),
        )
        .child(dynamic(move || {
            if !open.get() {
                return view().into_element();
            }
            let mut list = view().pb_1();
            for (value, label) in options.clone() {
                let set = set.clone();
                list = list.child(
                    view()
                        .px_3()
                        .py_2()
                        .ripples()
                        .on_press(move || {
                            open.set(false);
                            set(value.clone());
                        })
                        .child(text(label).text_size(px(15.0)).text_color(move || c(scheme().primary))),
                );
            }
            list.into_element()
        }))
        .into_element()
}
