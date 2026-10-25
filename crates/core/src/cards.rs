// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CardState {
    NotInDeck,
    New,
    Learning,
    Known,
    NeverForget,
    Due,
    Failed,
    Suspended,
    Blacklisted,
    Redundant,
    Locked,
    Unknown,
}

impl CardState {
    const ALL: [CardState; 12] = [
        CardState::NotInDeck,
        CardState::New,
        CardState::Learning,
        CardState::Known,
        CardState::NeverForget,
        CardState::Due,
        CardState::Failed,
        CardState::Suspended,
        CardState::Blacklisted,
        CardState::Redundant,
        CardState::Locked,
        CardState::Unknown,
    ];

    // Counted as known in the reading stats.
    pub fn is_known(self) -> bool {
        matches!(self, CardState::Known | CardState::NeverForget | CardState::Blacklisted | CardState::Redundant)
    }

    pub fn wire(self) -> &'static str {
        match self {
            CardState::NotInDeck => "not-in-deck",
            CardState::New => "new",
            CardState::Learning => "learning",
            CardState::Known => "known",
            CardState::NeverForget => "never-forget",
            CardState::Due => "due",
            CardState::Failed => "failed",
            CardState::Suspended => "suspended",
            CardState::Blacklisted => "blacklisted",
            CardState::Redundant => "redundant",
            CardState::Locked => "locked",
            CardState::Unknown => "unknown",
        }
    }

    pub fn from_wire(s: &str) -> CardState {
        Self::ALL.into_iter().find(|v| v.wire() == s).unwrap_or(CardState::Unknown)
    }

    pub fn label(self) -> &'static str {
        match self {
            CardState::NotInDeck => "Not in deck",
            CardState::New => "New",
            CardState::Learning => "Learning",
            CardState::Known => "Known",
            CardState::NeverForget => "Never forget",
            CardState::Due => "Due",
            CardState::Failed => "Failed",
            CardState::Suspended => "Suspended",
            CardState::Blacklisted => "Blacklisted",
            CardState::Redundant => "Redundant",
            CardState::Locked => "Locked",
            CardState::Unknown => "Unknown",
        }
    }

    // jpdb.io gives a word several states; this is the one to show.
    pub fn primary_of(states: &[CardState]) -> CardState {
        const ORDER: [CardState; 11] = [
            CardState::Blacklisted,
            CardState::NeverForget,
            CardState::Failed,
            CardState::Due,
            CardState::Learning,
            CardState::Known,
            CardState::Locked,
            CardState::Redundant,
            CardState::Suspended,
            CardState::New,
            CardState::NotInDeck,
        ];
        ORDER.into_iter().find(|want| states.contains(want)).unwrap_or(CardState::Unknown)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JpdbIds {
    pub vid: u64,
    pub sid: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WordKey(pub String);
