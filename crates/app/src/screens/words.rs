// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use egui::{Align, CornerRadius, Layout, RichText, Sense, UiBuilder, pos2, vec2};
use egui_material_icons::icons::ICON_DICTIONARY;
use rubiyomi_core::cards::WordKey;
use rubiyomi_core::grade::StateMap;
use rubiyomi_core::history::Looked;
use rubiyomi_core::library::BookId;
use rubiyomi_core::theme::Scheme;

use super::Ctx;
use super::reader::state_colour;
use crate::ui::m3::{self, c, font, text};

// More rows than this make every frame slow; a search narrows them down.
const SHOWN: usize = 200;

pub struct Jump {
    pub book: BookId,
    pub chapter: usize,
    pub offset: usize,
    pub len: usize,
}

#[derive(Default)]
pub struct WordsScreen {
    query: String,
    open: Option<WordKey>,
    // The states of the listed words, for the grader generation and the list they were asked for.
    states: Option<(u64, usize, StateMap)>,
    confirm_clear: bool,
}

fn matches(l: &Looked, q: &str) -> bool {
    q.is_empty()
        || l.spelling.contains(q)
        || l.reading.contains(q)
        || l.sentence.contains(q)
        || l.meanings.iter().any(|m| m.to_lowercase().contains(q))
}

impl WordsScreen {
    pub fn ui(&mut self, ui: &mut egui::Ui, cx: &mut Ctx<'_>) -> Option<Jump> {
        let s = cx.scheme;
        m3::top_app_bar(ui, s, "Words", false);
        let mut jump = None;
        egui::CentralPanel::default().frame(super::surface(s)).show(ui, |ui| {
            if cx.history.items().is_empty() {
                m3::empty_state(ui, s, ICON_DICTIONARY, "No words yet", "Words you look up while reading will be collected here.");
                return;
            }
            self.refresh_states(cx);
            egui::Frame::new().inner_margin(egui::Margin::symmetric(16, 8)).show(ui, |ui| {
                m3::text_field(ui, s, &mut self.query, "Search words, readings, meanings", false, None);
            });
            let q = self.query.trim().to_lowercase();
            let found: Vec<&Looked> = cx.history.items().iter().filter(|l| matches(l, &q)).collect();
            ui.horizontal(|ui| {
                ui.add_space(16.0);
                let count = match found.len() {
                    1 => "1 word".to_owned(),
                    n if n > SHOWN => format!("{SHOWN} of {n} words"),
                    n => format!("{n} words"),
                };
                ui.label(RichText::new(count).font(font(text::BODY_MEDIUM)).color(c(s.on_surface_variant)));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(8.0);
                    if m3::text_button(ui, s, "Clear all").clicked() {
                        self.confirm_clear = true;
                    }
                });
            });
            let mut remove = None;
            m3::scroll().auto_shrink([false, false]).show(ui, |ui| {
                for l in found.iter().take(SHOWN) {
                    let key = l.key();
                    let open = self.open.as_ref() == Some(&key);
                    let colour = self
                        .states
                        .as_ref()
                        .and_then(|(_, _, m)| m.get(&key))
                        .map(|st| state_colour(*st, s.dark))
                        .filter(|c| *c != egui::Color32::TRANSPARENT);
                    if row(ui, s, l, colour, open).clicked() {
                        self.open = if open { None } else { Some(key.clone()) };
                    }
                    if open {
                        egui::Frame::new().inner_margin(egui::Margin { left: 40, right: 16, top: 0, bottom: 12 }).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            if !l.sentence.is_empty() {
                                ui.label(RichText::new(&l.sentence).font(font(text::BODY_LARGE)).color(c(s.on_surface)));
                            }
                            let seen = if l.count == 1 { "Looked up once".to_owned() } else { format!("Looked up {} times", l.count) };
                            let book = if l.book_title.is_empty() { String::new() } else { format!(" · {}", l.book_title) };
                            ui.label(RichText::new(format!("{seen}{book}")).font(font(text::BODY_SMALL)).color(c(s.on_surface_variant)));
                            ui.horizontal(|ui| {
                                if cx.library.meta(&l.book).is_some() && m3::tonal_button(ui, s, "Open in the book").clicked() {
                                    jump = Some(Jump {
                                        book: l.book.clone(),
                                        chapter: l.chapter as usize,
                                        offset: l.offset as usize,
                                        len: if l.len > 0 { l.len as usize } else { l.spelling.chars().count() },
                                    });
                                }
                                if m3::text_button(ui, s, "Remove").clicked() {
                                    remove = Some(key.clone());
                                }
                            });
                        });
                    }
                }
            });
            if let Some(key) = remove {
                cx.history.remove(&key);
                self.open = None;
            }
        });
        if self.confirm_clear {
            self.clear_dialog(ui.ctx(), cx);
        }
        jump
    }

    fn refresh_states(&mut self, cx: &Ctx<'_>) {
        let Some(grader) = cx.grader else {
            self.states = None;
            return;
        };
        let n = cx.history.items().len();
        if self.states.as_ref().is_some_and(|(g, len, _)| *g == cx.generation && *len == n) {
            return;
        }
        let words: Vec<_> = cx.history.items().iter().take(SHOWN.max(n)).map(Looked::word).collect();
        self.states = Some((cx.generation, n, grader.states(&words)));
    }

    fn clear_dialog(&mut self, ctx: &egui::Context, cx: &mut Ctx<'_>) {
        let s = cx.scheme;
        let modal = egui::Modal::new(egui::Id::new("clear-words")).show(ctx, |ui| {
            ui.set_width(320.0_f32.min(ctx.content_rect().width() - 48.0));
            ui.label(RichText::new("Clear all words?").font(font(text::HEADLINE_SMALL)).color(c(s.on_surface)));
            ui.add_space(8.0);
            ui.label(
                RichText::new("The list of words you looked up is emptied. Your cards and reading stats stay.")
                    .font(font(text::BODY_MEDIUM))
                    .color(c(s.on_surface_variant)),
            );
            ui.add_space(16.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if m3::text_button(ui, s, "Clear").clicked() {
                    cx.history.clear();
                    self.open = None;
                    self.confirm_clear = false;
                }
                if m3::text_button(ui, s, "Cancel").clicked() {
                    self.confirm_clear = false;
                }
            });
        });
        if modal.should_close() {
            self.confirm_clear = false;
        }
    }
}

fn row(ui: &mut egui::Ui, s: &Scheme, l: &Looked, colour: Option<egui::Color32>, open: bool) -> egui::Response {
    ui.scope_builder(UiBuilder::new().sense(Sense::click()), |ui| {
        ui.set_width(ui.available_width());
        ui.set_min_height(64.0);
        let rect = ui.max_rect();
        let r = ui.response();
        let fill = if open { m3::alpha(c(s.on_surface), 0.05) } else { m3::alpha(c(s.on_surface), m3::state_layer(&r)) };
        ui.painter().rect_filled(egui::Rect::from_min_size(rect.min, vec2(rect.width(), 64.0)), CornerRadius::ZERO, fill);
        let dot = colour.unwrap_or(m3::alpha(c(s.on_surface_variant), 0.3));
        ui.painter().circle_filled(pos2(rect.left() + 22.0, rect.top() + 32.0), 5.0, dot);
        ui.horizontal(|ui| {
            ui.add_space(40.0);
            ui.vertical(|ui| {
                ui.add_space(10.0);
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&l.spelling).font(font(text::TITLE_LARGE)).color(c(s.on_surface)));
                    if l.reading != l.spelling {
                        ui.label(RichText::new(&l.reading).font(font(text::BODY_MEDIUM)).color(c(s.on_surface_variant)));
                    }
                });
                let meaning = l.meanings.first().map_or("", String::as_str);
                ui.set_width(ui.available_width() - 16.0);
                ui.add(egui::Label::new(RichText::new(meaning).font(font(text::BODY_MEDIUM)).color(c(s.on_surface_variant))).truncate());
                ui.add_space(8.0);
            });
        });
    })
    .response
}
