// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

pub mod anki;
pub mod jpdb;

use std::collections::HashMap;

use crate::cards::{CardState, WordKey};
use crate::finder::Word;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GradeButton {
    pub label: &'static str,
    pub value: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GradeError {
    Offline,
    Auth,
    PermissionDenied,
    NotFound(String),
    RateLimited,
    NotSetUp(String),
    Other(String),
}

impl std::fmt::Display for GradeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GradeError::Offline => write!(f, "no connection"),
            GradeError::Auth => write!(f, "the key was refused"),
            GradeError::PermissionDenied => write!(f, "permission was refused"),
            GradeError::RateLimited => write!(f, "asked to slow down"),
            GradeError::NotFound(why) | GradeError::NotSetUp(why) | GradeError::Other(why) => f.write_str(why),
        }
    }
}

pub type StateMap = HashMap<WordKey, CardState>;

pub trait Grader: Send + Sync {
    fn name(&self) -> &'static str;

    fn grades(&self) -> &'static [GradeButton];

    // The deck new words go into, by name, once one is chosen.
    fn deck_name(&self) -> Option<String>;

    fn actions(&self) -> &'static [&'static str] {
        &[]
    }

    // A word on the sync, when something about it needs saying.
    fn sync_note(&self) -> Option<String> {
        None
    }

    // Where the word can be seen on the service, as (label, url).
    fn link(&self, _word: &Word) -> Option<(&'static str, String)> {
        None
    }

    // Fetches what is needed to colour words without asking per word, telling
    // how far it is (steps done, steps in all).
    fn sync(&self, progress: &dyn Fn(usize, usize)) -> Result<(), GradeError>;

    // Answers from what is known on the device, without asking anyone.
    fn states(&self, words: &[Word]) -> StateMap;

    // Asks for fresh states of these words, for the next `states`.
    fn refresh(&self, words: &[Word]) -> Result<(), GradeError> {
        let _ = words;
        Ok(())
    }

    fn grade(&self, word: &Word, value: u8, sentence: &str) -> Result<CardState, GradeError>;

    fn add(&self, word: &Word, sentence: &str) -> Result<CardState, GradeError>;

    fn act(&self, word: &Word, action: &str, sentence: &str) -> Result<CardState, GradeError> {
        let _ = (word, action, sentence);
        Err(GradeError::Other("not available".into()))
    }
}
