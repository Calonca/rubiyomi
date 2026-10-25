// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

pub mod merge;

use std::borrow::Cow;
use std::sync::Mutex;

use lindera::dictionary::load_fs_dictionary;
use lindera::mode::Mode;
use lindera::segmenter::Segmenter;

use crate::dict::jmdict::{Entry, Jmdict};
use crate::dict::{DictState, DictStore};
use crate::kana::{is_katakana, to_hiragana};
use merge::{Merged, Morph, merge};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Word {
    pub start: usize,
    pub len: usize,
    pub spelling: String,
    pub reading: String,
    pub meanings: Vec<String>,
}

pub struct Finder {
    segmenter: Segmenter,
    jmdict: Mutex<Jmdict>,
}

impl Finder {
    pub fn open(store: &DictStore) -> Result<Self, String> {
        match store.state() {
            DictState::Ready(_) => {}
            DictState::Unpacking(step) => return Err(format!("{step}…")),
            DictState::Missing => return Err("No dictionary is installed.".into()),
            DictState::Failed(e) => return Err(e),
        }
        let dictionary = load_fs_dictionary(&store.ipadic_dir()).map_err(|e| format!("The word finder can't load: {e}"))?;
        let jmdict = Jmdict::open(&store.jmdict_path()).map_err(|e| format!("The dictionary can't open: {e}"))?;
        Ok(Finder { segmenter: Segmenter::new(Mode::Normal, dictionary, None), jmdict: Mutex::new(jmdict) })
    }

    fn morphs(&self, text: &str) -> Vec<Morph> {
        let Ok(tokens) = self.segmenter.segment(Cow::Borrowed(text)) else { return Vec::new() };
        // Lindera counts bytes; the reader counts chars.
        let mut morphs = Vec::with_capacity(tokens.len());
        let mut chars_before = 0;
        let mut byte_at = 0;
        for mut t in tokens {
            chars_before += text[byte_at..t.byte_start].chars().count();
            let surface = t.surface.to_string();
            let len = surface.chars().count();
            let d = t.details();
            let field = |i: usize| d.get(i).filter(|v| **v != "*").map(|v| v.to_string()).unwrap_or_default();
            morphs.push(Morph {
                start: chars_before,
                len,
                pos: [field(0), field(1), field(2)],
                base: field(6),
                reading: field(7),
                surface,
            });
            chars_before += len;
            byte_at = t.byte_end;
        }
        morphs
    }

    pub fn word_at(&self, text: &str, at: usize) -> Option<Word> {
        let merged = merge(&self.morphs(text));
        let w = merged.into_iter().find(|w| (w.start..w.start + w.len).contains(&at))?;
        let surface: String = text.chars().skip(w.start).take(w.len).collect();
        let reading = to_hiragana(&w.reading);
        let jmdict = self.jmdict.lock().unwrap_or_else(|e| e.into_inner());
        let entry = lookup(&jmdict, &w, &surface, &reading);
        Some(word(&w, &surface, &reading, entry.as_ref()))
    }
}

fn lookup(jmdict: &Jmdict, w: &Merged, surface: &str, reading: &str) -> Option<Entry> {
    let base = w.base.as_str();
    let hint = (base == surface && !reading.is_empty()).then_some(reading);
    jmdict
        .lookup_as(base, hint, jmdict_pos(&w.pos))
        // JMdict has 勉強 (a noun taking する), not 勉強する.
        .or_else(|| base.strip_suffix("する").filter(|n| !n.is_empty()).and_then(|n| jmdict.lookup_as(n, None, &["vs"])))
        .or_else(|| (base != surface).then(|| jmdict.lookup(surface, None)).flatten())
}

fn jmdict_pos(ipadic: &str) -> &'static [&'static str] {
    match ipadic {
        "名詞" => &["n", "pn", "adj-no", "adj-na", "vs"],
        "動詞" => &["v"],
        "形容詞" => &["adj-i", "adj-ix"],
        "副詞" => &["adv"],
        "助詞" => &["prt"],
        "助動詞" => &["aux", "cop"],
        "連体詞" => &["adj-pn"],
        "接続詞" => &["conj"],
        "感動詞" => &["int"],
        "接頭詞" => &["pref"],
        _ => &[],
    }
}

fn word(w: &Merged, surface: &str, reading: &str, entry: Option<&Entry>) -> Word {
    let mut out = Word { start: w.start, len: w.len, spelling: w.base.clone(), ..Word::default() };
    match entry {
        Some(e) => {
            out.spelling = e
                .kanji
                .iter()
                .find(|k| *k == &w.base)
                .or_else(|| e.kana.iter().find(|k| *k == &w.base))
                .or(e.kanji.first())
                .or(e.kana.first())
                .cloned()
                .unwrap_or_else(|| w.base.clone());
            let hint = if w.base == surface { reading } else { "" };
            out.reading = e.kana.iter().find(|k| *k == hint).or(e.kana.first()).cloned().unwrap_or_default();
            out.meanings = e.senses.clone();
        }
        None => {
            out.reading = if reading.is_empty() && surface.chars().all(is_katakana) { to_hiragana(surface) } else { reading.to_owned() };
        }
    }
    out
}
