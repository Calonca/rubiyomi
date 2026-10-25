// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use egui::RichText;
use egui_material_icons::icons::{
    ICON_AUTO_STORIES, ICON_DESCRIPTION, ICON_DICTIONARY, ICON_GAVEL, ICON_INFO, ICON_PALETTE, ICON_SCHOOL, ICON_TRANSLATE,
};
use rubiyomi_core::dict::DictVersion;
use rubiyomi_core::theme::Scheme;

use crate::ui::m3::{self, c, font, text};

const IPADIC_NOTICE: &str = include_str!("../../notices/ipadic.txt");

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Page {
    #[default]
    Main,
    About,
    Dictionary,
    WordFinder,
}

#[derive(Default)]
pub struct SettingsScreen {
    page: Page,
}

impl SettingsScreen {
    pub fn back(&mut self) -> bool {
        self.page = match self.page {
            Page::Main => return false,
            Page::About => Page::Main,
            Page::Dictionary | Page::WordFinder => Page::About,
        };
        true
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, s: &Scheme, dict: Option<DictVersion>) {
        let title = match self.page {
            Page::Main => "Settings",
            Page::About => "About",
            Page::Dictionary => "Dictionary",
            Page::WordFinder => "Word finder",
        };
        if m3::top_app_bar(ui, s, title, self.page != Page::Main) {
            self.back();
        }
        egui::CentralPanel::default().frame(super::surface(s)).show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| match self.page {
                Page::Main => self.main(ui, s),
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

    fn main(&mut self, ui: &mut egui::Ui, s: &Scheme) {
        m3::section_header(ui, s, "Reading");
        m3::list_item(ui, s, ICON_AUTO_STORIES, "Page", "Font size, line spacing, vertical or horizontal text", false);
        m3::list_item(ui, s, ICON_TRANSLATE, "Parsing", "How words are found in the text", false);
        m3::section_header(ui, s, "Words");
        if m3::list_item(ui, s, ICON_DICTIONARY, "Dictionary", "JMdict, English, on this device", true).clicked() {
            self.page = Page::Dictionary;
        }
        m3::list_item(ui, s, ICON_SCHOOL, "Reviews", "Send words you look up to jpdb.io or Anki", false);
        m3::section_header(ui, s, "App");
        m3::list_item(ui, s, ICON_PALETTE, "Appearance", "Colours and theme", false);
        if m3::list_item(ui, s, ICON_INFO, "About", concat!("Rubiyomi ", env!("CARGO_PKG_VERSION"), " · dictionary and licences"), true)
            .clicked()
        {
            self.page = Page::About;
        }
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
