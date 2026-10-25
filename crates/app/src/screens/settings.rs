// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use egui::RichText;
use egui_material_icons::icons::{
    ICON_AUTO_STORIES, ICON_DESCRIPTION, ICON_DICTIONARY, ICON_GAVEL, ICON_INFO, ICON_PALETTE, ICON_SCHOOL, ICON_TRANSLATE,
};
use rubiyomi_core::dict::DictVersion;
use rubiyomi_core::grade::anki::connect::{AnkiConnect, DEFAULT_URL};
use rubiyomi_core::http;
use rubiyomi_core::jpdb::{Deck, JpdbClient};
use rubiyomi_core::platform::Platform;
use rubiyomi_core::settings::{ReviewSource, Settings};
use rubiyomi_core::theme::Scheme;

use crate::reviews::{self, Reviews, Status};
use crate::task::{self, Task};
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
}

type AnkiLists = (Vec<String>, Vec<String>);
type Fields = (String, Result<Vec<String>, String>);

#[derive(Default)]
pub struct SettingsScreen {
    page: Page,
    draft: Settings,
    decks: Option<Result<Vec<Deck>, String>>,
    loading_decks: Option<Task<Result<Vec<Deck>, String>>>,
    anki: Option<Result<AnkiLists, String>>,
    loading_anki: Option<Task<Result<AnkiLists, String>>>,
    fields: Option<Fields>,
    loading_fields: Option<Task<Fields>>,
}

impl SettingsScreen {
    pub fn back(&mut self) -> bool {
        self.page = match self.page {
            Page::Main => return false,
            Page::Reviews => Page::Main,
            Page::About => Page::Main,
            Page::Dictionary | Page::WordFinder => Page::About,
        };
        true
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, s: &Scheme, platform: &dyn Platform, reviews: &mut Reviews, dict: Option<DictVersion>) {
        self.poll();
        let title = match self.page {
            Page::Main => "Settings",
            Page::About => "About",
            Page::Dictionary => "Dictionary",
            Page::WordFinder => "Word finder",
            Page::Reviews => "Reviews",
        };
        if m3::top_app_bar(ui, s, title, self.page != Page::Main) {
            self.back();
        }
        egui::CentralPanel::default().frame(super::surface(s)).show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| match self.page {
                Page::Main => self.main(ui, s, reviews),
                Page::Reviews => self.reviews(ui, s, platform, reviews),
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
    }

    fn main(&mut self, ui: &mut egui::Ui, s: &Scheme, reviews: &Reviews) {
        m3::section_header(ui, s, "Reading");
        m3::list_item(ui, s, ICON_AUTO_STORIES, "Page", "Font size, line spacing, vertical or horizontal text", false);
        m3::list_item(ui, s, ICON_TRANSLATE, "Parsing", "How words are found in the text", false);
        m3::section_header(ui, s, "Words");
        if m3::list_item(ui, s, ICON_DICTIONARY, "Dictionary", "JMdict, English, on this device", true).clicked() {
            self.page = Page::Dictionary;
        }
        if m3::list_item(ui, s, ICON_SCHOOL, "Reviews", &reviews.summary(), true).clicked() {
            self.draft = reviews.settings.clone();
            self.page = Page::Reviews;
        }
        m3::section_header(ui, s, "App");
        m3::list_item(ui, s, ICON_PALETTE, "Appearance", "Colours and theme", false);
        if m3::list_item(ui, s, ICON_INFO, "About", concat!("Rubiyomi ", env!("CARGO_PKG_VERSION"), " · dictionary and licences"), true)
            .clicked()
        {
            self.page = Page::About;
        }
    }

    fn poll(&mut self) {
        if let Some(d) = task::take(&mut self.loading_decks) {
            self.decks = Some(d);
        }
        if let Some(a) = task::take(&mut self.loading_anki) {
            self.anki = Some(a);
        }
        if let Some(f) = task::take(&mut self.loading_fields) {
            self.fields = Some(f);
        }
    }

    fn reviews(&mut self, ui: &mut egui::Ui, s: &Scheme, platform: &dyn Platform, reviews: &mut Reviews) {
        prose(ui, |ui| {
            hint(
                ui,
                s,
                "Words on the page take the colour of their card, and the word popup can add or grade them. Looking words up stays on this device.",
            );
            ui.add_space(12.0);
            let mut i = self.draft.reviews as usize;
            if m3::segmented(ui, s, &["None", "jpdb.io", "Anki"], &mut i) {
                self.draft.reviews = [ReviewSource::None, ReviewSource::Jpdb, ReviewSource::Anki][i];
            }
            ui.add_space(12.0);
            match self.draft.reviews {
                ReviewSource::None => hint(ui, s, "Words aren't coloured, and there are no review buttons."),
                ReviewSource::Jpdb => self.jpdb(ui, s),
                ReviewSource::Anki => self.anki(ui, s, platform),
            }
            ui.add_space(16.0);
            if m3::filled_button(ui, s, "Save and sync").clicked() {
                reviews.apply(ui.ctx(), platform, self.draft.clone());
            }
            ui.add_space(8.0);
            match &reviews.status {
                _ if self.draft != reviews.settings => hint(ui, s, "Not saved yet"),
                Status::Syncing => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        hint(ui, s, "Syncing…");
                    });
                }
                Status::Synced => hint(ui, s, "Up to date"),
                Status::Failed(e) => {
                    ui.label(RichText::new(e).font(font(text::BODY_MEDIUM)).color(c(s.error)));
                }
                Status::Off => {}
            }
            if let Some(note) = &reviews.note {
                ui.label(RichText::new(note).font(font(text::BODY_SMALL)).color(c(s.error)));
            }
        });
    }

    fn jpdb(&mut self, ui: &mut egui::Ui, s: &Scheme) {
        label(ui, "API key");
        hint(ui, s, "From the bottom of jpdb.io's settings page. It is kept in the system's keyring, never in Rubiyomi's files.");
        if ui
            .add(egui::TextEdit::singleline(&mut self.draft.jpdb_key).password(true).hint_text("API key").desired_width(f32::INFINITY))
            .changed()
        {
            self.decks = None;
        }
        ui.add_space(12.0);
        label(ui, "New words go into");
        ui.horizontal_wrapped(|ui| {
            let d = &mut self.draft;
            let current = if d.jpdb_deck.is_empty() { "No deck chosen".to_owned() } else { d.jpdb_deck_name.clone() };
            match &self.decks {
                Some(Ok(decks)) => {
                    egui::ComboBox::from_id_salt("jpdb-deck").selected_text(current).show_ui(ui, |ui| {
                        for deck in decks {
                            if ui.selectable_label(d.jpdb_deck == deck.id, &deck.name).clicked() {
                                d.jpdb_deck = deck.id.clone();
                                d.jpdb_deck_name = deck.name.clone();
                            }
                        }
                    });
                }
                Some(Err(e)) => {
                    ui.label(RichText::new(e).color(c(s.error)));
                }
                None => {
                    ui.label(current);
                }
            }
            if self.loading_decks.is_some() {
                ui.spinner();
            } else if !d.jpdb_key.trim().is_empty() && m3::text_button(ui, s, "Load decks").clicked() {
                let client = JpdbClient::new(&d.jpdb_key);
                self.loading_decks = Some(Task::spawn(ui.ctx(), move || client.decks().map_err(|e| e.to_string())));
            }
        });
        ui.add_space(8.0);
        hint(
            ui,
            s,
            "Your decks are synced when the app starts, so pages are coloured offline too. Grades and new cards go to jpdb.io straight away; nothing is queued while offline.",
        );
    }

    fn anki(&mut self, ui: &mut egui::Ui, s: &Scheme, platform: &dyn Platform) {
        let droid = platform.anki_droid_status();
        match droid {
            Some((false, _)) => {
                ui.label(RichText::new("AnkiDroid isn't installed.").color(c(s.error)));
                if m3::text_button(ui, s, "Get AnkiDroid").clicked() {
                    platform.open_url("https://play.google.com/store/apps/details?id=com.ichi2.anki");
                }
                return;
            }
            Some((true, false)) => {
                hint(ui, s, "Rubiyomi needs AnkiDroid's permission to read and add cards.");
                ui.add_space(8.0);
                if m3::filled_button(ui, s, "Allow access to AnkiDroid").clicked() {
                    platform.request_anki_permission();
                }
                return;
            }
            Some((true, true)) => hint(ui, s, "Connected to AnkiDroid."),
            None => {
                hint(ui, s, "Desktop Anki, through the AnkiConnect add-on. Keep Anki open while you read.");
                ui.add_space(8.0);
                label(ui, "Address");
                ui.add(egui::TextEdit::singleline(&mut self.draft.anki_url).hint_text(DEFAULT_URL).desired_width(f32::INFINITY));
                label(ui, "Key");
                ui.add(
                    egui::TextEdit::singleline(&mut self.draft.anki_key)
                        .password(true)
                        .hint_text("Only if AnkiConnect asks for one")
                        .desired_width(f32::INFINITY),
                );
            }
        }
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            if self.loading_anki.is_some() {
                ui.spinner();
                hint(ui, s, "Asking Anki…");
            } else if m3::text_button(ui, s, "Load decks and note types").clicked() {
                let backend = reviews::anki_backend(platform, &self.draft);
                let connect = droid.is_none().then(|| AnkiConnect::new(&self.draft.anki_url, &self.draft.anki_key, http::transport()));
                self.loading_anki = Some(Task::spawn(ui.ctx(), move || {
                    if let Some(connect) = connect
                        && !connect.request_permission().map_err(|e| e.to_string())?
                    {
                        return Err("Anki refused the connection".to_owned());
                    }
                    Ok((backend.decks().map_err(|e| e.to_string())?, backend.note_types().map_err(|e| e.to_string())?))
                }));
            }
            if let Some(Err(e)) = &self.anki {
                ui.label(RichText::new(e).color(c(s.error)));
            }
        });
        let m = &mut self.draft.anki;
        let lists = match &self.anki {
            Some(Ok(lists)) => lists.clone(),
            // What was chosen before still shows, so it can be kept.
            _ => (vec![m.deck.clone()], vec![m.note_type.clone()]),
        };
        if self.anki.is_none() && m.deck.is_empty() {
            return;
        }
        combo(ui, "anki-deck", "Deck", &mut m.deck, &lists.0, false);
        let before = m.note_type.clone();
        combo(ui, "anki-type", "Note type", &mut m.note_type, &lists.1, false);
        if m.note_type != before {
            m.word_field.clear();
            m.reading_field.clear();
            m.meaning_field.clear();
            m.sentence_field.clear();
            self.fields = None;
        }
        if m.note_type.is_empty() {
            return;
        }
        let have = self.fields.as_ref().is_some_and(|(t, _)| *t == m.note_type);
        if !have && self.loading_fields.is_none() {
            let backend = reviews::anki_backend(platform, &self.draft);
            let t = self.draft.anki.note_type.clone();
            self.loading_fields = Some(Task::spawn(ui.ctx(), move || {
                let fields = backend.fields(&t).map_err(|e| e.to_string());
                (t, fields)
            }));
        }
        let m = &mut self.draft.anki;
        match &self.fields {
            Some((t, Ok(fields))) if *t == m.note_type => {
                combo(ui, "anki-word", "Word field", &mut m.word_field, fields, false);
                combo(ui, "anki-reading", "Reading field", &mut m.reading_field, fields, true);
                combo(ui, "anki-meaning", "Meaning field", &mut m.meaning_field, fields, true);
                combo(ui, "anki-sentence", "Sentence field", &mut m.sentence_field, fields, true);
            }
            Some((_, Err(e))) => {
                ui.label(RichText::new(e).color(c(s.error)));
            }
            _ => {
                ui.spinner();
            }
        }
        ui.add_space(8.0);
        hint(ui, s, "New cards get the tag rubiyomi. Grades go to Anki straight away.");
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

fn combo(ui: &mut egui::Ui, id: &str, name: &str, value: &mut String, options: &[String], optional: bool) {
    ui.horizontal(|ui| {
        ui.label(name);
        let shown = if value.is_empty() { if optional { "(none)" } else { "Choose…" }.to_owned() } else { value.clone() };
        egui::ComboBox::from_id_salt(id).selected_text(shown).show_ui(ui, |ui| {
            if optional && ui.selectable_label(value.is_empty(), "(none)").clicked() {
                value.clear();
            }
            for o in options.iter().filter(|o| !o.is_empty()) {
                if ui.selectable_label(value == o, o).clicked() {
                    *value = o.clone();
                }
            }
        });
    });
}

fn label(ui: &mut egui::Ui, t: &str) {
    ui.label(RichText::new(t).font(font(text::LABEL_LARGE)));
}

fn hint(ui: &mut egui::Ui, s: &Scheme, t: &str) {
    ui.label(RichText::new(t).font(font(text::BODY_MEDIUM)).color(c(s.on_surface_variant)));
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
