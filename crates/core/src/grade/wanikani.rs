// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::{GradeButton, GradeError, Grader, StateMap};
use crate::cards::CardState;
use crate::finder::Word;
use crate::library::fsutil::{read_json_or_default, write_json};
use crate::wanikani::{Assignment, Subject, WaniKaniClient, WaniKaniError};

const WRONG: u8 = 1;
// 3, so the button takes the Known colour.
const CORRECT: u8 = 3;
const GRADES: [GradeButton; 2] = [GradeButton { label: "Wrong", value: WRONG }, GradeButton { label: "Correct", value: CORRECT }];

impl From<WaniKaniError> for GradeError {
    fn from(e: WaniKaniError) -> Self {
        match e {
            WaniKaniError::NoToken | WaniKaniError::Auth => GradeError::Auth,
            WaniKaniError::PermissionDenied => GradeError::PermissionDenied,
            WaniKaniError::Offline => GradeError::Offline,
            WaniKaniError::RateLimited => GradeError::RateLimited,
            WaniKaniError::Api(message) => GradeError::Other(message),
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
struct Store {
    subjects: HashMap<u64, Subject>,
    subjects_updated: Option<String>,
    // By subject id.
    assignments: HashMap<u64, Assignment>,
    assignments_updated: Option<String>,
    // "spelling|reading" to subject ids; kana vocabulary as "spelling|*".
    #[serde(skip)]
    index: HashMap<String, Vec<u64>>,
}

impl Store {
    fn reindex(&mut self) {
        self.index.clear();
        for s in self.subjects.values().filter(|s| !s.hidden) {
            // Suffixes are written 〜人; the reader finds 人.
            let spelling = s.characters.trim_start_matches(['〜', '~']);
            let keys: Vec<String> =
                if s.kana { vec![format!("{spelling}|*")] } else { s.readings.iter().map(|r| format!("{spelling}|{r}")).collect() };
            for k in keys {
                self.index.entry(k).or_default().push(s.id);
            }
        }
        for ids in self.index.values_mut() {
            ids.sort_unstable();
            ids.dedup();
        }
    }

    fn matches(&self, word: &Word) -> Vec<u64> {
        let by_reading = self.index.get(&format!("{}|{}", word.spelling, word.reading));
        let kana = self.index.get(&format!("{}|*", word.spelling));
        let mut ids: Vec<u64> = by_reading.into_iter().chain(kana).flatten().copied().collect();
        ids.dedup();
        ids
    }

    // None for an item WaniKani hides from the user.
    fn state(&self, subject: u64, now: i64) -> Option<CardState> {
        let Some(a) = self.assignments.get(&subject) else { return Some(CardState::Locked) };
        if a.hidden {
            return None;
        }
        Some(match a {
            Assignment { unlocked_at: None, .. } => CardState::Locked,
            Assignment { started_at: None, .. } => CardState::New,
            Assignment { available_at: Some(t), .. } if *t <= now => CardState::Due,
            Assignment { srs_stage: 0..=4, .. } => CardState::Learning,
            _ => CardState::Known,
        })
    }

    // The one assignment a review would go to, or why there is none.
    fn reviewable(&self, word: &Word, now: i64) -> Result<&Assignment, String> {
        let ids = self.matches(word);
        let id = match ids.as_slice() {
            [] => return Err("Not on WaniKani".into()),
            [id] => *id,
            _ => return Err("More than one WaniKani item matches".into()),
        };
        let a = self.assignments.get(&id).filter(|a| a.unlocked_at.is_some()).ok_or("Locked")?;
        if a.started_at.is_none() {
            return Err("Lesson not done yet".into());
        }
        match a.available_at {
            None if a.srs_stage >= 9 => Err("Burned".into()),
            None => Err("Not in reviews".into()),
            Some(t) if t > now => Err(format!("Next review {}", in_time(t - now))),
            Some(_) => Ok(a),
        }
    }
}

fn in_time(secs: i64) -> String {
    let minutes = (secs + 59) / 60;
    match minutes {
        ..60 => format!("in {minutes} min"),
        60..2880 => format!("in {} h", (minutes + 30) / 60),
        _ => format!("in {} days", (minutes + 720) / 1440),
    }
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

// WaniKani has no JMdict ids, so words are matched by spelling and reading
// (spelling alone for kana vocabulary). Only words it teaches get a state.
pub struct WaniKaniGrader {
    client: WaniKaniClient,
    store: Mutex<Store>,
    path: PathBuf,
}

impl WaniKaniGrader {
    pub fn new(client: WaniKaniClient, data_dir: &Path) -> Self {
        let path = data_dir.join("wanikani.json");
        let mut store: Store = read_json_or_default(&path);
        store.reindex();
        WaniKaniGrader { client, store: Mutex::new(store), path }
    }

    fn store(&self) -> std::sync::MutexGuard<'_, Store> {
        self.store.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn save(&self, store: &Store) {
        if let Err(e) = write_json(&self.path, store) {
            log::warn!("saving the WaniKani items: {e}");
        }
    }
}

impl Grader for WaniKaniGrader {
    fn name(&self) -> &'static str {
        "WaniKani"
    }

    fn grades(&self) -> &'static [GradeButton] {
        &GRADES
    }

    fn deck_name(&self) -> Option<String> {
        None
    }

    fn link(&self, word: &Word) -> Option<(&'static str, String)> {
        let store = self.store();
        let id = *store.matches(word).first()?;
        let url = store.subjects.get(&id)?.url.clone();
        (!url.is_empty()).then_some(("Open on WaniKani", url))
    }

    fn can_grade(&self, word: &Word) -> Result<(), String> {
        self.store().reviewable(word, now()).map(|_| ())
    }

    // The items change rarely, so after the first sync only the changed ones are
    // asked for; the assignments are read whole, as a reset can take some away.
    fn sync(&self, progress: &dyn Fn(usize, usize)) -> Result<(), GradeError> {
        let after = self.store().subjects_updated.clone();
        let subjects = self.client.subjects(after.as_deref(), progress)?;
        let assignments = self.client.assignments(None, progress)?;
        let mut store = self.store();
        for s in subjects.items {
            store.subjects.insert(s.id, s);
        }
        if subjects.updated_at.is_some() {
            store.subjects_updated = subjects.updated_at;
        }
        store.assignments = assignments.items.into_iter().map(|a| (a.subject_id, a)).collect();
        store.assignments_updated = assignments.updated_at.or(store.assignments_updated.take());
        store.reindex();
        self.save(&store);
        Ok(())
    }

    fn states(&self, words: &[Word]) -> StateMap {
        let (store, now) = (self.store(), now());
        words.iter().filter_map(|w| Some((w.key(), store.matches(w).into_iter().find_map(|id| store.state(id, now))?))).collect()
    }

    fn refresh(&self, _words: &[Word]) -> Result<(), GradeError> {
        let Some(after) = self.store().assignments_updated.clone() else { return Ok(()) };
        let changed = self.client.assignments(Some(&after), &|_, _| {})?;
        if changed.items.is_empty() {
            return Ok(());
        }
        let mut store = self.store();
        for a in changed.items {
            store.assignments.insert(a.subject_id, a);
        }
        if changed.updated_at.is_some() {
            store.assignments_updated = changed.updated_at;
        }
        self.save(&store);
        Ok(())
    }

    fn grade(&self, word: &Word, value: u8, _sentence: &str) -> Result<CardState, GradeError> {
        let (id, kana) = {
            let store = self.store();
            let a = store.reviewable(word, now()).map_err(GradeError::Other)?;
            (a.id, store.subjects.get(&a.subject_id).is_some_and(|s| s.kana))
        };
        // Wrong is one wrong answer: the reading, or the meaning where there is no reading.
        let wrong = u32::from(value == WRONG);
        let (meaning, reading) = if kana { (wrong, 0) } else { (0, wrong) };
        let a = self.client.review(id, meaning, reading).map_err(|e| match e {
            WaniKaniError::PermissionDenied => {
                GradeError::Other("This token can't submit reviews. Create one with reviews:create ticked".into())
            }
            e => e.into(),
        })?;
        let mut store = self.store();
        let subject = a.subject_id;
        store.assignments.insert(subject, a);
        self.save(&store);
        Ok(store.state(subject, now()).unwrap_or(CardState::Unknown))
    }

    fn add(&self, _word: &Word, _sentence: &str) -> Result<CardState, GradeError> {
        Err(GradeError::Other("WaniKani chooses its own words".into()))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::http::{Mock, Response};
    use crate::testing::tempdir;

    fn at(offset: i64) -> Value {
        json!(chrono::DateTime::from_timestamp(now() + offset, 0).unwrap().to_rfc3339())
    }

    fn vocab(id: u64, characters: &str, readings: &[&str]) -> Value {
        let readings: Vec<Value> = readings.iter().map(|r| json!({ "reading": r, "primary": true, "accepted_answer": true })).collect();
        json!({ "id": id, "object": "vocabulary",
            "data": { "characters": characters, "readings": readings, "hidden_at": null, "document_url": format!("https://www.wanikani.com/vocabulary/{characters}") } })
    }

    fn assignment(id: u64, subject: u64, stage: u8, unlocked: bool, started: bool, available: Option<i64>) -> Value {
        let maybe = |yes: bool| if yes { at(-86400) } else { Value::Null };
        json!({ "id": id, "object": "assignment",
            "data": { "subject_id": subject, "subject_type": "vocabulary", "srs_stage": stage,
                "unlocked_at": maybe(unlocked), "started_at": maybe(started),
                "available_at": available.map_or(Value::Null, at), "hidden": false } })
    }

    fn page(data: Vec<Value>) -> String {
        json!({ "object": "collection", "total_count": data.len(), "data_updated_at": "2026-10-01T10:00:00.000000Z",
            "pages": { "per_page": 500, "next_url": null }, "data": data })
        .to_string()
    }

    // Hand-written WaniKani: one item at each point of its life.
    fn server() -> std::sync::Arc<Mock> {
        Mock::new(|r| {
            let body = if r.url.contains("/subjects") {
                page(vec![
                    vocab(1, "一", &["いち"]),
                    vocab(2, "二", &["に"]),
                    vocab(3, "三", &["さん"]),
                    vocab(4, "四", &["よん"]),
                    vocab(5, "五", &["ご"]),
                    vocab(6, "〜人", &["にん"]),
                    vocab(7, "六", &["ろく"]),
                ])
            } else if r.url.ends_with("/reviews") {
                json!({ "resources_updated": { "assignment": assignment(11, 1, 5, true, true, Some(86400)) } }).to_string()
            } else {
                page(vec![
                    assignment(11, 1, 4, true, true, Some(-60)),
                    assignment(12, 2, 0, true, false, None),
                    assignment(13, 3, 2, true, true, Some(3 * 3600)),
                    assignment(14, 4, 9, true, true, None),
                    assignment(16, 6, 6, true, true, Some(86400)),
                ])
            };
            Ok(Response::ok(body))
        })
    }

    fn word(spelling: &str, reading: &str) -> Word {
        Word { spelling: spelling.into(), reading: reading.into(), jmdict_id: Some(1), ..Word::default() }
    }

    #[test]
    fn stages_become_card_states() {
        let grader = WaniKaniGrader::new(WaniKaniClient::with_transport("t", server()), &tempdir("wk-states"));
        grader.sync(&|_, _| {}).unwrap();
        let words: Vec<Word> =
            [("一", "いち"), ("二", "に"), ("三", "さん"), ("四", "よん"), ("人", "にん"), ("六", "ろく"), ("猫", "ねこ"), ("一", "ひと")]
                .iter()
                .map(|(s, r)| word(s, r))
                .collect();
        let states = grader.states(&words);
        let got: Vec<Option<CardState>> = words.iter().map(|w| states.get(&w.key()).copied()).collect();
        use CardState::*;
        assert_eq!(got, [Some(Due), Some(New), Some(Learning), Some(Known), Some(Known), Some(Locked), None, None]);
        let reasons: Vec<Result<(), String>> = words[..6].iter().map(|w| grader.can_grade(w)).collect();
        assert_eq!(reasons[0], Ok(()));
        assert_eq!(reasons[1], Err("Lesson not done yet".into()));
        assert_eq!(reasons[2], Err("Next review in 3 h".into()));
        assert_eq!(reasons[3], Err("Burned".into()));
        assert_eq!(reasons[5], Err("Locked".into()));
    }

    #[test]
    fn a_review_sends_the_wrong_answers_and_only_when_due() {
        let mock = server();
        let grader = WaniKaniGrader::new(WaniKaniClient::with_transport("t", mock.clone()), &tempdir("wk-review"));
        grader.sync(&|_, _| {}).unwrap();
        assert!(grader.grade(&word("三", "さん"), WRONG, "").is_err(), "not due yet");
        assert!(mock.requests().iter().all(|r| !r.url.ends_with("/reviews")), "nothing was sent for it");
        assert_eq!(grader.grade(&word("一", "いち"), WRONG, "").unwrap(), CardState::Known);
        let review = mock.requests().into_iter().find(|r| r.url.ends_with("/reviews")).unwrap();
        let body: Value = serde_json::from_str(&review.body).unwrap();
        assert_eq!(body, json!({ "review": { "assignment_id": 11, "incorrect_meaning_answers": 0, "incorrect_reading_answers": 1 } }));
        assert_eq!(grader.can_grade(&word("一", "いち")), Err("Next review in 24 h".into()), "the new state sticks");
    }
}
