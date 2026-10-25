// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{Days, Local, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::library::fsutil::{read_json_or_default, write_json};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Day {
    pub seconds: u64,
    pub chars: u64,
    pub lookups: u32,
}

impl Day {
    fn add(&mut self, other: Day) {
        self.seconds += other.seconds;
        self.chars += other.chars;
        self.lookups += other.lookups;
    }
}

// Reading done, day by day, by the device's calendar.
pub struct Stats {
    path: PathBuf,
    days: BTreeMap<NaiveDate, Day>,
}

pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

impl Stats {
    pub fn load(data_dir: &Path) -> Self {
        let path = data_dir.join("stats.json");
        Stats { days: read_json_or_default(&path), path }
    }

    pub fn add(&mut self, on: NaiveDate, done: Day) {
        if done == Day::default() {
            return;
        }
        self.days.entry(on).or_default().add(done);
        if let Err(e) = write_json(&self.path, &self.days) {
            log::warn!("saving reading stats: {e}");
        }
    }

    pub fn day(&self, on: NaiveDate) -> Day {
        self.days.get(&on).copied().unwrap_or_default()
    }

    // The days up to `last`, oldest first, empty ones included.
    pub fn recent(&self, last: NaiveDate, count: u64) -> Vec<(NaiveDate, Day)> {
        (0..count).rev().filter_map(|back| last.checked_sub_days(Days::new(back))).map(|d| (d, self.day(d))).collect()
    }

    pub fn total(&self) -> Day {
        let mut all = Day::default();
        for d in self.days.values() {
            all.add(*d);
        }
        all
    }

    // Days read in a row up to `on`; a day not read yet doesn't break it.
    pub fn streak(&self, on: NaiveDate) -> u32 {
        let read = |d: NaiveDate| self.day(d).seconds >= 60;
        let mut day = if read(on) { on } else { on.pred_opt().unwrap_or(on) };
        let mut n = 0;
        while read(day) {
            n += 1;
            match day.pred_opt() {
                Some(d) => day = d,
                None => break,
            }
        }
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::tempdir;

    #[test]
    fn a_streak_counts_back_and_today_unread_does_not_break_it() {
        let mut stats = Stats::load(&tempdir("stats-streak"));
        let d = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        for day in ["2026-10-20", "2026-10-22", "2026-10-23", "2026-10-24"] {
            stats.add(d(day), Day { seconds: 600, ..Day::default() });
        }
        stats.add(d("2026-10-21"), Day { seconds: 30, ..Day::default() });
        assert_eq!(stats.streak(d("2026-10-25")), 3, "today not read yet");
        assert_eq!(stats.streak(d("2026-10-24")), 3);
        assert_eq!(stats.streak(d("2026-10-27")), 0);
        assert_eq!(Stats::load(&stats.path.parent().unwrap().to_path_buf()).day(d("2026-10-20")).seconds, 600, "kept on disk");
    }
}
