// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

// English for the words with a kanji the reader can't read yet. A kanji is readable once it is in
// a word the reviews service counts as studied, or the service taught it on its own.

use std::collections::HashSet;

use crate::cards::CardState;
use crate::finder::Word;
use crate::furigana::is_kanji;
use crate::reader::layout::{LayoutConfig, Measure, Page, PlacedGlyph, PlacedRuby};
use crate::text::Chapter;

pub type Readable = HashSet<char>;

// Seen and learnt at least once: the word's kanji are no longer strangers.
pub fn studied(state: CardState) -> bool {
    state.is_known() || matches!(state, CardState::Learning | CardState::Due | CardState::Failed)
}

// A kanji to know, leaving out the repeat and abbreviation marks, which read as what they stand for.
fn is_real_kanji(c: char) -> bool {
    is_kanji(c) && !matches!(c, '々' | '〆' | 'ヶ' | 'ヵ')
}

pub fn add_kanji(readable: &mut Readable, spelling: &str) {
    readable.extend(spelling.chars().filter(|c| is_real_kanji(*c)));
}

pub fn readable_of<'a>(spellings: impl IntoIterator<Item = &'a str>) -> Readable {
    let mut out = Readable::new();
    for s in spellings {
        add_kanji(&mut out, s);
    }
    out
}

fn is_numeral(c: char) -> bool {
    "〇一二三四五六七八九十百千万億兆零壱弐参拾".contains(c)
}

// Whether a word, as the text has it, gets its English: it has a kanji not readable, and is
// neither a name nor a number.
pub fn needs_english(word: &Word, surface: &str, readable: &Readable) -> bool {
    let mut kanji = surface.chars().filter(|c| is_real_kanji(*c)).peekable();
    if word.jmdict_id.is_none() || word.is_name() || kanji.peek().is_none() {
        return false;
    }
    let kanji: Vec<char> = kanji.collect();
    !kanji.iter().all(|c| is_numeral(*c)) && kanji.iter().any(|c| !readable.contains(c))
}

// How many chars of the word as the text has it the English stands for: what it shares with the
// dictionary form, and at least up to its last kanji. The kana after it are the ending, which
// stays: 信じられなかった, 信じる → 信じ.
pub fn stem_len(surface: &str, spelling: &str) -> usize {
    let chars: Vec<char> = surface.chars().collect();
    let shared = chars.iter().zip(spelling.chars()).take_while(|(a, b)| **a == *b).count();
    let to_last_kanji = chars.iter().rposition(|c| is_kanji(*c)).map_or(0, |i| i + 1);
    shared.max(to_last_kanji)
}

fn without_brackets(s: &str) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    for c in s.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

// One or two words of the first sense, the most common one: its first gloss that short, else its
// shortest. The ending stays in Japanese, so a verb loses its "to".
pub fn short_meaning(senses: &[String]) -> Option<String> {
    let glosses: Vec<String> = senses
        .first()?
        .split("; ")
        .map(without_brackets)
        .map(|g| g.strip_prefix("to ").map(str::to_owned).unwrap_or(g))
        .filter(|g| !g.is_empty())
        .collect();
    let words = |g: &String| g.split_whitespace().count();
    glosses.iter().find(|g| words(g) <= 2).or_else(|| glosses.iter().min_by_key(|g| words(g))).cloned()
}

// The English shown for a word: over `stem` chars from `start`, of a word `len` long.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gloss {
    pub start: usize,
    pub stem: usize,
    pub len: usize,
    pub text: String,
}

// The chars of a chapter, at chapter offsets; an image counts one.
pub fn chapter_chars(chapter: &Chapter) -> Vec<char> {
    let mut out = Vec::with_capacity(chapter.char_len());
    for b in &chapter.blocks {
        match b.as_text() {
            Some(t) => out.extend(t.text.chars()),
            None => out.extend(std::iter::repeat_n('\u{fffc}', b.char_len())),
        }
    }
    out
}

// The words of a chapter that get their English, in order. `meanings` gives a word's senses when
// it came without them.
pub fn glosses(words: &[Word], chars: &[char], readable: &Readable, meanings: impl Fn(&Word) -> Vec<String>) -> Vec<Gloss> {
    let mut out = Vec::new();
    for (i, w) in words.iter().enumerate() {
        // As the colours: a word ends where the next starts.
        let end = words.get(i + 1).map_or(usize::MAX, |next| next.start).min(w.start + w.len).min(chars.len());
        if end <= w.start {
            continue;
        }
        let surface: String = chars[w.start..end].iter().collect();
        if !needs_english(w, &surface, readable) {
            continue;
        }
        let found;
        let senses = if w.meanings.is_empty() {
            found = meanings(w);
            &found
        } else {
            &w.meanings
        };
        let Some(text) = short_meaning(senses) else { continue };
        out.push(Gloss { start: w.start, stem: stem_len(&surface, &w.spelling).min(end - w.start), len: end - w.start, text });
    }
    out
}

// The glosses still wanted once more kanji are readable, as after a grade.
pub fn still_unread(glosses: &[Gloss], chars: &[char], readable: &Readable) -> Vec<Gloss> {
    glosses
        .iter()
        .filter(|g| chars.get(g.start..g.start + g.len).is_some_and(|w| w.iter().any(|c| is_real_kanji(*c) && !readable.contains(c))))
        .cloned()
        .collect()
}

// The English of the page's words over their stems, where furigana go.
pub fn above(page: &Page, glosses: &[Gloss], cfg: &LayoutConfig, measure: &dyn Measure) -> Vec<PlacedRuby> {
    let at = crate::furigana::glyphs_by_offset(page);
    let first = glosses.partition_point(|g| g.start + g.len <= page.start);
    let mut out = Vec::new();
    for g in glosses[first..].iter().take_while(|g| g.start < page.end) {
        let glyphs: Option<Vec<&PlacedGlyph>> = (g.start..g.start + g.stem).map(|o| at.get(&o).copied()).collect();
        if let Some(glyphs) = glyphs {
            out.extend(crate::furigana::place(&glyphs, g.text.clone(), true, cfg, measure));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(spelling: &str) -> Word {
        Word { spelling: spelling.into(), jmdict_id: Some(1), ..Word::default() }
    }

    // Forms from 芥川龍之介「羅生門」 and 夏目漱石「吾輩は猫である」, Aozora Bunko 127 and 789.
    #[test]
    fn the_ending_stays_and_the_stem_goes() {
        assert_eq!(stem_len("信じられなかった", "信じる"), 2);
        assert_eq!(stem_len("待っていた", "待つ"), 1);
        assert_eq!(stem_len("記憶している", "記憶"), 2);
        assert_eq!(stem_len("生れた", "生まれる"), 1, "spelt otherwise than the dictionary: up to the last kanji");
        assert_eq!(stem_len("薄暗い", "薄暗い"), 3, "the dictionary form itself is all stem");
        assert_eq!(stem_len("見当", "見当"), 2);
        assert_eq!(stem_len("引き出した", "引き出す"), 3);
    }

    #[test]
    fn a_word_with_a_kanji_not_read_gets_english() {
        let readable = readable_of(["下人", "雨"]);
        assert!(needs_english(&word("羅生門"), "羅生門", &readable));
        assert!(!needs_english(&word("下人"), "下人", &readable));
        assert!(!needs_english(&word("人下"), "人下", &readable), "kanji known from other words");
        assert!(needs_english(&word("雨やみ"), "雨やみ", &readable_of(["下人"])));
        assert!(!needs_english(&word("ある"), "ある", &readable), "kana alone");
        assert!(!needs_english(&word("人々"), "人々", &readable), "the repeat mark is no kanji to learn");
        assert!(!needs_english(&word("二十"), "二十", &Readable::new()), "a number");
        assert!(!needs_english(&Word { name: true, ..word("羅生門") }, "羅生門", &readable), "a name");
        assert!(!needs_english(&Word { jmdict_id: None, ..word("羅生門") }, "羅生門", &readable), "not in the dictionary");
    }

    #[test]
    fn studied_means_seen_and_learnt() {
        use CardState::*;
        for s in [Known, NeverForget, Learning, Due, Failed, Blacklisted, Redundant] {
            assert!(studied(s), "{s:?}");
        }
        for s in [New, NotInDeck, Locked, Suspended, Unknown] {
            assert!(!studied(s), "{s:?}");
        }
    }

    #[test]
    fn the_meaning_is_short_and_has_no_to() {
        let s = |x: &str| vec![x.to_owned()];
        assert_eq!(short_meaning(&s("to believe; to trust; to have faith in")).as_deref(), Some("believe"));
        assert_eq!(short_meaning(&s("companion; fellow; friend; mate")).as_deref(), Some("companion"));
        assert_eq!(short_meaning(&s("(something) that is very rare indeed; oddity")).as_deref(), Some("oddity"));
        assert_eq!(
            short_meaning(&s("a very long phrase here; a slightly long phrase")).as_deref(),
            Some("a slightly long phrase"),
            "the shortest"
        );
        assert_eq!(short_meaning(&["dim (light)".into(), "second sense".into()]).as_deref(), Some("dim"));
        assert_eq!(short_meaning(&[]), None);
    }

    #[test]
    fn glosses_cover_the_stem_of_each_word_not_read() {
        // 「下人の行方は、誰も知らない。」羅生門
        let text: Vec<char> = "下人の行方は誰も知らない".chars().collect();
        let w = |spelling: &str, start: usize, len: usize| Word { start, len, ..word(spelling) };
        let words = vec![w("下人", 0, 2), w("行方", 3, 2), w("誰", 6, 1), w("知る", 8, 4)];
        let readable = readable_of(["下人", "誰"]);
        let got = glosses(&words, &text, &readable, |w| match w.spelling.as_str() {
            "行方" => vec!["whereabouts".into()],
            "知る" => vec!["to know; to be aware of".into()],
            _ => Vec::new(),
        });
        assert_eq!(
            got,
            [Gloss { start: 3, stem: 2, len: 2, text: "whereabouts".into() }, Gloss { start: 8, stem: 1, len: 4, text: "know".into() }]
        );
    }
}
