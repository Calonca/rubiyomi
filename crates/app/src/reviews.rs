// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::sync::{Arc, Mutex};
use std::time::Duration;

use nsh_core::prelude::*;
use rubiyomi_core::grade::Grader;
use rubiyomi_core::grade::anki::connect::AnkiConnect;
use rubiyomi_core::grade::anki::{AnkiBackend, AnkiGrader};
use rubiyomi_core::grade::jpdb::JpdbGrader;
use rubiyomi_core::grade::wanikani::WaniKaniGrader;
use rubiyomi_core::http;
use rubiyomi_core::jpdb::JpdbClient;
use rubiyomi_core::platform::Platform;
use rubiyomi_core::settings::{Parser, ReviewSource, Settings};
use rubiyomi_core::wanikani::WaniKaniClient;

use crate::app::Shared;
use crate::ui::snackbar;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Off,
    Syncing,
    Synced,
    Failed(String),
}

#[derive(Clone, Copy)]
pub struct Reviews {
    pub status: Signal<Status>,
    pub progress: Signal<(usize, usize)>,
    // A word on the last sync, and on the last save.
    pub sync_note: Signal<Option<String>>,
    pub note: Signal<Option<String>>,
    // Bumped when AnkiDroid's permission is answered.
    pub permission: Signal<u64>,
    // The draft of the Settings page while it is open, for what leaves it.
    pub form: Signal<Option<Signal<Settings>>>,
    restarts: Signal<u64>,
}

impl Reviews {
    pub fn new() -> Self {
        Reviews {
            status: signal(Status::Off),
            progress: signal((0, 0)),
            sync_note: signal(None),
            note: signal(None),
            permission: signal(0),
            form: signal_with(None, SignalEquals::Never),
            restarts: signal(0),
        }
    }

    pub fn restart(&self) {
        self.restarts.update(|n| *n += 1);
    }

    pub fn syncing_text(&self) -> String {
        syncing_text(self.progress.get())
    }

    pub fn summary(&self, source: ReviewSource) -> String {
        summary(source, &self.status.get(), self.sync_note.with(|n| n.is_some()), self.progress.get())
    }
}

fn syncing_text((done, all): (usize, usize)) -> String {
    if all > 1 { format!("reading your decks, {} of {all}\u{2026}", (done + 1).min(all)) } else { "syncing\u{2026}".into() }
}

fn summary(source: ReviewSource, status: &Status, noted: bool, progress: (usize, usize)) -> String {
    let name = match source {
        ReviewSource::None => return "Off: words aren't coloured".into(),
        ReviewSource::Jpdb => "jpdb.io",
        ReviewSource::Anki => "Anki",
        ReviewSource::WaniKani => "WaniKani",
    };
    match status {
        Status::Off => format!("{name}: not set up"),
        Status::Syncing => format!("{name}: {}", syncing_text(progress)),
        Status::Synced if noted => format!("{name}: no cards to grade found"),
        Status::Synced => format!("{name}: up to date"),
        Status::Failed(e) => format!("{name}: couldn't sync, {e}"),
    }
}

pub fn anki_backend(platform: &dyn Platform, settings: &Settings) -> Box<dyn AnkiBackend> {
    platform.anki_droid().unwrap_or_else(|| Box::new(AnkiConnect::new(&settings.anki_url, &settings.anki_key, http::transport())))
}

pub fn parser_for(settings: &Settings) -> Option<Arc<JpdbClient>> {
    let key = settings.jpdb_key.trim();
    (settings.parser == Parser::Jpdb && !key.is_empty()).then(|| Arc::new(JpdbClient::new(key)))
}

pub fn grader_for(platform: &dyn Platform, settings: &Settings) -> Option<Arc<dyn Grader>> {
    let dir = platform.data_dir();
    match settings.reviews {
        ReviewSource::None => None,
        ReviewSource::Jpdb if settings.jpdb_key.trim().is_empty() => None,
        ReviewSource::Jpdb => {
            Some(Arc::new(JpdbGrader::new(JpdbClient::new(&settings.jpdb_key), &settings.jpdb_deck, &settings.jpdb_deck_name, &dir)))
        }
        ReviewSource::Anki => Some(Arc::new(AnkiGrader::new(anki_backend(platform, settings), settings.anki.clone()).saved_in(&dir))),
        ReviewSource::WaniKani if settings.wanikani_token.trim().is_empty() => None,
        ReviewSource::WaniKani => Some(Arc::new(WaniKaniGrader::new(WaniKaniClient::new(&settings.wanikani_token), &dir))),
    }
}

// The settings file gets written whatever happens to the keys, so a failing keyring can't undo
// `setup_done`. Says what became of the keys, if something needs saying.
fn store(platform: &dyn Platform, settings: &Settings) -> Option<String> {
    let e = settings.save(platform).err()?;
    log::warn!("saving settings: {e}");
    let keys = [&settings.jpdb_key, &settings.anki_key, &settings.wanikani_token];
    keys.iter()
        .any(|k| !k.trim().is_empty())
        .then(|| format!("The key couldn't be stored safely, so it will be asked again next time: {e}"))
}

// In a snackbar too, as the page that shows the note may not be the one open next.
fn keep(shared: &Shared, settings: &Settings) {
    let note = store(&*shared.platform, settings);
    if let Some(note) = &note {
        snackbar(note.clone());
    }
    shared.reviews.note.set(note);
}

pub fn apply(shared: &Shared, settings: Settings) {
    keep(shared, &settings);
    shared.settings.set(settings);
    shared.reviews.restart();
}

// How the words are found changes nothing that needs a sync.
pub fn apply_parsing(shared: &Shared, settings: Settings) {
    keep(shared, &settings);
    shared.parser.set(parser_for(&settings));
    shared.settings.set(settings);
}

// Syncs when the app starts and on each restart. It is an effect of `App`'s, so a sync outlives
// the screen that asked for it, and a new one cancels the last.
pub fn watch(shared: &Shared) {
    let shared = shared.clone();
    effect(move || {
        let again = shared.reviews.restarts.get() > 0;
        untracked(|| {
            if again {
                let settings = shared.settings.get_untracked();
                shared.parser.set(parser_for(&settings));
                shared.grader.set(grader_for(&*shared.platform, &settings));
            }
            sync(&shared);
        });
    });
}

fn sync(shared: &Shared) {
    let (reviews, graders) = (shared.reviews, shared.grader);
    reviews.sync_note.set(None);
    let Some(grader) = graders.get_untracked() else {
        reviews.status.set(Status::Off);
        return;
    };
    reviews.status.set(Status::Syncing);
    reviews.progress.set((0, 0));
    let counted = Arc::new(Mutex::new((0, 0)));
    let ticker = {
        let counted = counted.clone();
        set_interval(Duration::from_millis(200), move || reviews.progress.set(*counted.lock().unwrap_or_else(|e| e.into_inner())))
    };
    spawn(async move {
        let worker = grader.clone();
        let done = blocking(move || worker.sync(&|done, all| *counted.lock().unwrap_or_else(|e| e.into_inner()) = (done, all))).await;
        ticker.cancel();
        reviews.status.set(match done {
            Ok(Ok(())) => {
                reviews.sync_note.set(grader.sync_note());
                Status::Synced
            }
            Ok(Err(e)) => {
                log::warn!("syncing reviews: {e}");
                Status::Failed(e.to_string())
            }
            Err(e) => Status::Failed(e.to_string()),
        });
        // What reads the grader asks again, now it knows the cards.
        graders.set(Some(grader));
    });
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use rubiyomi_core::cards::CardState;
    use rubiyomi_core::finder::Word;
    use rubiyomi_core::http::{Request, Response, Transport, Unreachable};
    use rubiyomi_core::settings::Settings;

    use super::*;
    use crate::platform::NshPlatform;
    use crate::reader::words::full_colour;

    fn data_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rubiyomi-reviews-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn settings_file_holds_no_keys() {
        let dir = data_dir("keys");
        let platform = NshPlatform::at(dir.clone());
        let settings = Settings { jpdb_key: "made-up-key".into(), jpdb_deck: "5".into(), ..Settings::default() };

        settings.save(&platform).unwrap();

        let file = std::fs::read_to_string(dir.join("settings.json")).unwrap();
        assert!(!file.contains("made-up-key"), "{file}");
        assert_eq!(Settings::load(&platform).jpdb_key, "made-up-key");
    }

    // A system whose keyring refuses everything.
    struct NoKeyring(PathBuf);

    impl Platform for NoKeyring {
        fn data_dir(&self) -> PathBuf {
            self.0.clone()
        }
        fn bundled(&self, _: &str) -> Option<Box<dyn std::io::Read + Send>> {
            None
        }
        fn accent_color(&self) -> Option<u32> {
            None
        }
        fn pick_epub(&self) {}
        fn poll_events(&self) -> Vec<rubiyomi_core::platform::PlatformEvent> {
            Vec::new()
        }
        fn open_url(&self, _: &str) -> bool {
            false
        }
    }

    #[test]
    fn a_failing_keyring_keeps_setup_done_and_says_so() {
        let platform = NoKeyring(data_dir("no-keyring"));
        let note = store(&platform, &Settings { setup_done: true, jpdb_key: "made-up-key".into(), ..Settings::default() });

        assert!(note.is_some_and(|n| n.contains("asked again next time")));
        assert!(Settings::load(&platform).setup_done);
    }

    #[test]
    fn a_failing_keyring_says_nothing_when_there_is_no_key() {
        let platform = NoKeyring(data_dir("no-key"));
        assert_eq!(store(&platform, &Settings { setup_done: true, ..Settings::default() }), None);
    }

    #[test]
    fn no_key_no_grader() {
        let platform = NshPlatform::at(data_dir("no-grader"));
        let jpdb = Settings { reviews: ReviewSource::Jpdb, ..Settings::default() };
        assert!(grader_for(&platform, &jpdb).is_none());
        assert!(grader_for(&platform, &Settings { jpdb_key: "k".into(), ..jpdb }).is_some());
        assert!(grader_for(&platform, &Settings { reviews: ReviewSource::WaniKani, ..Settings::default() }).is_none());
    }

    // jpdb.io, as hand-written answers: one deck holding one word.
    struct Jpdb;

    impl Transport for Jpdb {
        fn post(&self, request: &Request) -> Result<Response, Unreachable> {
            let body = match request.url.rsplit('/').next().unwrap() {
                "list-user-decks" => r#"{"decks":[[5,"Mine"]]}"#,
                "list-vocabulary" => r#"{"vocabulary":[[100,7]]}"#,
                "lookup-vocabulary" => r#"{"vocabulary_info":[["猫","ねこ",["learning"]]]}"#,
                _ => "{}",
            };
            Ok(Response::ok(body))
        }

        fn get(&self, request: &Request) -> Result<Response, Unreachable> {
            self.post(request)
        }
    }

    // What the page and the popup do with a grader once it has synced.
    #[test]
    fn a_synced_grader_colours_a_word_and_grades_it() {
        let client = JpdbClient::with_transport("made-up-key", Arc::new(Jpdb));
        let grader: Arc<dyn Grader> = Arc::new(JpdbGrader::new(client, "5", "Mine", &data_dir("synced")));
        let cat = Word { spelling: "猫".into(), reading: "ねこ".into(), jmdict_id: Some(100), ..Word::default() };
        let words = std::slice::from_ref(&cat);

        grader.sync(&|_, _| {}).unwrap();
        grader.refresh(words).unwrap();

        let state = grader.states(words)[&cat.key()];
        assert_eq!(state, CardState::Learning);
        assert!(full_colour(state, true).is_some());
        assert!(grader.can_grade(&cat).is_ok());
        assert_eq!(grader.grade(&cat, 4, "").unwrap(), CardState::Learning);
    }

    #[test]
    fn the_summary_says_how_far_a_sync_is() {
        assert_eq!(summary(ReviewSource::Jpdb, &Status::Syncing, false, (1, 3)), "jpdb.io: reading your decks, 2 of 3\u{2026}");
        assert_eq!(summary(ReviewSource::Anki, &Status::Synced, true, (0, 0)), "Anki: no cards to grade found");
    }
}
