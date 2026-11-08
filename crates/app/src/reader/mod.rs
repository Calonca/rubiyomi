// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

pub mod page;
pub mod popup;
pub mod reading;
pub mod words;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use nsh_base::{SliderValue, slider};
use nsh_core::platform::Lifecycle;
use nsh_core::prelude::*;
#[cfg(not(target_os = "android"))]
use nsh_core::{TouchEvent, TouchPhase};
use rubiyomi_core::library::{BookId, BookMeta, Progress, fsutil::now};
use rubiyomi_core::platform::Platform;
use rubiyomi_core::reader::layout::{FixedMeasure, LayoutBlock, LayoutConfig, Page, layout, page_for_offset};
use rubiyomi_core::reader::runs::{Run, book_runs};
use rubiyomi_core::settings::{Furigana, PageSettings, Typeface};
use rubiyomi_core::text::{Block, Chapter, TextBlock};
use rubiyomi_core::theme::Scheme;

use crate::app::{Route, Shared, route, shared};
use crate::ui::{Newest, Ripples, c, class_colour, fill_parent, glyph, icon, page_scheme, scrim};
use page::{NshMeasure, PageStyle, PageTap, page_view};
#[cfg(not(target_os = "android"))]
use popup::Hover;
use popup::{Popup, popup};
use reading::Reading;
use words::Words;

// Room under the page for its number, whatever the margins.
const FOOTER: f32 = 28.0;
const CHAPTER_ROW: f32 = 56.0;
// A jump that hasn't reported its row by then isn't going to.
const LANDING: Duration = Duration::from_millis(500);

// Left to fallback, Pango picks a font per script under the locale's language, and a Latin locale
// can set 「 before a kanji in a face where it is half wide.
#[cfg(not(target_os = "android"))]
fn family(typeface: Typeface) -> &'static str {
    match typeface {
        Typeface::Serif => "Noto Serif CJK JP, serif",
        Typeface::Sans => "Noto Sans CJK JP, sans-serif",
    }
}

#[cfg(target_os = "android")]
fn family(typeface: Typeface) -> &'static str {
    match typeface {
        Typeface::Serif => "serif",
        Typeface::Sans => "sans-serif",
    }
}

fn config(width: f32, height: f32, p: &PageSettings) -> LayoutConfig {
    LayoutConfig {
        vertical: p.vertical,
        paginated: true,
        width: (width - 2.0 * p.margin).max(40.0),
        height: (height - p.margin - if p.show_position { p.margin.max(FOOTER) } else { p.margin }).max(40.0),
        font_size: p.font_size,
        line_height: p.line_height,
        show_ruby: p.furigana != Furigana::Hide,
        ruby_scale: 0.5,
        max_chars_per_line: p.max_chars_per_line,
        heading_scale: 1.25,
    }
}

// Without the book's readings, the lines keep their room for the dictionary's.
fn pages_of(chapter: &Chapter, cfg: &LayoutConfig, book_ruby: bool, measure: &NshMeasure) -> Vec<Page> {
    let runs_of = |t: &TextBlock| if book_ruby { book_runs(t) } else { vec![Run { start: 0, len: t.char_len(), ..Run::default() }] };
    let runs: Vec<Vec<Run>> = chapter.blocks.iter().map(|b| b.as_text().map(runs_of).unwrap_or_default()).collect();
    let mut start = 0;
    let mut blocks = Vec::with_capacity(chapter.blocks.len());
    for (i, b) in chapter.blocks.iter().enumerate() {
        blocks.push(match b {
            Block::Text(t) => LayoutBlock::Text { block: i, start, text: &t.text, runs: &runs[i], heading: t.heading },
            Block::Image(img) => LayoutBlock::Image { block: i, start, width: img.width, height: img.height },
        });
        start += b.char_len();
    }
    let mut pages = layout(&blocks, cfg, measure);
    if pages.is_empty() {
        pages.push(Page::default());
    }
    pages
}

// What a row of the list holds. Past the last page lies the next chapter: a swipe that comes to
// rest there opens it, so turning into a chapter is the same gesture as turning a page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Slot {
    Previous,
    Page(usize),
    Next,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Slots {
    pages: usize,
    previous: bool,
    next: bool,
    // Columns are read right to left, so the last page is the first row.
    reversed: bool,
}

impl Slots {
    fn count(&self) -> usize {
        self.pages + self.previous as usize + self.next as usize
    }

    fn at(&self, row: usize) -> Option<Slot> {
        let row = if self.reversed { self.count().checked_sub(row + 1)? } else { row };
        let row = match (self.previous, row) {
            (true, 0) => return Some(Slot::Previous),
            (true, row) => row - 1,
            (false, row) => row,
        };
        if row < self.pages { Some(Slot::Page(row)) } else { (self.next && row == self.pages).then_some(Slot::Next) }
    }

    fn row_of(&self, page: usize) -> usize {
        let row = page + self.previous as usize;
        if self.reversed { self.count() - 1 - row } else { row }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Zone {
    Left,
    Middle,
    Right,
}

fn zone(x: f32, width: f32) -> Zone {
    if x < width / 4.0 {
        Zone::Left
    } else if x > width * 3.0 / 4.0 {
        Zone::Right
    } else {
        Zone::Middle
    }
}

// Where the reader is to land once the chapter is laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Place {
    Offset(usize),
    Last,
}

// A new list starts at its first row, and is only then sent to the row of the place. What it
// reports before getting there is no turn: at row 0, it would open the chapter beside.
struct Landing {
    row: usize,
    done: Cell<bool>,
}

impl Landing {
    // A list sent to row 0 is there already, and has nothing to report.
    fn new(row: usize) -> Self {
        Landing { row, done: Cell::new(row == 0) }
    }

    // Whether a report of `row` counts.
    fn reached(&self, row: usize) -> bool {
        if row == self.row {
            self.done.set(true);
        }
        self.done.get()
    }

    // The reader moved the list: wherever it was going, the place is now theirs.
    fn release(&self) {
        self.done.set(true);
    }
}

struct Laid {
    pages: Vec<Page>,
    slots: Slots,
    landing: Landing,
    cfg: LayoutConfig,
    width: f32,
    height: f32,
    first: usize,
    generation: u64,
    made_for: LaidFor,
}

// What a layout was made for, to skip a new one for the same thing. Opening a chapter counts:
// the chapter already shown, opened again from the list, has to go back to its start.
#[derive(Clone, Copy, Debug, PartialEq)]
struct LaidFor {
    chapter: usize,
    opening: u64,
    cfg: LayoutConfig,
    typeface: Typeface,
    book_ruby: bool,
}

impl Laid {
    // Where the page starts, as a chapter offset.
    fn start_of(&self, page: usize) -> usize {
        self.pages.get(page).map_or(0, |p| p.start)
    }
}

// The title a chapter is listed by, as the book names it or else by its number.
fn chapter_title(meta: &BookMeta, i: usize) -> String {
    match meta.chapters.get(i) {
        Some(c) if !c.title.trim().is_empty() => c.title.clone(),
        _ => format!("Chapter {}", i + 1),
    }
}

// The page a place lands on, and the place from then on.
fn landing(pages: &[Page], place: Place) -> (usize, Place) {
    match place {
        Place::Offset(offset) => (page_for_offset(pages, offset), place),
        Place::Last => {
            let last = pages.len() - 1;
            (last, Place::Offset(pages[last].start))
        }
    }
}

// A jump by the slider on its way to `row`.
#[derive(Clone, Copy, Debug)]
struct Jump {
    row: usize,
    at: Instant,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Layer {
    Sheet,
    Chapters,
    Popup,
    Bar,
}

// Chapter, offset and word of the last progress written.
type Saved = (usize, usize, Option<(u32, u32)>);

pub type WordTap = Rc<dyn Fn(usize, usize)>;

#[derive(Clone)]
pub struct ReaderState {
    shared: Shared,
    meta: Rc<BookMeta>,
    index: Signal<usize>,
    chapter: Signal<Arc<Chapter>>,
    size: Signal<(f32, f32)>,
    laid: Signal<Option<Rc<Laid>>>,
    page: Signal<usize>,
    bar: Signal<bool>,
    chapters: Signal<bool>,
    sheet: Signal<bool>,
    list: ElementRef,
    heading: Rc<Cell<Option<usize>>>,
    jump: Rc<Cell<Option<Jump>>>,
    place: Rc<Cell<Place>>,
    opening: Rc<Cell<u64>>,
    // The word last looked up (chapter offset, length), kept for the progress file.
    mark: Signal<Option<(u32, u32)>>,
    words: Words,
    popup: Signal<Option<Popup>>,
    #[cfg(not(target_os = "android"))]
    hover: Hover,
    // Tasks end with their owner, and a card's buttons go with the card each time it is rebuilt.
    owner: Owner,
    // The mouse is dragging over a page.
    #[cfg(not(target_os = "android"))]
    selecting: Signal<bool>,
    // The view that keeps the keyboard focus inside the page.
    #[cfg(not(target_os = "android"))]
    keys: ElementRef,
    // Running while a hovered card waits to count.
    dwell: Rc<Cell<Option<Spawned<()>>>>,
    reading: Reading,
    saved: Rc<Cell<Saved>>,
    // Block and char offset of a tapped word: when set, it takes the tap.
    pub word_tap: Option<WordTap>,
}

impl ReaderState {
    fn offset(&self) -> usize {
        match self.place.get() {
            Place::Offset(offset) => offset,
            Place::Last => 0,
        }
    }

    fn save(&self) {
        let (index, offset, mark) = (self.index.get_untracked(), self.offset(), self.mark.get_untracked());
        if self.saved.get() == (index, offset, mark) {
            return;
        }
        self.saved.set((index, offset, mark));
        let progress = Progress {
            chapter: index as u32,
            offset: offset as u32,
            fraction: self.meta.fraction_at(index, offset),
            updated: now(),
            word: mark,
        };
        if let Err(e) = self.shared.library.save_progress(&self.meta.id, &progress) {
            log::warn!("saving progress: {e}");
        }
    }

    fn open_chapter(&self, index: usize, place: Place) {
        let Some(chapter) = self.shared.library.chapter(&self.meta.id, index) else { return };
        self.mark.set(None);
        self.close_popup();
        self.place.set(place);
        self.opening.set(self.opening.get() + 1);
        batch(|| {
            self.index.set(index);
            self.chapter.set(Arc::new(chapter));
        });
    }

    fn lay_out(&self, measures: &[NshMeasure; 2]) {
        let (width, height) = self.size.get();
        let (cfg, typeface, vertical, book_ruby) =
            self.shared.page.with(|p| (config(width, height, p), p.typeface, p.vertical, p.furigana != Furigana::Dictionary));
        let chapter = self.chapter.get();
        let index = self.index.get_untracked();
        if width <= 0.0 {
            return;
        }
        let made_for = LaidFor { chapter: index, opening: self.opening.get(), cfg, typeface, book_ruby };
        if self.laid.with_untracked(|l| l.as_ref().is_some_and(|l| l.made_for == made_for)) {
            return;
        }
        let started = Instant::now();
        let measure = &measures[(typeface == Typeface::Serif) as usize];
        let pages = pages_of(&chapter, &cfg, book_ruby, measure);
        log::info!("laid out chapter {index}: {} chars into {} pages in {:?}", chapter.char_len(), pages.len(), started.elapsed());
        // The word marked on the page shown, as after a look-up, stays in sight when the page is
        // laid out again, as on turning the phone; else the page's first line does.
        let mark = self.mark.get_untracked().map(|(start, _)| start as usize);
        let shown = self.laid.with_untracked(|l| {
            l.as_ref()
                .filter(|l| (l.made_for.chapter, l.made_for.opening) == (index, made_for.opening))
                .and_then(|l| l.pages.get(self.page.get_untracked()))
                .map(|p| p.start..p.end)
        });
        let place = match (mark, shown) {
            (Some(mark), Some(shown)) if shown.contains(&mark) => Place::Offset(mark),
            _ => self.place.get(),
        };
        let (first, place) = landing(&pages, place);
        self.place.set(place);
        let slots = Slots { pages: pages.len(), previous: index > 0, next: index + 1 < self.meta.chapters.len(), reversed: vertical };
        self.page.set(first);
        let generation = self.laid.with_untracked(|l| l.as_ref().map_or(0, |l| l.generation + 1));
        self.heading.set(None);
        let landing = Landing::new(slots.row_of(first));
        self.laid.set(Some(Rc::new(Laid { pages, slots, landing, cfg, width, height, first, generation, made_for })));
        self.save();
    }

    // The list has settled on `row`.
    fn rest_at(&self, slots: Slots, pages: &[Page], row: usize) {
        match self.jump.get() {
            // A row an earlier step of the drag was heading for.
            Some(jump) if jump.row != row && jump.at.elapsed() < LANDING => return,
            Some(_) => self.jump.set(None),
            None => {}
        }
        if self.heading.get() == Some(row) {
            self.heading.set(None);
        }
        let index = self.index.get_untracked();
        match slots.at(row) {
            Some(Slot::Page(at)) if at != self.page.get_untracked() => {
                let from = self.page.get_untracked();
                self.turned(&pages[from], at > from, false);
                self.close_popup();
                self.page.set(at);
                self.place.set(Place::Offset(pages[at].start));
                self.save();
            }
            Some(Slot::Next) => {
                self.turned(&pages[self.page.get_untracked()], true, true);
                self.open_chapter(index + 1, Place::Offset(0));
            }
            Some(Slot::Previous) => {
                self.turned(&pages[self.page.get_untracked()], false, true);
                self.open_chapter(index - 1, Place::Last);
            }
            _ => {}
        }
    }

    // Counts from the row the last turn is heading for, so a turn needn't wait for that one to end.
    fn turn(&self, rows: isize, animated: bool) {
        self.jump.set(None);
        let Some(laid) = self.laid.get_untracked() else { return };
        laid.landing.release();
        let from = self.heading.get().unwrap_or_else(|| laid.slots.row_of(self.page.get_untracked()));
        let row = from.saturating_add_signed(rows);
        if laid.slots.at(row).is_some() {
            self.heading.set(Some(row));
            self.list.scroll_to_index(row as u32, percentage(0.0), Points(0.0), animated);
        }
    }

    // A forward turn is a row further on, except in a reversed list.
    fn turn_forward(&self, forward: bool, animated: bool) {
        let vertical = self.shared.page.get_untracked().vertical;
        self.turn(if forward != vertical { 1 } else { -1 }, animated);
    }

    // The page moves at once and the list catches up, so the pages passed on the way are neither
    // read nor settled on. Saving is left to the end of the drag.
    fn jump_to_page(&self, at: usize) {
        let Some(laid) = self.laid.get_untracked() else { return };
        if at >= laid.pages.len() || at == self.page.get_untracked() {
            return;
        }
        let row = laid.slots.row_of(at);
        laid.landing.release();
        self.reading.skipped();
        self.close_popup();
        self.heading.set(None);
        self.jump.set(Some(Jump { row, at: Instant::now() }));
        self.page.set(at);
        self.place.set(Place::Offset(laid.pages[at].start));
        self.list.scroll_to_index(row as u32, percentage(0.0), Points(0.0), false);
    }

    // The word under a point of the page, as tap and hover see it.
    fn word_under(&self, laid: &Laid, at: usize, x: f32, y: f32) -> Option<(usize, usize)> {
        let (margin, font_size) = self.shared.page.with_untracked(|p| (p.margin, p.font_size));
        word_at(&laid.pages[at], x - margin, y - margin, font_size * 0.3)
    }

    fn tap(&self, laid: &Laid, at: usize, x: f32, y: f32) {
        // Open bars close first, whatever was tapped.
        if self.bar.get_untracked() {
            self.bar.set(false);
            return;
        }
        if let Some(word_tap) = &self.word_tap
            && let Some((block, offset)) = self.word_under(laid, at, x, y)
        {
            word_tap(block, offset);
            return;
        }
        self.reading.active();
        if self.popup.with_untracked(Option::is_some) {
            self.close_popup();
            return;
        }
        let tap_to_turn = self.shared.page.with_untracked(|p| p.tap_to_turn);
        match zone(x, laid.width) {
            Zone::Left if tap_to_turn => self.turn(-1, true),
            Zone::Right if tap_to_turn => self.turn(1, true),
            _ => self.bar.update(|b| *b = !*b),
        }
    }

    fn leave(&self) {
        self.reading.active();
        self.flush();
        self.save();
        route().set(Route::Library);
    }

    // What is open over the page, the topmost first.
    fn top(&self) -> Option<Layer> {
        if self.sheet.get_untracked() {
            Some(Layer::Sheet)
        } else if self.chapters.get_untracked() {
            Some(Layer::Chapters)
        } else if self.popup.with_untracked(Option::is_some) {
            Some(Layer::Popup)
        } else if self.bar.get_untracked() {
            Some(Layer::Bar)
        } else {
            None
        }
    }

    // The back button's way out: what is open closes first.
    fn back(&self) {
        match self.top() {
            Some(Layer::Sheet) => self.sheet.set(false),
            Some(Layer::Chapters) => self.chapters.set(false),
            Some(Layer::Popup) => self.close_popup(),
            Some(Layer::Bar) => self.bar.set(false),
            None => self.leave(),
        }
    }

    // A sheet or the chapter list is over the page: what is behind them stays as it is.
    fn covered(&self) -> bool {
        matches!(self.top(), Some(Layer::Sheet | Layer::Chapters))
    }

    // Escape shuts what is on top of the page, and otherwise goes to the library, open bars or not.
    fn escape(&self) {
        if matches!(self.top(), Some(Layer::Sheet | Layer::Chapters | Layer::Popup)) {
            self.back();
        } else {
            self.leave();
        }
    }

    // A window opens with the focus on nothing, and a focused view can go with its sheet or card.
    // NSH tells the views under a still mouse of Shift only with the focus inside them, so a press
    // or a release is heard without a move. Not on every move: taking the focus clears a selection.
    #[cfg(not(target_os = "android"))]
    fn keep_keys(&self) {
        self.laid.with(|_| ());
        self.page.with(|_| ());
        if self.popup.with(Option::is_none) && !self.sheet.get() && !self.chapters.get() {
            self.keys.focus();
        }
    }

    fn scheme(&self) -> Scheme {
        page_scheme(self.shared.page.with(|p| p.theme))
    }
}

// The word under a point of a page, as its block and chapter offset: the glyph nearest to it, or
// the one a reading stands over.
fn word_at(page: &Page, x: f32, y: f32, slop: f32) -> Option<(usize, usize)> {
    if let Some(g) = page.glyph_near(x, y, slop) {
        return Some((g.block, g.offset));
    }
    let ruby = page.ruby.iter().find(|r| r.rect.contains(x, y))?;
    Some((ruby.block, ruby.offset))
}

fn page_row(state: &ReaderState, laid: &Rc<Laid>, at: usize) -> AnyElement {
    let (typeface, vertical, margin) = state.shared.page.with(|p| (p.typeface, p.vertical, p.margin));
    let s = state.scheme();
    let style = PageStyle {
        family: family(typeface),
        ink: c(s.on_surface),
        ruby: rgba((s.on_surface.to_rgb() << 8) | 0xcc),
        vertical,
        width: laid.cfg.width,
        height: laid.cfg.height,
    };
    let chapter = state.chapter.get_untracked();
    let (library, id) = (state.shared.library.clone(), state.meta.id.clone());
    let file_of = move |block: usize| match chapter.blocks.get(block) {
        Some(Block::Image(i)) => Some(library.image_path(&id, &i.file).to_string_lossy().into_owned()),
        _ => None,
    };
    let (tapped, shown) = (state.clone(), laid.clone());
    let on_tap: PageTap = Rc::new(move |x, y| tapped.tap(&shown, at, x + margin, y + margin));
    let page_ink = state.page_ink();
    let ink = move |_, offset| page_ink.at(offset);
    let page = &laid.pages[at];
    // Their extent only sizes what's drawn centred over the word, so any measure will do.
    let mut readings = if state.shared.page.with(|p| p.furigana == Furigana::Dictionary) {
        let index = state.index.get_untracked();
        let words = state.words.colours.get().filter(|c| c.index == index);
        words.map(|c| rubiyomi_core::furigana::on_page(page, &c.words, &laid.cfg, &FixedMeasure)).unwrap_or_default()
    } else {
        page.ruby.clone()
    };
    rubiyomi_core::furigana::fit(&mut readings, laid.cfg.vertical);
    page_view(page, &readings, &style, &ink, on_tap, file_of).left(px(margin)).top(px(margin)).into_element()
}

fn next_chapter_row(state: &ReaderState, index: usize) -> AnyElement {
    let title = chapter_title(&state.meta, index);
    let dim = c(state.scheme().on_surface_variant);
    view().flex_1().items_center().justify_center().p_6().child(text(title).text_center().text_color(dim)).into_element()
}

fn row(state: &ReaderState, laid: &Rc<Laid>, at: usize) -> AnyElement {
    let index = state.index.get_untracked();
    match laid.slots.at(at) {
        Some(Slot::Page(p)) => page_row(state, laid, p),
        Some(Slot::Next) => next_chapter_row(state, index + 1),
        Some(Slot::Previous) => next_chapter_row(state, index.saturating_sub(1)),
        None => view().into_element(),
    }
}

fn pages(state: &ReaderState, laid: Rc<Laid>) -> AnyElement {
    let slots = laid.slots;
    let (width, height) = (laid.width, laid.height);
    let first = slots.row_of(laid.first);
    let cells = {
        let (state, laid) = (state.clone(), laid.clone());
        move |at: Position| {
            let (state, laid, tapped) = (state.clone(), laid.clone(), state.clone());
            let shown = laid.clone();
            #[cfg(not(target_os = "android"))]
            let (hovered, under, lifted) = (state.clone(), laid.clone(), state.selecting);
            let cell = view()
                .w(px(width))
                .h(px(height))
                .on_press(move |e: &PressEvent| {
                    if let Some(Slot::Page(p)) = shown.slots.at(at.index()) {
                        tapped.tap(&shown, p, e.position.x.0, e.position.y.0);
                    }
                })
                .child(dynamic(move || row(&state, &laid, at.index())));
            #[cfg(not(target_os = "android"))]
            let cell = cell.on_mouse_move(move |e: &MouseEvent| {
                hovered.selecting.set(e.pressed_button.is_some());
                hovered.shift_event(e.modifiers.shift, (e.page_position.x.0, e.page_position.y.0));
                if !e.modifiers.shift || e.pressed_button.is_some() {
                    hovered.hover.stop();
                } else if let Some(Slot::Page(p)) = under.slots.at(at.index())
                    && let Some((block, offset)) = hovered.word_under(&under, p, e.position.x.0, e.position.y.0)
                {
                    hovered.hover_word(block, offset);
                }
            });
            // Mouse moves stop at the window's edge and with a still mouse; the release doesn't.
            #[cfg(not(target_os = "android"))]
            let cell = cell.on_touch(move |e: &TouchEvent| {
                if matches!(e.phase, TouchPhase::End | TouchPhase::Cancel) {
                    lifted.set(false);
                }
            });
            cell
        }
    };
    let (scrolled, dragged) = (state.clone(), laid.clone());
    let list = virtual_list(slots.count(), cells);
    // NSH scrolls the list while a selection drag is past its edge, and an area per page can't
    // follow it there. One area over all pages can't either: the list renders only the pages near
    // the view, so a copy would miss those between the ends, and a column book's reversed rows
    // would copy backwards. A list that can't scroll keeps the drag on its page.
    #[cfg(not(target_os = "android"))]
    let list = {
        let selecting = state.selecting;
        list.scrollable(move || !selecting.get())
    };
    list
        // A new native list: one kept for a new layout would keep its scroll offset.
        .key(format!("pages-{}", laid.generation))
        .horizontal()
        .paging(true)
        .disable_interval_momentum(true)
        .bounces(false)
        .scrollbar(false)
        .estimated_item_size(Points(width))
        .initial_scroll_index(first)
        .element_ref(&state.list)
        .flex_1()
        .on_scroll_drag_start(move |_: &_| dragged.landing.release())
        .on_scroll(move |e: &ScrollEvent| {
            let (x, width) = (e.offset.x.0, e.size.width.0);
            if width <= 0.0 {
                return;
            }
            let row = (x / width).round();
            // A last report from the list of the chapter just left.
            let current = laid.made_for.chapter == scrolled.index.get_untracked()
                && scrolled.laid.with_untracked(|l| l.as_ref().is_some_and(|l| l.generation == laid.generation));
            if current && (x - row * width).abs() <= 2.0 && laid.landing.reached(row as usize) {
                scrolled.rest_at(laid.slots, &laid.pages, row as usize);
            }
        })
        .into_element()
}

fn footer(state: &ReaderState) -> AnyElement {
    let state = state.clone();
    let shown = {
        let state = state.clone();
        move || state.shared.page.with(|p| p.show_position) && !state.bar.get()
    };
    when(shown, move || {
        let state = state.clone();
        let label = {
            let state = state.clone();
            move || {
                let (page, index) = (state.page.get(), state.index.get());
                let total = state.laid.with(|l| l.as_ref().map_or(1, |l| l.pages.len()));
                let offset = state.laid.with(|l| l.as_ref().map_or(0, |l| l.start_of(page)));
                format!("{} · {:.0}%", page_figure(page, total), percent(state.meta.fraction_at(index, offset)))
            }
        };
        let colour = move || c(state.scheme().on_surface_variant);
        view()
            .absolute()
            .bottom(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .h(px(FOOTER))
            .items_center()
            .justify_center()
            .pointer_events(PointerEvents::None)
            .child(text(label).text_size(px(11.0)).text_color(colour))
    })
    .into_element()
}

fn icon_button(glyph: &'static str, name: &'static str, s: Scheme, on_press: impl Fn() + 'static) -> impl IntoElement {
    view().w(px(48.0)).h(px(48.0)).items_center().justify_center().label(name).ripples().on_press(on_press).child(icon(
        glyph,
        24.0,
        c(s.on_surface),
    ))
}

fn top_bar(state: &ReaderState) -> AnyElement {
    let s = state.scheme();
    let title = state.meta.title.clone();
    let chapter = {
        let chapter = state.chapter;
        move || chapter.with(|c| c.title.clone())
    };
    let (back, list, aa) = (state.clone(), state.clone(), state.clone());
    view()
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .right(px(0.0))
        .flex_row()
        .items_center()
        .px_1()
        .bg(c(s.surface_container))
        // A touch on a bar stays there, or Android would hand it to the pages behind.
        .on_press(|| {})
        .child(icon_button(glyph::ARROW_BACK, "Library", s, move || back.leave()))
        .child(
            view()
                .flex_1()
                .py_2()
                .child(text(title).text_size(px(16.0)).number_of_lines(1).text_color(c(s.on_surface)))
                .child(text(chapter).text_size(px(12.0)).number_of_lines(1).text_color(c(s.on_surface_variant))),
        )
        .child(icon_button(glyph::TEXT_FIELDS, "Page settings", s, move || aa.sheet.set(true)))
        .child(icon_button(glyph::LIST, "Chapters", s, move || list.chapters.set(true)))
        .into_element()
}

fn bottom_row(name: &'static str, figure: impl Fn() -> String + 'static, s: Scheme, track: impl IntoElement) -> impl IntoElement {
    view()
        .pb_2()
        .child(
            view()
                .flex_row()
                .justify_between()
                .child(text(name).text_size(px(14.0)).text_color(c(s.on_surface)))
                .child(text(figure).text_size(px(14.0)).text_color(c(s.on_surface_variant))),
        )
        .child(track)
}

fn fill(fraction: impl Fn() -> f32 + 'static, s: Scheme) -> impl IntoElement {
    view()
        .w_full()
        .h(px(4.0))
        .my_2()
        .rounded(px(2.0))
        .overflow_hidden()
        .bg(c(s.surface_container_highest))
        .child(view().h_full().w(move || percentage(fraction().clamp(0.0, 1.0))).bg(c(s.primary)))
}

fn page_figure(page: usize, total: usize) -> String {
    format!("{} / {total}", page + 1)
}

fn percent(fraction: f32) -> f32 {
    (fraction * 100.0).floor()
}

fn book_figure(chapter: usize, chapters: usize, book: f32) -> String {
    format!("{} / {chapters} · {:.0}%", chapter + 1, percent(book))
}

// Built once per layout and following the page through tracked closures: rebuilt on a page
// change, the slider would drop a drag in progress.
fn bottom_bar(state: &ReaderState) -> AnyElement {
    let (state, moved) = (state.clone(), state.clone());
    let theme = {
        let page = state.shared.page;
        computed(move || page.with(|p| p.theme))
    };
    dynamic(move || {
        let s = page_scheme(theme.get());
        let Some(laid) = state.laid.get() else { return view().into_element() };
        let total = laid.pages.len();
        let page = state.page;
        let book = {
            let (state, laid) = (state.clone(), laid.clone());
            move || {
                let offset = laid.start_of(state.page.get());
                state.meta.fraction_at(state.index.get(), offset)
            }
        };
        let figure = {
            let (state, book) = (state.clone(), book.clone());
            move || book_figure(state.index.get(), state.meta.chapters.len(), book())
        };
        let jump = {
            let moved = moved.clone();
            Newest::new(move |at| moved.jump_to_page(at))
        };
        let (last, ended) = (jump.clone(), moved.clone());
        let skim = slider()
            .min(1.0)
            .max(total.max(2) as f64)
            .step(1.0)
            .value(move || SliderValue::Single((page.get() + 1) as f64))
            .disabled(total < 2)
            .track_class("h-8 justify-center px-[10px]")
            .indicator_class(format!("h-1 rounded-full {}", class_colour(s.surface_container_highest)))
            .range_class(format!("rounded-full {}", class_colour(s.primary)))
            .thumb_class(format!("size-5 rounded-full {}", class_colour(s.primary)))
            .on_change(move |v: &SliderValue| jump.offer((v.start() as usize).saturating_sub(1)))
            .on_change_end(move |v: &SliderValue| {
                last.finish((v.start() as usize).saturating_sub(1));
                ended.save();
            });
        view()
            .absolute()
            .bottom(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .px_6()
            .pt_3()
            .bg(c(s.surface_container))
            .on_press(|| {})
            .child(bottom_row("Page", move || page_figure(page.get(), total), s, skim))
            .child(bottom_row("Book chapter", figure, s, fill(book, s)))
            .into_element()
    })
    .into_element()
}

fn bars(state: &ReaderState) -> AnyElement {
    let state = state.clone();
    let open = state.bar;
    when(
        move || open.get(),
        move || {
            fill_parent()
                .pointer_events(PointerEvents::BoxNone)
                .child(dynamic({
                    let state = state.clone();
                    move || top_bar(&state)
                }))
                .child(bottom_bar(&state))
        },
    )
    .into_element()
}

fn chapter_row(state: &ReaderState, at: usize, s: Scheme) -> AnyElement {
    let state = state.clone();
    let title = chapter_title(&state.meta, at);
    let percent = percent(state.meta.fraction_at(at, 0));
    let current = at == state.index.get_untracked();
    view()
        .h(px(CHAPTER_ROW))
        .px_4()
        .justify_center()
        .rounded(px(12.0))
        .when(current, |v| v.bg(c(s.surface_container_highest)))
        .ripples()
        .on_press(move || {
            state.chapters.set(false);
            state.bar.set(false);
            state.open_chapter(at, Place::Offset(0));
        })
        .child(text(title).number_of_lines(1).text_size(px(16.0)).text_color(c(s.on_surface)))
        .child(text(format!("{percent:.0}%")).text_size(px(12.0)).text_color(c(s.on_surface_variant)))
        .into_element()
}

fn chapter_list(state: &ReaderState) -> AnyElement {
    let state = state.clone();
    let open = state.chapters;
    when(
        move || open.get(),
        move || {
            let s = state.scheme();
            let count = state.meta.chapters.len();
            let height = (count as f32 * CHAPTER_ROW).min(state.size.get_untracked().1 * 0.7);
            let rows = {
                let state = state.clone();
                move |at: Position| {
                    let state = state.clone();
                    dynamic(move || chapter_row(&state, at.index(), s))
                }
            };
            let close = state.chapters;
            scrim().items_center().justify_center().on_press(move || close.set(false)).child(
                view()
                    .w(px(state.size.get_untracked().0.min(432.0) - 32.0))
                    .p_4()
                    .rounded(px(28.0))
                    .bg(c(s.surface_container_high))
                    .on_press(|| {})
                    .child(text("Chapters").text_size(px(24.0)).pb_2().text_color(c(s.on_surface)))
                    .child(
                        view().h(px(height)).child(
                            virtual_list(count, rows)
                                .estimated_item_size(Points(CHAPTER_ROW))
                                .initial_scroll_index(state.index.get_untracked())
                                .flex_1(),
                        ),
                    ),
            )
        },
    )
    .into_element()
}

// A pretend user for the headless runs, which have no input: `next`, `prev`, `bar`, `chapters`, `start`, `options`, `settle`, `sheet`.
#[cfg(feature = "dev-hooks")]
fn script(state: &ReaderState) {
    // `RUBIYOMI_OPEN_WORD=block:offset` looks a word up before the steps.
    let word = crate::hooks::var("RUBIYOMI_OPEN_WORD").and_then(|w| {
        let (block, offset) = w.split_once(':')?;
        Some((block.parse().ok()?, offset.parse().ok()?))
    });
    let steps = crate::hooks::var("RUBIYOMI_SCRIPT").unwrap_or_default();
    if steps.is_empty() && word.is_none() {
        return;
    }
    let mut steps: Vec<String> = word.map(|_| "word").into_iter().chain(steps.split(',')).rev().map(str::to_owned).collect();
    let state = state.clone();
    let step = Rc::new(RefCell::new(move || match steps.pop().as_deref() {
        Some("next") => state.turn_forward(true, true),
        Some("prev") => state.turn_forward(false, true),
        Some("bar") => state.bar.set(true),
        Some("chapters") => state.chapters.set(true),
        Some("sheet") => state.sheet.set(true),
        Some("settle") => state.reading.settle(),
        Some("options") => state.edit_popup(Popup::flip),
        Some("word") => {
            if let Some((block, offset)) = word {
                state.look_up(block, offset);
            }
        }
        Some("start") => state.open_chapter(state.index.get_untracked(), Place::Offset(0)),
        _ => {}
    }));
    set_interval(Duration::from_millis(700), move || (step.borrow_mut())());
}

// The size is taken at once the first time and after the window has been still for a moment, so
// dragging an edge doesn't lay a chapter out for every step.
fn settled(size: Signal<(f32, f32)>) -> impl Fn(&Bounds<Points>) + 'static {
    let timer: Rc<RefCell<Option<Spawned<()>>>> = Rc::default();
    move |b: &Bounds<Points>| {
        let next = (b.size.width.0, b.size.height.0);
        // The first layouts on Android are of a window still making room for the bars.
        #[cfg(target_os = "android")]
        let next = crate::android::page_area().unwrap_or(next);
        if let Some(t) = timer.borrow_mut().take() {
            t.cancel();
        }
        if size.get_untracked().0 <= 0.0 {
            size.set(next);
        } else if size.get_untracked() != next {
            *timer.borrow_mut() = Some(set_timeout(Duration::from_millis(150), move || size.set(next)));
        }
    }
}

pub fn screen(id: BookId) -> impl IntoElement {
    let shared = shared();
    let Some(meta) = shared.library.meta(&id) else {
        return text("This book is gone.").p_4().into_element();
    };
    let progress = shared.library.progress(&id);
    let index = (progress.chapter as usize).min(meta.chapters.len().saturating_sub(1));
    let Some(chapter) = shared.library.chapter(&id, index) else {
        return text("This book's files are damaged. Remove it and add it again.").p_4().into_element();
    };
    let mark = progress.word.filter(|_| index == progress.chapter as usize);
    let mut state = ReaderState {
        meta: Rc::new(meta),
        index: signal(index),
        chapter: signal_with(Arc::new(chapter), SignalEquals::Never),
        size: signal((0.0, 0.0)),
        laid: signal_with(None, SignalEquals::Never),
        page: signal(0),
        bar: signal(false),
        chapters: signal(false),
        sheet: signal(false),
        list: element_ref(),
        heading: Rc::default(),
        jump: Rc::default(),
        place: Rc::new(Cell::new(Place::Offset(progress.offset as usize))),
        opening: Rc::new(Cell::new(0)),
        mark: signal(mark),
        words: Words::new(),
        popup: signal_with(None, SignalEquals::Never),
        #[cfg(not(target_os = "android"))]
        hover: Hover::default(),
        owner: Owner::current(),
        #[cfg(not(target_os = "android"))]
        selecting: signal(false),
        #[cfg(not(target_os = "android"))]
        keys: element_ref(),
        dwell: Rc::default(),
        reading: Reading::new(),
        saved: Rc::new(Cell::new((index, progress.offset as usize, mark))),
        word_tap: None,
        shared,
    };
    // Cloned before the closure is set, so it doesn't hold itself.
    let tapped = state.clone();
    state.word_tap = Some(Rc::new(move |block, offset| tapped.look_up(block, offset)));
    let measures = [NshMeasure::new(family(Typeface::Sans)), NshMeasure::new(family(Typeface::Serif))];
    {
        let state = state.clone();
        effect(move || state.lay_out(&measures));
    }
    {
        let state = state.clone();
        let page = state.shared.page;
        // Apart, so the rest of the page's settings don't find the words again.
        let for_furigana = computed(move || page.with(|p| p.furigana == Furigana::Dictionary));
        effect(move || state.colour(for_furigana.get()));
    }
    state.watch_skimming();
    #[cfg(not(target_os = "android"))]
    {
        let state = state.clone();
        effect(move || state.keep_keys());
    }
    {
        let state = state.clone();
        effect(move || {
            if lifecycle() != Lifecycle::Active {
                state.flush();
                state.save();
            }
        });
    }
    {
        let (page, platform) = (state.shared.page, state.shared.platform.clone());
        effect(move || platform.keep_screen_on(page.with(|p| p.keep_awake)));
        let platform = state.shared.platform.clone();
        cleanup(move || platform.keep_screen_on(false));
    }
    #[cfg(target_os = "android")]
    {
        crate::android::full_screen(true);
        cleanup(|| crate::android::full_screen(false));
    }
    {
        let state = state.clone();
        on_back_press(move || {
            state.back();
            true
        });
    }
    let held = KeyOptions { repeat: true, ..KeyOptions::default() };
    for (key, forward) in [("pagedown", true), ("space", true), ("pageup", false)] {
        let state = state.clone();
        on_key_with(key, held, move |_| {
            if !state.covered() {
                state.turn_forward(forward, false);
            }
        });
    }
    for (key, rows) in [("right", 1), ("left", -1)] {
        let state = state.clone();
        on_key_with(key, held, move |_| {
            if !state.covered() {
                state.turn(rows, false);
            }
        });
    }
    {
        let state = state.clone();
        on_key("escape", move |_| state.escape());
    }
    #[cfg(feature = "dev-hooks")]
    script(&state);

    let (shown, background, size) = (state.clone(), state.clone(), state.size);
    let reader = view().flex_1();
    #[cfg(not(target_os = "android"))]
    let reader = reader.focusable(true).element_ref(&state.keys);
    reader
        .bg(move || c(background.scheme().surface))
        .on_layout(settled(size))
        .child(dynamic(move || match shown.laid.get() {
            Some(laid) => pages(&shown, laid),
            None => view().into_element(),
        }))
        .child(footer(&state))
        .child(bars(&state))
        .child(popup(&state))
        .child(chapter_list(&state))
        .child(crate::page_form::sheet(state.sheet, state.shared.page, state.shared.page_dragging, state.shared.grader, state.size))
        .child(state.skim_dialog())
        .into_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    const WITH_BOTH: Slots = Slots { pages: 3, previous: true, next: true, reversed: false };

    #[test]
    fn rows_past_the_pages_are_the_chapters_beside() {
        let rows: Vec<_> = (0..WITH_BOTH.count()).map(|r| WITH_BOTH.at(r).unwrap()).collect();
        assert_eq!(rows, [Slot::Previous, Slot::Page(0), Slot::Page(1), Slot::Page(2), Slot::Next]);
        assert_eq!(WITH_BOTH.at(5), None);
        assert_eq!(WITH_BOTH.row_of(2), 3);
    }

    #[test]
    fn a_reversed_list_starts_from_its_last_row() {
        let slots = Slots { reversed: true, ..WITH_BOTH };
        let rows: Vec<_> = (0..slots.count()).map(|r| slots.at(r).unwrap()).collect();
        assert_eq!(rows, [Slot::Next, Slot::Page(2), Slot::Page(1), Slot::Page(0), Slot::Previous]);
        assert_eq!(slots.row_of(0), 3);
        assert_eq!(slots.at(5), None);
    }

    #[test]
    fn the_first_and_last_chapters_have_nothing_beyond_them() {
        let alone = Slots { pages: 2, previous: false, next: false, reversed: false };
        assert_eq!(alone.count(), 2);
        assert_eq!(alone.at(0), Some(Slot::Page(0)));
        assert_eq!(alone.at(2), None);
    }

    #[test]
    fn a_list_on_its_way_to_the_place_turns_nothing() {
        let landing = Landing::new(7);
        assert!(!landing.reached(0), "the list's first report, before its jump");
        assert!(landing.reached(7));
        assert!(landing.reached(0), "from then on a report is a turn");
    }

    #[test]
    fn a_list_the_reader_moved_first_is_not_waited_for() {
        let landing = Landing::new(7);
        landing.release();
        assert!(landing.reached(8));
    }

    #[test]
    fn a_list_sent_to_row_0_is_there() {
        assert!(Landing::new(0).reached(1));
    }

    #[test]
    fn opening_a_chapter_again_is_a_new_layout_that_lands_on_its_start() {
        let pages = vec![Page { start: 0, end: 10, ..Page::default() }, Page { start: 10, end: 20, ..Page::default() }];
        let shown = LaidFor { chapter: 3, opening: 1, cfg: LayoutConfig::default(), typeface: Typeface::Sans, book_ruby: true };
        assert_ne!(shown, LaidFor { opening: 2, ..shown });
        assert_eq!(landing(&pages, Place::Offset(0)), (0, Place::Offset(0)));
    }

    #[test]
    fn the_last_page_is_a_place_once_laid_out() {
        let pages = vec![Page { start: 0, end: 10, ..Page::default() }, Page { start: 10, end: 20, ..Page::default() }];
        assert_eq!(landing(&pages, Place::Last), (1, Place::Offset(10)));
        assert_eq!(landing(&pages, Place::Offset(15)), (1, Place::Offset(15)));
    }

    #[test]
    fn the_bar_says_page_then_chapter_and_how_far_through_the_book() {
        assert_eq!(page_figure(3, 19), "4 / 19");
        assert_eq!(book_figure(2, 12, 0.609), "3 / 12 · 60%");
    }

    #[test]
    fn the_outer_quarters_turn_the_page() {
        assert_eq!(zone(50.0, 400.0), Zone::Left);
        assert_eq!(zone(200.0, 400.0), Zone::Middle);
        assert_eq!(zone(350.0, 400.0), Zone::Right);
    }
}
