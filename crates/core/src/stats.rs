// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub use chrono::NaiveDate;
use chrono::{Datelike, Days, Local, Months};
use serde::{Deserialize, Serialize};

use crate::library::fsutil::{read_json_or_default, write_json};

// What a day of reading adds up to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Day {
    pub seconds: u64,
    pub chars: u64,
    // The characters read while the clock ran: speed is these over `seconds`.
    pub timed_chars: u64,
    pub lookups: u32,
    pub chapters: u32,
    // Words on the pages read, and how many of them had no known card.
    pub words: u64,
    pub unknown: u64,
}

impl Day {
    pub fn add(&mut self, other: Day) {
        self.seconds += other.seconds;
        self.chars += other.chars;
        self.timed_chars += other.timed_chars;
        self.lookups += other.lookups;
        self.chapters += other.chapters;
        self.words += other.words;
        self.unknown += other.unknown;
    }

    pub fn chars_per_minute(&self) -> Option<f64> {
        (self.seconds > 0 && self.timed_chars > 0).then(|| self.timed_chars as f64 * 60.0 / self.seconds as f64)
    }

    pub fn unknown_rate(&self) -> Option<f64> {
        (self.words > 0).then(|| self.unknown as f64 / self.words as f64)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Granularity {
    Day,
    Week,
    Month,
}

// A stretch of days, summed: from `start` up to the day before `end`.
#[derive(Clone, Debug)]
pub struct Period {
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub stats: Day,
}

fn period_start(t: NaiveDate, g: Granularity) -> NaiveDate {
    match g {
        Granularity::Day => t,
        // Monday to Sunday.
        Granularity::Week => t - Days::new(t.weekday().num_days_from_monday() as u64),
        Granularity::Month => t.with_day(1).unwrap_or(t),
    }
}

fn next_start(start: NaiveDate, g: Granularity) -> NaiveDate {
    match g {
        Granularity::Day => start + Days::new(1),
        Granularity::Week => start + Days::new(7),
        Granularity::Month => start + Months::new(1),
    }
}

fn previous_start(start: NaiveDate, g: Granularity) -> NaiveDate {
    match g {
        Granularity::Day => start - Days::new(1),
        Granularity::Week => start - Days::new(7),
        Granularity::Month => start - Months::new(1),
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

    pub fn is_empty(&self) -> bool {
        self.days.values().all(|d| *d == Day::default())
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

    // The last `count` periods up to the one `today` is in, oldest first.
    pub fn periods(&self, g: Granularity, count: usize, today: NaiveDate) -> Vec<Period> {
        let mut starts = vec![period_start(today, g)];
        while starts.len() < count {
            let first = starts[starts.len() - 1];
            starts.push(previous_start(first, g));
        }
        starts.reverse();
        starts
            .into_iter()
            .map(|start| {
                let end = next_start(start, g);
                let mut stats = Day::default();
                for d in self.days.range(start..end).map(|(_, d)| d) {
                    stats.add(*d);
                }
                Period { start, end, stats }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::tempdir;

    #[test]
    fn weeks_run_monday_to_sunday_and_months_by_the_calendar() {
        let mut stats = Stats::load(&tempdir("stats-periods"));
        let d = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        for (day, chars) in [("2026-09-30", 1), ("2026-10-18", 10), ("2026-10-19", 100), ("2026-10-25", 1000)] {
            stats.add(d(day), Day { chars, ..Day::default() });
        }
        let weeks = stats.periods(Granularity::Week, 2, d("2026-10-25"));
        assert_eq!(weeks[1].start, d("2026-10-19"), "a Monday");
        assert_eq!(weeks.iter().map(|w| w.stats.chars).collect::<Vec<_>>(), [10, 1100]);
        let months = stats.periods(Granularity::Month, 2, d("2026-10-25"));
        assert_eq!(months.iter().map(|m| m.stats.chars).collect::<Vec<_>>(), [1, 1110]);
        assert_eq!(Stats::load(stats.path.parent().unwrap()).periods(Granularity::Day, 1, d("2026-10-25"))[0].stats.chars, 1000);
    }
}
