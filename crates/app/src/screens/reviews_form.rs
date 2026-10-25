// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use egui::RichText;
use rubiyomi_core::grade::anki::connect::{AnkiConnect, DEFAULT_URL};
use rubiyomi_core::grade::anki::{AnkiMapping, InDeck, guess_roles, inspect};
use rubiyomi_core::http;
use rubiyomi_core::jpdb::{Deck, JpdbClient};
use rubiyomi_core::platform::Platform;
use rubiyomi_core::settings::{ReviewSource, Settings};
use rubiyomi_core::theme::Scheme;

use crate::reviews;
use crate::task::{self, Task};
use crate::ui::m3::{self, c, font, text};

type AnkiLists = (Vec<String>, Vec<String>);
type Inspected = Result<Vec<InDeck>, String>;
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
    // What the chosen deck holds, looked at when it is chosen.
    inspected: Option<Inspected>,
    inspected_for: String,
    inspecting: Option<Task<Inspected>>,
    // The deck whose note type and fields were last chosen automatically.
    picked_for: String,
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
        if let Some(i) = task::take(&mut self.inspecting) {
            self.inspected = Some(i);
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
        let lists = match &self.anki {
            Some(Ok(lists)) => lists.clone(),
            // What was chosen before still shows, so it can be kept.
            _ => (vec![self.draft.anki.deck.clone()], vec![self.draft.anki.note_type.clone()]),
        };
        if !matches!(self.anki, Some(Ok(_))) && self.draft.anki.deck.is_empty() {
            return;
        }
        ui.add_space(12.0);
        label(ui, "Deck");
        hint(ui, s, "Where your Japanese cards are. Words you add from the reader go here too.");
        combo(ui, "anki-deck", &mut self.draft.anki.deck, &lists.0, "Choose a deck…");
        let deck = self.draft.anki.deck.clone();
        if deck.is_empty() {
            return;
        }
        if matches!(self.anki, Some(Ok(_))) && self.inspected_for != deck && self.inspecting.is_none() {
            self.inspected_for = deck.clone();
            self.inspected = None;
            let (backend, types, d) = (reviews::anki_backend(platform, &self.draft), lists.1.clone(), deck.clone());
            self.inspecting = Some(Task::spawn(ui.ctx(), move || inspect(&*backend, &d, &types).map_err(|e| e.to_string())));
        }
        let found: Vec<InDeck> = match &self.inspected {
            Some(Ok(found)) => found.clone(),
            Some(Err(e)) => {
                ui.label(RichText::new(e).color(c(s.error)));
                Vec::new()
            }
            None => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    hint(ui, s, &format!("Looking in {deck}…"));
                });
                Vec::new()
            }
        };
        let looked = matches!(self.inspected, Some(Ok(_)));
        // The note type the deck's cards use is the one to grade, so it is chosen for you.
        if looked && self.picked_for != deck {
            self.picked_for = deck.clone();
            if let Some(top) = found.first()
                && !found.iter().any(|t| t.note_type == self.draft.anki.note_type && is_mapped(&self.draft.anki, &t.fields))
            {
                self.draft.anki.note_type = top.note_type.clone();
                set_roles(&mut self.draft.anki, guess_roles(&top.fields));
            }
        }
        ui.add_space(12.0);
        label(ui, "Note type");
        if looked {
            let cards: usize = found.iter().map(|t| t.notes).sum();
            let said = match (cards, found.len()) {
                (0, _) => "This deck has no cards yet: choose the note type for the cards Rubiyomi adds.".to_owned(),
                (n, 1) => format!("All {n} cards in this deck are of this note type."),
                (n, _) => format!("{n} cards in this deck, of {} note types: the most used one is chosen.", found.len()),
            };
            hint(ui, s, &said);
        }
        let before = self.draft.anki.note_type.clone();
        let labels: Vec<(String, String)> = lists
            .1
            .iter()
            .map(|t| {
                let here = found.iter().find(|f| f.note_type == *t).map_or(String::new(), |f| format!("  ·  {} cards here", f.notes));
                (t.clone(), format!("{t}{here}"))
            })
            .collect();
        labelled_combo(ui, "anki-type", &mut self.draft.anki.note_type, &labels, "Choose a note type…");
        let note_type = self.draft.anki.note_type.clone();
        if note_type.is_empty() {
            return;
        }
        let in_deck = found.iter().find(|t| t.note_type == note_type).cloned();
        if in_deck.is_none() && !self.fields.as_ref().is_some_and(|(t, _)| *t == note_type) && self.loading_fields.is_none() {
            let backend = reviews::anki_backend(platform, &self.draft);
            let t = note_type.clone();
            self.loading_fields = Some(Task::spawn(ui.ctx(), move || {
                let fields = backend.fields(&t).map_err(|e| e.to_string());
                (t, fields)
            }));
        }
        let (fields, sample) = match (&in_deck, &self.fields) {
            (Some(t), _) => (t.fields.clone(), t.sample.clone()),
            (None, Some((t, Ok(fields)))) if *t == note_type => (fields.clone(), Vec::new()),
            (None, Some((t, Err(e)))) if *t == note_type => {
                ui.label(RichText::new(e).color(c(s.error)));
                return;
            }
            _ => {
                ui.spinner();
                return;
            }
        };
        if note_type != before || !fits(&self.draft.anki, &fields) {
            set_roles(&mut self.draft.anki, guess_roles(&fields));
        }
        ui.add_space(16.0);
        label(ui, "What goes in each field");
        hint(ui, s, "As a card for 読み would be filled. Change what a field holds and the preview follows.");
        ui.add_space(8.0);
        egui::Frame::new().fill(c(s.surface_container)).corner_radius(12).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            for (k, field) in fields.iter().enumerate() {
                if k > 0 {
                    ui.add_space(6.0);
                    ui.separator();
                    ui.add_space(6.0);
                }
                field_row(ui, s, &mut self.draft.anki, field, sample.get(k).map(String::as_str));
            }
        });
        if self.draft.anki.word_field.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new("Choose the field that holds the word.").font(font(text::BODY_MEDIUM)).color(c(s.error)));
        }
        ui.add_space(8.0);
        hint(ui, s, "New cards get the tag rubiyomi. Grades go to Anki straight away.");
    }
}

const ROLES: [&str; 4] = ["Word", "Reading", "Meaning", "Sentence"];

fn roles_of(m: &mut AnkiMapping) -> [&mut String; 4] {
    [&mut m.word_field, &mut m.reading_field, &mut m.meaning_field, &mut m.sentence_field]
}

fn set_roles(m: &mut AnkiMapping, roles: [String; 4]) {
    for (slot, value) in roles_of(m).into_iter().zip(roles) {
        *slot = value;
    }
}

// Every role either unused or on a field the note type has, and the word placed.
fn is_mapped(m: &AnkiMapping, fields: &[String]) -> bool {
    !m.word_field.is_empty() && fits(m, fields)
}

// The roles name only fields the note type has, and at least one is in use.
fn fits(m: &AnkiMapping, fields: &[String]) -> bool {
    let all = [&m.word_field, &m.reading_field, &m.meaning_field, &m.sentence_field];
    all.iter().any(|f| !f.is_empty()) && all.iter().all(|f| f.is_empty() || fields.contains(f))
}

fn field_row(ui: &mut egui::Ui, s: &Scheme, m: &mut AnkiMapping, field: &str, sample: Option<&str>) {
    let role = roles_of(m).iter().position(|r| r.as_str() == field);
    ui.horizontal(|ui| {
        ui.label(RichText::new(field).font(font(text::TITLE_MEDIUM)).color(c(s.on_surface)));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let shown = role.map_or("Nothing", |r| ROLES[r]);
            egui::ComboBox::from_id_salt(("anki-role", field)).selected_text(RichText::new(shown).font(font(text::BODY_LARGE))).show_ui(
                ui,
                |ui| {
                    for (r, name) in ROLES.iter().enumerate() {
                        if ui.selectable_label(role == Some(r), *name).clicked() {
                            let slots = roles_of(m);
                            if let Some(old) = role {
                                slots[old].clear();
                            }
                            *slots[r] = field.to_owned();
                        }
                    }
                    if ui.selectable_label(role.is_none(), "Nothing").clicked()
                        && let Some(old) = role
                    {
                        roles_of(m)[old].clear();
                    }
                },
            );
        });
    });
    let ink = c(s.on_surface);
    match role {
        Some(3) => {
            let font_id = font(text::BODY_LARGE);
            let mut job = egui::text::LayoutJob::default();
            let plain = egui::TextFormat { font_id: font_id.clone(), color: ink, ..Default::default() };
            let strong =
                egui::TextFormat { font_id, color: c(s.primary), underline: egui::Stroke::new(1.0, c(s.primary)), ..Default::default() };
            job.append("毎晩、本を", 0.0, plain.clone());
            job.append("読み", 0.0, strong);
            job.append("ながら眠った。", 0.0, plain);
            ui.label(job);
        }
        Some(r) => {
            let value = ["読み", "よみ", "reading · pronunciation (of a kanji)"][r];
            ui.label(RichText::new(value).font(font(if r == 0 { text::TITLE_LARGE } else { text::BODY_LARGE })).color(ink));
        }
        None => hint(ui, s, "Left empty"),
    }
    if let Some(sample) = sample.filter(|t| !t.trim().is_empty()) {
        let shown: String = sample.chars().take(60).collect();
        let more = if sample.chars().count() > 60 { "…" } else { "" };
        ui.label(RichText::new(format!("On one of your cards: {shown}{more}")).font(font(text::BODY_SMALL)).color(c(s.on_surface_variant)));
    }
}

fn combo(ui: &mut egui::Ui, id: &str, value: &mut String, options: &[String], none: &str) {
    let labels: Vec<(String, String)> = options.iter().map(|o| (o.clone(), o.clone())).collect();
    labelled_combo(ui, id, value, &labels, none);
}

fn labelled_combo(ui: &mut egui::Ui, id: &str, value: &mut String, options: &[(String, String)], none: &str) {
    ui.add_space(4.0);
    let shown = options
        .iter()
        .find(|(o, _)| o == value)
        .map_or_else(|| if value.is_empty() { none.to_owned() } else { value.clone() }, |(_, l)| l.clone());
    egui::ComboBox::from_id_salt(id)
        .width(ui.available_width().min(360.0))
        .selected_text(RichText::new(shown).font(font(text::BODY_LARGE)))
        .show_ui(ui, |ui| {
            for (o, l) in options.iter().filter(|(o, _)| !o.is_empty()) {
                if ui.selectable_label(value == o, l).clicked() {
                    *value = o.clone();
                }
            }
        });
}

fn label(ui: &mut egui::Ui, t: &str) {
    ui.label(RichText::new(t).font(font(text::LABEL_LARGE)));
}

pub fn hint(ui: &mut egui::Ui, s: &Scheme, t: &str) {
    ui.label(RichText::new(t).font(font(text::BODY_MEDIUM)).color(c(s.on_surface_variant)));
}
