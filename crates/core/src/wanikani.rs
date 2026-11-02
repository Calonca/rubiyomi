// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::http::{self, Request, Response, Transport, Unreachable};

const BASE: &str = "https://api.wanikani.com/v2/";
// Where a personal access token is made.
pub const TOKENS_URL: &str = "https://www.wanikani.com/settings/personal_access_tokens";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WaniKaniError {
    NoToken,
    Auth,
    PermissionDenied,
    Offline,
    RateLimited,
    Api(String),
}

impl std::fmt::Display for WaniKaniError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WaniKaniError::NoToken => write!(f, "No WaniKani token is set"),
            WaniKaniError::Auth => write!(f, "WaniKani refused the token"),
            WaniKaniError::PermissionDenied => write!(f, "the token doesn't allow this"),
            WaniKaniError::Offline => write!(f, "WaniKani can't be reached"),
            WaniKaniError::RateLimited => write!(f, "WaniKani asked to slow down"),
            WaniKaniError::Api(message) => write!(f, "WaniKani: {message}"),
        }
    }
}

// A vocabulary or kana vocabulary item; kana vocabulary has no readings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subject {
    pub id: u64,
    pub characters: String,
    pub readings: Vec<String>,
    pub kana: bool,
    pub url: String,
    pub hidden: bool,
}

// Times in Unix seconds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assignment {
    pub id: u64,
    pub subject_id: u64,
    pub srs_stage: u8,
    pub unlocked_at: Option<i64>,
    pub started_at: Option<i64>,
    pub available_at: Option<i64>,
    pub hidden: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct User {
    pub username: String,
    pub level: u64,
    // 3 on a free account.
    pub max_level: u64,
}

// A collection read to its end, with the newest change in it, for asking only
// what changed after it next time.
pub struct Collection<T> {
    pub items: Vec<T>,
    pub updated_at: Option<String>,
}

#[derive(Clone)]
pub struct WaniKaniClient {
    token: String,
    transport: Arc<dyn Transport>,
}

impl WaniKaniClient {
    pub fn new(token: &str) -> Self {
        Self::with_transport(token, http::transport())
    }

    pub fn with_transport(token: &str, transport: Arc<dyn Transport>) -> Self {
        WaniKaniClient { token: token.trim().to_owned(), transport }
    }

    fn send(&self, url: &str, body: Option<Value>) -> Result<Value, WaniKaniError> {
        if self.token.is_empty() {
            return Err(WaniKaniError::NoToken);
        }
        let mut headers = vec![("Authorization".into(), format!("Bearer {}", self.token)), ("Wanikani-Revision".into(), "20170710".into())];
        if body.is_some() {
            headers.push(("Content-Type".into(), "application/json".into()));
        }
        let request = Request { url: url.to_owned(), headers, body: body.as_ref().map_or(String::new(), Value::to_string) };
        let send = || -> Result<Response, Unreachable> {
            if body.is_some() { self.transport.post(&request) } else { self.transport.get(&request) }
        };
        let mut retried = false;
        loop {
            let answer = send().map_err(|_| WaniKaniError::Offline)?;
            match answer.status {
                200..=299 => {
                    return serde_json::from_str::<Value>(&answer.body)
                        .ok()
                        .filter(Value::is_object)
                        .ok_or(WaniKaniError::Api("the answer wasn't JSON".into()));
                }
                // 60 requests a minute; a sync takes a few dozen at most.
                429 if !retried => {
                    retried = true;
                    std::thread::sleep(Duration::from_secs(answer.retry_after.unwrap_or(5).clamp(1, 10)));
                }
                429 => return Err(WaniKaniError::RateLimited),
                401 => return Err(WaniKaniError::Auth),
                403 => return Err(WaniKaniError::PermissionDenied),
                status => {
                    let json: Value = serde_json::from_str(&answer.body).unwrap_or(Value::Null);
                    let message = json.get("error").and_then(Value::as_str).map_or_else(|| format!("error {status}"), str::to_owned);
                    return Err(WaniKaniError::Api(message));
                }
            }
        }
    }

    // Follows pages.next_url to the end, telling pages read and pages in all.
    fn collection<T>(
        &self,
        first: String,
        parse: impl Fn(&Value) -> Option<T>,
        progress: &dyn Fn(usize, usize),
    ) -> Result<Collection<T>, WaniKaniError> {
        let mut out = Collection { items: Vec::new(), updated_at: None };
        let mut next = Some(first);
        let mut page = 0;
        while let Some(url) = next.take() {
            let json = self.send(&url, None)?;
            let per_page = json.pointer("/pages/per_page").and_then(Value::as_u64).unwrap_or(500).max(1);
            let total = json.get("total_count").and_then(Value::as_u64).unwrap_or(0);
            page += 1;
            progress(page, total.div_ceil(per_page).max(page as u64) as usize);
            if out.updated_at.is_none() {
                out.updated_at = json.get("data_updated_at").and_then(Value::as_str).map(str::to_owned);
            }
            out.items.extend(json.get("data").and_then(Value::as_array).into_iter().flatten().filter_map(&parse));
            next = json.pointer("/pages/next_url").and_then(Value::as_str).map(str::to_owned);
        }
        Ok(out)
    }

    pub fn user(&self) -> Result<User, WaniKaniError> {
        let json = self.send(&format!("{BASE}user"), None)?;
        let data = json.get("data").ok_or(WaniKaniError::Api("no user in the answer".into()))?;
        Ok(User {
            username: data.get("username").and_then(Value::as_str).unwrap_or_default().to_owned(),
            level: data.get("level").and_then(Value::as_u64).unwrap_or(0),
            max_level: data.pointer("/subscription/max_level_granted").and_then(Value::as_u64).unwrap_or(0),
        })
    }

    pub fn subjects(&self, updated_after: Option<&str>, progress: &dyn Fn(usize, usize)) -> Result<Collection<Subject>, WaniKaniError> {
        let mut url = format!("{BASE}subjects?types=vocabulary,kana_vocabulary");
        if let Some(t) = updated_after {
            url.push_str(&format!("&updated_after={t}"));
        }
        self.collection(url, subject, progress)
    }

    pub fn assignments(
        &self,
        updated_after: Option<&str>,
        progress: &dyn Fn(usize, usize),
    ) -> Result<Collection<Assignment>, WaniKaniError> {
        let mut url = format!("{BASE}assignments?subject_types=vocabulary,kana_vocabulary");
        if let Some(t) = updated_after {
            url.push_str(&format!("&updated_after={t}"));
        }
        self.collection(url, assignment, progress)
    }

    // One review of an item that is due; WaniKani counts a wrong answer by how
    // many times it was given. Returns the assignment as it now stands.
    pub fn review(&self, assignment_id: u64, wrong_meaning: u32, wrong_reading: u32) -> Result<Assignment, WaniKaniError> {
        let json = self.send(
            &format!("{BASE}reviews"),
            Some(json!({ "review": {
                "assignment_id": assignment_id,
                "incorrect_meaning_answers": wrong_meaning,
                "incorrect_reading_answers": wrong_reading,
            }})),
        )?;
        json.pointer("/resources_updated/assignment").and_then(assignment).ok_or(WaniKaniError::Api("the answer had no assignment".into()))
    }
}

fn time(v: Option<&Value>) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(v?.as_str()?).ok().map(|t| t.timestamp())
}

fn subject(r: &Value) -> Option<Subject> {
    let kana = match r.get("object")?.as_str()? {
        "vocabulary" => false,
        "kana_vocabulary" => true,
        _ => return None,
    };
    let d = r.get("data")?;
    let readings = d
        .get("readings")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|x| x.get("accepted_answer").and_then(Value::as_bool).unwrap_or(true))
        .filter_map(|x| x.get("reading")?.as_str().map(str::to_owned))
        .collect();
    Some(Subject {
        id: r.get("id")?.as_u64()?,
        characters: d.get("characters")?.as_str()?.to_owned(),
        readings,
        kana,
        url: d.get("document_url").and_then(Value::as_str).unwrap_or_default().to_owned(),
        hidden: d.get("hidden_at").is_some_and(|h| !h.is_null()),
    })
}

fn assignment(r: &Value) -> Option<Assignment> {
    let d = r.get("data")?;
    Some(Assignment {
        id: r.get("id")?.as_u64()?,
        subject_id: d.get("subject_id")?.as_u64()?,
        srs_stage: d.get("srs_stage").and_then(Value::as_u64).unwrap_or(0) as u8,
        unlocked_at: time(d.get("unlocked_at")),
        started_at: time(d.get("started_at")),
        available_at: time(d.get("available_at")),
        hidden: d.get("hidden").and_then(Value::as_bool).unwrap_or(false),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::Mock;

    // Hand-written, in the shape WaniKani's API reference shows: two pages of subjects.
    #[test]
    fn subjects_follow_pages_and_keep_accepted_readings() {
        let mock = Mock::new(|r| {
            let page2 = r.url.contains("page_after_id");
            let body = if page2 {
                json!({ "object": "collection", "total_count": 2, "data_updated_at": null,
                    "pages": { "per_page": 1, "next_url": null },
                    "data": [{ "id": 9210, "object": "kana_vocabulary",
                        "data": { "characters": "ありがとう", "document_url": "https://www.wanikani.com/vocabulary/ありがとう", "hidden_at": null } }] })
            } else {
                json!({ "object": "collection", "total_count": 2, "data_updated_at": "2026-10-01T10:00:00.000000Z",
                    "pages": { "per_page": 1, "next_url": "https://api.wanikani.com/v2/subjects?page_after_id=2467&types=vocabulary,kana_vocabulary" },
                    "data": [{ "id": 2467, "object": "vocabulary",
                        "data": { "characters": "一", "document_url": "https://www.wanikani.com/vocabulary/一", "hidden_at": null,
                            "readings": [
                                { "reading": "いち", "primary": true, "accepted_answer": true },
                                { "reading": "ひと", "primary": false, "accepted_answer": false } ] } }] })
            };
            Ok(Response::ok(body.to_string()))
        });
        let client = WaniKaniClient::with_transport("t", mock.clone());
        let seen = std::sync::Mutex::new(Vec::new());
        let got = client.subjects(None, &|done, all| seen.lock().unwrap().push((done, all))).unwrap();
        assert_eq!(got.items.len(), 2);
        assert_eq!(got.items[0].readings, ["いち"]);
        assert!(got.items[1].kana && got.items[1].readings.is_empty());
        assert_eq!(got.updated_at.as_deref(), Some("2026-10-01T10:00:00.000000Z"));
        assert_eq!(*seen.lock().unwrap(), [(1, 2), (2, 2)]);
        let requests = mock.requests();
        assert!(requests[0].url.ends_with("subjects?types=vocabulary,kana_vocabulary"));
        assert!(requests.iter().all(|r| r.body.is_empty()
            && r.headers.contains(&("Authorization".into(), "Bearer t".into()))
            && r.headers.contains(&("Wanikani-Revision".into(), "20170710".into()))));
    }

    #[test]
    fn refusals_map_to_errors() {
        for (status, want) in [(401, WaniKaniError::Auth), (403, WaniKaniError::PermissionDenied)] {
            let mock = Mock::new(move |_| Ok(Response { status, body: "{}".into(), retry_after: None }));
            assert_eq!(WaniKaniClient::with_transport("t", mock).user(), Err(want));
        }
        let mock = Mock::new(|_| Ok(Response::ok("{}")));
        assert_eq!(WaniKaniClient::with_transport(" ", mock.clone()).user(), Err(WaniKaniError::NoToken));
        assert!(mock.requests().is_empty());
    }
}
