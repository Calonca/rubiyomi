// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use nsh_core::prelude::*;
use rubiyomi_core::cards::CardState;
use rubiyomi_core::finder::{Finder, Word, jpdb_words_tag, words_tag};
use rubiyomi_core::grade::{Grader, StateMap};
use rubiyomi_core::jpdb::JpdbClient;
use rubiyomi_core::settings::{CustomColours, PageSettings, WordColours, WordPalette};
use rubiyomi_core::text::Chapter;
use rubiyomi_core::theme::Color;

use super::ReaderState;
use crate::ui::snackbar;

// How a character of the page is inked: its colour, and the line under it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ink {
    pub colour: Option<Color>,
    pub line: Option<Color>,
}

// Cuts a line's text into runs of one ink, for as many spans. `ink` is asked for each char.
pub fn spans(text: &str, ink: impl Fn(usize) -> Ink) -> Vec<(String, Ink)> {
    let mut out: Vec<(String, Ink)> = Vec::new();
    for (i, ch) in text.chars().enumerate() {
        let here = ink(i);
        match out.last_mut() {
            Some((run, ink)) if *ink == here => run.push(ch),
            _ => out.push((ch.to_string(), here)),
        }
    }
    out
}

// Every state has its colour; only a state not known yet has none.
pub fn full_colour(state: CardState, dark: bool) -> Option<Color> {
    let v = match state {
        CardState::Known => [0x2e7d32, 0x69c67a],
        CardState::NeverForget => [0x00695c, 0x4db6ac],
        CardState::Learning => [0x9a6a00, 0xe7b95a],
        CardState::Due => [0xb35309, 0xef9a55],
        CardState::Failed => [0xc62828, 0xe06c6c],
        CardState::New => [0x1565c0, 0x6ba8e8],
        CardState::NotInDeck => [0x6b7280, 0x8a93a3],
        CardState::Locked => [0x5e35b1, 0x9b8ec4],
        CardState::Redundant => [0x78909c, 0x7f8896],
        CardState::Suspended => [0x90a4ae, 0x6d737d],
        CardState::Blacklisted => [0xb0bec5, 0x585e68],
        CardState::Unknown => return None,
    }[dark as usize];
    Some(Color::from_rgb(v))
}

// Colours what can be acted on; what can't be, or is already known, stays plain.
fn minimal_colour(state: CardState, dark: bool) -> Option<Color> {
    match state {
        CardState::New | CardState::Learning | CardState::Due | CardState::Failed => full_colour(state, dark),
        CardState::NotInDeck => Some(Color::from_rgb([0x90a4ae, 0x4a4f58][dark as usize])),
        _ => None,
    }
}

// None is plain ink.
pub fn state_colour(state: CardState, dark: bool, palette: WordPalette, custom: &CustomColours) -> Option<Color> {
    match palette {
        WordPalette::Full => full_colour(state, dark),
        WordPalette::Minimal => minimal_colour(state, dark),
        WordPalette::Custom => match custom.get(&state) {
            Some(pair) => pair.map(|p| Color::from_rgb(p[dark as usize])),
            None => full_colour(state, dark),
        },
    }
}

// Every state's colour in a palette, resolved once: `at` asks for each char of a page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StateColours([Option<Color>; CardState::ALL.len()]);

impl StateColours {
    pub fn of(palette: WordPalette, custom: &CustomColours, dark: bool) -> Self {
        StateColours(CardState::ALL.map(|state| state_colour(state, dark, palette, custom)))
    }

    pub fn chosen(page: &PageSettings, dark: bool) -> Self {
        Self::of(page.palette, &page.custom_colours, dark)
    }

    pub fn get(&self, state: CardState) -> Option<Color> {
        self.0[state as usize]
    }
}

#[derive(Clone)]
pub struct Colours {
    pub index: usize,
    // What the words were found with, so a change of finder finds them again.
    pub tag: Option<String>,
    // Sorted by start, in chapter offsets.
    pub words: Arc<Vec<Word>>,
    pub states: StateMap,
    // The card state of each char of the chapter: `ink` runs for every char of every redraw,
    // so it reads this instead of finding the word and hashing its key.
    state_at: Vec<Option<CardState>>,
}

impl Colours {
    pub fn new(index: usize, tag: Option<String>, words: Arc<Vec<Word>>, states: StateMap) -> Self {
        let mut state_at = vec![None; words.last().map_or(0, |w| w.start + w.len)];
        for (i, w) in words.iter().enumerate() {
            // As `word_at`: a word ends where the next starts.
            let end = words.get(i + 1).map_or(usize::MAX, |next| next.start).min(w.start + w.len);
            if let (Some(state), Some(chars)) = (states.get(&w.key()), state_at.get_mut(w.start..end)) {
                chars.fill(Some(*state));
            }
        }
        Colours { index, tag, words, states, state_at }
    }

    pub fn word_at(&self, offset: usize) -> Option<&Word> {
        let w = &self.words[self.words.partition_point(|w| w.start <= offset).checked_sub(1)?];
        (offset < w.start + w.len).then_some(w)
    }
}

// A chapter's words, from jpdb.io when it parses, else from the finder on the device; and
// why jpdb.io failed, as words found by the fallback aren't saved.
pub fn find_words(chapter: &Chapter, finder: Option<&Finder>, parser: Option<&JpdbClient>) -> (Vec<Word>, Option<String>) {
    let failed = match parser.map(|p| p.chapter_words(chapter)) {
        Some(Ok(words)) => return (words, None),
        Some(Err(e)) => {
            log::warn!("jpdb.io couldn't parse the chapter: {e}");
            Some(e.to_string())
        }
        None => None,
    };
    (finder.map(|f| f.chapter_words(chapter)).unwrap_or_default(), failed)
}

#[derive(Clone)]
pub struct Words {
    pub colours: Signal<Option<Rc<Colours>>>,
    colouring: Rc<Cell<u64>>,
    // The look-up last asked for, so a slow answer doesn't replace a newer card.
    pub looking: Rc<Cell<u64>>,
    pub grading: Rc<Cell<bool>>,
    prefetched: Rc<RefCell<HashSet<usize>>>,
    told_failed: Rc<Cell<bool>>,
}

impl Words {
    pub fn new() -> Self {
        Words {
            colours: signal_with(None, SignalEquals::Never),
            colouring: Rc::default(),
            looking: Rc::default(),
            grading: Rc::default(),
            prefetched: Rc::default(),
            told_failed: Rc::default(),
        }
    }
}

type Found = (Colours, Option<String>);

// What the chars of a page are inked with, read once for the page.
pub struct PageInk {
    mark: Option<(Range<usize>, Color)>,
    mode: WordColours,
    palette: StateColours,
    colours: Option<Rc<Colours>>,
}

impl PageInk {
    // What a char of the chapter looks like: marked as the word last looked up, or coloured by
    // what its word's card is.
    pub fn at(&self, offset: usize) -> Ink {
        let mut ink = Ink::default();
        if let Some((range, colour)) = &self.mark
            && range.contains(&offset)
        {
            ink.line = Some(*colour);
        }
        let state = self.colours.as_ref().and_then(|c| c.state_at.get(offset).copied().flatten());
        let colour = state.and_then(|s| self.palette.get(s));
        match self.mode {
            WordColours::Off => {}
            WordColours::Text => ink.colour = colour,
            WordColours::Underline => ink.line = ink.line.or(colour),
        }
        ink
    }
}

impl ReaderState {
    // Reading it subscribes to what the page's ink depends on.
    pub fn page_ink(&self) -> PageInk {
        let scheme = self.scheme();
        let (mode, palette) = self.shared.page.with(|p| (p.colours, StateColours::chosen(p, scheme.dark)));
        let index = self.index.get_untracked();
        let mark = self.mark.get().map(|(start, len)| (start as usize..(start + len) as usize, scheme.primary));
        let colours = (mode != WordColours::Off).then(|| self.words.colours.get().filter(|c| c.index == index)).flatten();
        PageInk { mark, mode, palette, colours }
    }

    // The words of the chapter shown are found aside and saved, so a chapter seen before is
    // coloured at once; the next chapter's are found ahead. Runs in an effect.
    pub fn colour(&self) {
        let index = self.index.get();
        let chapter = self.chapter.get();
        let finder = self.shared.finder.get().ok();
        let (grader, parser) = (self.shared.grader.get(), self.shared.parser.get());
        // Words found by jpdb.io are wanted for looking up too, so they're found without a grader.
        if grader.is_none() && parser.is_none() && !crate::hooks::fake_states_on() {
            if self.words.colours.get_untracked().is_some() {
                self.words.colours.set(None);
            }
            return;
        }
        let tag = self.words_tag_for(parser.as_deref());
        let known = self.words.colours.get_untracked().filter(|c| c.index == index && c.tag == tag).map(|c| c.words.clone());
        // Read here only when nothing could find the words, so the check is the read.
        let mut on_disk = None;
        if known.is_none() && finder.is_none() && parser.is_none() {
            on_disk = tag.as_ref().and_then(|t| self.shared.library.cached_words(&self.meta.id, index, t));
            if on_disk.is_none() {
                return;
            }
        }
        let job = self.words.colouring.get() + 1;
        self.words.colouring.set(job);
        let chapter = known.is_none().then(|| chapter.clone());
        let (library, id) = (self.shared.library.clone(), self.meta.id.clone());
        let (state, saving) = (self.clone(), tag.clone());
        let (grader_then, parser_then, finder_then) = (grader.clone(), parser.clone(), finder.clone());
        spawn(async move {
            let found = blocking(move || -> Found {
                let mut failed = None;
                let words =
                    known.or_else(|| on_disk.or_else(|| saving.as_ref().and_then(|t| library.cached_words(&id, index, t))).map(Arc::new));
                let words = words.unwrap_or_else(|| {
                    let Some(chapter) = &chapter else { return Arc::default() };
                    let (words, why) = find_words(chapter, finder_then.as_deref(), parser_then.as_deref());
                    match (&why, &saving) {
                        (None, Some(t)) => library.cache_words(&id, index, t, &words),
                        _ => failed = why,
                    }
                    Arc::new(words)
                });
                let states = grader_then
                    .map(|g| g.states(&words))
                    .unwrap_or_else(|| crate::hooks::fake_states(words.iter().map(Word::key), 7).unwrap_or_default());
                (Colours::new(index, saving, words, states), failed)
            })
            .await;
            let Ok((colours, failed)) = found else { return };
            if state.words.colouring.get() != job {
                return;
            }
            if let Some(e) = failed
                && !state.words.told_failed.replace(true)
            {
                snackbar(format!("jpdb.io couldn't find the words ({e}), so this device did. See Settings → Parsing."));
            }
            let words = colours.words.clone();
            state.words.colours.set(Some(Rc::new(colours)));
            state.ask_fresh(job, index, words, grader);
            state.prefetch(index + 1, finder, parser);
        });
    }

    // What the words of a chapter are cached under: jpdb.io's tag when it parses, else the finder's.
    fn words_tag_for(&self, parser: Option<&JpdbClient>) -> Option<String> {
        match parser {
            Some(_) => Some(jpdb_words_tag()),
            None => self.shared.dicts.installed().map(|v| words_tag(&v)),
        }
    }

    // The saved states show at once; fresh ones follow when the reviews service answers.
    fn ask_fresh(&self, job: u64, index: usize, words: Arc<Vec<Word>>, grader: Option<Arc<dyn Grader>>) {
        let Some(grader) = grader else { return };
        let state = self.clone();
        spawn(async move {
            let Some(tag) = state.words.colours.get_untracked().filter(|c| c.index == index).map(|c| c.tag.clone()) else { return };
            let asked = blocking(move || {
                if let Err(e) = grader.refresh(&words) {
                    log::warn!("asking for card states: {e}");
                }
                let states = grader.states(&words);
                Colours::new(index, tag, words, states)
            })
            .await;
            let Ok(colours) = asked else { return };
            if state.words.colouring.get() != job {
                return;
            }
            state.words.colours.set(Some(Rc::new(colours)));
        });
    }

    // Finds and saves the next chapter's words while this one is read.
    fn prefetch(&self, next: usize, finder: Option<Arc<Finder>>, parser: Option<Arc<JpdbClient>>) {
        if next >= self.meta.chapters.len() || !self.words.prefetched.borrow_mut().insert(next) {
            return;
        }
        let tag = self.words_tag_for(parser.as_deref());
        let Some(tag) = tag.filter(|_| finder.is_some() || parser.is_some()) else {
            self.words.prefetched.borrow_mut().remove(&next);
            return;
        };
        let (library, id) = (self.shared.library.clone(), self.meta.id.clone());
        spawn(async move {
            let _ = blocking(move || {
                if library.cached_words(&id, next, &tag).is_some() {
                    return;
                }
                if let Some(chapter) = library.chapter(&id, next)
                    && let (words, None) = find_words(&chapter, finder.as_deref(), parser.as_deref())
                {
                    library.cache_words(&id, next, &tag, &words);
                }
            })
            .await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 吾輩は猫である, 夏目漱石, Aozora Bunko 789.
    const OPENING: &str = "吾輩は猫である。";

    fn word(spelling: &str, start: usize, len: usize) -> Word {
        Word { spelling: spelling.into(), start, len, ..Word::default() }
    }

    #[test]
    fn a_char_has_the_state_of_the_word_word_at_finds() {
        let words = vec![word("吾輩", 0, 2), word("猫", 3, 1), word("で", 4, 3), word("ある", 5, 2), word("紙", 9, 1)];
        let states: StateMap = words.iter().take(4).map(|w| (w.key(), CardState::Learning)).collect();
        let colours = Colours::new(0, None, Arc::new(words), states);

        for offset in 0..12 {
            let by_word = colours.word_at(offset).and_then(|w| colours.states.get(&w.key()).copied());
            assert_eq!(colours.state_at.get(offset).copied().flatten(), by_word, "at {offset}");
        }
    }

    #[test]
    fn the_mark_stays_when_word_colours_are_off() {
        let colours =
            Rc::new(Colours::new(0, None, Arc::new(vec![word("猫", 0, 2)]), StateMap::from([(word("猫", 0, 2).key(), CardState::Known)])));
        let mark_colour = Color::from_rgb(0x2f6fed);
        let palette = StateColours::of(WordPalette::Full, &CustomColours::new(), true);
        let ink = |mode| PageInk { mark: Some((1..2, mark_colour)), mode, palette, colours: Some(colours.clone()) };

        assert_eq!(ink(WordColours::Off).at(1), Ink { colour: None, line: Some(mark_colour) });
        assert_eq!(ink(WordColours::Off).at(0), Ink::default());
        assert_eq!(ink(WordColours::Text).at(0).colour, full_colour(CardState::Known, true));
        assert_eq!(ink(WordColours::Underline).at(1).line, Some(mark_colour));
    }

    #[test]
    fn spans_cover_the_line_in_order() {
        let red = Ink { colour: Some(Color::from_rgb(0xc62828)), line: None };
        let ink = |i: usize| match i {
            0..=1 | 5..=6 => red,
            _ => Ink::default(),
        };
        let runs = spans(OPENING, ink);
        assert_eq!(runs.iter().map(|(t, _)| t.as_str()).collect::<String>(), OPENING);
        assert_eq!(runs.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>(), ["吾輩", "は猫で", "ある", "。"]);
        assert_eq!(runs[0].1, red);
    }

    #[test]
    fn a_line_of_one_ink_is_one_span() {
        assert_eq!(spans(OPENING, |_| Ink::default()).len(), 1);
        assert!(spans("", |_| Ink::default()).is_empty());
    }

    #[test]
    fn unknown_words_have_no_colour() {
        assert_eq!(full_colour(CardState::Unknown, false), None);
        assert!(full_colour(CardState::Known, true).is_some());
    }

    #[test]
    fn minimal_colours_what_can_be_acted_on() {
        use CardState::*;
        for dark in [false, true] {
            for state in [New, Learning, Due, Failed] {
                assert_eq!(minimal_colour(state, dark), full_colour(state, dark));
            }
            for state in [Known, NeverForget, Blacklisted, Suspended, Redundant, Locked, Unknown] {
                assert_eq!(minimal_colour(state, dark), None, "{state:?}");
            }
        }
        assert_eq!(minimal_colour(NotInDeck, false), Some(Color::from_rgb(0x90a4ae)));
        assert_eq!(minimal_colour(NotInDeck, true), Some(Color::from_rgb(0x4a4f58)));
    }

    #[test]
    fn a_custom_palette_starts_as_full_and_changes_a_state_at_a_time() {
        let mut custom = CustomColours::new();
        assert_eq!(StateColours::of(WordPalette::Custom, &custom, true), StateColours::of(WordPalette::Full, &custom, true));

        custom.insert(CardState::Known, Some([0x111111, 0xeeeeee]));
        custom.insert(CardState::New, None);
        let (light, dark) = (StateColours::of(WordPalette::Custom, &custom, false), StateColours::of(WordPalette::Custom, &custom, true));
        assert_eq!(light.get(CardState::Known), Some(Color::from_rgb(0x111111)));
        assert_eq!(dark.get(CardState::Known), Some(Color::from_rgb(0xeeeeee)));
        assert_eq!(dark.get(CardState::New), None);
        assert_eq!(dark.get(CardState::Due), full_colour(CardState::Due, true));
    }

    #[test]
    fn every_state_reads_its_own_colour() {
        let colours = StateColours::of(WordPalette::Full, &CustomColours::new(), false);
        for state in CardState::ALL {
            assert_eq!(colours.get(state), full_colour(state, false), "{state:?}");
        }
    }
}
