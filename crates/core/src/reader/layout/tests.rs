// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use super::*;
use crate::reader::runs::book_runs;
use crate::text::TextBlock;

// 宮沢賢治「銀河鉄道の夜」, public domain (author d. 1933), from Aozora Bunko:
// https://www.aozora.gr.jp/cards/000081/files/456_15050.html
const GINGA: &str = "「ではみなさんは、そういうふうに川だと云われたり、乳の流れたあとだと云われたりしていたこのぼんやりと白いものがほんとうは何かご承知ですか。」先生は、黒板に吊した大きな黒い星座の図の、上から下へ白くけぶった銀河帯のようなところを指しながら、みんなに問をかけました。";

fn lines(text: &str, chars_per_line: f32) -> Vec<String> {
    let block = TextBlock::plain(text);
    let runs = book_runs(&block);
    let cfg = LayoutConfig {
        width: chars_per_line * 20.0,
        height: 20.0 * 1.8 * 40.0,
        font_size: 20.0,
        line_height: 1.8,
        ..LayoutConfig::default()
    };
    let pages = layout(&[LayoutBlock::Text { block: 0, start: 0, text: &block.text, runs: &runs, heading: false }], &cfg, &FixedMeasure);
    let mut rows: Vec<(usize, f32, Vec<&PlacedGlyph>)> = Vec::new();
    for (p, page) in pages.iter().enumerate() {
        for g in &page.glyphs {
            match rows.iter_mut().find(|(rp, y, _)| *rp == p && (*y - g.rect.y).abs() < 0.5) {
                Some((_, _, row)) => row.push(g),
                None => rows.push((p, g.rect.y, vec![g])),
            }
        }
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
    rows.into_iter()
        .map(|(_, _, mut row)| {
            row.sort_by(|a, b| a.rect.x.total_cmp(&b.rect.x));
            row.iter().map(|g| g.text.as_str()).collect()
        })
        .collect()
}

#[test]
fn no_line_starts_or_ends_where_kinsoku_forbids() {
    for width in 4..=24 {
        let lines = lines(GINGA, width as f32);
        assert_eq!(lines.concat(), GINGA, "width {width}");
        for line in &lines[1..] {
            let first = line.chars().next().unwrap();
            assert!(!"、。」』）ーぁぃぅぇぉっゃゅょ".contains(first), "width {width}: a line starts with {first}: {lines:?}");
        }
        for line in &lines {
            let last = line.chars().last().unwrap();
            assert!(!"「『（".contains(last), "width {width}: a line ends with {last}: {lines:?}");
        }
    }
}
