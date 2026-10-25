// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::sync::Arc;

use serde_json::{Value, json};

use super::{AnkiBackend, Note};
use crate::grade::GradeError;
use crate::http::{Request, Transport};

pub const DEFAULT_URL: &str = "http://127.0.0.1:8765";

pub struct AnkiConnect {
    url: String,
    key: String,
    transport: Arc<dyn Transport>,
}

impl AnkiConnect {
    pub fn new(url: &str, key: &str, transport: Arc<dyn Transport>) -> Self {
        let url = if url.trim().is_empty() { DEFAULT_URL } else { url.trim() };
        AnkiConnect { url: url.to_owned(), key: key.trim().to_owned(), transport }
    }

    fn call(&self, action: &str, params: Value) -> Result<Value, GradeError> {
        let mut body = json!({ "action": action, "version": 6, "params": params });
        if !self.key.is_empty() {
            body["key"] = Value::from(self.key.clone());
        }
        let request =
            Request { url: self.url.clone(), headers: vec![("Content-Type".into(), "application/json".into())], body: body.to_string() };
        let answer =
            self.transport.post(&request).map_err(|_| GradeError::NotFound("Anki isn't running, or AnkiConnect isn't installed".into()))?;
        if answer.status != 200 {
            return Err(GradeError::Other(format!("AnkiConnect answered {}", answer.status)));
        }
        let json: Value = serde_json::from_str(&answer.body).map_err(|_| GradeError::Other("AnkiConnect's answer wasn't JSON".into()))?;
        match json.get("error") {
            Some(Value::String(e)) if e.contains("valid api key") => Err(GradeError::Auth),
            Some(Value::String(e)) => Err(GradeError::Other(format!("Anki: {e}"))),
            _ => Ok(json.get("result").cloned().unwrap_or(Value::Null)),
        }
    }

    pub fn request_permission(&self) -> Result<bool, GradeError> {
        let r = self.call("requestPermission", json!({}))?;
        Ok(r.get("permission").and_then(Value::as_str) == Some("granted"))
    }
}

fn strings(v: Value) -> Vec<String> {
    v.as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_owned)).collect()).unwrap_or_default()
}

fn ids(v: Value) -> Vec<u64> {
    v.as_array().map(|a| a.iter().filter_map(Value::as_u64).collect()).unwrap_or_default()
}

impl AnkiBackend for AnkiConnect {
    fn decks(&self) -> Result<Vec<String>, GradeError> {
        Ok(strings(self.call("deckNames", json!({}))?))
    }

    fn note_types(&self) -> Result<Vec<String>, GradeError> {
        Ok(strings(self.call("modelNames", json!({}))?))
    }

    fn fields(&self, note_type: &str) -> Result<Vec<String>, GradeError> {
        Ok(strings(self.call("modelFieldNames", json!({ "modelName": note_type }))?))
    }

    fn find_notes(&self, query: &str) -> Result<Vec<u64>, GradeError> {
        Ok(ids(self.call("findNotes", json!({ "query": query }))?))
    }

    fn notes(&self, list: &[u64]) -> Result<Vec<Note>, GradeError> {
        let r = self.call("notesInfo", json!({ "notes": list }))?;
        Ok(r.as_array()
            .into_iter()
            .flatten()
            .filter_map(|n| {
                let id = n.get("noteId")?.as_u64()?;
                let mut fields: Vec<(u64, String)> = n
                    .get("fields")?
                    .as_object()?
                    .values()
                    .filter_map(|f| Some((f.get("order")?.as_u64()?, f.get("value")?.as_str()?.to_owned())))
                    .collect();
                fields.sort_by_key(|(o, _)| *o);
                Some(Note { id, fields: fields.into_iter().map(|(_, v)| v).collect() })
            })
            .collect())
    }

    fn answer(&self, note: u64, ease: u8) -> Result<(), GradeError> {
        let cards = ids(self.call("findCards", json!({ "query": format!("nid:{note}") }))?);
        let card = *cards.first().ok_or_else(|| GradeError::NotFound("the note has no cards".into()))?;
        let r = self.call("answerCards", json!({ "answers": [{ "cardId": card, "ease": ease }] }))?;
        match r.as_array().and_then(|a| a.first()).and_then(Value::as_bool) {
            Some(false) => Err(GradeError::Other("Anki didn't take the grade".into())),
            _ => Ok(()),
        }
    }

    fn suspend(&self, note: u64) -> Result<(), GradeError> {
        let cards = ids(self.call("findCards", json!({ "query": format!("nid:{note}") }))?);
        self.call("suspend", json!({ "cards": cards }))?;
        Ok(())
    }

    fn add_note(&self, deck: &str, note_type: &str, fields: &[(String, String)], tags: &[&str]) -> Result<u64, GradeError> {
        let fields: serde_json::Map<String, Value> = fields.iter().map(|(k, v)| (k.clone(), Value::from(v.clone()))).collect();
        let r = self.call(
            "addNote",
            json!({ "note": { "deckName": deck, "modelName": note_type, "fields": fields, "tags": tags, "options": { "allowDuplicate": false } } }),
        )?;
        r.as_u64().ok_or_else(|| GradeError::Other("Anki didn't add the note".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{Mock, Response, Unreachable};

    // Answers in AnkiConnect's documented shape, written for the test.
    fn anki() -> Arc<Mock> {
        Mock::new(|r| {
            let body: Value = serde_json::from_str(&r.body).unwrap();
            let result = match body["action"].as_str().unwrap() {
                "deckNames" => json!(["Default", "日本語"]),
                "findCards" => json!([501]),
                "notesInfo" => {
                    json!([{ "noteId": 11, "fields": { "Reading": { "value": "ねこ", "order": 1 }, "Word": { "value": "猫", "order": 0 } } }])
                }
                "answerCards" => json!([true]),
                "addNote" => json!(777),
                _ => Value::Null,
            };
            Ok(Response::ok(json!({ "result": result, "error": null }).to_string()))
        })
    }

    #[test]
    fn requests_are_version_6_with_the_key() {
        let mock = anki();
        let c = AnkiConnect::new("", "secret", mock.clone());
        assert_eq!(c.decks().unwrap(), ["Default", "日本語"]);
        assert_eq!(c.notes(&[11]).unwrap(), [Note { id: 11, fields: vec!["猫".into(), "ねこ".into()] }]);
        c.answer(11, 3).unwrap();
        assert_eq!(c.add_note("日本語", "Vocab", &[("Word".into(), "猫".into())], &["rubiyomi"]).unwrap(), 777);
        assert_eq!(mock.requests()[0].url, DEFAULT_URL);
        let bodies: Vec<Value> = mock.requests().iter().map(|r| serde_json::from_str(&r.body).unwrap()).collect();
        assert!(bodies.iter().all(|b| b["version"] == 6 && b["key"] == "secret"));
        let action = |a: &str| bodies.iter().find(|b| b["action"] == a).unwrap().clone();
        assert_eq!(action("findCards")["params"]["query"], "nid:11");
        assert_eq!(action("answerCards")["params"]["answers"][0], json!({ "cardId": 501, "ease": 3 }));
        let note = &action("addNote")["params"]["note"];
        assert_eq!((note["deckName"].as_str(), note["fields"]["Word"].as_str()), (Some("日本語"), Some("猫")));
        assert_eq!(note["options"]["allowDuplicate"], false);
    }

    #[test]
    fn anki_down_and_a_wrong_key_are_told_apart() {
        let down = AnkiConnect::new("", "", Mock::new(|_| Err(Unreachable("refused".into()))));
        assert!(matches!(down.decks(), Err(GradeError::NotFound(_))));
        let refusing =
            AnkiConnect::new("", "", Mock::new(|_| Ok(Response::ok(r#"{"result":null,"error":"valid api key must be provided"}"#))));
        assert_eq!(refusing.decks(), Err(GradeError::Auth));
    }
}
