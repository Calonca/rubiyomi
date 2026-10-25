// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::cell::RefCell;
use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;

use egui::epaint::TextShape;
use egui::{Align2, Color32, FontFamily, FontId, Painter, Pos2, vec2};
use rubiyomi_core::reader::layout::{Form, Measure, Page, PlacedGlyph, Rect};

pub struct EguiMeasure {
    ctx: egui::Context,
    family: FontFamily,
    cache: RefCell<HashMap<(char, u32), f32>>,
}

impl EguiMeasure {
    pub fn new(ctx: &egui::Context, family: FontFamily) -> Self {
        EguiMeasure { ctx: ctx.clone(), family, cache: RefCell::new(HashMap::new()) }
    }
}

impl Measure for EguiMeasure {
    fn advance(&self, text: &str, size: f32) -> f32 {
        let font = FontId::new(size, self.family.clone());
        let mut cache = self.cache.borrow_mut();
        let mut total = 0.0;
        for ch in text.chars() {
            let key = (ch, size.to_bits());
            let w = *cache.entry(key).or_insert_with(|| self.ctx.fonts_mut(|f| f.glyph_width(&font, ch)));
            total += w;
        }
        total
    }
}

pub fn to_screen(origin: Pos2, r: &Rect) -> egui::Rect {
    egui::Rect::from_min_size(origin + vec2(r.x, r.y), vec2(r.w, r.h))
}

pub struct PaintOptions {
    pub family: FontFamily,
    pub text: Color32,
    pub ruby: Color32,
}

pub fn paint_page(painter: &Painter, origin: Pos2, page: &Page, o: &PaintOptions) {
    for g in &page.glyphs {
        paint_glyph(painter, origin, g, &o.family, o.text);
    }
    for r in &page.ruby {
        let rect = to_screen(origin, &r.rect);
        painter.text(rect.center(), Align2::CENTER_CENTER, &r.text, FontId::new(r.size, o.family.clone()), o.ruby);
    }
}

fn paint_glyph(painter: &Painter, origin: Pos2, g: &PlacedGlyph, family: &FontFamily, color: Color32) {
    if g.text.trim().is_empty() {
        return;
    }
    let rect = to_screen(origin, &g.rect);
    match g.form {
        Form::Upright => {
            let galley = painter.layout_no_wrap(g.text.clone(), FontId::new(g.size, family.clone()), color);
            painter.galley(rect.center() - galley.size() / 2.0, galley, color);
        }
        Form::Rotated => {
            let galley = painter.layout_no_wrap(g.text.clone(), FontId::new(g.size, family.clone()), color);
            let pos = rect.center() - galley.size() / 2.0;
            painter.add(TextShape::new(pos, galley, color).with_angle_and_anchor(FRAC_PI_2, Align2::CENTER_CENTER));
        }
        Form::Corner => {
            // Horizontal fonts put 、。 at the bottom left; a column wants them at the top right.
            let galley = painter.layout_no_wrap(g.text.clone(), FontId::new(g.size, family.clone()), color);
            let shift = vec2(g.size * 0.55, -g.size * 0.55);
            painter.galley(rect.center() - galley.size() / 2.0 + shift, galley, color);
        }
        Form::SmallKana => {
            let galley = painter.layout_no_wrap(g.text.clone(), FontId::new(g.size, family.clone()), color);
            let shift = vec2(g.size * 0.1, -g.size * 0.1);
            painter.galley(rect.center() - galley.size() / 2.0 + shift, galley, color);
        }
        Form::Combined => {
            let mut size = g.size;
            let mut galley = painter.layout_no_wrap(g.text.clone(), FontId::new(size, family.clone()), color);
            if galley.size().x > g.size {
                size *= g.size / galley.size().x;
                galley = painter.layout_no_wrap(g.text.clone(), FontId::new(size, family.clone()), color);
            }
            painter.galley(rect.center() - galley.size() / 2.0, galley, color);
        }
    }
}
