// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

mod canvas;
mod popup;

use std::collections::HashMap;
use std::sync::Arc;

use egui::{Align2, FontFamily, Sense, pos2, vec2};
use egui_material_icons::icons::{ICON_ARROW_BACK, ICON_LIST};
use rubiyomi_core::cards::CardState;
use rubiyomi_core::finder::Word;
use rubiyomi_core::grade::{GradeError, StateMap};
use rubiyomi_core::library::{BookMeta, Library, Progress, fsutil::now};
use rubiyomi_core::reader::layout::{self, LayoutBlock, LayoutConfig, Page, page_for_offset};
use rubiyomi_core::reader::runs::{Run, book_runs};
use rubiyomi_core::text::{Block, Chapter};

use super::Ctx;
use crate::task::{self, Task};
use crate::ui::fonts::BOOK_SERIF;
use crate::ui::m3::{self, c, font, text};
use canvas::{EguiMeasure, PaintOptions, paint_page, to_screen};
use popup::{Action, Content, Popup, state_colour};

const FONT_SIZE: f32 = 24.0;
const LINE_HEIGHT: f32 = 1.8;
const MARGIN: f32 = 24.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Turn {
    Next,
    Previous,
}

type Reviewed = (Word, Action, Result<CardState, GradeError>);

struct Colours {
    index: usize,
    generation: u64,
    // Sorted by start, in chapter offsets.
    words: Arc<Vec<Word>>,
    states: StateMap,
    asked: bool,
}

pub struct ReaderScreen {
    meta: BookMeta,
    index: usize,
    chapter: Chapter,
    runs: Vec<Vec<Run>>,
    pages: Vec<Page>,
    page: usize,
    laid_out: Option<LayoutConfig>,
    want_offset: Option<usize>,
    want_last: bool,
    save_after_layout: bool,
    bar: bool,
    chapters: bool,
    scroll_to_current: bool,
    popup: Option<Popup>,
    images: HashMap<String, egui::TextureHandle>,
    decoding: Vec<Task<(String, Option<egui::ColorImage>)>>,
    saved: (usize, usize),
    drag: egui::Vec2,
    colours: Option<Colours>,
    colouring: Option<Task<Colours>>,
    asking: Option<Task<(usize, StateMap)>>,
    grading: Option<Task<Reviewed>>,
}

impl ReaderScreen {
    pub fn open(library: &Library, meta: BookMeta) -> Result<Self, String> {
        let progress = library.progress(&meta.id);
        let index = (progress.chapter as usize).min(meta.chapters.len().saturating_sub(1));
        let chapter = library.chapter(&meta.id, index).ok_or("This book's files are damaged. Remove it and add it again.")?;
        let mut reader = ReaderScreen {
            meta,
            index,
            chapter,
            runs: Vec::new(),
            pages: Vec::new(),
            page: 0,
            laid_out: None,
            want_offset: Some(progress.offset as usize),
            want_last: false,
            save_after_layout: false,
            bar: false,
            chapters: false,
            scroll_to_current: false,
            popup: None,
            images: HashMap::new(),
            decoding: Vec::new(),
            saved: (index, progress.offset as usize),
            drag: egui::Vec2::ZERO,
            colours: None,
            colouring: None,
            asking: None,
            grading: None,
        };
        reader.runs = reader.chapter_runs();
        Ok(reader)
    }

    fn offset(&self) -> usize {
        self.pages.get(self.page).map_or(0, |p| p.start)
    }

    fn chapter_runs(&self) -> Vec<Vec<Run>> {
        self.chapter.blocks.iter().map(|b| b.as_text().map(book_runs).unwrap_or_default()).collect()
    }

    fn load_chapter(&mut self, library: &Library, index: usize) -> bool {
        let Some(chapter) = library.chapter(&self.meta.id, index) else { return false };
        self.popup = None;
        self.index = index;
        self.chapter = chapter;
        self.runs = self.chapter_runs();
        self.laid_out = None;
        self.page = 0;
        true
    }

    fn go_to_chapter(&mut self, library: &Library, index: usize) {
        if self.load_chapter(library, index) {
            self.want_offset = Some(0);
            self.save(library);
        }
    }

    fn turn(&mut self, library: &Library, turn: Turn) {
        self.popup = None;
        match turn {
            Turn::Next if self.page + 1 < self.pages.len() => self.page += 1,
            Turn::Next if self.index + 1 < self.meta.chapters.len() => {
                if self.load_chapter(library, self.index + 1) {
                    self.want_offset = Some(0);
                }
            }
            Turn::Previous if self.page > 0 => self.page -= 1,
            Turn::Previous if self.index > 0 => {
                if self.load_chapter(library, self.index - 1) {
                    // Its last page is known only once it is laid out.
                    self.want_last = true;
                    self.save_after_layout = true;
                    return;
                }
            }
            _ => return,
        }
        self.save(library);
    }

    fn save(&mut self, library: &Library) {
        let offset = self.want_offset.unwrap_or_else(|| self.offset());
        if self.saved == (self.index, offset) {
            return;
        }
        self.saved = (self.index, offset);
        let progress = Progress {
            chapter: self.index as u32,
            offset: offset as u32,
            fraction: self.meta.fraction_at(self.index, offset),
            updated: now(),
        };
        if let Err(e) = library.save_progress(&self.meta.id, &progress) {
            log::warn!("saving progress: {e}");
        }
    }

    fn ensure_layout(&mut self, ctx: &egui::Context, area: egui::Rect) {
        let config = LayoutConfig {
            vertical: false,
            paginated: true,
            width: (area.width() - 2.0 * MARGIN).max(40.0),
            height: (area.height() - 2.0 * MARGIN).max(40.0),
            font_size: FONT_SIZE,
            line_height: LINE_HEIGHT,
            show_ruby: true,
            ruby_scale: 0.5,
            max_chars_per_line: 0,
            heading_scale: 1.25,
        };
        if self.laid_out == Some(config) {
            return;
        }
        let keep = self.want_offset.take().or_else(|| self.laid_out.map(|_| self.offset()));
        let blocks: Vec<LayoutBlock<'_>> = self
            .chapter
            .blocks
            .iter()
            .enumerate()
            .scan(0usize, |start, (i, b)| {
                let here = *start;
                *start += b.char_len();
                Some(match b {
                    Block::Text(t) => LayoutBlock::Text { block: i, start: here, text: &t.text, runs: &self.runs[i], heading: t.heading },
                    Block::Image(img) => LayoutBlock::Image { block: i, start: here, width: img.width, height: img.height },
                })
            })
            .collect();
        let measure = EguiMeasure::new(ctx, FontFamily::Name(BOOK_SERIF.into()));
        self.pages = layout::layout(&blocks, &config, &measure);
        if self.pages.is_empty() {
            self.pages.push(Page::default());
        }
        self.laid_out = Some(config);
        if std::mem::take(&mut self.want_last) {
            self.page = self.pages.len() - 1;
        } else if let Some(offset) = keep {
            self.page = page_for_offset(&self.pages, offset);
        }
        self.page = self.page.min(self.pages.len() - 1);
    }

    fn image(&mut self, ctx: &egui::Context, library: &Library, file: &str) -> Option<egui::TextureHandle> {
        if let Some(t) = self.images.get(file) {
            return Some(t.clone());
        }
        if !self.decoding.is_empty() {
            return None;
        }
        let path = library.image_path(&self.meta.id, file);
        let name = file.to_owned();
        self.decoding.push(Task::spawn(ctx, move || {
            let image = image::open(&path).ok().map(|img| {
                let img = if img.width().max(img.height()) > 2048 { img.thumbnail(2048, 2048) } else { img };
                let rgba = img.to_rgba8();
                egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw())
            });
            (name, image)
        }));
        None
    }

    fn poll_images(&mut self, ctx: &egui::Context) {
        for (name, image) in crate::task::drain(&mut self.decoding) {
            if let Some(image) = image {
                let texture = ctx.load_texture(format!("img-{name}"), image, egui::TextureOptions::LINEAR);
                self.images.insert(name, texture);
            }
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, cx: &mut Ctx<'_>) -> bool {
        let ctx = ui.ctx().clone();
        let s = cx.scheme;
        self.poll_images(&ctx);
        let inset = m3::insets(&ctx);
        let full = ui.max_rect();
        let area = egui::Rect::from_min_max(
            full.min + vec2(inset.left as f32, inset.top as f32),
            full.max - vec2(inset.right as f32, inset.bottom as f32),
        );
        ui.painter().rect_filled(full, 0.0, c(s.surface));
        self.ensure_layout(&ctx, area);
        if std::mem::take(&mut self.save_after_layout) {
            self.save(cx.library);
        }
        self.colour(&ctx, cx);
        self.ask_fresh(&ctx, cx);
        self.poll_grading(&ctx, cx);
        self.page_ui(ui, cx, area);

        let mut close = false;
        if let Some(popup) = &mut self.popup
            && let Some(action) = popup.ui(&ctx, s, cx.grader.map(|g| &**g))
        {
            self.review(&ctx, cx, action);
        }
        if self.chapters {
            if let Some(i) = self.chapter_list(&ctx, cx) {
                self.go_to_chapter(cx.library, i);
                self.chapters = false;
                self.bar = false;
            }
        } else if self.bar {
            close = self.bars(&ctx, cx);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::BrowserBack) || i.key_pressed(egui::Key::Escape)) {
            if self.popup.is_some() {
                self.popup = None;
            } else if self.chapters {
                self.chapters = false;
            } else if self.bar {
                self.bar = false;
            } else {
                close = true;
            }
        } else if !self.chapters {
            self.keys(&ctx, cx.library);
        }
        if close {
            self.save(cx.library);
        }
        close
    }

    fn keys(&mut self, ctx: &egui::Context, library: &Library) {
        let (next, prev) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::ArrowRight) || i.key_pressed(egui::Key::PageDown) || i.key_pressed(egui::Key::Space),
                i.key_pressed(egui::Key::ArrowLeft) || i.key_pressed(egui::Key::PageUp),
            )
        });
        if next {
            self.turn(library, Turn::Next);
        }
        if prev {
            self.turn(library, Turn::Previous);
        }
    }

    fn page_ui(&mut self, ui: &mut egui::Ui, cx: &mut Ctx<'_>, area: egui::Rect) {
        let s = cx.scheme;
        let ctx = ui.ctx().clone();
        let response = ui.interact(area, ui.id().with("page"), Sense::click_and_drag());
        let origin = area.min + vec2(MARGIN, MARGIN);
        if let Some(page) = self.pages.get(self.page).cloned() {
            for img in &page.images {
                let file = match self.chapter.blocks.get(img.block) {
                    Some(Block::Image(i)) => i.file.clone(),
                    _ => continue,
                };
                let rect = to_screen(origin, &img.rect);
                match self.image(&ctx, cx.library, &file) {
                    Some(t) => {
                        egui::Image::new(&t).paint_at(ui, rect);
                    }
                    None => {
                        ui.painter().rect_filled(rect, 8.0, m3::alpha(c(s.on_surface_variant), 0.15));
                    }
                }
            }
            if let Some(popup) = &self.popup {
                // One rectangle per line the word spans.
                let mut lines: Vec<egui::Rect> = Vec::new();
                for r in page.glyphs.iter().filter(|g| popup.span.contains(&g.offset)).map(|g| to_screen(origin, &g.rect)) {
                    match lines.last_mut() {
                        Some(line) if (line.center().y - r.center().y).abs() < r.height() / 2.0 => *line = line.union(r),
                        _ => lines.push(r),
                    }
                }
                for line in lines {
                    ui.painter().rect_filled(line.expand(1.0), 0.0, m3::alpha(c(s.primary), 0.22));
                }
            }
            let options =
                PaintOptions { family: FontFamily::Name(BOOK_SERIF.into()), text: c(s.on_surface), ruby: m3::alpha(c(s.on_surface), 0.8) };
            let dark = ctx.theme() == egui::Theme::Dark;
            let colours = self.colours.as_ref().filter(|c| c.index == self.index);
            let ink = |offset: usize| {
                let c = colours?;
                let w = &c.words[c.words.partition_point(|w| w.start <= offset).checked_sub(1)?];
                (offset < w.start + w.len).then_some(())?;
                Some(state_colour(*c.states.get(&w.key())?, dark)).filter(|c| *c != egui::Color32::TRANSPARENT)
            };
            paint_page(ui.painter(), origin, &page, &options, ink);
        }
        if self.pages.len() > 1 && !self.bar {
            ui.painter().text(
                pos2(area.center().x, area.bottom() - MARGIN / 2.0),
                Align2::CENTER_CENTER,
                format!("{} / {}", self.page + 1, self.pages.len()),
                font(11.0),
                c(s.on_surface_variant),
            );
        }
        if self.chapters {
            return;
        }
        if response.dragged() {
            self.drag += response.drag_delta();
        }
        if response.drag_stopped() {
            let total = std::mem::take(&mut self.drag);
            if total.x.abs() > 48.0 && total.x.abs() > total.y.abs() {
                self.turn(cx.library, if total.x < 0.0 { Turn::Next } else { Turn::Previous });
            }
        } else if response.clicked()
            && let Some(p) = response.interact_pointer_pos()
        {
            let third = area.width() / 3.0;
            if self.popup.is_some() {
                self.popup = None;
            } else if !self.bar
                && let Some(offset) = self.glyph_at(p - origin)
            {
                self.look_up(cx, origin, offset);
            } else if self.bar || (p.x >= area.left() + third && p.x <= area.right() - third) {
                self.bar = !self.bar;
            } else {
                self.turn(cx.library, if p.x > area.right() - third { Turn::Next } else { Turn::Previous });
            }
        }
    }

    fn glyph_at(&self, at: egui::Vec2) -> Option<usize> {
        let page = self.pages.get(self.page)?;
        let hit = |r: &layout::Rect| r.contains(at.x, at.y);
        if let Some(g) = page.glyphs.iter().find(|g| hit(&g.rect) && !g.text.trim().is_empty()) {
            return Some(g.offset);
        }
        // Furigana stands for the kanji under it.
        let ruby = page.ruby.iter().find(|r| hit(&r.rect))?;
        page.glyphs.iter().find(|g| g.block == ruby.block && g.run == ruby.run).map(|g| g.offset)
    }

    fn look_up(&mut self, cx: &Ctx<'_>, origin: egui::Pos2, offset: usize) {
        let (block, local) = self.chapter.locate(offset);
        let Some(text) = self.chapter.blocks.get(block).and_then(Block::as_text) else { return };
        let block_start = self.chapter.block_start(block);
        let found = match cx.finder {
            Ok(finder) => finder.word_at(&text.text, local),
            Err(note) => {
                let rect = self.span_rect(origin, offset..offset + 1);
                self.popup = Some(Popup::new(rect, offset..offset + 1, Content::Message(note.to_owned())));
                return;
            }
        };
        let Some(word) = found else { return };
        let span = block_start + word.start..block_start + word.start + word.len;
        let rect = self.span_rect(origin, span.clone());
        let mut popup = Popup::new(rect, span, Content::Word(word.clone()));
        popup.sentence = sentence(&text.text, word.start);
        popup.state = cx.grader.and_then(|g| g.states(std::slice::from_ref(&word)).get(&word.key()).copied());
        popup.busy = self.grading.is_some();
        self.popup = Some(popup);
    }

    // Finding the words of a chapter takes a moment, so it runs aside; the
    // states are asked again whenever the grader says they changed.
    fn colour(&mut self, ctx: &egui::Context, cx: &Ctx<'_>) {
        if let Some(done) = task::take(&mut self.colouring) {
            self.colours = Some(done);
        }
        let (Some(grader), Ok(finder)) = (cx.grader, cx.finder) else {
            self.colours = None;
            return;
        };
        let fresh = self.colours.as_ref().is_some_and(|c| c.index == self.index && c.generation == cx.generation);
        if fresh || self.colouring.is_some() {
            return;
        }
        let (grader, finder, index, generation) = (grader.clone(), finder.clone(), self.index, cx.generation);
        let known = self.colours.as_ref().filter(|c| c.index == index).map(|c| c.words.clone());
        let texts: Vec<(usize, String)> = if known.is_some() {
            Vec::new()
        } else {
            let mut start = 0;
            let mut texts = Vec::new();
            for b in &self.chapter.blocks {
                if let Some(t) = b.as_text() {
                    texts.push((start, t.text.clone()));
                }
                start += b.char_len();
            }
            texts
        };
        self.colouring = Some(Task::spawn(ctx, move || {
            let words = known.unwrap_or_else(|| {
                let mut all = Vec::new();
                for (start, text) in &texts {
                    all.extend(finder.words(text).into_iter().map(|mut w| {
                        w.start += start;
                        w
                    }));
                }
                Arc::new(all)
            });
            let states = grader.states(&words);
            Colours { index, generation, words, states, asked: false }
        }));
    }

    // The saved states show at once; fresh ones follow when the reviews service answers.
    fn ask_fresh(&mut self, ctx: &egui::Context, cx: &Ctx<'_>) {
        if let Some((index, states)) = task::take(&mut self.asking)
            && let Some(c) = self.colours.as_mut().filter(|c| c.index == index)
        {
            c.states = states;
        }
        let Some(grader) = cx.grader else { return };
        let Some(c) = self.colours.as_mut().filter(|c| c.index == self.index && !c.asked) else { return };
        if self.asking.is_some() {
            return;
        }
        c.asked = true;
        let (grader, words, index) = (grader.clone(), c.words.clone(), c.index);
        self.asking = Some(Task::spawn(ctx, move || {
            if let Err(e) = grader.refresh(&words) {
                log::warn!("asking for card states: {e}");
            }
            (index, grader.states(&words))
        }));
    }

    fn review(&mut self, ctx: &egui::Context, cx: &mut Ctx<'_>, action: Action) {
        match &action {
            Action::Open(url) => {
                if !cx.platform.open_url(url)
                    && let Some(p) = &mut self.popup
                {
                    p.message = Some("No browser could be opened.".into());
                }
                return;
            }
            Action::Copy(text) => {
                if !cx.platform.copy_text(text) {
                    ctx.copy_text(text.clone());
                }
                if let Some(p) = &mut self.popup {
                    p.message = Some("Copied.".into());
                }
                return;
            }
            _ => {}
        }
        let (Some(grader), Some(popup)) = (cx.grader, &mut self.popup) else { return };
        let Content::Word(word) = &popup.content else { return };
        if self.grading.is_some() {
            return;
        }
        popup.busy = true;
        let (grader, word, sentence) = (grader.clone(), word.clone(), popup.sentence.clone());
        self.grading = Some(Task::spawn(ctx, move || {
            let result = match &action {
                Action::Grade(value) => grader.grade(&word, *value, &sentence),
                Action::Add => grader.add(&word, &sentence),
                Action::Act(what) => grader.act(&word, what, &sentence),
                Action::Open(_) | Action::Copy(_) => unreachable!("handled before"),
            };
            (word, action, result)
        }));
    }

    fn poll_grading(&mut self, ctx: &egui::Context, cx: &mut Ctx<'_>) {
        let Some((word, action, result)) = task::take(&mut self.grading) else { return };
        let key = word.key();
        let said = match result {
            Ok(state) => {
                if let Some(c) = &mut self.colours {
                    c.states.insert(key.clone(), state);
                }
                if let Some(p) = self.popup.as_mut().filter(|p| matches!(&p.content, Content::Word(w) if w.key() == key)) {
                    p.state = Some(state);
                }
                let grade = |v: u8| cx.grader.and_then(|g| g.grades().iter().find(|b| b.value == v)).map_or("", |b| b.label);
                match action {
                    Action::Grade(v) => format!("Graded {}.", grade(v).to_lowercase()),
                    Action::Add => format!("Added to {}.", cx.grader.and_then(|g| g.deck_name()).unwrap_or_default()),
                    Action::Act(what) => format!("{what}: done."),
                    Action::Open(_) | Action::Copy(_) => String::new(),
                }
            }
            Err(e) => format!("{}: {e}", cx.grader.map_or("Reviews", |g| g.name())),
        };
        match self.popup.as_mut().filter(|p| matches!(&p.content, Content::Word(w) if w.key() == key)) {
            Some(p) => {
                p.busy = false;
                p.message = Some(said);
            }
            None => cx.snackbar.show(ctx, said),
        }
    }

    fn span_rect(&self, origin: egui::Pos2, span: std::ops::Range<usize>) -> egui::Rect {
        let Some(page) = self.pages.get(self.page) else { return egui::Rect::NOTHING };
        page.glyphs
            .iter()
            .filter(|g| span.contains(&g.offset))
            .map(|g| to_screen(origin, &g.rect))
            .reduce(|a, b| a.union(b))
            .unwrap_or(egui::Rect::NOTHING)
    }

    fn bars(&mut self, ctx: &egui::Context, cx: &mut Ctx<'_>) -> bool {
        let s = cx.scheme;
        let inset = m3::insets(ctx);
        let screen = ctx.content_rect();
        let mut close = false;
        egui::Area::new(egui::Id::new("reader-top-bar")).order(egui::Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
            egui::Frame::new()
                .fill(c(s.surface_container))
                .inner_margin(egui::Margin { left: inset.left + 4, right: inset.right + 4, top: inset.top, bottom: 0 })
                .show(ui, |ui| {
                    ui.set_width(screen.width() - (inset.left + inset.right + 8) as f32);
                    ui.set_height(64.0);
                    ui.horizontal_centered(|ui| {
                        if m3::icon_button(ui, s, ICON_ARROW_BACK, c(s.on_surface)).on_hover_text("Library").clicked() {
                            close = true;
                        }
                        let right = 48.0;
                        ui.vertical(|ui| {
                            ui.set_width((ui.available_width() - right).max(40.0));
                            ui.add_space(10.0);
                            ui.add(egui::Label::new(egui::RichText::new(&self.meta.title).font(font(text::TITLE_MEDIUM))).truncate());
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&self.chapter.title).font(font(text::BODY_SMALL)).color(c(s.on_surface_variant)),
                                )
                                .truncate(),
                            );
                        });
                        if m3::icon_button(ui, s, ICON_LIST, c(s.on_surface)).on_hover_text("Chapters").clicked() {
                            self.chapters = true;
                            self.scroll_to_current = true;
                        }
                    });
                });
        });
        let percent = (self.meta.fraction_at(self.index, self.offset()) * 100.0).floor();
        let progress = format!(
            "Chapter {} of {} · page {} of {} · {percent:.0}%",
            self.index + 1,
            self.meta.chapters.len(),
            self.page + 1,
            self.pages.len()
        );
        let height = 48.0 + inset.bottom as f32;
        egui::Area::new(egui::Id::new("reader-bottom-bar"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos2(screen.left(), screen.bottom() - height))
            .show(ctx, |ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(screen.width(), height), Sense::hover());
                ui.painter().rect_filled(rect, 0.0, c(s.surface_container));
                ui.painter().text(
                    pos2(rect.center().x, rect.top() + 24.0),
                    Align2::CENTER_CENTER,
                    progress,
                    font(text::BODY_MEDIUM),
                    c(s.on_surface_variant),
                );
            });
        close
    }

    fn chapter_list(&mut self, ctx: &egui::Context, cx: &mut Ctx<'_>) -> Option<usize> {
        let s = cx.scheme;
        let mut picked = None;
        let modal = egui::Modal::new(egui::Id::new("chapters")).show(ctx, |ui| {
            let screen = ctx.content_rect();
            ui.set_width(400.0_f32.min(screen.width() - 48.0));
            ui.label(egui::RichText::new("Chapters").font(font(text::HEADLINE_SMALL)));
            ui.add_space(8.0);
            egui::ScrollArea::vertical().max_height(screen.height() * 0.7).show(ui, |ui| {
                for (i, ch) in self.meta.chapters.iter().enumerate() {
                    let title = if ch.title.trim().is_empty() { format!("Chapter {}", i + 1) } else { ch.title.clone() };
                    let percent = (self.meta.fraction_at(i, 0) * 100.0).floor();
                    let row = m3::row(ui, s, &title, &format!("{percent:.0}%"), i == self.index);
                    if i == self.index && std::mem::take(&mut self.scroll_to_current) {
                        row.scroll_to_me(Some(egui::Align::Center));
                    }
                    if row.clicked() {
                        picked = Some(i);
                    }
                }
            });
        });
        if modal.should_close() {
            self.chapters = false;
        }
        picked
    }
}

// The sentence around a word, for the card.
fn sentence(text: &str, at: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    let ends = |c: char| matches!(c, '。' | '！' | '？' | '!' | '?' | '\n');
    let start = chars[..at.min(chars.len())].iter().rposition(|&c| ends(c)).map_or(0, |i| i + 1);
    let mut end = chars[at.min(chars.len())..].iter().position(|&c| ends(c)).map_or(chars.len(), |i| at + i + 1);
    while end < chars.len() && matches!(chars[end], '」' | '』' | '）' | ')') {
        end += 1;
    }
    chars[start..end].iter().collect::<String>().trim().to_owned()
}
