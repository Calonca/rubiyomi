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
