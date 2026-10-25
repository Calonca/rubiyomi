// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::cards::WordKey;
use crate::finder::Word;
use crate::library::BookId;
use crate::library::fsutil::{now, read_json_or_default, write_json};

const KEEP: usize = 5000;

// A word looked up, where it was last seen, and how often.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Looked {
    pub spelling: String,
    pub reading: String,
    pub meanings: Vec<String>,
    pub jmdict_id: Option<u32>,
    pub sentence: String,
    pub book: BookId,
    pub book_title: String,
    pub chapter: u32,
    pub offset: u32,
    pub first: u64,
    pub last: u64,
    pub count: u32,
}

impl Looked {
    pub fn key(&self) -> WordKey {
        self.word().key()
    }

    pub fn word(&self) -> Word {
        Word {
            spelling: self.spelling.clone(),
            reading: self.reading.clone(),
            meanings: self.meanings.clone(),
            jmdict_id: self.jmdict_id,
            ..Word::default()
        }
    }
}

pub struct Place<'a> {
    pub book: &'a BookId,
    pub book_title: &'a str,
    pub chapter: u32,
    pub offset: u32,
    pub sentence: &'a str,
}

pub struct History {
    path: PathBuf,
    // Newest first.
    items: Vec<Looked>,
}

impl History {
    pub fn load(data_dir: &Path) -> Self {
        let path = data_dir.join("words.json");
        let items = read_json_or_default(&path);
        History { path, items }
    }

    pub fn items(&self) -> &[Looked] {
        &self.items
    }

    pub fn record(&mut self, word: &Word, at: Place<'_>) {
        let key = word.key();
        let time = now();
        let mut entry = match self.items.iter().position(|l| l.key() == key) {
            Some(i) => self.items.remove(i),
            None => Looked {
                spelling: word.spelling.clone(),
                reading: word.reading.clone(),
                meanings: word.meanings.iter().take(3).cloned().collect(),
                jmdict_id: word.jmdict_id,
                sentence: String::new(),
                book: at.book.clone(),
                book_title: String::new(),
                chapter: 0,
                offset: 0,
                first: time,
                last: time,
                count: 0,
            },
        };
        entry.sentence = at.sentence.to_owned();
        entry.book = at.book.clone();
        entry.book_title = at.book_title.to_owned();
        entry.chapter = at.chapter;
        entry.offset = at.offset;
        entry.last = time;
        entry.count += 1;
        self.items.insert(0, entry);
        self.items.truncate(KEEP);
        self.save();
    }

    pub fn remove(&mut self, key: &WordKey) {
        self.items.retain(|l| &l.key() != key);
        self.save();
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.save();
    }

    fn save(&self) {
        if let Err(e) = write_json(&self.path, &self.items) {
            log::warn!("saving the words looked up: {e}");
        }
    }
}
