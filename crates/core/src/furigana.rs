// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::collections::HashMap;
use std::ops::Range;

use crate::finder::Word;
use crate::kana::to_hiragana;
use crate::reader::layout::{LayoutConfig, Measure, Page, PlacedGlyph, PlacedRuby, Rect};

pub fn is_kanji(c: char) -> bool {
    matches!(c, '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}' | '\u{20000}'..='\u{3134F}' | '々' | '〆' | 'ヶ' | 'ヵ')
}

// A piece of a word: a run of kanji with what it reads as, or kana standing for themselves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    pub text: String,
    pub reading: Option<String>,
}

fn runs(word: &str) -> Vec<(bool, Vec<char>)> {
    let mut out: Vec<(bool, Vec<char>)> = Vec::new();
    for c in word.chars() {
        let kanji = is_kanji(c);
        match out.last_mut() {
            Some((k, run)) if *k == kanji => run.push(c),
            _ => out.push((kanji, vec![c])),
        }
    }
    out
}

// Each kanji run reads as what lies in the reading between the kana around it.
fn align(runs: &[(bool, Vec<char>)], reading: &[char]) -> Option<Vec<Range<usize>>> {
    let Some(((kanji, run), rest)) = runs.split_first() else { return reading.is_empty().then(Vec::new) };
    if !kanji {
        let kana: Vec<char> = to_hiragana(&run.iter().collect::<String>()).chars().collect();
        if !reading.starts_with(&kana) {
            return None;
        }
        return Some(shifted(align(rest, &reading[kana.len()..])?, kana.len()));
    }
    // A run with nothing after it reads as the rest; else it takes the shortest reading that lets
    // the rest line up.
    let ends: Box<dyn Iterator<Item = usize>> =
        if rest.is_empty() { Box::new(std::iter::once(reading.len())) } else { Box::new(1..reading.len()) };
    for end in ends.filter(|&e| e >= 1) {
        if let Some(tail) = align(rest, &reading[end..]) {
            return Some(std::iter::once(0..end).chain(shifted(tail, end)).collect());
        }
    }
    None
}

fn shifted(spans: Vec<Range<usize>>, by: usize) -> Vec<Range<usize>> {
    spans.into_iter().map(|r| r.start + by..r.end + by).collect()
}

// The word in pieces, its reading over each run of kanji; none when the reading doesn't fit it.
pub fn parts(spelling: &str, reading: &str) -> Option<Vec<Part>> {
    let runs = runs(spelling);
    if !runs.iter().any(|(k, _)| *k) {
        return None;
    }
    let reading: Vec<char> = to_hiragana(reading).chars().collect();
    let mut spans = align(&runs, &reading)?.into_iter();
    Some(
        runs.into_iter()
            .map(|(kanji, run)| Part {
                text: run.into_iter().collect(),
                reading: if kanji { spans.next().map(|r| reading[r].iter().collect()) } else { None },
            })
            .collect(),
    )
}

// Readings for the kanji of a word as the text has it, in chars into it. Only the kana of an
// inflected word differ from its dictionary form, so its kanji runs are the dictionary form's.
pub fn over_text(text: &str, spelling: &str, reading: &str) -> Vec<(Range<usize>, String)> {
    let Some(parts) = parts(spelling, reading) else { return Vec::new() };
    let mut known = parts.iter().filter_map(|p| Some((p.text.as_str(), p.reading.as_deref()?)));
    let mut out = Vec::new();
    let mut at = 0;
    for (kanji, run) in runs(text) {
        let len = run.len();
        if kanji {
            let run: String = run.into_iter().collect();
            match known.next() {
                Some((spelt, read)) if spelt == run => out.push((at..at + len, read.to_owned())),
                _ => return Vec::new(),
            }
        }
        at += len;
    }
    out
}

// The dictionary's readings of the page's words, over their kanji, where the book's would be.
// `words` are in chapter order. A run of kanji broken over two lines gets its reading over the
// longer piece.
pub fn on_page(page: &Page, words: &[Word], cfg: &LayoutConfig, measure: &dyn Measure) -> Vec<PlacedRuby> {
    let mut at: HashMap<usize, &PlacedGlyph> = HashMap::new();
    for g in &page.glyphs {
        for offset in g.offset..g.offset + g.len.max(1) {
            at.insert(offset, g);
        }
    }
    let first = words.partition_point(|w| w.start + w.len <= page.start);
    let mut out = Vec::new();
    for w in words[first..].iter().take_while(|w| w.start < page.end) {
        let glyphs: Option<Vec<&PlacedGlyph>> = (w.start..w.start + w.len).map(|o| at.get(&o).copied()).collect();
        let Some(glyphs) = glyphs else { continue };
        let text: Option<String> = glyphs.iter().enumerate().map(|(i, g)| g.text.chars().nth(w.start + i - g.offset)).collect();
        let Some(text) = text else { continue };
        let given = w.ruby.iter().map(|r| (r.start as usize..(r.start + r.len) as usize, r.text.clone())).filter(|(r, _)| r.end <= w.len);
        let readings: Vec<_> = if w.ruby.is_empty() { over_text(&text, &w.spelling, &w.reading) } else { given.collect() };
        for (range, reading) in readings {
            out.extend(place(&glyphs[range], reading, cfg, measure));
        }
    }
    out
}

fn place(glyphs: &[&PlacedGlyph], reading: String, cfg: &LayoutConfig, measure: &dyn Measure) -> Option<PlacedRuby> {
    let line = |g: &PlacedGlyph| if cfg.vertical { g.rect.x } else { g.rect.y };
    let mut pieces: Vec<Vec<&PlacedGlyph>> = Vec::new();
    for g in glyphs {
        match pieces.last_mut() {
            Some(piece) if line(piece[0]) == line(g) => {
                if !piece.iter().any(|p| std::ptr::eq(*p, *g)) {
                    piece.push(g);
                }
            }
            _ => pieces.push(vec![g]),
        }
    }
    let piece = pieces.into_iter().max_by_key(Vec::len)?;
    let (first, size) = (piece[0], piece[0].size);
    let ruby = size * cfg.ruby_scale;
    let rect = if cfg.vertical {
        let (top, bottom) = piece.iter().fold((f32::MAX, f32::MIN), |(t, b), g| (t.min(g.rect.y), b.max(g.rect.bottom())));
        let extent = reading.chars().count() as f32 * ruby;
        Rect { x: first.rect.x + size, y: top + (bottom - top - extent) / 2.0, w: ruby, h: extent }
    } else {
        let (left, right) = piece.iter().fold((f32::MAX, f32::MIN), |(l, r), g| (l.min(g.rect.x), r.max(g.rect.right())));
        let extent = measure.advance(&reading, ruby);
        Rect { x: left + (right - left - extent) / 2.0, y: first.rect.y - ruby, w: extent, h: ruby }
    };
    Some(PlacedRuby { text: reading, rect, size: ruby, block: first.block, run: first.run, offset: first.offset, generated: true })
}

// Readings side by side on a line that would run into each other are made smaller, each around
// its own centre, until they just meet.
pub fn fit(ruby: &mut [PlacedRuby], vertical: bool) {
    let line = |r: &PlacedRuby| if vertical { r.rect.x } else { r.rect.bottom() };
    let (at, extent) =
        (|r: &PlacedRuby| if vertical { r.rect.y } else { r.rect.x }, |r: &PlacedRuby| if vertical { r.rect.h } else { r.rect.w });
    let mut order: Vec<usize> = (0..ruby.len()).collect();
    order.sort_by(|&a, &b| line(&ruby[a]).total_cmp(&line(&ruby[b])).then(at(&ruby[a]).total_cmp(&at(&ruby[b]))));
    let mut scale = vec![1.0f32; ruby.len()];
    for pair in order.windows(2) {
        let (a, b) = (&ruby[pair[0]], &ruby[pair[1]]);
        if line(a) != line(b) {
            continue;
        }
        let apart = (at(b) + extent(b) / 2.0) - (at(a) + extent(a) / 2.0);
        let wanted = (extent(a) + extent(b)) / 2.0;
        if wanted > apart && wanted > 0.0 {
            let s = (apart / wanted).max(0.5);
            scale[pair[0]] = scale[pair[0]].min(s);
            scale[pair[1]] = scale[pair[1]].min(s);
        }
    }
    for (r, s) in ruby.iter_mut().zip(scale) {
        if s >= 1.0 {
            continue;
        }
        let rect = r.rect;
        r.size *= s;
        r.rect = if vertical {
            let h = rect.h * s;
            Rect { y: rect.y + (rect.h - h) / 2.0, h, w: rect.w * s, ..rect }
        } else {
            let w = rect.w * s;
            Rect { x: rect.x + (rect.w - w) / 2.0, y: rect.bottom() - rect.h * s, w, h: rect.h * s }
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::layout::{FixedMeasure, LayoutBlock, layout};
    use crate::reader::runs::Run;

    fn shown(spelling: &str, reading: &str) -> Vec<(String, Option<String>)> {
        parts(spelling, reading).unwrap().into_iter().map(|p| (p.text, p.reading)).collect()
    }

    fn part(text: &str, reading: Option<&str>) -> (String, Option<String>) {
        (text.to_owned(), reading.map(str::to_owned))
    }

    #[test]
    fn kana_between_kanji_split_the_reading() {
        assert_eq!(shown("引き出し", "ひきだし"), vec![part("引", Some("ひ")), part("き", None), part("出", Some("だ")), part("し", None)]);
        assert_eq!(shown("お茶", "おちゃ"), vec![part("お", None), part("茶", Some("ちゃ"))]);
    }

    #[test]
    fn kanji_side_by_side_share_one_reading() {
        assert_eq!(shown("吾輩", "わがはい"), vec![part("吾輩", Some("わがはい"))]);
    }

    #[test]
    fn a_katakana_reading_lines_up_as_hiragana() {
        assert_eq!(shown("食べる", "タベル"), vec![part("食", Some("た")), part("べる", None)]);
    }

    #[test]
    fn a_reading_that_does_not_fit_gives_nothing() {
        assert_eq!(parts("食べる", "のむ"), None);
        assert_eq!(parts("ねこ", "ねこ"), None);
    }

    #[test]
    fn an_inflected_word_keeps_its_kanji_readings() {
        // 吾輩は猫である, 夏目漱石, Aozora Bunko 789.
        assert_eq!(over_text("泣いていた", "泣く", "なく"), vec![(0..1, "な".to_owned())]);
        assert_eq!(over_text("引き出して", "引き出す", "ひきだす"), vec![(0..1, "ひ".to_owned()), (2..3, "だ".to_owned())]);
    }

    #[test]
    fn other_kanji_in_the_text_give_nothing() {
        assert!(over_text("観る", "見る", "みる").is_empty());
    }

    fn laid(text: &str, vertical: bool) -> Page {
        let runs = [Run { start: 0, len: text.chars().count(), ..Run::default() }];
        let blocks = [LayoutBlock::Text { block: 0, start: 0, text, runs: &runs, heading: false }];
        let cfg = LayoutConfig { vertical, width: 400.0, height: 400.0, font_size: 20.0, show_ruby: true, ..LayoutConfig::default() };
        layout(&blocks, &cfg, &FixedMeasure).remove(0)
    }

    fn word(start: usize, len: usize, spelling: &str, reading: &str) -> Word {
        Word { start, len, spelling: spelling.to_owned(), reading: reading.to_owned(), ..Word::default() }
    }

    #[test]
    fn readings_stand_over_their_kanji_alone() {
        for vertical in [false, true] {
            // 吾輩は猫である, 夏目漱石, Aozora Bunko 789.
            let page = laid("所で泣いていた", vertical);
            let cfg = LayoutConfig { vertical, show_ruby: true, ..LayoutConfig::default() };
            let ruby = on_page(&page, &[word(2, 5, "泣く", "なく")], &cfg, &FixedMeasure);
            assert_eq!(ruby.len(), 1);
            assert_eq!(ruby[0].text, "な");
            let kanji = page.glyphs.iter().find(|g| g.text == "泣").unwrap().rect;
            if vertical {
                assert_eq!(ruby[0].rect.x, kanji.right());
                assert!((ruby[0].rect.y + ruby[0].rect.h / 2.0 - (kanji.y + kanji.h / 2.0)).abs() < 0.01);
            } else {
                assert_eq!(ruby[0].rect.bottom(), kanji.y);
                assert!((ruby[0].rect.x + ruby[0].rect.w / 2.0 - (kanji.x + kanji.w / 2.0)).abs() < 0.01);
            }
        }
    }

    #[test]
    fn readings_that_would_meet_shrink_until_they_just_touch() {
        let at = |x: f32, w: f32| PlacedRuby {
            text: String::new(),
            rect: Rect { x, y: 90.0, w, h: 10.0 },
            size: 10.0,
            block: 0,
            run: 0,
            offset: 0,
            generated: true,
        };
        let mut ruby = vec![at(0.0, 40.0), at(30.0, 40.0), at(200.0, 20.0)];
        fit(&mut ruby, false);
        assert!(ruby[0].size < 10.0 && ruby[0].size == ruby[1].size);
        assert!((ruby[0].rect.right() - ruby[1].rect.x).abs() < 0.01);
        assert_eq!(ruby[0].rect.bottom(), 100.0);
        assert_eq!(ruby[2].size, 10.0);
    }
}
