// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::ops::Range;

use egui::{Color32, CornerRadius, RichText, vec2};
use rubiyomi_core::cards::CardState;
use rubiyomi_core::finder::Word;
use rubiyomi_core::grade::Grader;
use rubiyomi_core::theme::Scheme;

use crate::ui::m3::{self, c, font, text};

pub enum Content {
    Word(Word),
    Message(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Grade(u8),
    Add,
    Act(&'static str),
    Open(String),
    Copy(String),
}

pub struct Popup {
    pub anchor: egui::Rect,
    pub span: Range<usize>,
    pub content: Content,
    pub sentence: String,
    pub state: Option<CardState>,
    pub busy: bool,
    height: f32,
}

impl Popup {
    pub fn new(anchor: egui::Rect, span: Range<usize>, content: Content) -> Self {
        Popup { anchor, span, content, sentence: String::new(), state: None, busy: false, height: 200.0 }
    }

    fn place(&self, screen: egui::Rect, width: f32) -> egui::Pos2 {
        let gap = 8.0;
        let below = self.anchor.bottom() + gap;
        let y = if below + self.height <= screen.bottom() - 8.0 {
            below
        } else {
            (self.anchor.top() - gap - self.height).max(screen.top() + 8.0)
        };
        let x = (self.anchor.center().x - width / 2.0).clamp(screen.left() + 8.0, (screen.right() - width - 8.0).max(screen.left() + 8.0));
        egui::pos2(x, y)
    }

    pub fn ui(&mut self, ctx: &egui::Context, s: &Scheme, grader: Option<&dyn Grader>) -> Option<Action> {
        let mut action = None;
        let screen = ctx.content_rect();
        let width = if screen.width() < 480.0 { screen.width() - 16.0 } else { 360.0 };
        let pos = self.place(screen, width);
        let dark = ctx.theme() == egui::Theme::Dark;
        let area = egui::Area::new(egui::Id::new("word-popup")).order(egui::Order::Foreground).fixed_pos(pos).show(ctx, |ui| {
            egui::Frame::new()
                .fill(c(s.surface_container_high))
                .corner_radius(CornerRadius::same(16))
                .shadow(egui::epaint::Shadow { offset: [0, 2], blur: 8, spread: 0, color: m3::alpha(c(s.scrim), 0.25) })
                .inner_margin(16)
                .show(ui, |ui| {
                    ui.set_width(width - 32.0);
                    match &self.content {
                        Content::Message(m) => {
                            ui.label(RichText::new(m).color(c(s.on_surface_variant)));
                        }
                        Content::Word(word) => {
                            action = word_face(ui, s, word, self.state.filter(|_| grader.is_some()), dark, &self.sentence);
                            if let Some(g) = grader.filter(|_| word.jmdict_id.is_some())
                                && let Some(a) = self.footer(ui, s, g)
                            {
                                action = Some(a);
                            }
                        }
                    }
                });
        });
        self.height = area.response.rect.height();
        action
    }

    fn footer(&self, ui: &mut egui::Ui, s: &Scheme, g: &dyn Grader) -> Option<Action> {
        let mut action = None;
        ui.separator();
        if self.busy {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Sending…");
            });
            return None;
        }
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
            for b in g.grades() {
                if m3::tonal_button(ui, s, b.label).clicked() {
                    action = Some(Action::Grade(b.value));
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
            if m3::filled_button(ui, s, "Add to deck").clicked() {
                action = Some(Action::Add);
            }
            for a in g.actions() {
                if m3::text_button(ui, s, a).clicked() {
                    action = Some(Action::Act(a));
                }
            }
        });
        action
    }
}

// Known words keep the text colour: only what still needs work stands out.
pub fn state_colour(state: CardState, dark: bool) -> Option<Color32> {
    let (light, night) = match state {
        CardState::NotInDeck => (0x2563C9, 0x82B1FF),
        CardState::New => (0x00838F, 0x5FD4DD),
        CardState::Learning => (0x2E7D32, 0x81D486),
        CardState::Due => (0xC75D00, 0xFFB264),
        CardState::Failed => (0xC62828, 0xFF8A80),
        CardState::Locked => (0x7B5BB0, 0xC3AEEB),
        _ => return None,
    };
    let v: u32 = if dark { night } else { light };
    Some(Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

fn word_face(ui: &mut egui::Ui, s: &Scheme, word: &Word, state: Option<CardState>, dark: bool, sentence: &str) -> Option<Action> {
    let mut action = None;
    ui.horizontal(|ui| {
        ui.label(RichText::new(&word.spelling).font(font(text::HEADLINE_SMALL)).color(c(s.on_surface)));
        if word.reading != word.spelling && !word.reading.is_empty() {
            ui.label(RichText::new(&word.reading).font(font(text::TITLE_MEDIUM)).color(c(s.on_surface_variant)));
        }
    });
    let kinds = pos_labels(&word.pos);
    if state.is_some() || !kinds.is_empty() {
        ui.horizontal_wrapped(|ui| {
            if let Some(state) = state {
                m3::outlined_chip(ui, state.label(), state_colour(state, dark).unwrap_or(c(s.on_surface_variant)));
            }
            if !kinds.is_empty() {
                ui.label(RichText::new(kinds.join(", ")).font(font(text::LABEL_MEDIUM)).color(c(s.on_surface_variant)));
            }
        });
    }
    ui.add_space(4.0);
    if word.meanings.is_empty() {
        ui.label(RichText::new("Not in the dictionary.").color(c(s.on_surface_variant)));
    }
    egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
        for (i, m) in word.meanings.iter().take(8).enumerate() {
            ui.label(RichText::new(format!("{}. {m}", i + 1)).font(font(text::BODY_MEDIUM)).color(c(s.on_surface)));
        }
    });
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        // jpdb.io's vocabulary ids are JMdict's.
        if let Some(id) = word.jmdict_id
            && m3::text_button(ui, s, "jpdb.io").clicked()
        {
            action = Some(Action::Open(format!("https://jpdb.io/vocabulary/{id}/{}", urlencode(&word.spelling))));
        }
        if m3::text_button(ui, s, "Jisho").clicked() {
            action = Some(Action::Open(format!("https://jisho.org/search/{}", urlencode(&word.spelling))));
        }
        if m3::text_button(ui, s, "Copy word").clicked() {
            action = Some(Action::Copy(word.spelling.clone()));
        }
        if !sentence.is_empty() && m3::text_button(ui, s, "Copy sentence").clicked() {
            action = Some(Action::Copy(sentence.to_owned()));
        }
    });
    action
}

fn pos_labels(tags: &[String]) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for t in tags {
        let label = match t.as_str() {
            "n" | "n-adv" | "n-t" => "noun",
            "pn" => "pronoun",
            "v1" | "v1-s" => "ichidan verb",
            t if t.starts_with("v5") => "godan verb",
            "vs" | "vs-i" | "vs-s" => "する verb",
            "vk" => "くる verb",
            "vt" => "transitive",
            "vi" => "intransitive",
            "adj-i" | "adj-ix" => "い-adjective",
            "adj-na" => "な-adjective",
            "adj-no" => "の-adjective",
            "adj-pn" => "pre-noun adjectival",
            "adv" | "adv-to" => "adverb",
            "aux-v" | "aux" => "auxiliary",
            "conj" => "conjunction",
            "ctr" => "counter",
            "exp" => "expression",
            "int" => "interjection",
            "prt" => "particle",
            "pref" => "prefix",
            "suf" => "suffix",
            _ => continue,
        };
        if !out.contains(&label) {
            out.push(label);
        }
    }
    out.truncate(4);
    out
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') { (b as char).to_string() } else { format!("%{b:02X}") }
        })
        .collect()
}
