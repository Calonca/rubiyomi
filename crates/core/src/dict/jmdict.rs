// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::fmt;
use std::io::Read;
use std::path::Path;

use rusqlite::{Connection, OpenFlags, params};
use serde::Deserialize;
use serde::de::{DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor};

pub const SOURCE: &str = "JMdict (English), property of the Electronic Dictionary Research and Development Group, \
     https://www.edrdg.org/wiki/index.php/JMdict-EDICT_Dictionary_Project, through jmdict-simplified";
pub const LICENCE: &str = "CC BY-SA 4.0, https://creativecommons.org/licenses/by-sa/4.0/";

const SENSES: usize = 5;
const GLOSSES: usize = 4;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub id: u32,
    pub kanji: Vec<String>,
    pub kana: Vec<String>,
    pub pos: Vec<String>,
    pub senses: Vec<String>,
    pub common: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BuildInfo {
    pub version: String,
    pub date: String,
    pub words: usize,
}

#[derive(Deserialize)]
struct Text {
    text: String,
    #[serde(default)]
    common: bool,
}

#[derive(Deserialize)]
struct Gloss {
    text: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Sense {
    #[serde(default)]
    part_of_speech: Vec<String>,
    #[serde(default)]
    gloss: Vec<Gloss>,
}

#[derive(Deserialize)]
struct JsonWord {
    id: String,
    #[serde(default)]
    kanji: Vec<Text>,
    #[serde(default)]
    kana: Vec<Text>,
    #[serde(default)]
    sense: Vec<Sense>,
}

#[derive(Debug)]
pub enum BuildError {
    Json(serde_json::Error),
    Db(rusqlite::Error),
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuildError::Json(e) => write!(f, "the dictionary file can't be read: {e}"),
            BuildError::Db(e) => write!(f, "the dictionary index can't be written: {e}"),
        }
    }
}

impl std::error::Error for BuildError {}

fn create(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "PRAGMA journal_mode = OFF; PRAGMA synchronous = OFF;
         CREATE TABLE entry(id INTEGER PRIMARY KEY, kanji TEXT NOT NULL, kana TEXT NOT NULL, pos TEXT NOT NULL, gloss TEXT NOT NULL, common INTEGER NOT NULL);
         CREATE TABLE form(surface TEXT NOT NULL, entry_id INTEGER NOT NULL, common INTEGER NOT NULL);
         CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);",
    )
}

fn insert(conn: &Connection, w: JsonWord) -> rusqlite::Result<()> {
    let Ok(id) = w.id.parse::<u32>() else { return Ok(()) };
    let common = w.kanji.iter().chain(&w.kana).any(|t| t.common);
    let kanji: Vec<&str> = w.kanji.iter().map(|t| t.text.as_str()).collect();
    let kana: Vec<&str> = w.kana.iter().map(|t| t.text.as_str()).collect();
    let mut pos: Vec<&str> = Vec::new();
    for p in w.sense.iter().flat_map(|s| &s.part_of_speech) {
        if !pos.contains(&p.as_str()) {
            pos.push(p);
        }
    }
    let gloss: Vec<String> = w
        .sense
        .iter()
        .take(SENSES)
        .map(|s| s.gloss.iter().take(GLOSSES).map(|g| g.text.as_str()).collect::<Vec<_>>().join("; "))
        .filter(|s| !s.is_empty())
        .collect();
    conn.prepare_cached("INSERT OR REPLACE INTO entry VALUES (?1, ?2, ?3, ?4, ?5, ?6)")?.execute(params![
        id,
        kanji.join("|"),
        kana.join("|"),
        pos.join(","),
        gloss.join("\n"),
        common
    ])?;
    let mut form = conn.prepare_cached("INSERT INTO form VALUES (?1, ?2, ?3)")?;
    for t in w.kanji.iter().chain(&w.kana) {
        form.execute(params![t.text, id, t.common])?;
    }
    Ok(())
}

pub fn build(json: impl Read, out: &Path, mut progress: impl FnMut(usize)) -> Result<BuildInfo, BuildError> {
    let _ = std::fs::remove_file(out);
    let conn = Connection::open(out).map_err(BuildError::Db)?;
    create(&conn).map_err(BuildError::Db)?;
    conn.execute_batch("BEGIN").map_err(BuildError::Db)?;
    let mut info = BuildInfo::default();
    let mut error = None;
    {
        let seed = Root { conn: &conn, info: &mut info, error: &mut error, progress: &mut progress };
        let mut de = serde_json::Deserializer::from_reader(std::io::BufReader::with_capacity(1 << 16, json));
        seed.deserialize(&mut de).map_err(BuildError::Json)?;
    }
    if let Some(e) = error {
        return Err(BuildError::Db(e));
    }
    conn.execute(
        "INSERT INTO meta VALUES ('version', ?1), ('date', ?2), ('source', ?3), ('licence', ?4)",
        params![info.version, info.date, SOURCE, LICENCE],
    )
    .map_err(BuildError::Db)?;
    conn.execute_batch("COMMIT; CREATE INDEX form_surface ON form(surface); VACUUM;").map_err(BuildError::Db)?;
    Ok(info)
}

struct Root<'a, P: FnMut(usize)> {
    conn: &'a Connection,
    info: &'a mut BuildInfo,
    error: &'a mut Option<rusqlite::Error>,
    progress: &'a mut P,
}

impl<'de, P: FnMut(usize)> DeserializeSeed<'de> for Root<'_, P> {
    type Value = ();
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        d.deserialize_map(self)
    }
}

impl<'de, P: FnMut(usize)> Visitor<'de> for Root<'_, P> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a jmdict-simplified object")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "version" => self.info.version = map.next_value()?,
                "dictDate" => self.info.date = map.next_value()?,
                "words" => map.next_value_seed(Words { conn: self.conn, info: self.info, error: self.error, progress: self.progress })?,
                _ => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(())
    }
}

struct Words<'a, P: FnMut(usize)> {
    conn: &'a Connection,
    info: &'a mut BuildInfo,
    error: &'a mut Option<rusqlite::Error>,
    progress: &'a mut P,
}

impl<'de, P: FnMut(usize)> DeserializeSeed<'de> for Words<'_, P> {
    type Value = ();
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        d.deserialize_seq(self)
    }
}

impl<'de, P: FnMut(usize)> Visitor<'de> for Words<'_, P> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a list of words")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        while let Some(word) = seq.next_element::<JsonWord>()? {
            if self.error.is_none()
                && let Err(e) = insert(self.conn, word)
            {
                *self.error = Some(e);
            }
            self.info.words += 1;
            if self.info.words.is_multiple_of(1000) {
                (self.progress)(self.info.words);
            }
        }
        Ok(())
    }
}

pub struct Jmdict {
    conn: Connection,
}

impl Jmdict {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
        Ok(Jmdict { conn })
    }

    pub fn meta(&self, key: &str) -> Option<String> {
        self.conn.query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0)).ok()
    }

    pub fn entries(&self, surface: &str) -> Vec<Entry> {
        let Ok(mut st) = self.conn.prepare_cached(
            "SELECT e.id, e.kanji, e.kana, e.pos, e.gloss, e.common FROM form f JOIN entry e ON e.id = f.entry_id
             WHERE f.surface = ?1 GROUP BY e.id ORDER BY MAX(f.common) DESC, e.common DESC, e.id ASC LIMIT 12",
        ) else {
            return Vec::new();
        };
        let split = |s: String, sep: char| s.split(sep).filter(|x| !x.is_empty()).map(str::to_owned).collect::<Vec<_>>();
        st.query_map([surface], |r| {
            Ok(Entry {
                id: r.get(0)?,
                kanji: split(r.get(1)?, '|'),
                kana: split(r.get(2)?, '|'),
                pos: split(r.get(3)?, ','),
                senses: split(r.get(4)?, '\n'),
                common: r.get(5)?,
            })
        })
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
    }

    pub fn lookup(&self, spelling: &str, reading: Option<&str>) -> Option<Entry> {
        self.lookup_as(spelling, reading, &[])
    }

    pub fn lookup_as(&self, spelling: &str, reading: Option<&str>, pos: &[&str]) -> Option<Entry> {
        let entries = self.entries(spelling);
        let reads = |e: &Entry| reading.is_none_or(|r| e.kana.iter().any(|k| k == r));
        let fits = |e: &Entry| pos.is_empty() || e.pos.iter().any(|p| pos.iter().any(|want| p.starts_with(want)));
        entries
            .iter()
            .find(|e| reads(e) && fits(e))
            .or_else(|| entries.iter().find(|e| fits(e)))
            .or_else(|| entries.iter().find(|e| reads(e)))
            .or(entries.first())
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::tempdir;

    // Entries written for the test in jmdict-simplified's shape.
    fn sample_json() -> String {
        serde_json::json!({
            "version": "0.0.1-test",
            "languages": ["eng"],
            "dictDate": "2026-01-01",
            "tags": {},
            "words": [
                {"id": "100", "kanji": [{"common": true, "text": "猫", "tags": []}], "kana": [{"common": true, "text": "ねこ", "tags": [], "appliesToKanji": ["*"]}],
                 "sense": [{"partOfSpeech": ["n"], "gloss": [{"lang": "eng", "text": "cat"}]}]},
                {"id": "200", "kanji": [{"common": true, "text": "待つ", "tags": []}], "kana": [{"common": true, "text": "まつ", "tags": []}],
                 "sense": [{"partOfSpeech": ["v5t", "vt"], "gloss": [{"lang": "eng", "text": "to wait"}]}, {"partOfSpeech": ["v5t"], "gloss": [{"lang": "eng", "text": "to await"}]}]},
                {"id": "300", "kanji": [{"common": false, "text": "行く", "tags": []}], "kana": [{"common": false, "text": "ゆく", "tags": []}],
                 "sense": [{"partOfSpeech": ["v5k-s"], "gloss": [{"lang": "eng", "text": "to go (literary)"}]}]},
                {"id": "301", "kanji": [{"common": true, "text": "行く", "tags": []}], "kana": [{"common": true, "text": "いく", "tags": []}],
                 "sense": [{"partOfSpeech": ["v5k-s"], "gloss": [{"lang": "eng", "text": "to go"}]}]}
            ]
        })
        .to_string()
    }

    #[test]
    fn builds_and_looks_up() {
        let path = tempdir("jmdict").join("jmdict.sqlite");
        build(sample_json().as_bytes(), &path, |_| {}).unwrap();
        let d = Jmdict::open(&path).unwrap();
        assert_eq!(d.meta("licence").as_deref(), Some(LICENCE), "the index carries the data's licence");
        let cat = d.lookup("ねこ", None).unwrap();
        assert_eq!((cat.id, cat.kanji[0].as_str(), cat.senses[0].as_str()), (100, "猫", "cat"));
        let wait = d.lookup("待つ", Some("まつ")).unwrap();
        assert_eq!(wait.senses, ["to wait", "to await"]);
        assert_eq!(d.lookup("行く", None).unwrap().id, 301, "the common one first");
        assert_eq!(d.lookup("行く", Some("ゆく")).unwrap().id, 300, "the reading decides");
        assert!(d.lookup("犬", None).is_none());
        assert_eq!(d.lookup_as("待つ", None, &["n"]).unwrap().id, 200, "nothing fits: the best there is");
        assert!(build(&b"{\"words\": [ {\"id\": "[..], &path, |_| {}).is_err());
    }
}
