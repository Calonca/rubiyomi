// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

pub mod connect;
pub mod fields;

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::{GradeButton, GradeError, Grader, StateMap};
use crate::cards::CardState;
use crate::finder::Word;

const GRADES: [GradeButton; 4] = [
    GradeButton { label: "Again", value: 1 },
    GradeButton { label: "Hard", value: 2 },
    GradeButton { label: "Good", value: 3 },
    GradeButton { label: "Easy", value: 4 },
];

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AnkiMapping {
    pub deck: String,
    pub note_type: String,
    pub word_field: String,
    pub reading_field: String,
    pub meaning_field: String,
    pub sentence_field: String,
}

impl AnkiMapping {
    pub fn is_set(&self) -> bool {
        !self.deck.is_empty() && !self.note_type.is_empty() && !self.word_field.is_empty()
    }

    pub fn search(&self) -> String {
        format!("deck:\"{}\" note:\"{}\"", escape(&self.deck), escape(&self.note_type))
    }
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    pub id: u64,
    pub fields: Vec<String>,
}

pub trait AnkiBackend: Send + Sync {
    fn decks(&self) -> Result<Vec<String>, GradeError>;
    fn note_types(&self) -> Result<Vec<String>, GradeError>;
    fn fields(&self, note_type: &str) -> Result<Vec<String>, GradeError>;
    fn find_notes(&self, query: &str) -> Result<Vec<u64>, GradeError>;
    fn notes(&self, ids: &[u64]) -> Result<Vec<Note>, GradeError>;
    fn answer(&self, note: u64, ease: u8) -> Result<(), GradeError>;
    fn add_note(&self, deck: &str, note_type: &str, fields: &[(String, String)], tags: &[&str]) -> Result<u64, GradeError>;
}

pub struct AnkiGrader {
    backend: Box<dyn AnkiBackend>,
    mapping: AnkiMapping,
    index: Mutex<HashMap<String, u64>>,
    states: Mutex<HashMap<u64, CardState>>,
    synced: Mutex<bool>,
}

impl AnkiGrader {
    pub fn new(backend: Box<dyn AnkiBackend>, mapping: AnkiMapping) -> Self {
        AnkiGrader { backend, mapping, index: Mutex::default(), states: Mutex::default(), synced: Mutex::new(false) }
    }

    fn check(&self) -> Result<(), GradeError> {
        if self.mapping.is_set() { Ok(()) } else { Err(GradeError::NotSetUp("Choose an Anki deck, note type and word field first".into())) }
    }

    fn note_of(&self, word: &Word) -> Option<u64> {
        let index = self.index.lock().unwrap_or_else(|e| e.into_inner());
        index.get(&word.spelling).copied().or_else(|| (!word.reading.is_empty()).then(|| index.get(&word.reading).copied()).flatten())
    }

    fn search_states(&self, within: &str) -> Result<HashMap<u64, CardState>, GradeError> {
        let base = format!("{} {within}", self.mapping.search());
        let all: Vec<u64> = self.backend.find_notes(base.trim())?;
        let mut states: HashMap<u64, CardState> = all.iter().map(|&n| (n, CardState::Known)).collect();
        // Weakest first, so the strongest claim wins.
        for (query, state) in [
            ("is:new", CardState::New),
            ("is:due", CardState::Due),
            ("is:learn", CardState::Learning),
            ("is:suspended", CardState::Suspended),
        ] {
            let hits: HashSet<u64> = self.backend.find_notes(&format!("{} {query}", base.trim()))?.into_iter().collect();
            for n in hits {
                states.insert(n, state);
            }
        }
        Ok(states)
    }

    pub fn indexed(&self) -> usize {
        self.index.lock().unwrap_or_else(|e| e.into_inner()).len()
    }
}

impl Grader for AnkiGrader {
    fn name(&self) -> &'static str {
        "Anki"
    }

    fn grades(&self) -> &'static [GradeButton] {
        &GRADES
    }

    fn deck_name(&self) -> Option<String> {
        (!self.mapping.deck.is_empty()).then(|| self.mapping.deck.clone())
    }

    fn sync(&self, progress: &dyn Fn(usize, usize)) -> Result<(), GradeError> {
        self.check()?;
        progress(0, 1);
        let ids = self.backend.find_notes(&self.mapping.search())?;
        let fields = self.backend.fields(&self.mapping.note_type)?;
        let word_at = fields
            .iter()
            .position(|f| *f == self.mapping.word_field)
            .ok_or_else(|| GradeError::NotSetUp(format!("The note type has no field {}", self.mapping.word_field)))?;
        let mut index = HashMap::new();
        for chunk in ids.chunks(500) {
            for note in self.backend.notes(chunk)? {
                if let Some(f) = note.fields.get(word_at) {
                    let word = fields::plain(f);
                    if !word.is_empty() {
                        index.entry(word).or_insert(note.id);
                    }
                }
            }
        }
        let states = self.search_states("")?;
        *self.index.lock().unwrap_or_else(|e| e.into_inner()) = index;
        *self.states.lock().unwrap_or_else(|e| e.into_inner()) = states;
        *self.synced.lock().unwrap_or_else(|e| e.into_inner()) = true;
        Ok(())
    }

    fn states(&self, words: &[Word]) -> StateMap {
        if !*self.synced.lock().unwrap_or_else(|e| e.into_inner()) {
            return StateMap::new();
        }
        let states = self.states.lock().unwrap_or_else(|e| e.into_inner());
        words
            .iter()
            .filter(|w| w.jmdict_id.is_some())
            .map(|w| (w.key(), self.note_of(w).and_then(|n| states.get(&n).copied()).unwrap_or(CardState::NotInDeck)))
            .collect()
    }

    fn grade(&self, word: &Word, value: u8, _sentence: &str) -> Result<CardState, GradeError> {
        self.check()?;
        let note =
            self.note_of(word).ok_or_else(|| GradeError::NotFound(format!("{} isn't in your Anki deck: add it first", word.spelling)))?;
        self.backend.answer(note, value.clamp(1, 4))?;
        let state = self.search_states(&format!("nid:{note}"))?.get(&note).copied().unwrap_or(CardState::Known);
        self.states.lock().unwrap_or_else(|e| e.into_inner()).insert(note, state);
        Ok(state)
    }

    fn add(&self, word: &Word, sentence: &str) -> Result<CardState, GradeError> {
        self.check()?;
        if self.note_of(word).is_some() {
            return Err(GradeError::Other(format!("{} is already in your deck", word.spelling)));
        }
        let m = &self.mapping;
        let mut values = vec![(m.word_field.clone(), word.spelling.clone())];
        if !m.reading_field.is_empty() {
            values.push((m.reading_field.clone(), word.reading.clone()));
        }
        if !m.meaning_field.is_empty() {
            values.push((m.meaning_field.clone(), word.meanings.iter().take(5).cloned().collect::<Vec<_>>().join("<br>")));
        }
        if !m.sentence_field.is_empty() && !sentence.is_empty() {
            let sentence =
                html_escape(sentence).replacen(&html_escape(&word.spelling), &format!("<b>{}</b>", html_escape(&word.spelling)), 1);
            values.push((m.sentence_field.clone(), sentence));
        }
        let id = self.backend.add_note(&m.deck, &m.note_type, &values, &["rubiyomi"])?;
        self.index.lock().unwrap_or_else(|e| e.into_inner()).insert(word.spelling.clone(), id);
        self.states.lock().unwrap_or_else(|e| e.into_inner()).insert(id, CardState::New);
        Ok(CardState::New)
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}
