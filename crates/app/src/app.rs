// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::path::PathBuf;
use std::sync::Arc;

use egui_material_icons::icons::{ICON_DICTIONARY, ICON_INSIGHTS, ICON_LOCAL_LIBRARY, ICON_SETTINGS};
use rubiyomi_core::library::{ImportError, Imported, Library};
use rubiyomi_core::platform::{Platform, PlatformEvent};
use rubiyomi_core::theme::{self, Scheme};

use crate::screens::library::LibraryScreen;
use crate::screens::reader::ReaderScreen;
use crate::screens::{self, Ctx};
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
}

impl App {
    pub fn new(ctx: &egui::Context, platform: Arc<dyn Platform>) -> Self {
        fonts::install(ctx);
        let waker = ctx.clone();
        platform.set_waker(Box::new(move || waker.request_repaint()));
        let seed = platform.accent_color().unwrap_or(theme::DEFAULT_SEED);
        let schemes = [false, true].map(|dark| theme::scheme_from_seed(seed, dark));
        for s in &schemes {
            m3::apply(ctx, s);
        }
        let library = Library::open(platform.data_dir());
        App {
            platform,
            library,
            schemes,
            section: 0,
            shelf: LibraryScreen::default(),
            reader: None,
            snackbar: m3::Snackbar::default(),
            imports: Vec::new(),
        }
    }

    fn follow_system(&mut self, ctx: &egui::Context) {
        let ppp = ctx.pixels_per_point();
        let [l, t, r, b] = self.platform.insets().map(|px| (px / ppp).round().clamp(0.0, 127.0) as i8);
        m3::set_insets(ctx, egui::Margin { left: l, top: t, right: r, bottom: b });
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
        let ctx = ui.ctx().clone();
        self.follow_system(&ctx);
        self.handle_events(&ctx);
        let s = self.schemes[(ctx.theme() == egui::Theme::Dark) as usize];
        let mut cx = Ctx { scheme: &s, platform: &*self.platform, library: &self.library, snackbar: &mut self.snackbar };
        if let Some(reader) = &mut self.reader {
            if reader.ui(ui, &mut cx) {
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
            1 => screens::words::ui(ui, &s),
            2 => screens::stats::ui(ui, &s),
            _ => screens::settings::ui(ui, &s),
        }
        self.snackbar.ui(&ctx, &s, 80.0);
        if ctx.input(|i| i.key_pressed(egui::Key::BrowserBack)) {
            if self.section != 0 {
                self.section = 0;
            } else if !self.platform.to_background() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}
