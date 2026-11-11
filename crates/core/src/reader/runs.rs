// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use crate::text::TextBlock;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Run {
    pub start: usize,
    pub len: usize,
    pub ruby: Option<String>,
    pub generated: bool,
    pub word: Option<usize>,
    // Shown as this text instead, one glyph for the whole run: a word's English (see `gloss`).
    pub swap: Option<String>,
}

impl Run {
    pub fn end(&self) -> usize {
        self.start + self.len
    }
}

pub fn book_runs(block: &TextBlock) -> Vec<Run> {
    let total = block.char_len();
    let mut runs = Vec::new();
    let mut at = 0;
    let mut ruby: Vec<_> = block.ruby.iter().filter(|r| r.len > 0 && (r.start + r.len) as usize <= total).collect();
    ruby.sort_by_key(|r| r.start);
    for r in ruby {
        let (start, len) = (r.start as usize, r.len as usize);
        if start < at {
            continue;
        }
        if start > at {
            runs.push(Run { start: at, len: start - at, ..Run::default() });
        }
        runs.push(Run { start, len, ruby: Some(r.text.clone()), ..Run::default() });
        at = start + len;
    }
    if at < total {
        runs.push(Run { start: at, len: total - at, ..Run::default() });
    }
    runs
}

// The runs with each swap, (start, len, text) in block chars and in order, put in for the chars it
// covers. What is left of a run with a reading the swap cut into loses the reading.
pub fn with_swaps(runs: &[Run], swaps: &[(usize, usize, String)]) -> Vec<Run> {
    if swaps.is_empty() {
        return runs.to_vec();
    }
    let mut out = Vec::with_capacity(runs.len() + 2 * swaps.len());
    let mut swaps = swaps.iter().filter(|s| s.1 > 0).peekable();
    let mut done = 0;
    for run in runs {
        let mut at = run.start.max(done);
        while at < run.end() {
            while swaps.peek().is_some_and(|s| s.0 + s.1 <= at) {
                swaps.next();
            }
            match swaps.peek() {
                Some(&(start, len, text)) if *start <= at => {
                    if *start == at {
                        out.push(Run { start: *start, len: *len, swap: Some(text.clone()), ..Run::default() });
                    }
                    at = start + len;
                }
                next => {
                    let end = next.map_or(run.end(), |s| s.0.min(run.end()));
                    let whole = at == run.start && end == run.end();
                    out.push(Run { start: at, len: end - at, ruby: run.ruby.clone().filter(|_| whole), ..run.clone() });
                    at = end;
                }
            }
        }
        done = done.max(at);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(runs: &[Run]) -> Vec<(usize, usize, Option<&str>, Option<&str>)> {
        runs.iter().map(|r| (r.start, r.len, r.ruby.as_deref(), r.swap.as_deref())).collect()
    }

    #[test]
    fn a_swap_takes_its_chars_from_the_runs_around_it() {
        let runs = [
            Run { start: 0, len: 3, ..Run::default() },
            Run { start: 3, len: 2, ruby: Some("らしょう".into()), ..Run::default() },
            Run { start: 5, len: 4, ..Run::default() },
        ];
        let got = with_swaps(&runs, &[(1, 1, "a".into()), (4, 2, "b".into())]);
        assert_eq!(
            shape(&got),
            [
                (0, 1, None, None),
                (1, 1, None, Some("a")),
                (2, 1, None, None),
                (3, 1, None, None),
                (4, 2, None, Some("b")),
                (6, 3, None, None)
            ],
            "the reading cut into is dropped"
        );
        assert_eq!(got.iter().map(|r| r.len).sum::<usize>(), 9);
        assert_eq!(with_swaps(&runs, &[]), runs);
    }
}
