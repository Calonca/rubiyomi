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
use rubiyomi_core::finder::{Finder, Word};
use rubiyomi_core::grade::{GradeError, StateMap};
use rubiyomi_core::history::Place;
use rubiyomi_core::jpdb::JpdbClient;
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
const SLIDE: f64 = 0.22;
// Faster than this, pages are being flipped through, not read.
const SKIMMING_CPM: f64 = 1500.0;
const SKIMMING_PAGE: f64 = 1.5;
// After this long without a flip, a run of skimmed pages is settled.
const SKIM_SETTLES: f64 = 3.0;
// Up to this many pages are a quick flip, dropped without asking.
const SKIM_QUIET: u32 = 2;

// Pages flipped through too fast to have been read, held back from the stats until the
// reader says whether they count.
#[derive(Default)]
struct Skim {
    day: Day,
    seconds: f64,
    pages: u32,
    last: f64,
}

// The page left behind, sliding out while the new one slides in.
struct Slide {
    from: Page,
    chapter: usize,
    start: f64,
    forward: bool,
    // How far a swipe had already moved it.
    from_progress: f32,
}

// A swipe let go before it turned the page, going back into place.
struct Snap {
    start: f64,
    dx: f32,
}

fn ease(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

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
    // What the words were found with, so a change of finder finds them again.
    tag: Option<String>,
    // Sorted by start, in chapter offsets.
    words: Arc<Vec<Word>>,
    states: StateMap,
    asked: bool,
    // When the colours came, to fade them in; long ago for saved ones.
    shown: f64,
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
    slide: Option<Slide>,
    snap: Option<Snap>,
    prefetched: HashSet<usize>,
    // When the page in view came, to tell reading it from flipping past it.
    page_since: f64,
    skim: Option<Skim>,
    skim_question: Option<Skim>,
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
            slide: None,
            snap: None,
            prefetched: HashSet::new(),
            page_since: 0.0,
            skim: None,
            skim_question: None,
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
        let from = self.pages.get(self.page).cloned();
        let chapter = self.index;
        let read = from.as_ref().map_or(Day::default(), |page| self.page_read(page));
        let gap = self.active();
        let moved = match turn {
            Turn::Next if self.page + 1 < self.pages.len() => {
                self.page += 1;
                true
            }
            Turn::Next if self.index + 1 < self.meta.chapters.len() => {
                let loaded = self.load_chapter(library, self.index + 1);
                if loaded {
                    self.want_offset = Some(0);
                }
                loaded
            }
            Turn::Previous if self.page > 0 => {
                self.page -= 1;
                true
            }
            Turn::Previous if self.index > 0 => {
                let loaded = self.load_chapter(library, self.index - 1);
                if loaded {
                    // Its last page is known only once it is laid out.
                    self.want_last = true;
                    self.save_after_layout = true;
                }
                loaded
            }
            _ => false,
        };
        if turn == Turn::Next && moved {
            let mut read = read;
            if gap.is_some() {
                read.timed_chars = read.chars;
            }
            read.chapters = (self.index != chapter) as u32;
            let on_page = self.now - self.page_since;
            let skimmed = read.chars > 0 && (on_page < SKIMMING_PAGE || read.chars as f64 * 60.0 / on_page > SKIMMING_CPM);
            if skimmed {
                let skim = self.skim.get_or_insert_with(Skim::default);
                skim.day.add(read);
                if let Some(gap) = gap {
                    self.seconds -= gap;
                    skim.seconds += gap;
                }
                skim.pages += 1;
                skim.last = self.now;
            } else {
                self.settle_skim();
                self.pending.add(read);
            }
        }
        if moved {
            self.page_since = self.now;
        }
        self.flush(cx);
        if !moved {
            return;
        }
        self.popup = None;
        self.mark = None;
        self.slide = from.map(|from| Slide { from, chapter, start: self.now, forward: turn == Turn::Next, from_progress: 0.0 });
        if !self.save_after_layout {
            self.save(library);
        }
    }

    // Time counts from one touch of the page to the next, unless it was left alone too long.
    // Gives the time counted, if the clock was running since the last touch.
    fn active(&mut self) -> Option<f64> {
        let gap = self.last_active.map(|last| (self.now - last).max(0.0)).filter(|gap| *gap < IDLE);
        if let Some(gap) = gap {
            self.seconds += gap;
        }
        self.last_active = Some(self.now);
        gap
    }

    // A run of skimmed pages is over: a quick flip is dropped, a longer one asked about.
    fn settle_skim(&mut self) {
        let Some(skim) = self.skim.take() else { return };
        if skim.pages > SKIM_QUIET && self.skim_question.is_none() {
            self.skim_question = Some(skim);
        }
    }

    fn skim_dialog(&mut self, ctx: &egui::Context, cx: &mut Ctx<'_>, s: &Scheme) {
        let Some(skim) = &self.skim_question else { return };
        let mut answer = None;
        let frame = egui::Frame::new().fill(c(s.surface_container_high)).corner_radius(28).inner_margin(egui::Margin::same(24));
        let seconds = skim.seconds.round() as u64;
        let time = if seconds < 60 { format!("{seconds} s") } else { format!("{} min {} s", seconds / 60, seconds % 60) };
        let body = format!(
            "{} pages, {} characters, went by in {time}: faster than reading. Count them in your stats, or leave them out and measure from now?",
            skim.pages, skim.day.chars
        );
        egui::Modal::new(egui::Id::new("skimmed")).frame(frame).show(ctx, |ui| {
            m3::show_next_frame(ui);
            ui.set_width(320.0_f32.min(ctx.content_rect().width() - 48.0));
            ui.label(egui::RichText::new("Skimmed?").font(font(text::HEADLINE_SMALL)).color(c(s.on_surface)));
            ui.add_space(8.0);
            ui.label(egui::RichText::new(body).font(font(text::BODY_MEDIUM)).color(c(s.on_surface_variant)));
            ui.add_space(16.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if m3::text_button(ui, s, "Count them").clicked() {
                    answer = Some(true);
                }
                if m3::text_button(ui, s, "Leave out").clicked() {
                    answer = Some(false);
                }
            });
        });
        match answer {
            Some(true) => {
                if let Some(skim) = self.skim_question.take() {
                    self.pending.add(skim.day);
                    self.seconds += skim.seconds;
                    self.flush(cx);
                }
            }
            Some(false) => {
                self.skim_question = None;
                self.last_active = Some(self.now);
                self.page_since = self.now;
            }
            None => {}
        }
    }

    // What reading a page adds to the stats: its characters, and its words with how many are unknown.
    fn page_read(&self, page: &Page) -> Day {
        let chars = page.glyphs.iter().filter(|g| !g.text.trim().is_empty()).map(|g| g.len.max(1) as u64).sum::<u64>();
        let mut day = Day { chars, ..Day::default() };
        if let Some(c) = self.colours.as_ref().filter(|c| c.index == self.index)
            && let (Some(lo), Some(hi)) = (page.glyphs.first().map(|g| g.offset), page.glyphs.last().map(|g| g.offset))
        {
            for w in c.words[c.words.partition_point(|w| w.start < lo)..].iter().take_while(|w| w.start <= hi) {
                day.words += 1;
                if !c.states.get(&w.key()).is_some_and(|s| s.is_known()) {
                    day.unknown += 1;
                }
            }
        }
        day
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
            height: (area.height() - p.margin - if p.show_position { p.margin.max(FOOTER) } else { p.margin }).max(40.0),
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
            self.page_since = self.now;
        }
        if let Some(skim) = &self.skim {
            let left = SKIM_SETTLES - (self.now - skim.last);
            if left <= 0.0 {
                self.settle_skim();
            } else {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(left));
            }
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
        if self.skim_question.is_some() {
            self.skim_dialog(&ctx, cx, s);
        } else if self.sheet {
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
        } else if !self.chapters && !self.sheet && self.skim_question.is_none() {
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
            m3::scroll().max_height(screen.height() * 0.75).show(ui, |ui| {
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
        let rest = area.min + vec2(p.margin, p.margin);
        let thickness = (p.font_size * 0.055).max(1.0);
        let mut origin = rest;
        let width = area.width();
        if let Some(slide) = &self.slide {
            let from = slide.from_progress;
            let t = ((self.now - slide.start) / (SLIDE * (1.0 - from as f64)).max(0.02)).clamp(0.0, 1.0) as f32;
            let progress = from + (1.0 - from) * ease(t);
            // Columns are read right to left, so a vertical book turns the other way.
            let dir = if slide.forward != p.vertical { -1.0 } else { 1.0 };
            origin = rest + vec2(-dir * (1.0 - progress) * width, 0.0);
            let old = rest + vec2(dir * progress * width, 0.0);
            self.paint_plain(ui, s, &p, &slide.from, slide.chapter, old);
            if t >= 1.0 {
                self.slide = None;
            } else {
                ctx.request_repaint();
            }
        } else {
            // The page follows the finger, with the one beside it coming in.
            let dx = if let Some(snap) = &self.snap {
                let t = ((self.now - snap.start) / SLIDE).clamp(0.0, 1.0) as f32;
                let dx = snap.dx * (1.0 - ease(t));
                if t >= 1.0 {
                    self.snap = None;
                } else {
                    ctx.request_repaint();
                }
                dx
            } else if response.dragged() && self.drag.x.abs() > self.drag.y.abs() {
                (self.drag.x + response.drag_delta().x).clamp(-width, width)
            } else {
                0.0
            };
            if dx != 0.0 {
                origin = rest + vec2(dx, 0.0);
                let beside = if (dx < 0.0) != p.vertical { self.page.checked_add(1) } else { self.page.checked_sub(1) };
                if let Some(page) = beside.and_then(|i| self.pages.get(i)).cloned() {
                    let at = rest + vec2(dx + if dx < 0.0 { width } else { -width }, 0.0);
                    self.paint_plain(ui, s, &p, &page, self.index, at);
                }
            }
        }
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
            let fade = colours.map_or(1.0, |c| ((self.now - c.shown) / 0.15).clamp(0.0, 1.0) as f32);
            if fade < 1.0 {
                ctx.request_repaint();
            }
            let text = c(s.on_surface);
            let colour_of = |w: &Word| -> Option<egui::Color32> {
                let colour = state_colour(*colours?.states.get(&w.key())?, s.dark);
                (colour != egui::Color32::TRANSPARENT).then(|| m3::blend(text, colour, fade))
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
        if !self.bar && p.show_position {
            let percent = (self.meta.fraction_at(self.index, self.offset()) * 100.0).floor();
            let label = format!("{} / {} · {percent:.0}%", self.page + 1, self.pages.len());
            ui.painter().text(
                pos2(area.center().x, area.bottom() - p.margin.max(FOOTER) / 2.0),
                Align2::CENTER_CENTER,
                label,
                font(11.0),
                c(s.on_surface_variant),
            );
        }
        if self.chapters || self.sheet || self.skim_question.is_some() {
            return;
        }
        if response.dragged() {
            self.drag += response.drag_delta();
        }
        if response.drag_stopped() {
            let total = std::mem::take(&mut self.drag);
            let sideways = total.x.abs() > total.y.abs();
            let dx = total.x.clamp(-width, width);
            if sideways && dx.abs() > 48.0 {
                let forward = if p.vertical { dx > 0.0 } else { dx < 0.0 };
                self.turn(cx, if forward { Turn::Next } else { Turn::Previous });
                match &mut self.slide {
                    Some(slide) if slide.start == self.now => slide.from_progress = (dx.abs() / width).min(0.95),
                    _ => self.snap = Some(Snap { start: self.now, dx }),
                }
            } else if sideways && dx != 0.0 {
                self.snap = Some(Snap { start: self.now, dx });
            }
        } else if response.clicked()
            && let Some(p) = response.interact_pointer_pos()
        {
            let third = area.width() / 3.0;
            let edge = (p.x < area.left() + third) || (p.x > area.right() - third);
            self.active();
            // Another word tapped with the card open takes the card over; the same word,
            // or anywhere else, closes it.
            let tapped = if self.bar { None } else { self.glyph_at(p - rest) };
            let same = tapped.is_some_and(|o| self.popup.as_ref().is_some_and(|c| c.span.contains(&o)));
            if let Some(offset) = tapped.filter(|_| !same) {
                self.look_up(cx, rest, offset);
            } else if self.popup.is_some() {
                self.popup = None;
            } else if self.bar || !edge || !cx.page.tap_to_turn {
                self.bar = !self.bar;
            } else {
                let right = p.x > area.right() - third;
                let forward = if cx.page.vertical { !right } else { right };
                self.turn(cx, if forward { Turn::Next } else { Turn::Previous });
            }
        }
    }

    // A page without its colours or marks, while it moves.
    fn paint_plain(&self, ui: &egui::Ui, s: &Scheme, p: &PageSettings, page: &Page, chapter: usize, origin: egui::Pos2) {
        if chapter == self.index {
            for img in &page.images {
                if let Some(Block::Image(i)) = self.chapter.blocks.get(img.block)
                    && let Some(t) = self.images.get(&i.file)
                {
                    egui::Image::new(t).paint_at(ui, to_screen(origin, &img.rect));
                }
            }
        }
        let options = PaintOptions { family: family(p.typeface), text: c(s.on_surface), ruby: m3::alpha(c(s.on_surface), 0.8) };
        let faint = |block: usize, run: usize| p.furigana == Furigana::Blur && !self.revealed.contains(&(block, run));
        paint_page(ui.painter(), origin, page, &options, |_| None, faint);
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
        let parsed = self.colours.as_ref().filter(|c| cx.parser.is_some() && c.index == self.index && c.tag.as_deref() == cx.words_tag);
        let parsed = parsed.and_then(|c| {
            let w = &c.words[..c.words.partition_point(|w| w.start <= offset)];
            w.last()
                .filter(|w| offset < w.start + w.len && w.start >= block_start)
                .map(|w| Word { start: w.start - block_start, ..w.clone() })
        });
        let found = match (parsed, cx.finder) {
            (Some(word), _) => Some(word),
            (None, Ok(finder)) => finder.word_at(&text.text, local),
            (None, Err(note)) => {
                let rect = self.span_rect(origin, offset..offset + 1);
                self.popup = Some(Popup::new(rect, offset..offset + 1, Content::Message(note.to_owned())));
                return;
            }
        };
        let Some(word) = found else { return };
        let span = block_start + word.start..block_start + word.start + word.len;
        let word_rect = self.span_rect(origin, span.clone());
        // Across the lines the card keeps clear of the whole sentence, so it can be read
        // with the card open; along them it stays at the word, as the sentence's last
        // piece often starts at the head of a line far from it.
        let around = sentence_range(&text.text, word.start);
        let sentence_rect = self.span_rect(origin, block_start + around.start..block_start + around.end);
        let rect = match (sentence_rect.is_positive(), cx.page.vertical) {
            (false, _) => word_rect,
            (true, true) => {
                egui::Rect::from_min_max(pos2(sentence_rect.left(), word_rect.top()), pos2(sentence_rect.right(), word_rect.bottom()))
            }
            (true, false) => {
                egui::Rect::from_min_max(pos2(word_rect.left(), sentence_rect.top()), pos2(word_rect.right(), sentence_rect.bottom()))
            }
        };
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
        if let Some(old) = &self.popup {
            popup.continue_from(old);
        }
        self.mark = Some(popup.span.clone());
        self.popup = Some(popup);
        self.save(cx.library);
    }

    // A chapter's words are saved once found, so a chapter seen before is coloured on its
    // first frame; finding them takes a moment, so it runs aside, and the next chapter's
    // are found ahead. The states are asked again whenever the grader says they changed.
    fn colour(&mut self, ctx: &egui::Context, cx: &Ctx<'_>) {
        if let Some(mut done) = task::take(&mut self.colouring) {
            if let Some(old) = self.colours.as_ref().filter(|c| c.index == done.index) {
                done.shown = old.shown;
            } else {
                done.shown = self.now;
            }
            self.colours = Some(done);
        }
        // Words found by jpdb.io are wanted for looking up too, so they're found without a grader.
        if cx.grader.is_none() && cx.parser.is_none() {
            self.colours = None;
            return;
        }
        let grader = cx.grader.cloned();
        let tag = cx.words_tag.map(str::to_owned);
        let same = |c: &Colours| c.index == self.index && c.tag == tag;
        let fresh = self.colours.as_ref().is_some_and(|c| same(c) && c.generation == cx.generation);
        if fresh || self.colouring.is_some() {
            if fresh {
                self.prefetch(cx);
            }
            return;
        }
        let (index, generation) = (self.index, cx.generation);
        let known = self.colours.as_ref().filter(|c| same(c)).map(|c| c.words.clone());
        if known.is_none()
            && let Some(t) = &tag
            && let Some(words) = cx.library.cached_words(&self.meta.id, index, t)
        {
            let words = Arc::new(words);
            let states = grader.map(|g| g.states(&words)).unwrap_or_default();
            self.colours = Some(Colours { index, generation, tag, words, states, asked: false, shown: f64::NEG_INFINITY });
            return;
        }
        let finder = cx.finder.ok().cloned();
        if known.is_none() && finder.is_none() && cx.parser.is_none() {
            return;
        }
        let save = tag.clone().map(|tag| (cx.library.clone(), self.meta.id.clone(), tag));
        let parser = cx.parser.cloned();
        let chapter = if known.is_some() { None } else { Some(self.chapter.clone()) };
        self.colouring = Some(Task::spawn(ctx, move || {
            let words = known.unwrap_or_else(|| {
                let chapter = chapter.as_ref().expect("a chapter to split");
                let (words, keep) = find_words(chapter, finder.as_deref(), parser.as_deref());
                if keep && let Some((library, id, tag)) = &save {
                    library.cache_words(id, index, tag, &words);
                }
                Arc::new(words)
            });
            let states = grader.map(|g| g.states(&words)).unwrap_or_default();
            Colours { index, generation, tag, words, states, asked: false, shown: 0.0 }
        }));
    }

    // Finds and saves the next chapter's words while this one is read.
    fn prefetch(&mut self, cx: &Ctx<'_>) {
        let next = self.index + 1;
        if next >= self.meta.chapters.len() || !self.prefetched.insert(next) {
            return;
        }
        let finder = cx.finder.ok().cloned();
        let parser = cx.parser.cloned();
        let Some(tag) = cx.words_tag.filter(|_| finder.is_some() || parser.is_some()) else {
            self.prefetched.remove(&next);
            return;
        };
        if cx.library.cached_words(&self.meta.id, next, tag).is_some() {
            return;
        }
        let (library, id, tag) = (cx.library.clone(), self.meta.id.clone(), tag.to_owned());
        std::thread::spawn(move || {
            if let Some(chapter) = library.chapter(&id, next)
                && let (words, true) = find_words(&chapter, finder.as_deref(), parser.as_deref())
            {
                library.cache_words(&id, next, &tag, &words);
            }
        });
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
        let book = self.meta.fraction_at(self.index, self.offset());
        let chapter = (self.page + 1) as f32 / self.pages.len().max(1) as f32;
        let rows = [
            (
                format!("Chapter {} of {}", self.index + 1, self.meta.chapters.len()),
                format!("page {} of {}", self.page + 1, self.pages.len()),
                chapter,
            ),
            ("Book".to_owned(), format!("{:.0}%", (book * 100.0).floor()), book),
        ];
        let height = 16.0 + 40.0 * rows.len() as f32 + inset.bottom as f32;
        egui::Area::new(egui::Id::new("reader-bottom-bar"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos2(screen.left(), screen.bottom() - height))
            .show(ctx, |ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(screen.width(), height), Sense::hover());
                let p = ui.painter();
                p.rect_filled(rect, 0.0, c(s.surface_container));
                let left = rect.left() + 24.0 + inset.left as f32;
                let right = rect.right() - 24.0 - inset.right as f32;
                for (i, (name, figure, fraction)) in rows.iter().enumerate() {
                    let top = rect.top() + 12.0 + 40.0 * i as f32;
                    p.text(pos2(left, top), Align2::LEFT_TOP, name, font(text::BODY_MEDIUM), c(s.on_surface));
                    p.text(pos2(right, top), Align2::RIGHT_TOP, figure, font(text::BODY_MEDIUM), c(s.on_surface_variant));
                    m3::progress(p, s, egui::Rect::from_min_max(pos2(left, top + 24.0), pos2(right, top + 28.0)), *fraction);
                }
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
            m3::scroll().auto_shrink([false, false]).min_scrolled_height(height).max_height(height).show(ui, |ui| {
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

// Where the sentence around a word starts and ends, in the block's chars.
fn sentence_range(text: &str, at: usize) -> Range<usize> {
    let chars: Vec<char> = text.chars().collect();
    let ends = |c: char| matches!(c, '。' | '！' | '？' | '!' | '?' | '\n');
    let start = chars[..at.min(chars.len())].iter().rposition(|&c| ends(c)).map_or(0, |i| i + 1);
    let mut end = chars[at.min(chars.len())..].iter().position(|&c| ends(c)).map_or(chars.len(), |i| at + i + 1);
    while end < chars.len() && matches!(chars[end], '」' | '』' | '）' | ')') {
        end += 1;
    }
    start..end
}

// The sentence around a word, for the card.
fn sentence(text: &str, at: usize) -> String {
    let range = sentence_range(text, at);
    text.chars().skip(range.start).take(range.len()).collect::<String>().trim().to_owned()
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

// A chapter's words, from jpdb.io when it parses, else from the finder on the device; and
// whether they may be saved, which words found by the fallback may not.
fn find_words(chapter: &Chapter, finder: Option<&Finder>, parser: Option<&JpdbClient>) -> (Vec<Word>, bool) {
    if let Some(parser) = parser {
        match parser.chapter_words(chapter) {
            Ok(words) => return (words, true),
            Err(e) => log::warn!("jpdb.io couldn't parse the chapter: {e}"),
        }
        return (finder.map(|f| f.chapter_words(chapter)).unwrap_or_default(), false);
    }
    (finder.map(|f| f.chapter_words(chapter)).unwrap_or_default(), true)
}
