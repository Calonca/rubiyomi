// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

pub mod connect;
pub mod fields;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::{GradeButton, GradeError, Grader, StateMap};
use crate::cards::CardState;
use crate::finder::Word;
use crate::library::fsutil::{read_json, write_json};

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
    fn suspend(&self, note: u64) -> Result<(), GradeError>;
}

pub struct AnkiGrader {
    backend: Box<dyn AnkiBackend>,
    mapping: AnkiMapping,
    index: Mutex<HashMap<String, u64>>,
    states: Mutex<HashMap<u64, CardState>>,
    synced: Mutex<bool>,
    saved: Option<PathBuf>,
}

// What the last sync found, so words are coloured before the next one answers.
#[derive(Serialize, Deserialize)]
struct Saved {
    // The deck and note type it was found in: another choice starts afresh.
    search: String,
    word_field: String,
    index: HashMap<String, u64>,
    states: HashMap<u64, CardState>,
}

impl AnkiGrader {
    pub fn new(backend: Box<dyn AnkiBackend>, mapping: AnkiMapping) -> Self {
        AnkiGrader { backend, mapping, index: Mutex::default(), states: Mutex::default(), synced: Mutex::new(false), saved: None }
    }

    // Keeps what is synced in `data_dir`, and starts from what was kept there.
    pub fn saved_in(mut self, data_dir: &Path) -> Self {
        let path = data_dir.join("anki-cards.json");
        if let Some(saved) = read_json::<Saved>(&path)
            && saved.search == self.mapping.search()
            && saved.word_field == self.mapping.word_field
        {
            self.index = Mutex::new(saved.index);
            self.states = Mutex::new(saved.states);
            self.synced = Mutex::new(true);
        }
        self.saved = Some(path);
        self
    }

    fn save(&self) {
        let Some(path) = &self.saved else { return };
        let saved = Saved {
            search: self.mapping.search(),
            word_field: self.mapping.word_field.clone(),
            index: self.index.lock().unwrap_or_else(|e| e.into_inner()).clone(),
            states: self.states.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        };
        if let Err(e) = write_json(path, &saved) {
            log::warn!("saving the Anki cards: {e}");
        }
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

    fn actions(&self) -> &'static [&'static str] {
        &[SUSPEND]
    }

    // Grading needs the word's card, so finding none is the thing to know.
    fn sync_note(&self) -> Option<String> {
        (self.indexed() == 0).then(|| {
            format!(
                "No cards of note type {} in deck {}, so there's nothing to grade yet: choose the note type your cards there use, or add words from the reader.",
                self.mapping.note_type, self.mapping.deck
            )
        })
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
        self.save();
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
        self.save();
        Ok(state)
    }

    fn add(&self, word: &Word, sentence: &str) -> Result<CardState, GradeError> {
        self.check()?;
        if self.note_of(word).is_some() {
            return Err(GradeError::Other(format!("{} is already in your deck", word.spelling)));
        }
        let m = &self.mapping;
        let mut values = vec![(m.word_field.clone(), word.spelling.clone())];
        // A field chosen twice keeps what went into it first, so the word is never overwritten.
        let mut put = |field: &str, value: String| {
            if !field.is_empty() && !values.iter().any(|(f, _)| f == field) {
                values.push((field.to_owned(), value));
            }
        };
        put(&m.reading_field, word.reading.clone());
        put(&m.meaning_field, word.meanings.iter().take(5).cloned().collect::<Vec<_>>().join("<br>"));
        if !sentence.is_empty() {
            let sentence =
                html_escape(sentence).replacen(&html_escape(&word.spelling), &format!("<b>{}</b>", html_escape(&word.spelling)), 1);
            put(&m.sentence_field, sentence);
        }
        let id = self.backend.add_note(&m.deck, &m.note_type, &values, &["rubiyomi"])?;
        self.index.lock().unwrap_or_else(|e| e.into_inner()).insert(word.spelling.clone(), id);
        self.states.lock().unwrap_or_else(|e| e.into_inner()).insert(id, CardState::New);
        self.save();
        Ok(CardState::New)
    }

    fn act(&self, word: &Word, action: &str, _sentence: &str) -> Result<CardState, GradeError> {
        if action != SUSPEND {
            return Err(GradeError::Other("not available".into()));
        }
        self.check()?;
        let note =
            self.note_of(word).ok_or_else(|| GradeError::NotFound(format!("{} isn't in your Anki deck: add it first", word.spelling)))?;
        self.backend.suspend(note)?;
        self.states.lock().unwrap_or_else(|e| e.into_inner()).insert(note, CardState::Suspended);
        self.save();
        Ok(CardState::Suspended)
    }
}

const SUSPEND: &str = "Suspend";

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

// A note type as one deck uses it: how many of the deck's notes are of it, its
// fields, and what one of those notes holds in each, for the setup to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InDeck {
    pub note_type: String,
    pub notes: usize,
    pub fields: Vec<String>,
    pub sample: Vec<String>,
}

// The note types `deck` holds notes of, most used first.
pub fn inspect(backend: &dyn AnkiBackend, deck: &str, note_types: &[String]) -> Result<Vec<InDeck>, GradeError> {
    let mut found = Vec::new();
    for t in note_types {
        let search = AnkiMapping { deck: deck.to_owned(), note_type: t.clone(), ..AnkiMapping::default() }.search();
        let ids = backend.find_notes(&search)?;
        let Some(&first) = ids.first() else { continue };
        let fields = backend.fields(t)?;
        let sample =
            backend.notes(&[first])?.into_iter().next().map(|n| n.fields.iter().map(|f| fields::plain(f)).collect()).unwrap_or_default();
        found.push(InDeck { note_type: t.clone(), notes: ids.len(), fields, sample });
    }
    found.sort_by_key(|t| std::cmp::Reverse(t.notes));
    Ok(found)
}

// Which field takes the word, reading, meaning and sentence, guessed from their names.
pub fn guess_roles(fields: &[String]) -> [String; 4] {
    let hints: [&[&str]; 4] = [
        &["word", "expression", "vocab", "term", "kanji", "単語", "語彙", "front"],
        &["reading", "kana", "furigana", "読み", "yomi"],
        &["meaning", "definition", "gloss", "english", "意味", "back"],
        &["sentence", "example", "context", "例文", "文"],
    ];
    let mut roles: [String; 4] = Default::default();
    for (role, words) in hints.iter().enumerate() {
        let free = |f: &&String| !roles.contains(f);
        let pick = words.iter().find_map(|w| fields.iter().filter(free).find(|f| f.to_lowercase().contains(w))).cloned();
        if let Some(f) = pick {
            roles[role] = f;
        }
    }
    if roles[0].is_empty()
        && let Some(f) = fields.iter().find(|f| !roles.contains(f))
    {
        roles[0] = f.clone();
    }
    roles
}

#[cfg(test)]
mod guess_tests {
    use super::guess_roles;

    #[test]
    fn roles_follow_the_field_names_and_never_share_a_field() {
        let names = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(guess_roles(&names(&["Front", "Back"])), ["Front", "", "Back", ""].map(String::from));
        let jpdb = names(&["Expression", "Reading", "Meaning", "Sentence"]);
        assert_eq!(guess_roles(&jpdb), ["Expression", "Reading", "Meaning", "Sentence"].map(String::from));
        assert_eq!(guess_roles(&names(&["Notes", "Extra"]))[0], "Notes", "something holds the word");
    }
}
