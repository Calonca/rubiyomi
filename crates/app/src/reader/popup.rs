// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;
#[cfg(not(target_os = "android"))]
use std::time::Instant;

use nsh_core::prelude::*;
use nsh_core::{LayoutCurve, LayoutTransition};
use rubiyomi_core::cards::CardState;
use rubiyomi_core::finder::{Finder, Word, jpdb_words_tag};
use rubiyomi_core::grade::Grader;
use rubiyomi_core::history::Place;
use rubiyomi_core::platform::Platform;
use rubiyomi_core::reader::layout::{Page, Rect};
use rubiyomi_core::text::Block;
use rubiyomi_core::theme::{Color, Scheme};

use super::ReaderState;
use super::words::{Colours, StateColours, full_colour};
use crate::ui::{Ripples, c, fill_parent, glyph, icon, snackbar};

// Below this width the card spans the screen: narrower, it looks like a tooltip.
const STRETCH_BELOW: f32 = 480.0;
const MAX_WIDTH: f32 = 300.0;
const MAX_HEIGHT_FRACTION: f32 = 0.4;
const GAP: f32 = 8.0;
// A card hovered this long was read, not passed on the way.
const DWELL: Duration = Duration::from_secs(1);
// Shift held this long is a peek: the card goes on release. Shorter is a press that toggles it.
#[cfg(not(target_os = "android"))]
const HOLD: Duration = Duration::from_secs(1);

#[derive(Clone, Debug, PartialEq)]
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
}

#[derive(Clone, Debug)]
pub struct Popup {
    // In chapter offsets.
    pub span: Range<usize>,
    // What the card keeps clear of, then the word itself, in reader coordinates; none when the
    // word is off the page.
    anchor: Option<(Rect, Rect)>,
    pub content: Content,
    pub sentence: String,
    pub state: Option<CardState>,
    pub busy: bool,
    pub message: Option<String>,
    vertical: bool,
    // The card's other face: what can be done with the word.
    options: bool,
}

// What the card shows for the word at `local` in a block's text: the word, or why there is none.
pub fn lookup(finder: &Result<Arc<Finder>, String>, parsed: Option<Word>, text: &str, local: usize) -> Option<Content> {
    match (parsed, finder) {
        (Some(word), _) => Some(Content::Word(word)),
        (None, Ok(finder)) => finder.word_at(text, local).map(Content::Word),
        (None, Err(note)) => Some(Content::Message(note.clone())),
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

// The text of a sentence, for the card.
fn sentence(text: &str, range: &Range<usize>) -> String {
    text.chars().skip(range.start).take(range.len()).collect::<String>().trim().to_owned()
}

fn union(a: Rect, b: Rect) -> Rect {
    let (x, y) = (a.x.min(b.x), a.y.min(b.y));
    Rect { x, y, w: a.right().max(b.right()) - x, h: a.bottom().max(b.bottom()) - y }
}

// The box of what a page shows of a span.
fn span_rect(page: &Page, span: &Range<usize>) -> Option<Rect> {
    page.glyphs.iter().filter(|g| span.contains(&g.offset)).map(|g| g.rect).reduce(union)
}

// Where the card goes, as its left edge and either its top or its bottom edge.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Spot {
    left: f32,
    top: Option<f32>,
    bottom: Option<f32>,
}

fn width_for(screen: f32) -> f32 {
    if screen < STRETCH_BELOW { screen - GAP * 2.0 } else { MAX_WIDTH }
}

// Over what hasn't been read yet: under the line, from the word on, or above it when the word
// is low. Columns are read right to left, so there the card goes to the left of the word's
// column, or to its right; a phone is too narrow for either, and then it goes as across lines.
// A sentence too tall to clear leaves the card clear of the word alone.
fn spot(screen: (f32, f32), card: f32, anchor: Rect, word: Rect, vertical: bool) -> Spot {
    let (sw, sh) = screen;
    let across = |x: f32| x.clamp(GAP, (sw - card - GAP).max(GAP));
    if vertical {
        let top = Some(anchor.y.clamp(GAP, sh * (1.0 - MAX_HEIGHT_FRACTION) - GAP));
        let (left, right) = (anchor.x - GAP - card, anchor.right() + GAP);
        if left >= GAP {
            return Spot { left, top, bottom: None };
        }
        if right + card <= sw - GAP {
            return Spot { left: right, top, bottom: None };
        }
    }
    let lowest = sh * (1.0 - MAX_HEIGHT_FRACTION) - GAP;
    let clear_of = |r: Rect| {
        if r.bottom() + GAP < sh * 0.55 {
            Some(Spot { left: across(r.x), top: Some(r.bottom() + GAP), bottom: None })
        } else {
            let bottom = sh - r.y + GAP;
            (bottom <= lowest).then(|| Spot { left: across(r.x), top: None, bottom: Some(bottom.max(GAP)) })
        }
    };
    clear_of(anchor).or_else(|| clear_of(word)).unwrap_or_else(|| Spot {
        left: across(word.x),
        top: None,
        bottom: Some((sh - word.y + GAP).clamp(GAP, lowest)),
    })
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

// One press of Shift over the page.
#[cfg(not(target_os = "android"))]
#[derive(Clone, Copy, Default)]
struct ShiftPress {
    down: Option<Instant>,
    card_at_press: bool,
    word_moved: bool,
    // Where the mouse last was: a key's release is told at the same spot.
    at: (f32, f32),
}

// Whether letting go of Shift closes the card.
#[cfg(not(target_os = "android"))]
fn release_closes(held: Duration, card_at_press: bool, word_moved: bool) -> bool {
    held >= HOLD || (card_at_press && !word_moved)
}

// The word under the mouse while Shift is held.
#[cfg(not(target_os = "android"))]
#[derive(Clone, Default)]
pub struct Hover {
    press: Rc<Cell<ShiftPress>>,
    // The char last asked about: resting on one that is no word mustn't ask again.
    seen: Rc<Cell<Option<(usize, usize)>>>,
    busy: Rc<Cell<bool>>,
    next: Rc<Cell<Option<(usize, usize)>>>,
}

#[cfg(not(target_os = "android"))]
impl Hover {
    pub fn stop(&self) {
        self.seen.set(None);
        self.next.set(None);
    }
}

impl Popup {
    pub fn flip(&mut self) {
        self.options = !self.options;
    }
}

impl ReaderState {
    // Changes the card shown, if there is one.
    pub fn edit_popup(&self, f: impl FnOnce(&mut Popup)) {
        self.popup.update(|p| {
            if let Some(p) = p {
                f(p);
            }
        });
    }

    pub fn close_popup(&self) {
        if self.popup.with_untracked(Option::is_some) {
            self.popup.set(None);
        }
    }

    // A tap on the word at `offset` of block `block`: the card of the word, or off with the card
    // if it is the word shown.
    pub fn look_up(&self, block: usize, offset: usize) {
        self.reading.active();
        let shown = self.popup_shows(offset);
        // A click settles a card still waiting to count.
        if let Some(timer) = self.dwell.take() {
            timer.cancel();
            if shown {
                self.note_shown();
            }
        }
        if shown {
            self.close_popup();
            return;
        }
        self.find(block, offset, false);
    }

    fn popup_shows(&self, offset: usize) -> bool {
        self.popup.with_untracked(|p| p.as_ref().is_some_and(|p| p.span.contains(&offset)))
    }

    // The mouse is over the char at `offset` with Shift held. One look-up at a time, as each may
    // ask the card's service: while one runs, only the newest char waits.
    #[cfg(not(target_os = "android"))]
    pub fn hover_word(&self, block: usize, offset: usize) {
        if self.hover.seen.replace(Some((block, offset))) == Some((block, offset)) {
            return;
        }
        if self.hover.busy.get() {
            self.hover.next.set(Some((block, offset)));
        } else {
            self.hover_find(block, offset);
        }
    }

    // Hears every mouse event over the page and the card. A release can pass unseen, out of the
    // window, so it only counts at the spot of the last event: a later move with Shift up closes
    // nothing.
    #[cfg(not(target_os = "android"))]
    pub fn shift_event(&self, shift: bool, at: (f32, f32)) {
        let mut press = self.hover.press.get();
        let still = press.at == at;
        press.at = at;
        if shift && press.down.is_none() {
            press.down = Some(Instant::now());
            press.card_at_press = self.popup.with_untracked(Option::is_some);
            press.word_moved = false;
        } else if !shift
            && let Some(down) = press.down.take()
            && still
            && release_closes(down.elapsed(), press.card_at_press, press.word_moved || self.hover.busy.get())
        {
            if let Some(timer) = self.dwell.take() {
                timer.cancel();
            }
            // A look-up on its way would bring the card back.
            self.words.looking.set(self.words.looking.get() + 1);
            self.close_popup();
        }
        self.hover.press.set(press);
    }

    #[cfg(not(target_os = "android"))]
    fn hover_find(&self, block: usize, offset: usize) {
        if !self.popup_shows(offset) {
            self.reading.active();
            // A cell can go while its look-up runs, and its task with it, leaving `busy` set.
            let state = self.clone();
            self.owner.run(move || state.find(block, offset, true));
        }
    }

    #[cfg(not(target_os = "android"))]
    fn hover_landed(&self) {
        self.hover.busy.set(false);
        if let Some((block, offset)) = self.hover.next.take() {
            self.hover_find(block, offset);
        }
    }

    fn find(&self, block: usize, offset: usize, hover: bool) {
        let chapter = self.chapter.get_untracked();
        let Some(text) = chapter.blocks.get(block).and_then(Block::as_text) else { return };
        #[cfg(not(target_os = "android"))]
        if hover {
            self.hover.busy.set(true);
        }
        let block_start = chapter.block_start(block);
        let local = offset.saturating_sub(block_start);
        let index = self.index.get_untracked();
        let parsed = self.shared.parser.get_untracked().and_then(|_| {
            let colours =
                self.words.colours.get_untracked().filter(|c| c.index == index && c.tag.as_deref() == Some(&*jpdb_words_tag()))?;
            let w = colours.word_at(offset).filter(|w| w.start >= block_start)?;
            Some(Word { start: w.start - block_start, ..w.clone() })
        });
        let (finder, grader, text) = (self.shared.finder.get_untracked(), self.shared.grader.get_untracked(), text.text.clone());
        let job = self.words.looking.get() + 1;
        self.words.looking.set(job);
        let state = self.clone();
        self.spawn_here(async move {
            let found = blocking(move || {
                let content = lookup(&finder, parsed, &text, local);
                let card = match (&content, &grader) {
                    (Some(Content::Word(w)), Some(g)) => g.states(std::slice::from_ref(w)).get(&w.key()).copied(),
                    _ => None,
                };
                let range = match &content {
                    Some(Content::Word(w)) => Some(sentence_range(&text, w.start)),
                    _ => None,
                };
                let around = range.as_ref().map(|r| sentence(&text, r)).unwrap_or_default();
                (content, card, around, range)
            })
            .await;
            if let Ok((Some(content), card, around, range)) = found
                && state.words.looking.get() == job
                && state.index.get_untracked() == index
            {
                state.show(content, card, (around, range), offset, block_start, hover);
            }
            #[cfg(not(target_os = "android"))]
            if hover {
                state.hover_landed();
            }
        });
    }

    // `around` is the sentence of a word and where it lies in the block.
    fn show(
        &self,
        content: Content,
        card: Option<CardState>,
        (around, sentence_span): (String, Option<Range<usize>>),
        offset: usize,
        block_start: usize,
        hover: bool,
    ) {
        let span = match &content {
            Content::Word(w) => block_start + w.start..block_start + w.start + w.len,
            Content::Message(_) => offset..offset + 1,
        };
        let (vertical, margin) = self.shared.page.with_untracked(|p| (p.vertical, p.margin));
        let anchor = self.laid.get_untracked().and_then(|laid| {
            let page = laid.pages.get(self.page.get_untracked())?;
            let word = span_rect(page, &span)?;
            let shift = |r: Rect| Rect { x: r.x + margin, y: r.y + margin, ..r };
            // Across the lines the card keeps clear of the whole sentence, so it can be read with
            // the card open; along them it stays at the word, as the sentence's last piece often
            // starts at the head of a line far from it.
            let rect = match &sentence_span {
                Some(range) => {
                    let sentence = span_rect(page, &(block_start + range.start..block_start + range.end));
                    match sentence {
                        Some(s) if vertical => Rect { x: s.x, y: word.y, w: s.w, h: word.h },
                        Some(s) => Rect { x: word.x, y: s.y, w: word.w, h: s.h },
                        None => word,
                    }
                }
                None => word,
            };
            Some((shift(rect), shift(word)))
        });
        let is_word = matches!(content, Content::Word(_));
        let popup = Popup {
            span: span.clone(),
            anchor,
            content,
            sentence: around,
            state: card,
            busy: false,
            message: None,
            vertical,
            options: false,
        };
        self.popup.set(Some(popup));
        if !is_word {
            return;
        }
        if hover {
            #[cfg(not(target_os = "android"))]
            {
                let mut press = self.hover.press.get();
                press.word_moved = true;
                self.hover.press.set(press);
            }
            self.dwell(span);
        } else {
            self.note_shown();
        }
    }

    // A card passed under the mouse is a peek: it counts only if it stays.
    fn dwell(&self, span: Range<usize>) {
        let state = self.clone();
        let timer = set_timeout(DWELL, move || {
            state.dwell.take();
            if state.popup.with_untracked(|p| p.as_ref().is_some_and(|p| p.span == span)) {
                state.note_shown();
            }
        });
        if let Some(previous) = self.dwell.replace(Some(timer)) {
            previous.cancel();
        }
    }

    // The word on the card was looked up: it goes in the history,
    // the stats and the progress file.
    fn note_shown(&self) {
        let mark = self.popup.with_untracked(|popup| {
            let popup = popup.as_ref()?;
            let Content::Word(word) = &popup.content else { return None };
            let span = &popup.span;
            self.shared.history.borrow_mut().record(
                word,
                Place {
                    book: &self.meta.id,
                    book_title: &self.meta.title,
                    chapter: self.index.get_untracked() as u32,
                    offset: span.start as u32,
                    sentence: &popup.sentence,
                },
            );
            Some((span.start as u32, span.len() as u32))
        });
        let Some(mark) = mark else { return };
        self.reading.looked_up();
        self.mark.set(Some(mark));
        self.save();
    }

    fn spawn_here(&self, task: impl Future<Output = ()> + 'static) {
        self.owner.run(|| {
            spawn(task);
        });
    }

    fn review(&self, action: Action) {
        if let Action::Open(url) = &action {
            if !self.shared.platform.open_url(url) {
                self.say("No browser could be opened.");
            }
            return;
        }
        let Some(grader) = self.shared.grader.get_untracked() else { return };
        let Some((word, sentence)) = self.popup.with_untracked(|p| match p.as_ref()?.content.clone() {
            Content::Word(w) => Some((w, p.as_ref()?.sentence.clone())),
            Content::Message(_) => None,
        }) else {
            return;
        };
        if self.words.grading.replace(true) {
            return;
        }
        self.edit_popup(|p| p.busy = true);
        let grade = |v: u8| grader.grades().iter().find(|b| b.value == v).map_or("", |b| b.label).to_lowercase();
        let done = match &action {
            Action::Grade(v) => format!("Graded {}.", grade(*v)),
            Action::Add => format!("Added to {}.", grader.deck_name().unwrap_or_default()),
            Action::Act(what) => format!("{what}: done."),
            Action::Open(_) => String::new(),
        };
        let state = self.clone();
        self.spawn_here(async move {
            let asked = word.clone();
            let result = blocking(move || match &action {
                Action::Grade(value) => grader.grade(&asked, *value, &sentence),
                Action::Add => grader.add(&asked, &sentence),
                Action::Act(what) => grader.act(&asked, what, &sentence),
                Action::Open(_) => unreachable!("handled before"),
            })
            .await;
            state.words.grading.set(false);
            if let Ok(result) = result {
                state.graded(word, done, result);
            }
        });
    }

    fn say(&self, message: &str) {
        self.edit_popup(|p| p.message = Some(message.to_owned()));
    }

    fn graded(&self, word: Word, action: String, result: Result<CardState, rubiyomi_core::grade::GradeError>) {
        let key = word.key();
        let grader = self.shared.grader.get_untracked();
        let said = match result {
            Ok(card) => {
                if let Some(old) = self.words.colours.get_untracked() {
                    let mut states = old.states.clone();
                    states.insert(key.clone(), card);
                    self.words.colours.set(Some(Rc::new(Colours::new(old.index, old.tag.clone(), old.words.clone(), states))));
                }
                self.edit_popup(|p| {
                    if matches!(&p.content, Content::Word(w) if w.key() == key) {
                        p.state = Some(card);
                    }
                });
                action
            }
            Err(e) => format!("{}: {e}", grader.as_ref().map_or("Reviews", |g| g.name())),
        };
        let shown = self.popup.with_untracked(|p| p.as_ref().is_some_and(|p| matches!(&p.content, Content::Word(w) if w.key() == key)));
        if shown {
            self.edit_popup(|p| {
                p.busy = false;
                p.message = Some(said);
            });
        } else {
            snackbar(said);
        }
    }
}

fn alpha(color: Color, a: f32) -> Rgba {
    rgba((color.to_rgb() << 8) | (a * 255.0) as u32)
}

fn chip(label: String, ink: Color, filled: Rgba, outline: Option<Rgba>) -> impl IntoElement {
    let chip = view().px(px(6.0)).py(px(2.0)).mr(px(4.0)).mb(px(4.0)).rounded(px(100.0)).bg(filled);
    let chip = match outline {
        Some(line) => chip.border_1().border_color(line),
        None => chip,
    };
    chip.child(text(label).text_size(px(10.0)).text_color(c(ink)))
}

fn tile(glyph: &'static str, label: String, s: Scheme, enabled: bool, on_press: impl Fn() + 'static) -> impl IntoElement {
    let ink = if enabled { s.on_surface } else { s.on_surface_variant };
    let tile = view()
        .h(px(36.0))
        .px_1()
        .gap(px(6.0))
        .flex_row()
        .items_center()
        .rounded(px(6.0))
        .child(icon(glyph, 18.0, c(ink)))
        .child(text(label).text_size(px(13.0)).text_color(c(ink)));
    if enabled { tile.ripples().on_press(on_press) } else { tile }
}

fn grade_button(label: &'static str, colour: Color, enabled: bool, on_press: impl Fn() + 'static) -> impl IntoElement {
    let button = view()
        .flex_1()
        .h(px(34.0))
        .mx(px(2.0))
        .items_center()
        .justify_center()
        .rounded(px(4.0))
        .bg(alpha(colour, if enabled { 0.14 } else { 0.06 }))
        .border_1()
        .border_color(alpha(colour, 0.45))
        .child(text(label).text_size(px(12.0)).text_color(alpha(colour, if enabled { 1.0 } else { 0.5 })));
    if enabled { button.ripples().on_press(on_press) } else { button }
}

fn meanings(word: &Word, s: Scheme) -> AnyElement {
    if word.meanings.is_empty() {
        return text("Not in the dictionary.").text_size(px(13.0)).text_color(c(s.on_surface_variant)).into_element();
    }
    let rows = word.meanings.iter().enumerate().map(|(i, meaning)| {
        view()
            .flex_row()
            .pb_1()
            .child(text(format!("{}.", i + 1)).text_size(px(13.0)).w(px(20.0)).text_color(c(s.on_surface_variant)))
            .child(view().flex_1().child(text(meaning.clone()).text_size(px(14.0)).line_height(px(19.6)).text_color(c(s.on_surface))))
    });
    view().selectable(true).children(rows).into_element()
}

// What can be done with the word, and its grades, which stay in sight under the rest.
fn options_face(state: &ReaderState, word: &Word, card: Option<CardState>, busy: bool, s: Scheme) -> (AnyElement, Option<AnyElement>) {
    let grader = state.shared.grader.get();
    let mut face = view();
    let act = |action: Action| {
        let state = state.clone();
        move || state.review(action.clone())
    };
    // Without reviews, jpdb.io still serves as a dictionary to look further in.
    let link = match &grader {
        Some(g) => g.link(word),
        None => rubiyomi_core::jpdb::vocabulary_url(word).map(|url| ("Open on jpdb.io", url)),
    };
    if let Some((label, url)) = link {
        face = face.child(tile(glyph::OPEN_IN_NEW, label.to_owned(), s, true, act(Action::Open(url))));
    }
    let Some(g) = grader.filter(|_| word.jmdict_id.is_some()) else { return (face.into_element(), None) };
    // Which service the rest acts on, as a deck can carry another's name.
    let on = match g.deck_name() {
        Some(deck) => format!("{} · deck {deck}", g.name()),
        None => g.name().to_owned(),
    };
    face = face.child(text(on).text_size(px(11.0)).pt_2().text_color(c(s.on_surface_variant)));
    if let Some(deck) = g.deck_name() {
        face = face.child(tile(glyph::ADD, format!("Add to {deck}"), s, !busy, act(Action::Add)));
    }
    for &a in g.actions() {
        let mark = match a {
            "Blacklist" => glyph::BLOCK,
            "Suspend" => glyph::PAUSE,
            _ => glyph::ALL_INCLUSIVE,
        };
        face = face.child(tile(mark, a.to_owned(), s, !busy, act(Action::Act(a))));
    }
    // A grader that says why a word can't be graded shows its buttons disabled, with the reason.
    let allowed = g.can_grade(word);
    let mut pinned = None;
    if has_card(card) || (card.is_some() && allowed.is_err()) {
        let grades = g.grades();
        let dark = s.dark;
        let buttons = grades.iter().map(|b| {
            // Good and bad answers keep their colours in every word palette.
            let colour = full_colour(grade_state(b.value, grades.len()), dark).unwrap_or(s.on_surface);
            grade_button(b.label, colour, !busy && allowed.is_ok(), act(Action::Grade(b.value)))
        });
        let row = view().flex_row().mt_2().children(buttons);
        let why = allowed.err().map(|why| text(why).text_size(px(12.0)).pt_1().text_color(c(s.on_surface_variant)));
        pinned = Some(view().flex_none().child(row).child(why).into_element());
    }
    (face.into_element(), pinned)
}

// What the card shows. The card itself stays from word to word, so its frame can glide.
fn card_content(state: &ReaderState, popup: &Popup, s: Scheme, grader: Option<Arc<dyn Grader>>) -> AnyElement {
    let mut card = view().flex_shrink(1.0).min_h(px(0.0));
    let word = match &popup.content {
        Content::Message(m) => {
            return text(m.clone()).text_size(px(13.0)).text_color(c(s.on_surface_variant)).into_element();
        }
        Content::Word(w) => w.clone(),
    };
    let flip = state.clone();
    let options = popup.options;
    let head =
        view().flex_row().items_start().child(view().flex_1().selectable(true).child(spelt(&word.spelling, &word.reading, s))).child(
            view()
                .w(px(48.0))
                .h(px(48.0))
                // The touch target reaches the card's corner, and the header stays its height.
                .mt(px(-8.0))
                .mr(px(-8.0))
                .items_center()
                .justify_center()
                .label(if options { "Back" } else { "Options" })
                .ripples()
                .on_press(move || flip.edit_popup(Popup::flip))
                .child(icon(if options { glyph::CLOSE } else { glyph::MORE_HORIZ }, 24.0, c(s.on_surface_variant))),
        );
    card = card.child(head);
    let card_state = popup.state.filter(|_| grader.is_some());
    let kinds = pos_labels(&word.pos);
    if !options && (card_state.is_some() || !kinds.is_empty()) {
        let mut chips = view().flex_row().flex_wrap().pt_1();
        if let Some(st) = card_state {
            let colours = state.shared.page.with(|p| StateColours::chosen(p, s.dark));
            let colour = colours.get(st).unwrap_or(s.on_surface_variant);
            chips = chips.child(chip(st.label().to_owned(), colour, alpha(colour, 0.16), Some(alpha(colour, 0.5))));
        }
        for k in kinds {
            chips = chips.child(chip(k.to_owned(), s.on_surface_variant, alpha(s.outline_variant, 0.6), None));
        }
        card = card.child(chips);
    }
    if popup.busy {
        card = card.child(view().w_full().h(px(2.0)).my_2().bg(c(s.primary)));
    }
    if let Some(m) = &popup.message {
        card = card.child(text(m.clone()).text_size(px(12.0)).pt_1().text_color(c(s.on_surface_variant)));
    }
    let (face, grades) = if options { options_face(state, &word, popup.state, popup.busy, s) } else { (meanings(&word, s), None) };
    card.child(scroll().mt_2().pr(px(6.0)).flex_shrink(1.0).min_h(px(0.0)).child(face)).child(grades).into_element()
}

// The card of the word looked up, over the page.
pub fn popup(state: &ReaderState) -> impl IntoElement {
    let state = state.clone();
    let open = {
        let state = state.clone();
        computed(move || state.popup.with(Option::is_some))
    };
    dynamic(move || {
        if !open.get() {
            return view().into_element();
        }
        fill_parent().pointer_events(PointerEvents::BoxNone).child(card(&state)).into_element()
    })
}

// The word with its reading over each run of kanji alone, or over all of it when they can't be
// told apart.
fn spelt(spelling: &str, reading: &str, s: Scheme) -> AnyElement {
    let (ink, faint) = (c(s.on_surface), c(s.on_surface_variant));
    let over = |r: &str| text(r.to_owned()).text_size(px(13.0)).line_height(px(18.0)).h(px(18.0)).text_color(faint).select_none();
    let big = |t: &str| text(t.to_owned()).text_size(px(30.0)).text_color(ink);
    match rubiyomi_core::furigana::parts(spelling, reading) {
        Some(parts) => view()
            .flex_row()
            .items_end()
            .children(parts.into_iter().map(|p| {
                view().flex_col().items_center().child(over(p.reading.as_deref().unwrap_or(""))).child(big(&p.text)).into_element()
            }))
            .into_element(),
        None => view().children((!reading.is_empty() && reading != spelling).then(|| over(reading))).child(big(spelling)).into_element(),
    }
}

const GLIDE: LayoutTransition = LayoutTransition::new(LayoutCurve::EaseOut, 160.0);

fn card(state: &ReaderState) -> AnyElement {
    let frame = element_ref();
    let (width, height) = state.size.get_untracked();
    let inner = width_for(width).min(width);
    let place = {
        let state = state.clone();
        computed(move || {
            state.popup.with(|p| {
                p.as_ref().map(|p| {
                    let middle = Rect { x: GAP, y: height / 2.0, w: 0.0, h: 0.0 };
                    let (anchor, word) = p.anchor.unwrap_or((middle, middle));
                    spot((width, height), inner, anchor, word, p.vertical)
                })
            })
        })
    };
    let edge = |side: fn(Spot) -> Option<f32>| move || place.get().and_then(side).map_or(Length::Auto, |at| px(at).into());
    let background = state.clone();
    let card = view()
        .element_ref(&frame)
        .absolute()
        .left(edge(|p| Some(p.left)))
        .top(edge(|p| p.top))
        .bottom(edge(|p| p.bottom))
        .w(px(inner))
        .max_h(px(height * MAX_HEIGHT_FRACTION))
        .flex_col()
        .pl(px(14.0))
        .pr(px(8.0))
        .py_2()
        .rounded(px(12.0))
        .overflow_hidden()
        .bg(move || c(background.scheme().surface_container_high))
        .shadow_md()
        // The page below it never sees a press on the card.
        .on_press(|| {});
    // The card can open under the mouse, and then Shift is told here.
    #[cfg(not(target_os = "android"))]
    let card = {
        let heard = state.clone();
        card.on_mouse_move(move |e: &MouseEvent| heard.shift_event(e.modifiers.shift, (e.page_position.x.0, e.page_position.y.0)))
    };
    let state = state.clone();
    // The first frame is the card appearing, in its place: only a card already up glides.
    let shown = Cell::new(false);
    card.child(dynamic(move || {
        let Some(popup) = state.popup.get() else { return view().into_element() };
        if shown.replace(true) {
            frame.animate_layout(GLIDE);
        }
        let grader = state.shared.grader.get();
        card_content(&state, &popup, state.scheme(), grader)
    }))
    .into_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    // 吾輩は猫である, 夏目漱石, Aozora Bunko 789.
    const TEXT: &str = "吾輩は猫である。名前はまだ無い。「どこで生れたか」とんと見当がつかぬ。";

    #[cfg(not(target_os = "android"))]
    #[test]
    fn a_long_shift_is_a_peek_and_a_short_one_toggles() {
        let (short, long) = (HOLD / 2, HOLD);
        // A peek closes whichever card shows, a sweep's too.
        assert!(release_closes(long, false, false));
        assert!(release_closes(long, false, true));
        assert!(release_closes(long, true, false));
        assert!(release_closes(long, true, true));
        // No card at the press: the one it opened stays.
        assert!(!release_closes(short, false, false));
        assert!(!release_closes(short, false, true));
        // A card at the press closes, on its word or over no word, unless the press moved it on.
        assert!(release_closes(short, true, false));
        assert!(!release_closes(short, true, true));
    }

    #[test]
    fn popup_when_finder_is_not_ready() {
        let waiting = Err("Preparing the dictionary…".to_owned());
        assert_eq!(lookup(&waiting, None, TEXT, 2), Some(Content::Message("Preparing the dictionary…".to_owned())));
    }

    #[test]
    fn a_word_parsed_elsewhere_needs_no_finder() {
        let word = Word { spelling: "猫".to_owned(), ..Word::default() };
        assert_eq!(lookup(&Err("x".to_owned()), Some(word.clone()), TEXT, 3), Some(Content::Word(word)));
    }

    #[test]
    fn the_sentence_is_the_one_around_the_word() {
        let around = |at| sentence(TEXT, &sentence_range(TEXT, at));
        assert_eq!(around(3), "吾輩は猫である。");
        assert_eq!(around(10), "名前はまだ無い。");
        assert_eq!(around(20), "「どこで生れたか」とんと見当がつかぬ。");
    }

    #[test]
    fn the_card_goes_under_a_high_word_and_over_a_low_one() {
        let high = Rect { x: 100.0, y: 100.0, w: 40.0, h: 20.0 };
        let low = Rect { x: 100.0, y: 700.0, w: 40.0, h: 20.0 };
        assert_eq!(spot((400.0, 800.0), 300.0, high, high, false).top, Some(128.0));
        let over = spot((400.0, 800.0), 300.0, low, low, false);
        assert_eq!((over.top, over.bottom), (None, Some(108.0)));
        assert_eq!(over.left, 92.0);
    }

    #[test]
    fn a_tall_anchor_keeps_the_card_on_screen() {
        let sentence = Rect { x: 100.0, y: 150.0, w: 40.0, h: 330.0 };
        let over = spot((400.0, 850.0), 300.0, sentence, sentence, false);
        let bottom = over.bottom.unwrap();
        assert!(bottom <= 850.0 * 0.6 - GAP);
        assert!(850.0 - bottom - 850.0 * MAX_HEIGHT_FRACTION >= GAP - 0.01);
    }

    #[test]
    fn a_sentence_too_tall_to_clear_still_leaves_the_word_in_sight() {
        let sentence = Rect { x: 20.0, y: 172.0, w: 370.0, h: 370.0 };
        let word = Rect { x: 20.0, y: 360.0, w: 120.0, h: 45.0 };
        let at = spot((411.0, 869.0), 395.0, sentence, word, false);
        assert_eq!((at.top, at.bottom), (Some(word.bottom() + GAP), None));
    }

    #[test]
    fn the_card_sits_left_of_a_column() {
        let word = Rect { x: 300.0, y: 50.0, w: 20.0, h: 80.0 };
        assert_eq!(spot((600.0, 800.0), 200.0, word, word, true).left, 92.0);
        let near_left = Rect { x: 20.0, ..word };
        assert_eq!(spot((600.0, 800.0), 200.0, near_left, near_left, true).left, 48.0);
    }

    #[test]
    fn a_phone_too_narrow_for_beside_a_column_puts_the_card_off_the_word() {
        let high = Rect { x: 180.0, y: 100.0, w: 30.0, h: 60.0 };
        let low = Rect { y: 700.0, ..high };
        let narrow = (400.0, 800.0);

        assert_eq!(spot(narrow, 384.0, high, high, true), spot(narrow, 384.0, high, high, false));
        assert_eq!(spot(narrow, 384.0, high, high, true).top, Some(high.bottom() + GAP));
        assert_eq!(spot(narrow, 384.0, low, low, true), spot(narrow, 384.0, low, low, false));
        assert_eq!(spot(narrow, 384.0, low, low, true).bottom, Some(800.0 - low.y + GAP));
    }
}
