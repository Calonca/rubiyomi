// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

// The debug hooks of the headless runs (tool/headless.sh) are environment variables, read
// only here and only when built with the `dev-hooks` feature, which no release build has.

use rubiyomi_core::cards::WordKey;
use rubiyomi_core::grade::StateMap;

pub fn var(name: &str) -> Option<String> {
    #[cfg(feature = "dev-hooks")]
    return std::env::var(name).ok();
    #[cfg(not(feature = "dev-hooks"))]
    return {
        let _ = name;
        None
    };
}

// `RUBIYOMI_FAKE_STATES=1`: the headless runs have no reviews service, so words get made-up card
// states to show the colours.
pub fn fake_states_on() -> bool {
    var("RUBIYOMI_FAKE_STATES").is_some()
}

// The first `kinds` of the states, handed to `keys` in turn.
#[cfg(feature = "dev-hooks")]
pub fn fake_states(keys: impl IntoIterator<Item = WordKey>, kinds: usize) -> Option<StateMap> {
    use rubiyomi_core::cards::CardState::*;
    if !fake_states_on() {
        return None;
    }
    let order = [New, Learning, Known, Failed, Due, NotInDeck, Blacklisted];
    let order = &order[..kinds.min(order.len())];
    Some(keys.into_iter().enumerate().map(|(i, key)| (key, order[i % order.len()])).collect())
}

#[cfg(not(feature = "dev-hooks"))]
pub fn fake_states(_: impl IntoIterator<Item = WordKey>, _: usize) -> Option<StateMap> {
    None
}
