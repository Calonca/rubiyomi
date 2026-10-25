// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::ops::Range;

use egui::{CornerRadius, RichText};
use rubiyomi_core::finder::Word;
use rubiyomi_core::theme::Scheme;

use crate::ui::m3::{self, c, font, text};

pub enum Content {
    Word(Word),
    Message(String),
}

pub struct Popup {
    pub anchor: egui::Rect,
    pub span: Range<usize>,
    pub content: Content,
    height: f32,
}

impl Popup {
    pub fn new(anchor: egui::Rect, span: Range<usize>, content: Content) -> Self {
        Popup { anchor, span, content, height: 200.0 }
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

    pub fn ui(&mut self, ctx: &egui::Context, s: &Scheme) {
        let screen = ctx.content_rect();
        let width = (screen.width() - 16.0).min(380.0);
        let pos = self.place(screen, width);
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
                        Content::Word(word) => word_face(ui, s, word),
                    }
                });
        });
        self.height = area.response.rect.height();
    }
}

fn word_face(ui: &mut egui::Ui, s: &Scheme, word: &Word) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(&word.spelling).font(font(text::HEADLINE_SMALL)).color(c(s.on_surface)));
        if word.reading != word.spelling && !word.reading.is_empty() {
            ui.label(RichText::new(&word.reading).font(font(text::TITLE_MEDIUM)).color(c(s.on_surface_variant)));
        }
    });
    ui.add_space(4.0);
    if word.meanings.is_empty() {
        ui.label(RichText::new("Not in the dictionary.").color(c(s.on_surface_variant)));
        return;
    }
    egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
        for (i, m) in word.meanings.iter().enumerate() {
            ui.label(RichText::new(format!("{}. {m}", i + 1)).font(font(text::BODY_MEDIUM)).color(c(s.on_surface)));
        }
    });
}
