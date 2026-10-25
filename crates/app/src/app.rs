// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::path::PathBuf;
use std::sync::Arc;

use egui_material_icons::icons::{ICON_DICTIONARY, ICON_INSIGHTS, ICON_LOCAL_LIBRARY, ICON_SETTINGS};
use rubiyomi_core::dict::{DictState, DictStore};
use rubiyomi_core::finder::Finder;
use rubiyomi_core::history::History;
use rubiyomi_core::library::{ImportError, Imported, Library};
use rubiyomi_core::platform::{Platform, PlatformEvent};
use rubiyomi_core::settings::PageSettings;
use rubiyomi_core::stats::Stats;
use rubiyomi_core::theme::{self, Scheme};

use crate::reviews::Reviews;
use crate::screens::Ctx;
use crate::screens::library::LibraryScreen;
use crate::screens::reader::ReaderScreen;
use crate::screens::settings::SettingsScreen;
use crate::screens::setup::Setup;
use crate::screens::stats::StatsScreen;
use crate::screens::words::WordsScreen;
use crate::task::{self, Task};
use crate::ui::{fonts, m3};

pub struct App {
    platform: Arc<dyn Platform>,
    library: Library,
    schemes: [Scheme; 2],
    section: usize,
    shelf: LibraryScreen,
    reader: Option<ReaderScreen>,
    snackbar: m3::Snackbar,
    imports: Vec<Task<(String, Result<Imported, ImportError>)>>,
    settings: SettingsScreen,
    setup: Option<Setup>,
    dicts: DictStore,
    loading: Option<Task<Result<Finder, String>>>,
    finder: Result<Arc<Finder>, String>,
    reviews: Reviews,
    words_tag: Option<String>,
    page: PageSettings,
    history: History,
    stats: Stats,
    words: WordsScreen,
    stats_screen: StatsScreen,
    awake: bool,
    // From launch until the reopened book's first page is drawn.
    reopening: Option<std::time::Instant>,
}

impl App {
    pub fn new(ctx: &egui::Context, platform: Arc<dyn Platform>) -> Self {
        let launched = std::time::Instant::now();
        fonts::install(ctx);
        let waker = ctx.clone();
        platform.set_waker(Box::new(move || waker.request_repaint()));
        let seed = platform.accent_color().unwrap_or(theme::DEFAULT_SEED);
        let schemes = [false, true].map(|dark| theme::scheme_from_seed(seed, dark));
        for s in &schemes {
            m3::apply(ctx, s);
        }
        let library = Library::open(platform.data_dir());
        let dicts = DictStore::new(&platform.data_dir());
        let loading = {
            let (platform, dicts) = (platform.clone(), dicts.clone());
            Task::spawn(ctx, move || {
                dicts.prepare(&*platform);
                Finder::open(&dicts)
            })
        };
        let reviews = Reviews::new(ctx, &*platform);
        let dir = platform.data_dir();
        let page = PageSettings::load(&dir);
        let reader =
            page.open_last_on_launch.then(|| library.last_read()).flatten().and_then(|(meta, _)| {
                ReaderScreen::open(&library, meta, None).map_err(|e| log::warn!("reopening the last book: {e}")).ok()
            });
        let words_tag = dicts.installed().map(|v| rubiyomi_core::finder::words_tag(&v));
        App {
            reopening: reader.is_some().then_some(launched),
            page,
            history: History::load(&dir),
            stats: Stats::load(&dir),
            words: WordsScreen::default(),
            stats_screen: StatsScreen::default(),
            awake: false,
            platform,
            library,
            schemes,
            section: 0,
            shelf: LibraryScreen::default(),
            reader,
            snackbar: m3::Snackbar::default(),
            imports: Vec::new(),
            settings: SettingsScreen::default(),
            setup: (!reviews.settings.setup_done).then(|| Setup::new(&reviews)),
            dicts,
            loading: Some(loading),
            finder: Err("Preparing the dictionary…".into()),
            words_tag,
            reviews,
        }
    }

    // eframe is built without its own link opener, so links go through the platform.
    fn open_links(&self, ctx: &egui::Context) {
        let mut urls = Vec::new();
        ctx.output_mut(|o| {
            o.commands.retain(|c| match c {
                egui::OutputCommand::OpenUrl(open) => {
                    urls.push(open.url.clone());
                    false
                }
                _ => true,
            })
        });
        for url in urls {
            if !self.platform.open_url(&url) {
                log::warn!("couldn't open {url}");
            }
        }
    }

    fn poll_finder(&mut self) {
        if let Some(result) = task::take(&mut self.loading) {
            if let Err(e) = &result {
                log::warn!("word finder: {e}");
            }
            self.finder = result.map(Arc::new);
            self.words_tag = self.dicts.installed().map(|v| rubiyomi_core::finder::words_tag(&v));
        } else if self.loading.is_some()
            && let DictState::Unpacking(step) = self.dicts.state()
        {
            self.finder = Err(format!("{step}…"));
        }
    }

    fn follow_system(&mut self, ctx: &egui::Context) {
        let ppp = ctx.pixels_per_point();
        let [l, t, r, b] = self.platform.insets().map(|px| (px / ppp).round().clamp(0.0, 127.0) as i8);
        let keyboard = self.platform.keyboard() / ppp;
        // The keyboard covers the navigation bar, so its inset goes while it is up.
        m3::set_insets(ctx, egui::Margin { left: l, top: t, right: r, bottom: if keyboard > 0.0 { 0 } else { b } });
        m3::set_keyboard(ctx, keyboard);
        if let Some(dark) = self.platform.dark_mode() {
            let want = if dark { egui::Theme::Dark } else { egui::Theme::Light };
            if ctx.theme() != want {
                ctx.set_theme(want);
            }
        }
    }

    fn import(&mut self, ctx: &egui::Context, path: PathBuf, name: String, temporary: bool) {
        let library = self.library.clone();
        self.imports.push(Task::spawn(ctx, move || {
            let result = library.import_file(&path);
            if temporary {
                let _ = std::fs::remove_file(&path);
            }
            (name, result)
        }));
        self.snackbar.show(ctx, "Adding…");
    }

    fn handle_events(&mut self, ctx: &egui::Context) {
        for event in self.platform.poll_events() {
            match event {
                PlatformEvent::Imported { path, name, temporary } => self.import(ctx, path, name, temporary),
                PlatformEvent::ImportFailed(why) => self.snackbar.show(ctx, format!("Couldn't add it: {why}")),
                PlatformEvent::Permission { granted: true } => {
                    self.snackbar.show(ctx, "AnkiDroid access allowed");
                    self.reviews.start(ctx, &*self.platform);
                }
                PlatformEvent::Permission { granted: false } => self.snackbar.show(ctx, "AnkiDroid access was refused"),
            }
        }
        let dropped: Vec<PathBuf> =
            ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect());
        for path in dropped {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            self.import(ctx, path, name, false);
        }
        for (name, result) in task::drain(&mut self.imports) {
            match result {
                Ok(imported) if imported.already => self.snackbar.show(ctx, format!("{} is already in your library", imported.meta.title)),
                Ok(imported) => self.snackbar.show(ctx, format!("Added {}", imported.meta.title)),
                Err(e) => self.snackbar.show(ctx, format!("{name}: {e}")),
            }
            self.shelf.refresh(ctx, &self.library);
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.follow_system(ui.ctx());
        // NativeActivity's window keeps its size under the keyboard, so the app makes room
        // itself; always through the same scope, or every widget's id would change with the
        // keyboard and the text field would lose its focus.
        let mut room = ui.max_rect();
        room.max.y = (room.max.y - m3::keyboard(ui.ctx())).max(room.min.y + 120.0);
        ui.scope_builder(egui::UiBuilder::new().max_rect(room), |ui| self.screen(ui));
        self.open_links(ui.ctx());
    }
}

impl App {
    fn screen(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.handle_events(&ctx);
        self.poll_finder();
        self.reviews.poll();
        let s = self.schemes[(ctx.theme() == egui::Theme::Dark) as usize];
        let awake = self.reader.is_some() && self.page.keep_awake;
        if awake != self.awake {
            self.awake = awake;
            self.platform.keep_screen_on(awake);
        }
        let mut cx = Ctx {
            scheme: &s,
            platform: &*self.platform,
            library: &self.library,
            snackbar: &mut self.snackbar,
            finder: self.finder.as_ref().map_err(String::as_str),
            grader: self.reviews.grader.as_ref(),
            generation: self.reviews.generation,
            words_tag: self.words_tag.as_deref(),
            page: &mut self.page,
            history: &mut self.history,
            stats: &mut self.stats,
        };
        if let Some(setup) = &mut self.setup {
            if setup.ui(ui, &s, &*self.platform, &mut self.reviews) {
                self.setup = None;
            }
            self.snackbar.ui(&ctx, &s, 0.0);
            return;
        }
        if let Some(reader) = &mut self.reader {
            let close = reader.ui(ui, &mut cx);
            if let Some(since) = self.reopening.take() {
                log::info!("reopened the last book in {} ms", since.elapsed().as_millis());
            }
            if close {
                self.reader = None;
                self.shelf.refresh(&ctx, &self.library);
            }
            self.snackbar.ui(&ctx, &s, 0.0);
            return;
        }
        let sections = [(ICON_LOCAL_LIBRARY, "Library"), (ICON_DICTIONARY, "Words"), (ICON_INSIGHTS, "Stats"), (ICON_SETTINGS, "Settings")];
        m3::navigation_bar(ui, &s, &sections, &mut self.section);
        match self.section {
            0 => self.reader = self.shelf.ui(ui, &mut cx),
            1 => {
                if let Some(jump) = self.words.ui(ui, &mut cx)
                    && let Some(meta) = self.library.meta(&jump.book)
                {
                    match ReaderScreen::open(&self.library, meta, Some((jump.chapter, jump.offset, jump.len))) {
                        Ok(reader) => self.reader = Some(reader),
                        Err(e) => self.snackbar.show(&ctx, e),
                    }
                }
            }
            2 => self.stats_screen.ui(ui, &s, &self.stats),
            _ => {
                let colours = self.reviews.grader.is_some();
                if self.settings.ui(ui, &s, &*self.platform, &mut self.reviews, self.dicts.installed(), &mut self.page, colours)
                    && let Err(e) = self.page.save(&self.platform.data_dir())
                {
                    log::warn!("saving the page settings: {e}");
                }
            }
        }
        self.snackbar.ui(&ctx, &s, 80.0);
        if ctx.input(|i| i.key_pressed(egui::Key::BrowserBack)) && !(self.section == 3 && self.settings.back()) {
            if self.section != 0 {
                self.section = 0;
            } else if !self.platform.to_background() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}
