// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::ops::Range;

use egui::{Align, Color32, CornerRadius, Layout, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Vec2, pos2, vec2};
use egui_material_icons::MaterialIcon;
use egui_material_icons::icons::{
    ICON_ADD, ICON_ALL_INCLUSIVE, ICON_BLOCK, ICON_CLOSE, ICON_CONTENT_COPY, ICON_MORE_HORIZ, ICON_OPEN_IN_NEW,
};
use rubiyomi_core::cards::CardState;
use rubiyomi_core::finder::Word;
use rubiyomi_core::grade::Grader;
use rubiyomi_core::theme::Scheme;

use crate::ui::m3::{self, c};

// Below this width the card spans the screen: narrower, it looks like a tooltip.
const STRETCH_BELOW: f32 = 480.0;
const MAX_WIDTH: f32 = 300.0;
const MAX_HEIGHT_FRACTION: f32 = 0.4;
const GAP: f32 = 8.0;

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
    pub anchor: Rect,
    pub span: Range<usize>,
    pub content: Content,
    pub sentence: String,
    pub state: Option<CardState>,
    pub busy: bool,
    pub message: Option<String>,
    // The card's other face: what can be done with the word.
    options: bool,
    // Unknown until the card has been laid out once; until then it is drawn invisibly.
    size: Option<Vec2>,
    shown_at: Option<f64>,
}

fn width_for(screen: f32) -> f32 {
    if screen < STRETCH_BELOW { screen - GAP * 2.0 } else { MAX_WIDTH }
}

// Over what hasn't been read yet: under the line, from the word on, or above it
// when there is no room.
fn place(screen: Rect, card: Vec2, anchor: Rect) -> Pos2 {
    let x = anchor.left().clamp(screen.left() + GAP, (screen.right() - card.x - GAP).max(screen.left() + GAP));
    let mut y = anchor.bottom() + GAP;
    if y + card.y > screen.bottom() - GAP {
        y = anchor.top() - GAP - card.y;
    }
    pos2(x, y.clamp(screen.top() + GAP, (screen.bottom() - card.y - GAP).max(screen.top() + GAP)))
}

// Whatever the grade, its button takes the colour of the state it leads to.
fn grade_state(value: u8, count: usize) -> CardState {
    let order: &[CardState] = if count >= 5 {
        &[CardState::Failed, CardState::Due, CardState::Learning, CardState::Known, CardState::New]
    } else {
        &[CardState::Failed, CardState::Learning, CardState::Known, CardState::New]
    };
    order[(value.max(1) as usize - 1).min(order.len() - 1)]
}

// Only a word in a deck has a card to grade.
fn has_card(state: Option<CardState>) -> bool {
    !matches!(state, None | Some(CardState::NotInDeck | CardState::Locked | CardState::Blacklisted | CardState::Unknown))
}

impl Popup {
    pub fn new(anchor: Rect, span: Range<usize>, content: Content) -> Self {
        Popup {
            anchor,
            span,
            content,
            sentence: String::new(),
            state: None,
            busy: false,
            message: None,
            options: false,
            size: None,
            shown_at: None,
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, s: &Scheme, grader: Option<&dyn Grader>) -> Option<Action> {
        let screen = ctx.content_rect();
        let width = width_for(screen.width()).min(screen.width());
        let at = place(screen, self.size.unwrap_or(vec2(width, 0.0)), self.anchor);
        let now = ctx.input(|i| i.time);
        let opacity = match (self.size, self.shown_at) {
            (None, _) => 0.0,
            (Some(_), None) => {
                self.shown_at = Some(now);
                0.0
            }
            (Some(_), Some(t)) => ((now - t) / 0.12).clamp(0.0, 1.0) as f32,
        };
        if opacity < 1.0 {
            ctx.request_repaint();
        }
        let dark = ctx.theme() == egui::Theme::Dark;
        let mut action = None;
        let response = egui::Area::new(egui::Id::new("word-popup")).order(egui::Order::Foreground).fixed_pos(at).show(ctx, |ui| {
            egui::Frame::new()
                .fill(c(s.surface_container_high))
                .corner_radius(CornerRadius::same(12))
                .shadow(egui::Shadow { offset: [0, 3], blur: 10, spread: 3, color: Color32::from_black_alpha(170) })
                .inner_margin(egui::Margin { left: 14, right: 8, top: 8, bottom: 8 })
                .show(ui, |ui| {
                    ui.multiply_opacity(opacity);
                    ui.set_width(width - 22.0);
                    ui.set_max_height(screen.height() * MAX_HEIGHT_FRACTION - 16.0);
                    action = self.body(ui, s, grader, dark);
                });
        });
        let size = response.response.rect.size();
        if self.size.is_none_or(|old| (size - old).length() > 0.5) {
            self.size = Some(size);
            ctx.request_repaint();
        }
        action
    }

    fn body(&mut self, ui: &mut egui::Ui, s: &Scheme, grader: Option<&dyn Grader>, dark: bool) -> Option<Action> {
        let word = match &self.content {
            Content::Message(m) => {
                ui.label(RichText::new(m).size(13.0).color(c(s.on_surface_variant)));
                return None;
            }
            Content::Word(w) => w.clone(),
        };
        ui.horizontal(|ui| {
            let w = ui.available_width() - 32.0;
            ui.allocate_ui_with_layout(vec2(w, 0.0), Layout::top_down(Align::Min), |ui| {
                ui.set_width(w);
                ui.spacing_mut().item_spacing.y = 0.0;
                if !word.reading.is_empty() && word.reading != word.spelling {
                    ui.add(egui::Label::new(RichText::new(&word.reading).size(13.0).color(c(s.on_surface_variant))).selectable(true));
                }
                ui.add(egui::Label::new(RichText::new(&word.spelling).size(30.0).color(c(s.on_surface))).selectable(true));
            });
            let icon = if self.options { ICON_CLOSE } else { ICON_MORE_HORIZ };
            if small_icon_button(ui, s, icon).on_hover_text(if self.options { "Back" } else { "Options" }).clicked() {
                self.options = !self.options;
            }
        });
        let state = self.state.filter(|_| grader.is_some());
        let kinds = pos_labels(&word.pos);
        if !self.options && (state.is_some() || !kinds.is_empty()) {
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                if let Some(st) = state {
                    state_chip(ui, st, state_colour(st, dark));
                }
                for k in &kinds {
                    pill(ui, s, k);
                }
            });
        }
        if self.busy {
            ui.add_space(6.0);
            progress_bar(ui, s);
            ui.add_space(6.0);
        }
        if let Some(m) = &self.message {
            ui.add_space(6.0);
            ui.label(RichText::new(m).size(12.0).color(c(s.on_surface_variant)));
        }
        ui.add_space(6.0);
        let mut action = None;
        egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
            ui.set_width(ui.available_width() - 6.0);
            if self.options {
                action = self.options_face(ui, s, grader, &word, dark);
            } else {
                meanings(ui, s, &word);
            }
        });
        action
    }

    fn options_face(&self, ui: &mut egui::Ui, s: &Scheme, grader: Option<&dyn Grader>, word: &Word, dark: bool) -> Option<Action> {
        let mut action = None;
        if option_tile(ui, s, ICON_CONTENT_COPY, "Copy", true) {
            action = Some(Action::Copy(word.spelling.clone()));
        }
        // jpdb.io's vocabulary ids are JMdict's.
        if let Some(id) = word.jmdict_id
            && option_tile(ui, s, ICON_OPEN_IN_NEW, "Open on jpdb.io", true)
        {
            action = Some(Action::Open(format!("https://jpdb.io/vocabulary/{id}/{}", urlencode(&word.spelling))));
        }
        let Some(g) = grader.filter(|_| word.jmdict_id.is_some()) else { return action };
        if let Some(deck) = g.deck_name()
            && option_tile(ui, s, ICON_ADD, &format!("Add to {deck}"), !self.busy)
        {
            action = Some(Action::Add);
        }
        for &a in g.actions() {
            let icon = if a.contains("lacklist") { ICON_BLOCK } else { ICON_ALL_INCLUSIVE };
            if option_tile(ui, s, icon, a, !self.busy) {
                action = Some(Action::Act(a));
            }
        }
        if has_card(self.state) {
            ui.add_space(8.0);
            let grades = g.grades();
            let gap = 4.0;
            let n = grades.len() as f32;
            let bw = (ui.available_width() - gap * (n - 1.0)) / n;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for b in grades {
                    let colour = state_colour(grade_state(b.value, grades.len()), dark);
                    if grade_button(ui, b.label, colour, bw, !self.busy) {
                        action = Some(Action::Grade(b.value));
                    }
                }
            });
        }
        action
    }
}

fn meanings(ui: &mut egui::Ui, s: &Scheme, word: &Word) {
    if word.meanings.is_empty() {
        ui.label(RichText::new("Not in the dictionary.").size(13.0).color(c(s.on_surface_variant)));
        return;
    }
    for (i, meaning) in word.meanings.iter().enumerate() {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let (r, _) = ui.allocate_exact_size(vec2(20.0, 18.0), Sense::hover());
            ui.painter().text(
                r.left_top() + vec2(0.0, 1.0),
                egui::Align2::LEFT_TOP,
                format!("{}.", i + 1),
                egui::FontId::proportional(13.0),
                c(s.on_surface_variant),
            );
            ui.add(
                egui::Label::new(RichText::new(meaning).size(14.0).color(c(s.on_surface)).line_height(Some(19.6))).wrap().selectable(true),
            );
        });
        ui.add_space(4.0);
    }
}

// Every state has its colour; only a state not known yet has none.
pub fn state_colour(state: CardState, dark: bool) -> Color32 {
    let v = match state {
        CardState::Known => [0x2e7d32, 0x69c67a],
        CardState::NeverForget => [0x00695c, 0x4db6ac],
        CardState::Learning => [0x9a6a00, 0xe7b95a],
        CardState::Due => [0xb35309, 0xef9a55],
        CardState::Failed => [0xc62828, 0xe06c6c],
        CardState::New => [0x1565c0, 0x6ba8e8],
        CardState::NotInDeck => [0x6b7280, 0x8a93a3],
        CardState::Locked => [0x5e35b1, 0x9b8ec4],
        CardState::Redundant => [0x78909c, 0x7f8896],
        CardState::Suspended => [0x90a4ae, 0x6d737d],
        CardState::Blacklisted => [0xb0bec5, 0x585e68],
        CardState::Unknown => return Color32::TRANSPARENT,
    }[dark as usize];
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

fn state_chip(ui: &mut egui::Ui, state: CardState, colour: Color32) {
    let galley = ui.painter().layout_no_wrap(state.label().to_owned(), egui::FontId::proportional(10.0), colour);
    let (rect, _) = ui.allocate_exact_size(galley.size() + vec2(12.0, 4.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(255), m3::alpha(colour, 0.16));
    ui.painter().rect_stroke(rect, CornerRadius::same(255), Stroke::new(1.0, m3::alpha(colour, 0.5)), StrokeKind::Inside);
    ui.painter().galley(rect.min + vec2(6.0, 2.0), galley, colour);
}

fn pill(ui: &mut egui::Ui, s: &Scheme, label: &str) {
    let ink = c(s.on_surface_variant);
    let galley = ui.painter().layout_no_wrap(label.to_owned(), egui::FontId::proportional(10.0), ink);
    let (rect, _) = ui.allocate_exact_size(galley.size() + vec2(12.0, 4.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(255), m3::alpha(c(s.outline_variant), 0.6));
    ui.painter().galley(rect.min + vec2(6.0, 2.0), galley, ink);
}

fn progress_bar(ui: &mut egui::Ui, s: &Scheme) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 2.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::ZERO, c(s.primary_container));
    let t = ui.input(|i| i.time) as f32;
    let span = rect.width() * 0.3;
    let x = rect.left() + (t * 0.8).fract() * (rect.width() + span) - span;
    let bar = Rect::from_min_max(pos2(x.max(rect.left()), rect.top()), pos2((x + span).min(rect.right()), rect.bottom()));
    ui.painter().rect_filled(bar, CornerRadius::ZERO, c(s.primary));
    ui.ctx().request_repaint();
}

fn small_icon_button(ui: &mut egui::Ui, s: &Scheme, icon: MaterialIcon) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
    ui.painter().circle_filled(rect.center(), 16.0, m3::alpha(c(s.on_surface), m3::state_layer(&response)));
    m3::paint_icon(ui, icon, rect.center(), 18.0, c(s.on_surface_variant));
    response
}

fn option_tile(ui: &mut egui::Ui, s: &Scheme, icon: MaterialIcon, label: &str, enabled: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), if enabled { Sense::click() } else { Sense::hover() });
    if enabled && response.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(6), m3::alpha(c(s.on_surface), 0.08));
    }
    let ink = if enabled { c(s.on_surface) } else { c(s.on_surface_variant) };
    m3::paint_icon(ui, icon, pos2(rect.left() + 13.0, rect.center().y), 18.0, ink);
    ui.painter().text(pos2(rect.left() + 32.0, rect.center().y), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(13.0), ink);
    enabled && response.clicked()
}

fn grade_button(ui: &mut egui::Ui, label: &str, colour: Color32, width: f32, enabled: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 34.0), if enabled { Sense::click() } else { Sense::hover() });
    let radius = CornerRadius::same(4);
    ui.painter().rect_filled(rect, radius, m3::alpha(colour, if enabled { 0.14 } else { 0.06 }));
    if enabled && response.hovered() {
        ui.painter().rect_filled(rect, radius, m3::alpha(colour, 0.08));
    }
    ui.painter().rect_stroke(rect, radius, Stroke::new(1.0, m3::alpha(colour, 0.45)), StrokeKind::Inside);
    let ink = if enabled { colour } else { m3::alpha(colour, 0.5) };
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(12.0), ink);
    enabled && response.clicked()
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
