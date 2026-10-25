// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::sync::{Arc, Mutex};

use rubiyomi_core::grade::anki::connect::AnkiConnect;
use rubiyomi_core::grade::anki::{AnkiBackend, AnkiGrader};
use rubiyomi_core::grade::jpdb::JpdbGrader;
use rubiyomi_core::grade::{GradeError, Grader};
use rubiyomi_core::http;
use rubiyomi_core::jpdb::JpdbClient;
use rubiyomi_core::platform::Platform;
use rubiyomi_core::settings::{ReviewSource, Settings};

use crate::task::{self, Task};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Off,
    Syncing,
    Synced,
    Failed(String),
}

pub struct Reviews {
    pub settings: Settings,
    pub grader: Option<Arc<dyn Grader>>,
    pub status: Status,
    // Bumped whenever the states may have changed, so pages recolour.
    pub generation: u64,
    pub note: Option<String>,
    syncing: Option<Task<Result<(), GradeError>>>,
    progress: Arc<Mutex<(usize, usize)>>,
}

impl Reviews {
    pub fn new(ctx: &egui::Context, platform: &dyn Platform) -> Self {
        let mut me = Reviews {
            settings: Settings::load(platform),
            grader: None,
            status: Status::Off,
            generation: 0,
            note: None,
            syncing: None,
            progress: Arc::default(),
        };
        me.start(ctx, platform);
        me
    }

    pub fn apply(&mut self, ctx: &egui::Context, platform: &dyn Platform, settings: Settings) {
        if let Err(e) = settings.save(platform) {
            log::warn!("saving settings: {e}");
            self.note = Some(format!("The key couldn't be stored safely, so it will be asked again next time: {e}"));
        } else {
            self.note = None;
        }
        self.settings = settings;
        self.start(ctx, platform);
    }

    pub fn start(&mut self, ctx: &egui::Context, platform: &dyn Platform) {
        self.grader = grader(platform, &self.settings);
        self.generation += 1;
        let Some(grader) = self.grader.clone() else {
            self.status = Status::Off;
            self.syncing = None;
            return;
        };
        self.status = Status::Syncing;
        let (progress, waker) = (self.progress.clone(), ctx.clone());
        *progress.lock().unwrap_or_else(|e| e.into_inner()) = (0, 0);
        self.syncing = Some(Task::spawn(ctx, move || {
            grader.sync(&|done, all| {
                *progress.lock().unwrap_or_else(|e| e.into_inner()) = (done, all);
                waker.request_repaint();
            })
        }));
    }

    pub fn poll(&mut self) {
        if let Some(result) = task::take(&mut self.syncing) {
            self.generation += 1;
            self.status = match result {
                Ok(()) => Status::Synced,
                Err(e) => {
                    log::warn!("syncing reviews: {e}");
                    Status::Failed(e.to_string())
                }
            };
        }
    }

    pub fn syncing_text(&self) -> String {
        match *self.progress.lock().unwrap_or_else(|e| e.into_inner()) {
            (done, all) if all > 1 => format!("reading your decks, {} of {all}…", (done + 1).min(all)),
            _ => "syncing…".into(),
        }
    }

    pub fn summary(&self) -> String {
        let name = match self.settings.reviews {
            ReviewSource::None => return "Off: words aren't coloured".into(),
            ReviewSource::Jpdb => "jpdb.io",
            ReviewSource::Anki => "Anki",
        };
        match &self.status {
            Status::Off => format!("{name}: not set up"),
            Status::Syncing => format!("{name}: {}", self.syncing_text()),
            Status::Synced => format!("{name}: up to date"),
            Status::Failed(e) => format!("{name}: couldn't sync, {e}"),
        }
    }
}

pub fn anki_backend(platform: &dyn Platform, settings: &Settings) -> Box<dyn AnkiBackend> {
    platform.anki_droid().unwrap_or_else(|| Box::new(AnkiConnect::new(&settings.anki_url, &settings.anki_key, http::transport())))
}

fn grader(platform: &dyn Platform, settings: &Settings) -> Option<Arc<dyn Grader>> {
    match settings.reviews {
        ReviewSource::None => None,
        ReviewSource::Jpdb if settings.jpdb_key.trim().is_empty() => None,
        ReviewSource::Jpdb => Some(Arc::new(JpdbGrader::new(
            JpdbClient::new(&settings.jpdb_key),
            &settings.jpdb_deck,
            &settings.jpdb_deck_name,
            &platform.data_dir(),
        ))),
        ReviewSource::Anki => {
            Some(Arc::new(AnkiGrader::new(anki_backend(platform, settings), settings.anki.clone()).saved_in(&platform.data_dir())))
        }
    }
}
