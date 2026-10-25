// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use egui::RichText;
use rubiyomi_core::grade::anki::connect::{AnkiConnect, DEFAULT_URL};
use rubiyomi_core::http;
use rubiyomi_core::jpdb::{Deck, JpdbClient};
use rubiyomi_core::platform::Platform;
use rubiyomi_core::settings::{ReviewSource, Settings};
use rubiyomi_core::theme::Scheme;

use crate::reviews;
use crate::task::{self, Task};
use crate::ui::m3::{self, c, font, text};

type AnkiLists = (Vec<String>, Vec<String>);
type Fields = (String, Result<Vec<String>, String>);

// Where reviews come from, as Settings and the first-launch setup both ask it.
// Decks and note types load by themselves once there is enough to ask with.
#[derive(Default)]
pub struct ReviewsForm {
    pub draft: Settings,
    decks: Option<Result<Vec<Deck>, String>>,
    decks_for: String,
    loading_decks: Option<Task<Result<Vec<Deck>, String>>>,
    anki: Option<Result<AnkiLists, String>>,
    anki_for: (String, String),
    loading_anki: Option<Task<Result<AnkiLists, String>>>,
    fields: Option<Fields>,
    loading_fields: Option<Task<Fields>>,
}

impl ReviewsForm {
    pub fn new(settings: &Settings) -> Self {
        ReviewsForm { draft: settings.clone(), ..ReviewsForm::default() }
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

    pub fn ui(&mut self, ui: &mut egui::Ui, s: &Scheme, platform: &dyn Platform) {
        self.poll();
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
            ReviewSource::Jpdb => self.jpdb(ui, s, platform),
            ReviewSource::Anki => self.anki(ui, s, platform),
        }
    }

    fn jpdb(&mut self, ui: &mut egui::Ui, s: &Scheme, platform: &dyn Platform) {
        label(ui, "API key");
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            hint(ui, s, "Copy it from the bottom of ");
            ui.hyperlink_to(
                RichText::new("jpdb.io's settings page").font(font(text::BODY_MEDIUM)).color(c(s.primary)).underline(),
                "https://jpdb.io/settings",
            );
            hint(ui, s, ". It is kept in the system's keyring, never in Rubiyomi's files.");
        });
        ui.add_space(4.0);
        let paste = || platform.clipboard_text();
        let paste = platform.can_paste().then_some(&paste as &dyn Fn() -> Option<String>);
        let field = m3::text_field(ui, s, &mut self.draft.jpdb_key, "API key", true, paste);
        let key = self.draft.jpdb_key.trim().to_owned();
        if !field.has_focus() && !key.is_empty() && key != self.decks_for && self.loading_decks.is_none() {
            self.decks_for = key.clone();
            self.decks = None;
            let client = JpdbClient::new(&key);
            self.loading_decks = Some(Task::spawn(ui.ctx(), move || client.decks().map_err(|e| e.to_string())));
        }
        ui.add_space(12.0);
        label(ui, "New words go into");
        ui.horizontal_wrapped(|ui| {
            let d = &mut self.draft;
            let current = if d.jpdb_deck.is_empty() { "No deck chosen".to_owned() } else { d.jpdb_deck_name.clone() };
            match &self.decks {
                _ if self.loading_decks.is_some() => {
                    ui.spinner();
                    hint(ui, s, "Asking jpdb.io…");
                }
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
                    if m3::text_button(ui, s, "Try again").clicked() {
                        self.decks_for.clear();
                    }
                }
                None if d.jpdb_key.trim().is_empty() => hint(ui, s, "Your decks show here once the key is in."),
                None => {
                    ui.label(current);
                }
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
        let mut typing = false;
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
                typing |= m3::text_field(ui, s, &mut self.draft.anki_url, DEFAULT_URL, false, None).has_focus();
                ui.add_space(4.0);
                label(ui, "Key");
                typing |= m3::text_field(ui, s, &mut self.draft.anki_key, "Only if AnkiConnect asks for one", true, None).has_focus();
            }
        }
        let asked_with = (self.draft.anki_url.trim().to_owned(), self.draft.anki_key.trim().to_owned());
        if !typing && self.loading_anki.is_none() && (self.anki.is_none() || asked_with != self.anki_for) {
            self.anki_for = asked_with;
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
        ui.add_space(8.0);
        if self.loading_anki.is_some() {
            ui.horizontal(|ui| {
                ui.spinner();
                hint(ui, s, "Asking Anki…");
            });
        } else if let Some(Err(e)) = &self.anki {
            ui.label(RichText::new(e).color(c(s.error)));
            if m3::text_button(ui, s, "Try again").clicked() {
                self.anki = None;
            }
        }
        let m = &mut self.draft.anki;
        let lists = match &self.anki {
            Some(Ok(lists)) => lists.clone(),
            // What was chosen before still shows, so it can be kept.
            _ => (vec![m.deck.clone()], vec![m.note_type.clone()]),
        };
        if !matches!(self.anki, Some(Ok(_))) && m.deck.is_empty() {
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
                // The word's own field can't take anything else.
                let others: Vec<String> = fields.iter().filter(|f| **f != m.word_field).cloned().collect();
                for field in [&mut m.reading_field, &mut m.meaning_field, &mut m.sentence_field] {
                    if *field == m.word_field {
                        field.clear();
                    }
                }
                combo(ui, "anki-reading", "Reading field", &mut m.reading_field, &others, true);
                combo(ui, "anki-meaning", "Meaning field", &mut m.meaning_field, &others, true);
                combo(ui, "anki-sentence", "Sentence field", &mut m.sentence_field, &others, true);
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
}

fn combo(ui: &mut egui::Ui, id: &str, name: &str, value: &mut String, options: &[String], optional: bool) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(name).font(font(text::BODY_LARGE)));
        let shown = if value.is_empty() { if optional { "(none)" } else { "Choose…" }.to_owned() } else { value.clone() };
        egui::ComboBox::from_id_salt(id).selected_text(RichText::new(shown).font(font(text::BODY_LARGE))).show_ui(ui, |ui| {
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

pub fn hint(ui: &mut egui::Ui, s: &Scheme, t: &str) {
    ui.label(RichText::new(t).font(font(text::BODY_MEDIUM)).color(c(s.on_surface_variant)));
}
