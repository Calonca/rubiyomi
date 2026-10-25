// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use egui::RichText;
use rubiyomi_core::platform::Platform;
use rubiyomi_core::settings::{Parser, ReviewSource};
use rubiyomi_core::theme::Scheme;

use super::reviews_form::{ReviewsForm, hint};
use crate::reviews::Reviews;
use crate::ui::m3::{self, c, font, text};

// Shown once, on the first launch; everything in it is in Settings too.
pub struct Setup {
    step: usize,
    form: ReviewsForm,
}

impl Setup {
    pub fn new(reviews: &Reviews) -> Self {
        Setup { step: 0, form: ReviewsForm::new(&reviews.settings) }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, s: &Scheme, platform: &dyn Platform, reviews: &mut Reviews) -> bool {
        let ctx = ui.ctx().clone();
        let inset = m3::insets(&ctx);
        let mut done = false;
        let frame = super::surface(s).inner_margin(egui::Margin {
            left: inset.left + 24,
            right: inset.right + 24,
            top: inset.top + 32,
            bottom: inset.bottom + 16,
        });
        egui::CentralPanel::default().frame(frame).show(ui, |ui| {
            m3::scroll().show(ui, |ui| {
                ui.set_width((ui.available_width() - 12.0).min(560.0));
                match self.step {
                    0 => self.welcome(ui, s),
                    _ => done = self.reviews(ui, s, platform, reviews),
                }
            });
        });
        if ctx.input(|i| i.key_pressed(egui::Key::BrowserBack)) {
            if self.step > 0 {
                self.step -= 1;
            } else {
                platform.to_background();
            }
        }
        done
    }

    fn welcome(&mut self, ui: &mut egui::Ui, s: &Scheme) {
        ui.label(RichText::new("Welcome to Rubiyomi").font(font(text::HEADLINE_SMALL)).color(c(s.on_surface)));
        ui.add_space(16.0);
        ui.label(
            RichText::new(
                "Read Japanese books with their furigana, and tap any word to see what it means. The dictionary is on this device, so it works offline.",
            )
            .font(font(text::BODY_LARGE))
            .color(c(s.on_surface)),
        );
        ui.add_space(12.0);
        hint(ui, s, "First, a few choices. All of them can be changed later in Settings.");
        ui.add_space(32.0);
        if m3::filled_button(ui, s, "Get started").clicked() {
            self.step = 1;
        }
    }

    fn reviews(&mut self, ui: &mut egui::Ui, s: &Scheme, platform: &dyn Platform, reviews: &mut Reviews) -> bool {
        ui.label(RichText::new("Reviews").font(font(text::HEADLINE_SMALL)).color(c(s.on_surface)));
        ui.add_space(4.0);
        hint(ui, s, "Do you study with jpdb.io or Anki?");
        ui.add_space(16.0);
        self.form.ui(ui, s, platform);
        if self.form.draft.reviews == ReviewSource::Jpdb {
            ui.add_space(12.0);
            hint(ui, s, "jpdb.io will also find the words in the text, as its own reader does. You can change that in Settings → Parsing.");
        }
        ui.add_space(24.0);
        let mut done = false;
        ui.horizontal(|ui| {
            if m3::text_button(ui, s, "Back").clicked() {
                self.step = 0;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = if self.form.draft.reviews == ReviewSource::None { "Finish" } else { "Save and finish" };
                if m3::filled_button(ui, s, label).clicked() {
                    let mut chosen = self.form.draft.clone();
                    chosen.setup_done = true;
                    if chosen.reviews == ReviewSource::Jpdb {
                        chosen.parser = Parser::Jpdb;
                    }
                    reviews.apply(ui.ctx(), platform, chosen);
                    done = true;
                }
            });
        });
        done
    }
}
