// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use egui::RichText;
use egui_material_icons::icons::{ICON_AUTO_STORIES, ICON_DESCRIPTION, ICON_DICTIONARY, ICON_GAVEL, ICON_INFO, ICON_SCHOOL};
use rubiyomi_core::dict::DictVersion;
use rubiyomi_core::platform::Platform;
use rubiyomi_core::settings::{PageSettings, ReviewSource};
use rubiyomi_core::theme::Scheme;

use super::reviews_form::{ReviewsForm, hint};
use crate::reviews::{Reviews, Status};
use crate::ui::m3::{self, c, font, text};

const IPADIC_NOTICE: &str = include_str!("../../notices/ipadic.txt");

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Page {
    #[default]
    Main,
    About,
    Dictionary,
    WordFinder,
    Reviews,
    Reading,
}

// Where the reader was going when the dialog about unsaved changes came up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Leave {
    Back,
    Tab(usize),
}

#[derive(Default)]
pub struct SettingsScreen {
    page: Page,
    form: ReviewsForm,
    leaving: Option<Leave>,
    // The pass the dialog first showed in: the tap that opened it isn't one that closes it.
    leave_shown: Option<u64>,
    // A tab to go to once the dialog is answered, for the app to take.
    go_to: Option<usize>,
}

impl SettingsScreen {
    // Changes to the reviews that would be lost by leaving their page.
    pub fn unsaved(&self, reviews: &Reviews) -> bool {
        self.page == Page::Reviews && self.form.draft.reviews != ReviewSource::None && self.form.draft != reviews.settings
    }

    // Asks about unsaved changes first; says whether leaving has to wait for the answer.
    pub fn ask_leave(&mut self, reviews: &Reviews, to: Leave) -> bool {
        if !self.unsaved(reviews) {
            return false;
        }
        self.leaving = Some(to);
        true
    }

    pub fn take_tab(&mut self) -> Option<usize> {
        self.go_to.take()
    }

    pub fn back(&mut self, reviews: &Reviews) -> bool {
        if self.leaving.is_some() {
            self.leaving = None;
            return true;
        }
        if self.ask_leave(reviews, Leave::Back) {
            return true;
        }
        self.go_back()
    }

    fn go_back(&mut self) -> bool {
        self.page = match self.page {
            Page::Main => return false,
            Page::Reviews | Page::Reading => Page::Main,
            Page::About => Page::Main,
            Page::Dictionary | Page::WordFinder => Page::About,
        };
        true
    }

    // Says whether the page settings changed.
    #[allow(clippy::too_many_arguments)]
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        s: &Scheme,
        platform: &dyn Platform,
        reviews: &mut Reviews,
        dict: Option<DictVersion>,
        page: &mut PageSettings,
        colours_matter: bool,
    ) -> bool {
        let mut changed = false;
        let title = match self.page {
            Page::Main => "Settings",
            Page::About => "About",
            Page::Dictionary => "Dictionary",
            Page::WordFinder => "Word finder",
            Page::Reviews => "Reviews",
            Page::Reading => "Page",
        };
        if m3::top_app_bar(ui, s, title, self.page != Page::Main) {
            self.back(reviews);
        }
        if self.leaving.is_some() {
            self.leave_dialog(ui.ctx(), s, platform, reviews);
        }
        egui::CentralPanel::default().frame(super::surface(s)).show(ui, |ui| {
            m3::scroll().show(ui, |ui| match self.page {
                Page::Main => self.main(ui, s, reviews),
                Page::Reviews => self.reviews(ui, s, platform, reviews),
                Page::Reading => changed = super::page_form::ui(ui, s, page, colours_matter),
                Page::About => self.about(ui, s, dict.as_ref()),
                Page::Dictionary => dictionary(ui, s, dict.as_ref()),
                Page::WordFinder => prose(ui, |ui| {
                    words(
                        ui,
                        s,
                        "Rubiyomi finds the words in a sentence with lindera (MIT) and the mecab-ipadic dictionary, whose notice follows.",
                    );
                    ui.add_space(8.0);
                    ui.label(RichText::new(IPADIC_NOTICE).font(egui::FontId::monospace(11.0)).color(c(s.on_surface_variant)));
                }),
            });
        });
        changed
    }

    fn main(&mut self, ui: &mut egui::Ui, s: &Scheme, reviews: &Reviews) {
        m3::section_header(ui, s, "Reading");
        if m3::list_item(ui, s, ICON_AUTO_STORIES, "Page", "Theme, text size, furigana, vertical text", true).clicked() {
            self.page = Page::Reading;
        }
        m3::section_header(ui, s, "Words");
        if m3::list_item(ui, s, ICON_DICTIONARY, "Dictionary", "JMdict, English, on this device", true).clicked() {
            self.page = Page::Dictionary;
        }
        if m3::list_item(ui, s, ICON_SCHOOL, "Reviews", &reviews.summary(), true).clicked() {
            self.form = ReviewsForm::new(&reviews.settings);
            self.page = Page::Reviews;
        }
        m3::section_header(ui, s, "App");
        if m3::list_item(ui, s, ICON_INFO, "About", concat!("Rubiyomi ", env!("CARGO_PKG_VERSION"), " · dictionary and licences"), true)
            .clicked()
        {
            self.page = Page::About;
        }
    }

    fn leave_dialog(&mut self, ctx: &egui::Context, s: &Scheme, platform: &dyn Platform, reviews: &mut Reviews) {
        let mut answer = None;
        let pass = ctx.cumulative_pass_nr();
        let opened_now = *self.leave_shown.get_or_insert(pass) == pass;
        let modal = egui::Modal::new(egui::Id::new("unsaved-reviews")).show(ctx, |ui| {
            ui.set_width(320.0_f32.min(ctx.content_rect().width() - 48.0));
            ui.label(RichText::new("Save changes?").font(font(text::HEADLINE_SMALL)).color(c(s.on_surface)));
            ui.add_space(8.0);
            ui.label(
                RichText::new("Your reviews settings have changes that aren't saved yet.")
                    .font(font(text::BODY_MEDIUM))
                    .color(c(s.on_surface_variant)),
            );
            ui.add_space(16.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if m3::text_button(ui, s, "Save").clicked() {
                    answer = Some(true);
                }
                if m3::text_button(ui, s, "Discard").clicked() {
                    answer = Some(false);
                }
                if m3::text_button(ui, s, "Cancel").clicked() {
                    self.leaving = None;
                }
            });
        });
        if modal.should_close() && !opened_now {
            self.leaving = None;
        }
        if self.leaving.is_none() {
            self.leave_shown = None;
        }
        let (Some(save), Some(to)) = (answer, self.leaving.take()) else { return };
        self.leave_shown = None;
        if save {
            reviews.apply(ctx, platform, self.form.draft.clone());
        } else {
            self.form = ReviewsForm::new(&reviews.settings);
        }
        match to {
            Leave::Back => {
                self.go_back();
            }
            Leave::Tab(i) => self.go_to = Some(i),
        }
    }

    fn reviews(&mut self, ui: &mut egui::Ui, s: &Scheme, platform: &dyn Platform, reviews: &mut Reviews) {
        prose(ui, |ui| {
            self.form.ui(ui, s, platform);
            // Turning reviews off needs nothing more, so it happens at once.
            if self.form.draft.reviews == ReviewSource::None {
                if reviews.settings.reviews != ReviewSource::None {
                    let mut off = reviews.settings.clone();
                    off.reviews = ReviewSource::None;
                    self.form.draft.reviews = ReviewSource::None;
                    reviews.apply(ui.ctx(), platform, off);
                }
                return;
            }
            ui.add_space(16.0);
            if m3::filled_button(ui, s, "Save and sync").clicked() {
                reviews.apply(ui.ctx(), platform, self.form.draft.clone());
            }
            ui.add_space(8.0);
            status(ui, s, reviews, self.form.draft != reviews.settings);
        });
    }

    fn about(&mut self, ui: &mut egui::Ui, s: &Scheme, dict: Option<&DictVersion>) {
        m3::section_header(ui, s, concat!("Rubiyomi ", env!("CARGO_PKG_VERSION")));
        m3::list_item(ui, s, ICON_GAVEL, "Licence", "MIT · © 2026 Alessandro La Conca", false);
        let version = dict.map_or_else(|| "Not installed yet".to_owned(), |d| format!("JMdict {} · CC BY-SA 4.0", d.jmdict_date));
        if m3::list_item(ui, s, ICON_DICTIONARY, "Dictionary", &version, true).clicked() {
            self.page = Page::Dictionary;
        }
        if m3::list_item(ui, s, ICON_DESCRIPTION, "Word finder", "lindera and mecab-ipadic · notice", true).clicked() {
            self.page = Page::WordFinder;
        }
    }
}

fn dictionary(ui: &mut egui::Ui, s: &Scheme, dict: Option<&DictVersion>) {
    prose(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            words(ui, s, "This app uses the ");
            ui.hyperlink_to(link("JMdict/EDICT"), "https://www.edrdg.org/wiki/index.php/JMdict-EDICT_Dictionary_Project");
            words(ui, s, " dictionary files. These files are the property of the ");
            ui.hyperlink_to(link("Electronic Dictionary Research and Development Group"), "https://www.edrdg.org/");
            words(ui, s, ", and are used in conformance with the Group's ");
            ui.hyperlink_to(link("licence"), "https://www.edrdg.org/edrdg/licence.html");
            words(ui, s, ".");
        });
        ui.add_space(12.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            words(ui, s, "The dictionary data, and the index Rubiyomi builds from it, are licensed under ");
            ui.hyperlink_to(link("Creative Commons Attribution-ShareAlike 4.0"), "https://creativecommons.org/licenses/by-sa/4.0/");
            words(ui, s, ". Rubiyomi claims no copyright over them.");
        });
        ui.add_space(12.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            words(ui, s, "The English JMdict comes through ");
            ui.hyperlink_to(link("jmdict-simplified"), "https://github.com/scriptin/jmdict-simplified");
            words(ui, s, ". Each Rubiyomi release bundles its newest release.");
        });
        ui.add_space(12.0);
        let installed = dict.map_or_else(
            || "Not installed yet.".to_owned(),
            |d| format!("Installed: JMdict of {}, jmdict-simplified {}; mecab-ipadic {}.", d.jmdict_date, d.jmdict, d.ipadic),
        );
        words(ui, s, &installed);
    });
}

fn prose(ui: &mut egui::Ui, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new().inner_margin(egui::Margin::symmetric(16, 8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        body(ui);
    });
}

fn words(ui: &mut egui::Ui, s: &Scheme, t: &str) {
    ui.label(RichText::new(t).font(font(text::BODY_LARGE)).color(c(s.on_surface)));
}

fn link(t: &str) -> RichText {
    RichText::new(t).font(font(text::BODY_LARGE))
}

pub fn status(ui: &mut egui::Ui, s: &Scheme, reviews: &Reviews, unsaved: bool) {
    match &reviews.status {
        _ if unsaved => hint(ui, s, "Not saved yet"),
        Status::Syncing => {
            ui.horizontal(|ui| {
                ui.spinner();
                let text = reviews.syncing_text();
                let mut first = text.chars();
                let text: String = first.next().map(|c| c.to_uppercase().chain(first).collect()).unwrap_or_default();
                hint(ui, s, &text);
            });
        }
        Status::Synced => hint(ui, s, "Up to date"),
        Status::Failed(e) => {
            ui.label(RichText::new(e).font(font(text::BODY_MEDIUM)).color(c(s.error)));
        }
        Status::Off => {}
    }
    if let Some(note) = reviews.sync_note.as_ref().filter(|_| !unsaved) {
        ui.label(RichText::new(note).font(font(text::BODY_SMALL)).color(c(s.error)));
    }
    if let Some(note) = &reviews.note {
        ui.label(RichText::new(note).font(font(text::BODY_SMALL)).color(c(s.error)));
    }
}
