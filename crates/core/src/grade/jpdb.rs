// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::{GradeButton, GradeError, Grader, StateMap};
use crate::cards::{CardState, JpdbIds};
use crate::finder::Word;
use crate::jpdb::{JpdbClient, JpdbError, Review, Vocabulary};
use crate::library::fsutil::{read_json_or_default, write_json};

const GRADES: [GradeButton; 5] = [
    GradeButton { label: "Nothing", value: 1 },
    GradeButton { label: "Something", value: 2 },
    GradeButton { label: "Hard", value: 3 },
    GradeButton { label: "Okay", value: 4 },
    GradeButton { label: "Easy", value: 5 },
];

const NEVER_FORGET: &str = "Never forget";
const BLACKLIST: &str = "Blacklist";

impl From<JpdbError> for GradeError {
    fn from(e: JpdbError) -> Self {
        match e {
            JpdbError::NoKey | JpdbError::Auth => GradeError::Auth,
            JpdbError::Offline => GradeError::Offline,
            JpdbError::RateLimited => GradeError::RateLimited,
            JpdbError::Api(message) => GradeError::Other(message),
        }
    }
}

// jpdb.io's vid is the JMdict entry id, so words found offline are matched to
// the synced decks by it. A sync learns only which cards the decks hold, which
// is quick; the states are asked for the words of the chapter being read, and
// kept for reading offline.
pub struct JpdbGrader {
    client: JpdbClient,
    deck: String,
    deck_name: String,
    synced: Mutex<HashMap<u64, Vec<Vocabulary>>>,
    path: PathBuf,
}

impl JpdbGrader {
    pub fn new(client: JpdbClient, deck: &str, deck_name: &str, data_dir: &Path) -> Self {
        let path = data_dir.join("jpdb-decks.json");
        let synced = read_json_or_default(&path);
        JpdbGrader { client, deck: deck.to_owned(), deck_name: deck_name.to_owned(), synced: Mutex::new(synced), path }
    }

    fn save(&self, map: &HashMap<u64, Vec<Vocabulary>>) {
        if let Err(e) = write_json(&self.path, map) {
            log::warn!("saving the jpdb.io decks: {e}");
        }
    }

    fn in_decks(&self, word: &Word) -> Option<Vocabulary> {
        let vid = word.jmdict_id? as u64;
        let synced = self.synced.lock().unwrap_or_else(|e| e.into_inner());
        let list = synced.get(&vid)?;
        list.iter().find(|v| v.spelling == word.spelling).or(list.first()).cloned()
    }

    fn ids(&self, word: &Word, sentence: &str) -> Result<JpdbIds, GradeError> {
        if let Some(v) = self.in_decks(word) {
            return Ok(v.ids);
        }
        let found = self.client.parse(if sentence.is_empty() { &word.spelling } else { sentence })?;
        let by_id = word.jmdict_id.and_then(|id| found.iter().find(|v| v.ids.vid == id as u64));
        by_id
            .or_else(|| found.iter().find(|v| v.spelling == word.spelling))
            .or_else(|| found.iter().find(|v| !word.reading.is_empty() && v.reading == word.reading))
            .map(|v| v.ids)
            .ok_or_else(|| GradeError::Other(format!("jpdb.io doesn't know {}", word.spelling)))
    }

    fn state_of(&self, ids: JpdbIds) -> Result<CardState, GradeError> {
        let found = self.client.lookup(&[ids])?.into_iter().next().flatten();
        Ok(found.map_or(CardState::NotInDeck, |v| CardState::primary_of(&v.states)))
    }

    // Otherwise the next colouring would undo what was just done.
    fn remember(&self, word: &Word, ids: JpdbIds, state: CardState) {
        let mut synced = self.synced.lock().unwrap_or_else(|e| e.into_inner());
        let list = synced.entry(ids.vid).or_default();
        match list.iter_mut().find(|v| v.ids == ids) {
            Some(v) => v.states = vec![state],
            None => list.push(Vocabulary { ids, spelling: word.spelling.clone(), reading: word.reading.clone(), states: vec![state] }),
        }
    }

    fn done(&self, word: &Word, ids: JpdbIds) -> Result<CardState, GradeError> {
        let state = self.state_of(ids)?;
        self.remember(word, ids, state);
        Ok(state)
    }
}

impl Grader for JpdbGrader {
    fn name(&self) -> &'static str {
        "jpdb.io"
    }

    fn grades(&self) -> &'static [GradeButton] {
        &GRADES
    }

    fn actions(&self) -> &'static [&'static str] {
        &[NEVER_FORGET, BLACKLIST]
    }

    fn link(&self, word: &Word) -> Option<(&'static str, String)> {
        Some(("Open on jpdb.io", crate::jpdb::vocabulary_url(word)?))
    }

    fn deck_name(&self) -> Option<String> {
        (!self.deck.trim().is_empty()).then(|| if self.deck_name.is_empty() { self.deck.clone() } else { self.deck_name.clone() })
    }

    fn sync(&self, progress: &dyn Fn(usize, usize)) -> Result<(), GradeError> {
        let decks: Vec<String> =
            self.client.decks()?.into_iter().map(|d| d.id).chain(["never-forget".into(), "blacklist".into()]).collect();
        let mut pairs = Vec::new();
        for (i, deck) in decks.iter().enumerate() {
            progress(i, decks.len());
            pairs.extend(self.client.deck_vocabulary(deck)?);
        }
        pairs.sort_by_key(|i| (i.vid, i.sid));
        pairs.dedup();
        let mut synced = self.synced.lock().unwrap_or_else(|e| e.into_inner());
        let old = std::mem::take(&mut *synced);
        let known = |ids: JpdbIds| old.get(&ids.vid).and_then(|l| l.iter().find(|v| v.ids == ids)).cloned();
        let mut map: HashMap<u64, Vec<Vocabulary>> = HashMap::new();
        for ids in pairs {
            let v = known(ids).unwrap_or(Vocabulary { ids, spelling: String::new(), reading: String::new(), states: Vec::new() });
            map.entry(ids.vid).or_default().push(v);
        }
        self.save(&map);
        *synced = map;
        Ok(())
    }

    fn states(&self, words: &[Word]) -> StateMap {
        words
            .iter()
            .filter(|w| w.jmdict_id.is_some())
            .map(|w| (w.key(), self.in_decks(w).map_or(CardState::NotInDeck, |v| CardState::primary_of(&v.states))))
            .collect()
    }

    fn refresh(&self, words: &[Word]) -> Result<(), GradeError> {
        let mut ids: Vec<JpdbIds> = {
            let synced = self.synced.lock().unwrap_or_else(|e| e.into_inner());
            words.iter().filter_map(|w| synced.get(&(w.jmdict_id? as u64))).flatten().map(|v| v.ids).collect()
        };
        ids.sort_by_key(|i| (i.vid, i.sid));
        ids.dedup();
        if ids.is_empty() {
            return Ok(());
        }
        let found = self.client.lookup(&ids)?;
        let mut synced = self.synced.lock().unwrap_or_else(|e| e.into_inner());
        for v in found.into_iter().flatten() {
            if let Some(slot) = synced.get_mut(&v.ids.vid).and_then(|l| l.iter_mut().find(|o| o.ids == v.ids)) {
                *slot = v;
            }
        }
        self.save(&synced);
        Ok(())
    }

    fn grade(&self, word: &Word, value: u8, sentence: &str) -> Result<CardState, GradeError> {
        let ids = self.ids(word, sentence)?;
        let review = match value {
            1 => Review::Nothing,
            2 => Review::Something,
            3 => Review::Hard,
            4 => Review::Okay,
            _ => Review::Easy,
        };
        self.client.review(ids, review)?;
        self.done(word, ids)
    }

    fn add(&self, word: &Word, sentence: &str) -> Result<CardState, GradeError> {
        if self.deck.trim().is_empty() {
            return Err(GradeError::NotSetUp("Choose a jpdb.io deck in Settings first".into()));
        }
        let ids = self.ids(word, sentence)?;
        self.client.add_to_deck(&self.deck, ids)?;
        if !sentence.is_empty() {
            self.client.set_card_sentence(ids, sentence)?;
        }
        self.done(word, ids)
    }

    fn act(&self, word: &Word, action: &str, sentence: &str) -> Result<CardState, GradeError> {
        let deck = match action {
            NEVER_FORGET => "never-forget",
            BLACKLIST => "blacklist",
            _ => return Err(GradeError::Other("not available".into())),
        };
        let ids = self.ids(word, sentence)?;
        self.client.add_to_deck(deck, ids)?;
        self.done(word, ids)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::http::{Mock, Response};
    use crate::testing::tempdir;

    // A hand-written jpdb.io, in the shape its API documents: one deck holding 猫 (vid 100).
    fn server() -> std::sync::Arc<Mock> {
        Mock::new(|r| {
            let body: Value = serde_json::from_str(&r.body).unwrap_or(Value::Null);
            let answer = match r.url.rsplit('/').next().unwrap() {
                "list-user-decks" => json!({ "decks": [[5, "Mine"]] }),
                "list-vocabulary" if body["id"] == 5 => json!({ "vocabulary": [[100, 7]] }),
                "list-vocabulary" => json!({ "vocabulary": [] }),
                "lookup-vocabulary" => {
                    let n = body["list"].as_array().map_or(0, |l| l.len());
                    json!({ "vocabulary_info": (0..n).map(|_| json!(["猫", "ねこ", ["learning"]])).collect::<Vec<_>>() })
                }
                "parse" => json!({ "tokens": [[[0]]], "vocabulary": [[300, 9, "犬", "いぬ", ["not-in-deck"]]] }),
                _ => json!({}),
            };
            Ok(Response::ok(answer.to_string()))
        })
    }

    fn word(spelling: &str, reading: &str, id: u32) -> Word {
        Word { spelling: spelling.into(), reading: reading.into(), jmdict_id: Some(id), ..Word::default() }
    }

    #[test]
    fn grades_and_adds_send_jpdb_ids_and_stick() {
        let mock = server();
        let grader = JpdbGrader::new(JpdbClient::with_transport("k", mock.clone()), "5", "Mine", &tempdir("jpdb-grade"));
        grader.sync(&|_, _| {}).unwrap();
        assert!(mock.requests().iter().all(|r| !r.url.ends_with("lookup-vocabulary")), "a sync reads the decks only");
        let cat = word("猫", "ねこ", 100);
        assert_eq!(grader.states(std::slice::from_ref(&cat))[&cat.key()], CardState::Unknown, "in a deck, state not asked yet");
        grader.refresh(std::slice::from_ref(&cat)).unwrap();
        assert_eq!(grader.states(std::slice::from_ref(&cat))[&cat.key()], CardState::Learning);
        assert_eq!(grader.grade(&cat, 4, "").unwrap(), CardState::Learning);
        let dog = word("犬", "いぬ", 300);
        grader.add(&dog, "犬がいた。").unwrap();
        let bodies: Vec<(String, Value)> =
            mock.requests().iter().map(|r| (r.url.clone(), serde_json::from_str(&r.body).unwrap())).collect();
        let body = |end: &str| &bodies.iter().find(|(u, _)| u.ends_with(end)).unwrap().1;
        assert_eq!(body("/review"), &json!({ "vid": 100, "sid": 7, "grade": "okay" }));
        assert_eq!(body("add-vocabulary")["vocabulary"], json!([[300, 9]]));
        assert_eq!(body("set-card-sentence")["sentence"], "犬がいた。");
        assert!(mock.requests().iter().all(|r| r.headers.contains(&("Authorization".into(), "Bearer k".into()))));
        assert_eq!(grader.states(std::slice::from_ref(&dog))[&dog.key()], CardState::Learning, "the add sticks");
    }

    #[test]
    fn no_key_no_call() {
        let mock = server();
        let grader = JpdbGrader::new(JpdbClient::with_transport(" ", mock.clone()), "5", "Mine", &tempdir("jpdb-nokey"));
        assert_eq!(grader.sync(&|_, _| {}), Err(GradeError::Auth));
        assert!(mock.requests().is_empty());
    }
}
