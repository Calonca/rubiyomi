// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::cards::{CardState, JpdbIds};
use crate::http::{self, Request, Transport};

const BASE: &str = "https://jpdb.io/api/v1/";

// jpdb.io's vocabulary ids are JMdict's.
pub fn vocabulary_url(word: &crate::finder::Word) -> Option<String> {
    let spelling: String = word
        .spelling
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') { (b as char).to_string() } else { format!("%{b:02X}") }
        })
        .collect();
    Some(format!("https://jpdb.io/vocabulary/{}/{spelling}", word.jmdict_id?))
}
const MINIMUM_GAP: Duration = Duration::from_millis(250);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JpdbError {
    NoKey,
    Auth,
    Offline,
    RateLimited,
    Api(String),
}

impl std::fmt::Display for JpdbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JpdbError::NoKey => write!(f, "No jpdb.io API key is set"),
            JpdbError::Auth => write!(f, "jpdb.io refused the API key"),
            JpdbError::Offline => write!(f, "jpdb.io can't be reached"),
            JpdbError::RateLimited => write!(f, "jpdb.io asked to slow down"),
            JpdbError::Api(message) => write!(f, "jpdb.io: {message}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Deck {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Vocabulary {
    pub ids: JpdbIds,
    pub spelling: String,
    pub reading: String,
    pub states: Vec<CardState>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Review {
    Nothing,
    Something,
    Hard,
    Okay,
    Easy,
}

impl Review {
    fn wire(self) -> &'static str {
        match self {
            Review::Nothing => "nothing",
            Review::Something => "something",
            Review::Hard => "hard",
            Review::Okay => "okay",
            Review::Easy => "easy",
        }
    }
}

#[derive(Clone)]
pub struct JpdbClient {
    key: String,
    transport: Arc<dyn Transport>,
    last: Arc<Mutex<Option<Instant>>>,
}

impl JpdbClient {
    pub fn new(key: &str) -> Self {
        Self::with_transport(key, http::transport())
    }

    pub fn with_transport(key: &str, transport: Arc<dyn Transport>) -> Self {
        JpdbClient { key: key.trim().to_owned(), transport, last: Arc::new(Mutex::new(None)) }
    }

    fn post(&self, path: &str, body: Value) -> Result<Value, JpdbError> {
        if self.key.is_empty() {
            return Err(JpdbError::NoKey);
        }
        // One request at a time, a little apart, as jpdb.io asks.
        let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
        let request = Request {
            url: format!("{BASE}{path}"),
            headers: vec![("Authorization".into(), format!("Bearer {}", self.key)), ("Content-Type".into(), "application/json".into())],
            body: body.to_string(),
        };
        let mut retried = false;
        loop {
            if let Some(since) = last.map(|at| at.elapsed())
                && since < MINIMUM_GAP
            {
                std::thread::sleep(MINIMUM_GAP - since);
            }
            let answer = self.transport.post(&request);
            *last = Some(Instant::now());
            let answer = answer.map_err(|_| JpdbError::Offline)?;
            match answer.status {
                200 => {
                    return serde_json::from_str::<Value>(&answer.body)
                        .ok()
                        .filter(Value::is_object)
                        .ok_or(JpdbError::Api("the answer wasn't JSON".into()));
                }
                429 if !retried => {
                    retried = true;
                    std::thread::sleep(Duration::from_secs(answer.retry_after.unwrap_or(2).clamp(1, 10)));
                }
                429 => return Err(JpdbError::RateLimited),
                status => return Err(api_error(status, &answer.body)),
            }
        }
    }

    pub fn parse(&self, text: &str) -> Result<Vec<Vocabulary>, JpdbError> {
        let json = self.post(
            "parse",
            json!({
                "text": text,
                "token_fields": ["vocabulary_index"],
                "vocabulary_fields": ["vid", "sid", "spelling", "reading", "card_state"],
            }),
        )?;
        Ok(rows(&json, "vocabulary")
            .iter()
            .filter_map(|r| {
                Some(Vocabulary {
                    ids: JpdbIds { vid: r.first()?.as_u64()?, sid: r.get(1)?.as_u64()? },
                    spelling: text_at(r, 2),
                    reading: text_at(r, 3),
                    states: states(r.get(4)),
                })
            })
            .collect())
    }

    // The words of each text as jpdb.io finds them, at char offsets into it, with
    // their meanings: the reader's words when jpdb.io does the parsing.
    pub fn parse_words(&self, texts: &[String]) -> Result<Vec<Vec<crate::finder::Word>>, JpdbError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let json = self.post(
            "parse",
            json!({
                "text": texts,
                "token_fields": ["vocabulary_index", "position", "length"],
                "vocabulary_fields": ["vid", "spelling", "reading", "meanings", "part_of_speech"],
                // Char offsets, as the reader counts.
                "position_length_encoding": "utf32",
            }),
        )?;
        let vocabulary = rows(&json, "vocabulary");
        let per_text = json.get("tokens").and_then(Value::as_array).cloned().unwrap_or_default();
        Ok(per_text
            .iter()
            .map(|tokens| {
                let mut words: Vec<crate::finder::Word> = tokens
                    .as_array()
                    .map(|l| l.iter().filter_map(Value::as_array).collect::<Vec<_>>())
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|t| {
                        let v = vocabulary.get(t.first()?.as_u64()? as usize)?;
                        let len = t.get(2)?.as_u64()? as usize;
                        (len > 0).then_some(())?;
                        Some(crate::finder::Word {
                            start: t.get(1)?.as_u64()? as usize,
                            len,
                            spelling: text_at(v, 1),
                            reading: text_at(v, 2),
                            meanings: meanings(v.get(3)),
                            pos: v
                                .get(4)
                                .and_then(Value::as_array)
                                .map(|l| l.iter().filter_map(Value::as_str).map(str::to_owned).collect())
                                .unwrap_or_default(),
                            jmdict_id: v.first()?.as_u64().and_then(|id| u32::try_from(id).ok()),
                        })
                    })
                    .collect();
                words.sort_by_key(|w| w.start);
                words
            })
            .collect())
    }

    // A chapter's words at chapter offsets, its blocks sent a few thousand chars at a time.
    pub fn chapter_words(&self, chapter: &crate::text::Chapter) -> Result<Vec<crate::finder::Word>, JpdbError> {
        const BATCH: usize = 4000;
        let mut all = Vec::new();
        let (mut texts, mut starts, mut size) = (Vec::new(), Vec::new(), 0);
        let mut start = 0;
        let mut send = |texts: &mut Vec<String>, starts: &mut Vec<usize>| -> Result<(), JpdbError> {
            for (words, at) in self.parse_words(texts)?.into_iter().zip(starts.iter()) {
                all.extend(words.into_iter().map(|mut w| {
                    w.start += at;
                    w
                }));
            }
            texts.clear();
            starts.clear();
            Ok(())
        };
        for b in &chapter.blocks {
            if let Some(t) = b.as_text().filter(|t| !t.text.trim().is_empty()) {
                texts.push(t.text.clone());
                starts.push(start);
                size += t.text.chars().count();
                if size >= BATCH {
                    send(&mut texts, &mut starts)?;
                    size = 0;
                }
            }
            start += b.char_len();
        }
        send(&mut texts, &mut starts)?;
        Ok(all)
    }

    pub fn review(&self, ids: JpdbIds, grade: Review) -> Result<(), JpdbError> {
        self.post("review", json!({ "vid": ids.vid, "sid": ids.sid, "grade": grade.wire() })).map(|_| ())
    }

    pub fn add_to_deck(&self, deck: &str, ids: JpdbIds) -> Result<(), JpdbError> {
        self.post("deck/add-vocabulary", json!({ "id": deck_id(deck), "vocabulary": [[ids.vid, ids.sid]], "occurences": [1] })).map(|_| ())
    }

    pub fn set_card_sentence(&self, ids: JpdbIds, sentence: &str) -> Result<(), JpdbError> {
        self.post("set-card-sentence", json!({ "vid": ids.vid, "sid": ids.sid, "sentence": sentence })).map(|_| ())
    }

    pub fn lookup(&self, list: &[JpdbIds]) -> Result<Vec<Option<Vocabulary>>, JpdbError> {
        let mut out = Vec::with_capacity(list.len());
        for chunk in list.chunks(200) {
            let pairs: Vec<[u64; 2]> = chunk.iter().map(|i| [i.vid, i.sid]).collect();
            let json = self.post("lookup-vocabulary", json!({ "list": pairs, "fields": ["spelling", "reading", "card_state"] }))?;
            let rows = json.get("vocabulary_info").and_then(Value::as_array).cloned().unwrap_or_default();
            for (i, ids) in chunk.iter().enumerate() {
                let row = rows.get(i).and_then(Value::as_array);
                out.push(row.map(|r| Vocabulary { ids: *ids, spelling: text_at(r, 0), reading: text_at(r, 1), states: states(r.get(2)) }));
            }
        }
        Ok(out)
    }

    pub fn decks(&self) -> Result<Vec<Deck>, JpdbError> {
        let json = self.post("list-user-decks", json!({ "fields": ["id", "name"] }))?;
        Ok(rows(&json, "decks").iter().filter(|r| r.len() >= 2).map(|r| Deck { id: plain(&r[0]), name: plain(&r[1]) }).collect())
    }

    pub fn deck_vocabulary(&self, deck: &str) -> Result<Vec<JpdbIds>, JpdbError> {
        let json = self.post("deck/list-vocabulary", json!({ "id": deck_id(deck), "fetch_occurences": false }))?;
        Ok(rows(&json, "vocabulary").iter().filter_map(|r| Some(JpdbIds { vid: r.first()?.as_u64()?, sid: r.get(1)?.as_u64()? })).collect())
    }
}

fn deck_id(deck: &str) -> Value {
    deck.trim().parse::<u64>().map(Value::from).unwrap_or_else(|_| Value::from(deck.trim()))
}

fn rows(json: &Value, key: &str) -> Vec<Vec<Value>> {
    json.get(key).and_then(Value::as_array).map(|l| l.iter().filter_map(|r| r.as_array().cloned()).collect()).unwrap_or_default()
}

fn text_at(row: &[Value], i: usize) -> String {
    row.get(i).and_then(Value::as_str).unwrap_or_default().to_owned()
}

// Plain strings, or lists of parts should jpdb.io send them split.
fn meanings(v: Option<&Value>) -> Vec<String> {
    let Some(Value::Array(list)) = v else { return Vec::new() };
    list.iter()
        .filter_map(|m| match m {
            Value::String(s) => Some(s.clone()),
            Value::Array(parts) => Some(parts.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("; ")),
            _ => None,
        })
        .collect()
}

fn plain(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn states(v: Option<&Value>) -> Vec<CardState> {
    v.and_then(Value::as_array).map(|l| l.iter().filter_map(Value::as_str).map(CardState::from_wire).collect()).unwrap_or_default()
}

fn api_error(status: u16, body: &str) -> JpdbError {
    let json: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let code = json.get("error").and_then(Value::as_str).unwrap_or("");
    if matches!(status, 401 | 403) || code == "bad_key" {
        return JpdbError::Auth;
    }
    let message = json.get("error_message").and_then(Value::as_str).filter(|m| !m.is_empty()).unwrap_or(if code.is_empty() {
        "the request failed"
    } else {
        code
    });
    JpdbError::Api(message.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{Mock, Response};

    #[test]
    fn parsed_words_sit_at_char_offsets_with_their_meanings() {
        // Hand-written, in the shape jpdb.io documents: tokens per text, then the vocabulary they point to.
        let mock = Mock::new(|r| {
            let body: Value = serde_json::from_str(&r.body).unwrap();
            assert_eq!(body["position_length_encoding"], "utf32");
            Ok(Response::ok(
                json!({
                    "tokens": [[[1, 3, 2], [0, 0, 2]], []],
                    "vocabulary": [[1467640, "羅生", "らしょう", ["a gate"], ["n"]], [1538750, "下人", "げにん", ["servant", "menial"], ["n"]]],
                })
                .to_string(),
            ))
        });
        let client = JpdbClient::with_transport("key", mock);
        let words = client.parse_words(&["羅生門の下人".into(), "".into()]).unwrap();
        assert_eq!(words.len(), 2);
        let w: Vec<(usize, usize, &str)> = words[0].iter().map(|w| (w.start, w.len, w.spelling.as_str())).collect();
        assert_eq!(w, [(0, 2, "羅生"), (3, 2, "下人")]);
        assert_eq!(words[0][1].meanings, ["servant", "menial"]);
        assert_eq!(words[0][1].jmdict_id, Some(1538750));
    }
}
