// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use nsh_core::prelude::*;
use rubiyomi_core::reader::layout::Page;
use rubiyomi_core::stats::{Day, today};

use super::ReaderState;
use crate::ui::{self, c};

// Longer than this without touching the page is not reading.
const IDLE: f64 = 180.0;
// Faster than this, pages are being flipped through, not read.
const SKIMMING_CPM: f64 = 1500.0;
const SKIMMING_PAGE: f64 = 1.5;
// After this long without a flip, a run of skimmed pages is settled.
const SKIM_SETTLES: f64 = 3.0;
// Up to this many pages are a quick flip, dropped without asking.
const SKIM_QUIET: u32 = 2;

// Pages flipped through too fast to have been read, held back from the stats until the
// reader says whether they count.
#[derive(Clone, Debug)]
pub struct Skim {
    day: Day,
    seconds: f64,
    pages: u32,
    last: Instant,
}

struct Clock {
    // Reading done since the stats were last written.
    pending: Day,
    seconds: f64,
    last_active: Instant,
    // When the page in view came, to tell reading it from flipping past it.
    page_since: Instant,
    skim: Option<Skim>,
}

#[derive(Clone)]
pub struct Reading {
    clock: Rc<RefCell<Clock>>,
    question: Signal<Option<Skim>>,
}

impl Reading {
    pub fn new() -> Self {
        let now = Instant::now();
        let clock = Clock { pending: Day::default(), seconds: 0.0, last_active: now, page_since: now, skim: None };
        Reading { clock: Rc::new(RefCell::new(clock)), question: signal_with(None, SignalEquals::Never) }
    }

    pub fn looked_up(&self) {
        self.clock.borrow_mut().pending.lookups += 1;
    }

    // A jump by the slider is not reading the pages in between.
    pub fn skipped(&self) {
        let mut clock = self.clock.borrow_mut();
        clock.last_active = Instant::now();
        clock.page_since = clock.last_active;
    }

    // Time counts from one touch of the page to the next, unless it was left alone too long.
    // Gives the time counted, if the clock was running since the last touch.
    pub fn active(&self) -> Option<f64> {
        let mut clock = self.clock.borrow_mut();
        let now = Instant::now();
        let gap = Some(now.duration_since(clock.last_active).as_secs_f64()).filter(|gap| *gap < IDLE);
        clock.seconds += gap.unwrap_or(0.0);
        clock.last_active = now;
        gap
    }

    // A run of skimmed pages is over: a quick flip is dropped, a longer one asked about.
    pub fn settle(&self) {
        let skim = self.clock.borrow_mut().skim.take();
        if let Some(skim) = skim
            && skim.pages > SKIM_QUIET
            && self.question.with_untracked(Option::is_none)
        {
            self.question.set(Some(skim));
        }
    }
}

impl ReaderState {
    // What reading a page adds to the stats: its characters, and its words with how many are unknown.
    fn page_read(&self, page: &Page) -> Day {
        let chars = page.glyphs.iter().filter(|g| !g.text.trim().is_empty()).map(|g| g.len.max(1) as u64).sum::<u64>();
        let mut day = Day { chars, ..Day::default() };
        let index = self.index.get_untracked();
        if let Some(c) = self.words.colours.get_untracked().filter(|c| c.index == index)
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

    pub fn flush(&self) {
        let mut clock = self.reading.clock.borrow_mut();
        let mut done = std::mem::take(&mut clock.pending);
        done.seconds = clock.seconds.floor() as u64;
        clock.seconds = clock.seconds.fract();
        drop(clock);
        self.shared.stats.borrow_mut().add(today(), done);
    }

    // The reader moved on from `from`: a page forward counts as read, unless it went by too fast.
    pub fn turned(&self, from: &Page, forward: bool, new_chapter: bool) {
        let gap = self.reading.active();
        let on_page = {
            let mut clock = self.reading.clock.borrow_mut();
            let on_page = clock.page_since.elapsed().as_secs_f64();
            clock.page_since = Instant::now();
            on_page
        };
        if forward {
            let mut read = self.page_read(from);
            if gap.is_some() {
                read.timed_chars = read.chars;
            }
            read.chapters = new_chapter as u32;
            let skimmed = read.chars > 0 && (on_page < SKIMMING_PAGE || read.chars as f64 * 60.0 / on_page > SKIMMING_CPM);
            if skimmed {
                let mut clock = self.reading.clock.borrow_mut();
                if let Some(gap) = gap {
                    clock.seconds -= gap;
                }
                let skim = clock.skim.get_or_insert_with(|| Skim { day: Day::default(), seconds: 0.0, pages: 0, last: Instant::now() });
                skim.day.add(read);
                skim.pages += 1;
                skim.last = Instant::now();
                skim.seconds += gap.unwrap_or(0.0);
            } else {
                self.reading.settle();
                self.reading.clock.borrow_mut().pending.add(read);
            }
        }
        self.flush();
    }

    // Settles a run of skimmed pages once the reader has stopped flipping.
    pub fn watch_skimming(&self) {
        let reading = self.reading.clone();
        set_interval(Duration::from_secs(1), move || {
            let over = reading.clock.borrow().skim.as_ref().is_some_and(|s| s.last.elapsed().as_secs_f64() >= SKIM_SETTLES);
            if over {
                reading.settle();
            }
        });
    }

    fn answer_skim(&self, count: bool) {
        let Some(skim) = self.reading.question.get_untracked() else { return };
        self.reading.question.set(None);
        if count {
            let mut clock = self.reading.clock.borrow_mut();
            clock.pending.add(skim.day);
            clock.seconds += skim.seconds;
            drop(clock);
            self.flush();
        } else {
            self.reading.skipped();
        }
    }

    pub fn skim_dialog(&self) -> AnyElement {
        let state = self.clone();
        dynamic(move || {
            let Some(skim) = state.reading.question.get() else { return view().into_element() };
            let s = state.scheme();
            let seconds = skim.seconds.round() as u64;
            let time = if seconds < 60 { format!("{seconds} s") } else { format!("{} min {} s", seconds / 60, seconds % 60) };
            let body = format!(
                "{} pages, {} characters, went by in {time}: faster than reading. Count them in your stats, or leave them out and measure from now?",
                skim.pages, skim.day.chars
            );
            let button = |label: &'static str, count: bool| {
                let state = state.clone();
                ui::tonal_button_in(label, move || s, move || state.answer_skim(count))
            };
            ui::scrim()
                .items_center()
                .justify_center()
                .child(
                    view()
                        .w(px(state.size.get_untracked().0.min(368.0) - 48.0))
                        .p_6()
                        .rounded(px(28.0))
                        .bg(c(s.surface_container_high))
                        .on_press(|| {})
                        .child(text("Skimmed?").text_size(px(24.0)).pb_2().text_color(c(s.on_surface)))
                        .child(text(body).text_size(px(14.0)).text_color(c(s.on_surface_variant)))
                        .child(view().flex_row().justify_end().pt_4().child(button("Leave out", false)).child(button("Count them", true))),
                )
                .into_element()
        })
        .into_element()
    }
}
