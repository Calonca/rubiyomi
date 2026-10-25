// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

mod canvas;
mod popup;

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::Arc;

use egui::{Align2, FontFamily, Sense, pos2, vec2};
use egui_material_icons::icons::{ICON_ARROW_BACK, ICON_LIST, ICON_TEXT_FIELDS};
use rubiyomi_core::cards::CardState;
use rubiyomi_core::finder::Word;
use rubiyomi_core::grade::{GradeError, StateMap};
use rubiyomi_core::history::Place;
use rubiyomi_core::library::{BookMeta, Library, Progress, fsutil::now};
use rubiyomi_core::reader::layout::{self, LayoutBlock, LayoutConfig, Page, page_for_offset};
use rubiyomi_core::reader::runs::{Run, book_runs};
use rubiyomi_core::settings::{Furigana, PageSettings, Typeface, WordColours};
use rubiyomi_core::stats::{Day, today};
use rubiyomi_core::text::{Block, Chapter};
use rubiyomi_core::theme::Scheme;

use super::Ctx;
use crate::task::{self, Task};
use crate::ui::fonts::{BOOK_SANS, BOOK_SERIF};
use crate::ui::m3::{self, c, font, text};
use crate::ui::page;
use canvas::{EguiMeasure, PaintOptions, paint_page, to_screen};
pub use popup::state_colour;
use popup::{Action, Content, Popup};

// Room under the page for its number, whatever the margins.
const FOOTER: f32 = 28.0;
// Longer than this without touching the page is not reading.
const IDLE: f64 = 180.0;

fn family(t: Typeface) -> FontFamily {
    FontFamily::Name(
        match t {
            Typeface::Serif => BOOK_SERIF,
            Typeface::Sans => BOOK_SANS,
        }
        .into(),
    )
}

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
    laid_out: Option<(LayoutConfig, Typeface)>,
    want_offset: Option<usize>,
    want_last: bool,
    save_after_layout: bool,
    bar: bool,
    chapters: bool,
    scroll_to_current: bool,
    popup: Option<Popup>,
    images: HashMap<String, egui::TextureHandle>,
    decoding: Vec<Task<(String, Option<egui::ColorImage>)>>,
    saved: (usize, usize, Option<Range<usize>>),
    // Where the last look-up was, marked until the page turns.
    mark: Option<Range<usize>>,
    drag: egui::Vec2,
    colours: Option<Colours>,
    colouring: Option<Task<Colours>>,
    asking: Option<Task<(usize, StateMap)>>,
    grading: Option<Task<Reviewed>>,
    sheet: bool,
    // Furigana of the words looked up, shown when the rest is faint.
    revealed: HashSet<(usize, usize)>,
    now: f64,
    last_active: Option<f64>,
    // Reading done since the stats were last written.
    pending: Day,
    seconds: f64,
}

impl ReaderScreen {
    // `at` opens the book at a word (chapter, offset, length) instead of where it was left.
    pub fn open(library: &Library, meta: BookMeta, at: Option<(usize, usize, usize)>) -> Result<Self, String> {
        let mut progress = library.progress(&meta.id);
        if let Some((chapter, offset, len)) = at {
            progress.chapter = chapter as u32;
            progress.offset = offset as u32;
            progress.word = Some((offset as u32, len as u32));
        }
        let index = (progress.chapter as usize).min(meta.chapters.len().saturating_sub(1));
        let mark = progress.word.filter(|_| index == progress.chapter as usize).map(|(at, len)| at as usize..(at + len) as usize);
        let chapter = library.chapter(&meta.id, index).ok_or("This book's files are damaged. Remove it and add it again.")?;
        let mut reader = ReaderScreen {
            meta,
            index,
            chapter,
            runs: Vec::new(),
            pages: Vec::new(),
            page: 0,
            laid_out: None,
            want_offset: Some(mark.as_ref().map_or(progress.offset as usize, |m| m.start)),
            want_last: false,
            save_after_layout: false,
            bar: false,
            chapters: false,
            scroll_to_current: false,
            popup: None,
            images: HashMap::new(),
            decoding: Vec::new(),
            saved: (index, progress.offset as usize, mark.clone()),
            mark,
            drag: egui::Vec2::ZERO,
            colours: None,
            colouring: None,
            asking: None,
            grading: None,
            sheet: false,
            revealed: HashSet::new(),
            now: 0.0,
            last_active: None,
            pending: Day::default(),
            seconds: 0.0,
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
        self.mark = None;
        self.revealed.clear();
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

    fn turn(&mut self, cx: &mut Ctx<'_>, turn: Turn) {
        let library = cx.library;
        self.popup = None;
        self.mark = None;
        if turn == Turn::Next
            && let Some(page) = self.pages.get(self.page)
        {
            self.pending.chars += page.glyphs.iter().filter(|g| !g.text.trim().is_empty()).map(|g| g.len.max(1) as u64).sum::<u64>();
        }
        self.active();
        self.flush(cx);
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

    // Time counts from one touch of the page to the next, unless it was left alone too long.
    fn active(&mut self) {
        if let Some(last) = self.last_active {
            let gap = self.now - last;
            if gap > 0.0 && gap < IDLE {
                self.seconds += gap;
            }
        }
        self.last_active = Some(self.now);
    }

    fn flush(&mut self, cx: &mut Ctx<'_>) {
        let mut done = std::mem::take(&mut self.pending);
        done.seconds = self.seconds.floor() as u64;
        self.seconds = self.seconds.fract();
        cx.stats.add(today(), done);
    }

    fn save(&mut self, library: &Library) {
        let offset = self.want_offset.unwrap_or_else(|| self.offset());
        let now_at = (self.index, offset, self.mark.clone());
        if self.saved == now_at {
            return;
        }
        self.saved = now_at;
        let progress = Progress {
            chapter: self.index as u32,
            offset: offset as u32,
            fraction: self.meta.fraction_at(self.index, offset),
            updated: now(),
            word: self.mark.as_ref().map(|m| (m.start as u32, m.len() as u32)),
        };
        if let Err(e) = library.save_progress(&self.meta.id, &progress) {
            log::warn!("saving progress: {e}");
        }
    }

    fn ensure_layout(&mut self, ctx: &egui::Context, area: egui::Rect, p: &PageSettings) {
        let config = LayoutConfig {
            vertical: p.vertical,
            paginated: true,
            width: (area.width() - 2.0 * p.margin).max(40.0),
            height: (area.height() - p.margin - p.margin.max(FOOTER)).max(40.0),
            font_size: p.font_size,
            line_height: p.line_height,
            show_ruby: p.furigana != Furigana::Hide,
            ruby_scale: 0.5,
            max_chars_per_line: p.max_chars_per_line,
            heading_scale: 1.25,
        };
        if self.laid_out == Some((config, p.typeface)) {
            return;
        }
        if self.laid_out.is_some() {
            self.popup = None;
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
        let measure = EguiMeasure::new(ctx, family(p.typeface));
        self.pages = layout::layout(&blocks, &config, &measure);
        if self.pages.is_empty() {
            self.pages.push(Page::default());
        }
        self.laid_out = Some((config, p.typeface));
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
        let page_scheme = page::scheme(cx.page.theme, cx.scheme);
        let s = &page_scheme;
        self.now = ctx.input(|i| i.time);
        if self.last_active.is_none() {
            self.last_active = Some(self.now);
        }
        self.poll_images(&ctx);
        let inset = m3::insets(&ctx);
        let full = ui.max_rect();
        let area = egui::Rect::from_min_max(
            full.min + vec2(inset.left as f32, inset.top as f32),
            full.max - vec2(inset.right as f32, inset.bottom as f32),
        );
        ui.painter().rect_filled(full, 0.0, c(s.surface));
        self.ensure_layout(&ctx, area, cx.page);
        if std::mem::take(&mut self.save_after_layout) {
            self.save(cx.library);
        }
        self.colour(&ctx, cx);
        self.ask_fresh(&ctx, cx);
        self.poll_grading(&ctx, cx);
        self.page_ui(ui, cx, s, area);

        let mut close = false;
        if let Some(popup) = &mut self.popup
            && let Some(action) = popup.ui(&ctx, s, cx.grader.map(|g| &**g))
        {
            self.review(&ctx, cx, action);
        }
        if self.sheet {
            self.page_sheet(&ctx, cx, s);
        } else if self.chapters {
            if let Some(i) = self.chapter_list(&ctx, s) {
                self.go_to_chapter(cx.library, i);
                self.chapters = false;
                self.bar = false;
            }
        } else if self.bar {
            close = self.bars(&ctx, s);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::BrowserBack) || i.key_pressed(egui::Key::Escape)) {
            if self.popup.is_some() {
                self.popup = None;
            } else if self.sheet {
                self.sheet = false;
            } else if self.chapters {
                self.chapters = false;
            } else if self.bar {
                self.bar = false;
            } else {
                close = true;
            }
        } else if !self.chapters && !self.sheet {
            self.keys(&ctx, cx);
        }
        if close {
            self.active();
            self.flush(cx);
            self.save(cx.library);
        }
        close
    }

    fn keys(&mut self, ctx: &egui::Context, cx: &mut Ctx<'_>) {
        let (right, left) = ctx.input(|i| (i.key_pressed(egui::Key::ArrowRight), i.key_pressed(egui::Key::ArrowLeft)));
        let (forward, back) =
            ctx.input(|i| (i.key_pressed(egui::Key::PageDown) || i.key_pressed(egui::Key::Space), i.key_pressed(egui::Key::PageUp)));
        // Columns run right to left, so the arrows swap.
        let (next, prev) = if cx.page.vertical { (left, right) } else { (right, left) };
        if next || forward {
            self.turn(cx, Turn::Next);
        }
        if prev || back {
            self.turn(cx, Turn::Previous);
        }
    }

    fn page_sheet(&mut self, ctx: &egui::Context, cx: &mut Ctx<'_>, s: &Scheme) {
        let screen = ctx.content_rect();
        let frame = egui::Frame::new().fill(c(s.surface_container_high)).corner_radius(28).inner_margin(egui::Margin::symmetric(8, 16));
        let modal = egui::Modal::new(egui::Id::new("page-settings")).frame(frame).show(ctx, |ui| {
            ui.set_width(440.0_f32.min(screen.width() - 32.0));
            ui.horizontal(|ui| {
                ui.add_space(16.0);
                ui.label(egui::RichText::new("Page").font(font(text::HEADLINE_SMALL)).color(c(s.on_surface)));
            });
            egui::ScrollArea::vertical().max_height(screen.height() * 0.75).show(ui, |ui| {
                if super::page_form::ui(ui, s, cx.page, cx.grader.is_some())
                    && let Err(e) = cx.page.save(&cx.platform.data_dir())
                {
                    log::warn!("saving the page settings: {e}");
                }
            });
        });
        if modal.should_close() {
            self.sheet = false;
        }
    }

    fn page_ui(&mut self, ui: &mut egui::Ui, cx: &mut Ctx<'_>, s: &Scheme, area: egui::Rect) {
        let ctx = ui.ctx().clone();
        let p = cx.page.clone();
        let response = ui.interact(area, ui.id().with("page"), Sense::click_and_drag());
        let origin = area.min + vec2(p.margin, p.margin);
        let thickness = (p.font_size * 0.055).max(1.0);
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
            let accent = c(s.primary);
            let selected = self.popup.as_ref().map(|p| p.span.clone());
            if let Some(mark) = self.mark.clone().filter(|m| Some(m) != selected.as_ref()) {
                for line in line_boxes(&page, origin, &mark, p.vertical) {
                    ui.painter().rect_filled(line.expand(1.0), 0.0, m3::alpha(accent, 0.12));
                    underline(ui.painter(), line, p.vertical, thickness, accent);
                }
            }
            if let Some(span) = selected {
                for line in line_boxes(&page, origin, &span, p.vertical) {
                    ui.painter().rect_filled(line.expand(1.0), 0.0, m3::alpha(accent, 0.22));
                }
            }
            let options = PaintOptions { family: family(p.typeface), text: c(s.on_surface), ruby: m3::alpha(c(s.on_surface), 0.8) };
            let colours = self.colours.as_ref().filter(|c| c.index == self.index && p.colours != WordColours::Off);
            let colour_of = |w: &Word| -> Option<egui::Color32> {
                Some(state_colour(*colours?.states.get(&w.key())?, s.dark)).filter(|c| *c != egui::Color32::TRANSPARENT)
            };
            let ink = |offset: usize| {
                let c = colours.filter(|_| p.colours == WordColours::Text)?;
                let w = &c.words[c.words.partition_point(|w| w.start <= offset).checked_sub(1)?];
                (offset < w.start + w.len).then_some(())?;
                colour_of(w)
            };
            let faint = |block: usize, run: usize| p.furigana == Furigana::Blur && !self.revealed.contains(&(block, run));
            paint_page(ui.painter(), origin, &page, &options, ink, faint);
            // Drawn a word and a line at a time, so a line under a word doesn't break between its glyphs.
            if let Some(c) = colours.filter(|_| p.colours == WordColours::Underline) {
                let lo = page.glyphs.iter().map(|g| g.offset).min().unwrap_or(0);
                let hi = page.glyphs.iter().map(|g| g.offset).max().unwrap_or(0);
                for w in c.words[c.words.partition_point(|w| w.start + w.len <= lo)..].iter().take_while(|w| w.start <= hi) {
                    let Some(colour) = colour_of(w) else { continue };
                    for line in line_boxes(&page, origin, &(w.start..w.start + w.len), p.vertical) {
                        underline(ui.painter(), line, p.vertical, thickness, colour);
                    }
                }
            }
        }
        if !self.bar {
            let percent = (self.meta.fraction_at(self.index, self.offset()) * 100.0).floor();
            let label = match (self.pages.len() > 1, p.show_percent) {
                (true, true) => format!("{} / {} · {percent:.0}%", self.page + 1, self.pages.len()),
                (true, false) => format!("{} / {}", self.page + 1, self.pages.len()),
                (false, true) => format!("{percent:.0}%"),
                (false, false) => String::new(),
            };
            ui.painter().text(
                pos2(area.center().x, area.bottom() - p.margin.max(FOOTER) / 2.0),
                Align2::CENTER_CENTER,
                label,
                font(11.0),
                c(s.on_surface_variant),
            );
        }
        if self.chapters || self.sheet {
            return;
        }
        if response.dragged() {
            self.drag += response.drag_delta();
        }
        if response.drag_stopped() {
            let total = std::mem::take(&mut self.drag);
            if total.x.abs() > 48.0 && total.x.abs() > total.y.abs() {
                let forward = if p.vertical { total.x > 0.0 } else { total.x < 0.0 };
                self.turn(cx, if forward { Turn::Next } else { Turn::Previous });
            }
        } else if response.clicked()
            && let Some(p) = response.interact_pointer_pos()
        {
            let third = area.width() / 3.0;
            let edge = (p.x < area.left() + third) || (p.x > area.right() - third);
            self.active();
            if self.popup.is_some() {
                self.popup = None;
            } else if !self.bar
                && let Some(offset) = self.glyph_at(p - origin)
            {
                self.look_up(cx, origin, offset);
            } else if self.bar || !edge || !cx.page.tap_to_turn {
                self.bar = !self.bar;
            } else {
                let right = p.x > area.right() - third;
                let forward = if cx.page.vertical { !right } else { right };
                self.turn(cx, if forward { Turn::Next } else { Turn::Previous });
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

    fn look_up(&mut self, cx: &mut Ctx<'_>, origin: egui::Pos2, offset: usize) {
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
        if let Some(page) = self.pages.get(self.page) {
            self.revealed.extend(page.glyphs.iter().filter(|g| span.contains(&g.offset)).map(|g| (g.block, g.run)));
        }
        let mut popup = Popup::new(rect, span, Content::Word(word.clone()));
        popup.vertical = cx.page.vertical;
        popup.sentence = sentence(&text.text, word.start);
        let place = Place {
            book: &self.meta.id,
            book_title: &self.meta.title,
            chapter: self.index as u32,
            offset: popup.span.start as u32,
            sentence: &popup.sentence,
        };
        cx.history.record(&word, place);
        self.pending.lookups += 1;
        popup.state = cx.grader.and_then(|g| g.states(std::slice::from_ref(&word)).get(&word.key()).copied());
        popup.busy = self.grading.is_some();
        self.mark = Some(popup.span.clone());
        self.popup = Some(popup);
        self.save(cx.library);
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

    fn bars(&mut self, ctx: &egui::Context, s: &Scheme) -> bool {
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
                        let right = 96.0;
                        ui.vertical(|ui| {
                            ui.set_width((ui.available_width() - right).max(40.0));
                            ui.add_space(10.0);
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&self.meta.title).font(font(text::TITLE_MEDIUM)).color(c(s.on_surface)),
                                )
                                .truncate(),
                            );
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&self.chapter.title).font(font(text::BODY_SMALL)).color(c(s.on_surface_variant)),
                                )
                                .truncate(),
                            );
                        });
                        if m3::icon_button(ui, s, ICON_TEXT_FIELDS, c(s.on_surface)).on_hover_text("Page").clicked() {
                            self.sheet = true;
                        }
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

    fn chapter_list(&mut self, ctx: &egui::Context, s: &Scheme) -> Option<usize> {
        let mut picked = None;
        let frame = egui::Frame::new().fill(c(s.surface_container_high)).corner_radius(28).inner_margin(egui::Margin::same(16));
        let modal = egui::Modal::new(egui::Id::new("chapters")).frame(frame).show(ctx, |ui| {
            let screen = ctx.content_rect();
            ui.set_width(400.0_f32.min(screen.width() - 48.0));
            ui.label(egui::RichText::new("Chapters").font(font(text::HEADLINE_SMALL)).color(c(s.on_surface)));
            ui.add_space(8.0);
            // A fixed height: left to itself the list measured one row on the first frame and kept it.
            let rows = self.meta.chapters.len() as f32 * (56.0 + ui.spacing().item_spacing.y);
            let height = rows.min(screen.height() * 0.7);
            egui::ScrollArea::vertical().auto_shrink([false, false]).min_scrolled_height(height).max_height(height).show(ui, |ui| {
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

// Under a line of text; left of a column, as its furigana is on the right.
fn underline(painter: &egui::Painter, line: egui::Rect, vertical: bool, thickness: f32, colour: egui::Color32) {
    let stroke = egui::Stroke::new(thickness, colour);
    if vertical {
        let x = line.left() - thickness;
        painter.line_segment([pos2(x, line.top()), pos2(x, line.bottom())], stroke);
    } else {
        let y = line.bottom() + thickness;
        painter.line_segment([pos2(line.left(), y), pos2(line.right(), y)], stroke);
    }
}

// One box per line (or column) a span of the chapter covers on this page.
fn line_boxes(page: &Page, origin: egui::Pos2, span: &Range<usize>, vertical: bool) -> Vec<egui::Rect> {
    let mut lines: Vec<egui::Rect> = Vec::new();
    let same = |a: egui::Rect, b: egui::Rect| {
        if vertical {
            (a.center().x - b.center().x).abs() < b.width() / 2.0
        } else {
            (a.center().y - b.center().y).abs() < b.height() / 2.0
        }
    };
    for r in page.glyphs.iter().filter(|g| span.contains(&g.offset)).map(|g| to_screen(origin, &g.rect)) {
        match lines.last_mut() {
            Some(line) if same(*line, r) => *line = line.union(r),
            _ => lines.push(r),
        }
    }
    lines
}
