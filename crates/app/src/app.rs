// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use nsh_core::prelude::*;
#[cfg(feature = "dev-hooks")]
use rubiyomi_core::cards::CardState;
use rubiyomi_core::dict::{DictState, DictStore};
use rubiyomi_core::finder::Finder;
use rubiyomi_core::grade::Grader;
use rubiyomi_core::history::History;
use rubiyomi_core::jpdb::JpdbClient;
use rubiyomi_core::library::{BookId, Library};
use rubiyomi_core::platform::{Platform, PlatformEvent};
#[cfg(feature = "dev-hooks")]
use rubiyomi_core::settings::WordPalette;
use rubiyomi_core::settings::{PageSettings, Settings};
use rubiyomi_core::stats::Stats;
use rubiyomi_core::theme::DEFAULT_SEED;

use crate::platform::NshPlatform;
use crate::reviews::{self, Reviews};
use crate::ui::{self, Ripples, c, glyph, scheme};

#[derive(Clone, Debug, PartialEq)]
pub enum Route {
    Library,
    Reader(BookId),
    Words,
    Stats,
    Settings,
    Setup,
}

// What every screen reads, provided once by `App`.
#[derive(Clone)]
pub struct Shared {
    pub platform: Arc<NshPlatform>,
    pub library: Library,
    pub dicts: DictStore,
    pub finder: Signal<Result<Arc<Finder>, String>>,
    pub settings: Signal<Settings>,
    pub page: Signal<PageSettings>,
    pub page_dragging: Signal<bool>,
    pub history: Rc<RefCell<History>>,
    pub stats: Rc<RefCell<Stats>>,
    pub grader: Signal<Option<Arc<dyn Grader>>>,
    pub parser: Signal<Option<Arc<JpdbClient>>>,
    pub reviews: Reviews,
}

pub fn shared() -> Shared {
    use_context::<Shared>().expect("App provides Shared")
}

pub fn route() -> Signal<Route> {
    use_context::<Signal<Route>>().expect("App provides the route")
}

impl Shared {
    fn open(platform: Arc<NshPlatform>) -> Self {
        let dir = platform.data_dir();
        let settings = Settings::load(&*platform);
        let (grader, parser) = (reviews::grader_for(&*platform, &settings), reviews::parser_for(&settings));
        let dicts = DictStore::new(&dir);
        let finder = signal_with(Err("Preparing the dictionary…".to_owned()), SignalEquals::Never);
        spawn_finder(platform.clone(), dicts.clone(), finder);
        Shared {
            library: Library::open(&dir),
            dicts,
            finder,
            page: signal(PageSettings::load(&dir)),
            page_dragging: signal(false),
            history: Rc::new(RefCell::new(History::load(&dir))),
            stats: Rc::new(RefCell::new(Stats::load(&dir))),
            grader: signal_with(grader, SignalEquals::Never),
            parser: signal_with(parser, SignalEquals::Never),
            settings: signal(settings),
            reviews: Reviews::new(),
            platform,
        }
    }
}

// Unpacking and indexing block, so they run on a blocking thread; meanwhile the unpacking
// step shows where the finder's absence is explained.
fn spawn_finder(platform: Arc<NshPlatform>, dicts: DictStore, finder: Signal<Result<Arc<Finder>, String>>) {
    let loading = Rc::new(Cell::new(true));
    let watch = {
        let (dicts, loading) = (dicts.clone(), loading.clone());
        set_interval(Duration::from_millis(300), move || {
            if loading.get()
                && let DictState::Unpacking(step) = dicts.state()
            {
                // Each set runs what watches the finder, the words of the chapter among them.
                let note = format!("{step}…");
                if finder.with_untracked(|f| f.as_ref().err() != Some(&note)) {
                    finder.set(Err(note));
                }
            }
        })
    };
    spawn(async move {
        let opened = blocking(move || {
            dicts.prepare(&*platform);
            Finder::open(&dicts).map(Arc::new)
        })
        .await;
        loading.set(false);
        watch.cancel();
        let opened = opened.unwrap_or_else(|e| Err(e.to_string()));
        if let Err(e) = &opened {
            log::warn!("word finder: {e}");
        }
        finder.set(opened);
    });
}

// Page settings are saved when they differ from what was loaded or last saved, and not during a drag.
fn persist_page(shared: &Shared) {
    let (page, dragging, platform) = (shared.page, shared.page_dragging, shared.platform.clone());
    let saved = RefCell::new(page.get_untracked());
    effect(move || {
        let held = dragging.get();
        if !page.with(|current| *saved.borrow() != *current) || held {
            return;
        }
        let current = page.get_untracked();
        *saved.borrow_mut() = current.clone();
        if let Err(e) = current.save(&platform.data_dir()) {
            log::warn!("saving page settings: {e}");
        }
    });
}

fn handle_events(shared: &Shared) {
    for event in shared.platform.poll_events() {
        match event {
            PlatformEvent::Imported { path, name, temporary } => crate::library::import(shared.library.clone(), path, name, temporary),
            PlatformEvent::ImportFailed(why) => ui::snackbar(format!("Couldn't add it: {why}")),
            PlatformEvent::Permission { granted } => {
                ui::snackbar(if granted { "AnkiDroid access allowed" } else { "AnkiDroid access was refused" });
                shared.reviews.permission.update(|n| *n += 1);
                if granted {
                    shared.reviews.restart();
                }
            }
            PlatformEvent::Back => {}
        }
    }
}

fn tab(glyph: &'static str, label: &'static str, at: Route) -> impl IntoElement {
    let (route, shared) = (route(), shared());
    let target = at.clone();
    let press = move || {
        let target = target.clone();
        crate::settings::leave(&shared, move || route.set(target));
    };
    let on = computed(move || route.get() == at);
    view()
        .flex_1()
        .h(px(80.0))
        .pt_3()
        .items_center()
        .label(label)
        .ripples()
        .on_press(press)
        .child(
            view()
                .w(px(64.0))
                .h(px(32.0))
                .rounded(px(16.0))
                .items_center()
                .justify_center()
                .bg(move || if on.get() { c(scheme().secondary_container) } else { rgba(0) })
                .child(ui::icon(
                    glyph,
                    24.0,
                    move || {
                        if on.get() { c(scheme().on_secondary_container) } else { c(scheme().on_surface_variant) }
                    },
                )),
        )
        .child(
            text(label)
                .text_size(px(12.0))
                .pt_1()
                .text_color(move || if on.get() { c(scheme().on_surface) } else { c(scheme().on_surface_variant) }),
        )
}

fn tabs(at: Route) -> AnyElement {
    let body = match at {
        Route::Words => crate::words::screen().into_element(),
        Route::Stats => crate::stats::screen().into_element(),
        Route::Settings => crate::settings::screen().into_element(),
        _ => crate::library::screen().into_element(),
    };
    view()
        .flex_1()
        .child(view().flex_1().child(if matches!(at, Route::Library) { body } else { ui::content(body) }))
        .child(
            view().bg(move || c(scheme().surface_container)).child(
                view()
                    .flex_row()
                    .w_full()
                    .max_w(px(ui::CONTENT_W))
                    .self_center()
                    .child(tab(glyph::LOCAL_LIBRARY, "Library", Route::Library))
                    .child(tab(glyph::DICTIONARY, "Words", Route::Words))
                    .child(tab(glyph::INSIGHTS, "Stats", Route::Stats))
                    .child(tab(glyph::SETTINGS, "Settings", Route::Settings)),
            ),
        )
        .into_element()
}

#[component]
pub fn App() -> impl IntoElement {
    let platform = Arc::new(NshPlatform::new());
    ui::init(platform.accent_color().unwrap_or(DEFAULT_SEED));
    ui::load_icons();
    let shared = Shared::open(platform);
    crate::library::reextract_stale(shared.library.clone());
    if let Some(path) = crate::hooks::var("RUBIYOMI_IMPORT") {
        let path = std::path::PathBuf::from(path);
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        crate::library::import(shared.library.clone(), path, name, false);
    }
    let wanted = crate::hooks::var("RUBIYOMI_SCREEN");
    let first = if let Some(route) = wanted.as_deref().and_then(|w| debug_route(w, &shared)) {
        route
    } else if !shared.settings.get_untracked().setup_done {
        Route::Setup
    } else if shared.page.get_untracked().open_last_on_launch {
        shared.library.last_read().map_or(Route::Library, |(meta, _)| Route::Reader(meta.id))
    } else {
        Route::Library
    };
    persist_page(&shared);
    reviews::watch(&shared);
    #[cfg(feature = "dev-hooks")]
    debug_hooks(&shared);
    provide_context(shared.clone());
    let route = signal(first);
    provide_context(route);
    let page = shared.page;
    set_interval(Duration::from_millis(250), move || handle_events(&shared));

    view()
        .flex_1()
        .when(cfg!(not(target_os = "android")), |v| v.font_family(ui::DESKTOP_FAMILY))
        // In the reader, what shows around a camera cutout is the page.
        .bg(move || {
            let reading = route.with(|r| matches!(r, Route::Reader(_)));
            c(if reading { ui::page_scheme(page.with(|p| p.theme)).surface } else { scheme().surface })
        })
        .child(view().flex_1().child(dynamic(move || match route.get() {
            Route::Setup => view().flex_1().p_safe_area().child(ui::content(crate::setup::screen())).into_element(),
            Route::Reader(id) => reading(crate::reader::screen(id)),
            at => view().flex_1().p_safe_area().child(tabs(at)).into_element(),
        })))
        .child(ui::snackbar_host())
}

// What the camera takes on one edge. Read again as the window turns.
fn cutout(edge: usize) -> Points {
    let _turned = window();
    #[cfg(target_os = "android")]
    return px(crate::android::cutout()[edge]);
    #[cfg(not(target_os = "android"))]
    {
        let _ = edge;
        px(0.0)
    }
}

// The reader keeps clear of the cutout alone, so bars hiding or showing never resize its pages.
fn reading(screen: impl IntoElement) -> AnyElement {
    view().flex_1().pl(move || cutout(0)).pt(move || cutout(1)).pr(move || cutout(2)).pb(move || cutout(3)).child(screen).into_element()
}

// A debug hook for the headless runs: `RUBIYOMI_SCREEN=library|settings|setup|words|stats|reader` picks the first screen.
#[cfg(feature = "dev-hooks")]
fn debug_route(name: &str, shared: &Shared) -> Option<Route> {
    Some(match name {
        "settings" => Route::Settings,
        "setup" => Route::Setup,
        "library" => Route::Library,
        "words" => Route::Words,
        "stats" => Route::Stats,
        "reader" => Route::Reader(shared.library.books().first()?.id.clone()),
        _ => return None,
    })
}

#[cfg(not(feature = "dev-hooks"))]
fn debug_route(_: &str, _: &Shared) -> Option<Route> {
    None
}

// More debug hooks: `RUBIYOMI_SET_FONT=24` sets the text size at launch (a change that must survive a restart);
// `RUBIYOMI_SET_PALETTE=full|minimal|custom` changes the palette three seconds in, with the screen up;
// `RUBIYOMI_SET_CUSTOM=known` gives that state the first colour of the custom grid at once.
#[cfg(feature = "dev-hooks")]
fn debug_hooks(shared: &Shared) {
    if let Some(size) = crate::hooks::var("RUBIYOMI_SET_FONT").and_then(|v| v.parse::<f32>().ok()) {
        shared.page.update(|p| p.font_size = size);
    }
    if let Some(name) = crate::hooks::var("RUBIYOMI_SET_PALETTE") {
        let page = shared.page;
        let palette = WordPalette::ALL.into_iter().find(|p| p.label().eq_ignore_ascii_case(&name)).unwrap_or_default();
        set_timeout(Duration::from_secs(3), move || page.update(|p| p.palette = palette));
    }
    if let Some(wire) = crate::hooks::var("RUBIYOMI_SET_CUSTOM") {
        crate::colours_page::set_custom(shared.page, CardState::from_wire(&wire), Some(crate::colours_page::SWATCHES[0]));
    }
    if crate::hooks::var("RUBIYOMI_SAMPLE_STATS").is_some() {
        sample_history(shared);
    }
}

// `RUBIYOMI_SAMPLE_STATS=1` writes made-up reading days and looked-up words into the data
// directory, unless there are some already; only the headless runs set it.
#[cfg(feature = "dev-hooks")]
fn sample_history(shared: &Shared) {
    use rubiyomi_core::finder::Word;
    use rubiyomi_core::history::Place;
    use rubiyomi_core::stats::Day;

    let today = rubiyomi_core::stats::today();
    if shared.stats.borrow().is_empty() {
        let days = std::iter::successors(Some(today), |d| d.pred_opt());
        for (back, date) in (0..70u64).zip(days) {
            if back % 5 == 3 {
                continue;
            }
            let chars = 800 + (back * 977) % 5200;
            let seconds = chars * 60 / (380 + (back * 31) % 160);
            let day = Day {
                seconds,
                chars,
                timed_chars: chars,
                lookups: (4 + back * 7 % 23) as u32,
                chapters: (back % 4 == 0) as u32 + (back % 9 == 0) as u32,
                words: chars / 2,
                unknown: chars / 2 * (30 - back.min(60) / 4) / 100,
            };
            shared.stats.borrow_mut().add(date, day);
        }
    }
    if shared.history.borrow().items().is_empty() {
        let book = shared.library.books().into_iter().next();
        let id = book.as_ref().map_or_else(|| rubiyomi_core::library::BookId(String::new()), |b| b.id.clone());
        let title = book.map(|b| b.title).unwrap_or_default();
        let sample = [
            ("猫", "ねこ", "cat", "吾輩は猫である。"),
            ("名前", "なまえ", "name; given name", "名前はまだ無い。"),
            ("事", "こと", "thing; matter", ""),
        ];
        for (n, (spelling, reading, meaning, sentence)) in sample.into_iter().enumerate() {
            let word = Word {
                spelling: spelling.into(),
                reading: reading.into(),
                meanings: meaning.split("; ").map(str::to_owned).collect(),
                len: spelling.chars().count(),
                ..Word::default()
            };
            for _ in 0..=n {
                let place = Place { book: &id, book_title: &title, chapter: 0, offset: 40 * n as u32, sentence };
                shared.history.borrow_mut().record(&word, place);
            }
        }
    }
}
