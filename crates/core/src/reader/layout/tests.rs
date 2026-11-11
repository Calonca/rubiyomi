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

#[test]
fn an_overhanging_reading_leaves_its_word_alone() {
    struct Hanging;
    impl Measure for Hanging {
        fn advance(&self, text: &str, size: f32) -> f32 {
            FixedMeasure.advance(text, size)
        }
        fn ruby_overhangs(&self) -> bool {
            true
        }
    }
    // 夏目漱石「吾輩は猫である」, public domain, from Aozora Bunko.
    let text = "吾輩は猫";
    let runs =
        [Run { start: 0, len: 2, ruby: Some("わがはいわがはい".to_owned()), ..Run::default() }, Run { start: 2, len: 2, ..Run::default() }];
    let cfg = LayoutConfig { width: 200.0, height: 200.0, font_size: 20.0, show_ruby: true, ..LayoutConfig::default() };
    let blocks = [LayoutBlock::Text { block: 0, start: 0, text, runs: &runs, heading: false }];
    let page = &layout(&blocks, &cfg, &Hanging)[0];
    let xs: Vec<f32> = page.glyphs.iter().map(|g| g.rect.x).collect();
    assert_eq!(xs, [0.0, 20.0, 40.0, 60.0]);
    let reading = page.ruby[0].rect;
    assert_eq!(reading.x + reading.w / 2.0, 20.0);
    assert!(reading.w > 40.0);
}

#[test]
fn a_swapped_stem_is_one_glyph_and_its_ending_stays() {
    // 芥川龍之介「羅生門」, public domain, from Aozora Bunko: 下人は待っていた.
    let text = "下人は待っていた";
    let runs = crate::reader::runs::with_swaps(&[Run { start: 0, len: 8, ..Run::default() }], &[(3, 1, "wait".into())]);
    for vertical in [false, true] {
        let cfg = LayoutConfig { vertical, width: 400.0, height: 400.0, font_size: 20.0, ..LayoutConfig::default() };
        let page = &layout(&[LayoutBlock::Text { block: 0, start: 10, text, runs: &runs, heading: false }], &cfg, &FixedMeasure)[0];
        let shown: Vec<(&str, usize, usize, bool)> = page.glyphs.iter().map(|g| (g.text.as_str(), g.offset, g.len, g.gloss)).collect();
        assert_eq!(&shown[2..5], [("は", 12, 1, false), ("wait", 13, 1, true), ("っ", 14, 1, false)], "vertical {vertical}");
        let english = &page.glyphs[3];
        assert_eq!(english.form, if vertical { Form::Rotated } else { Form::Upright });
        let along = if vertical { english.rect.h } else { english.rect.w };
        assert!(along > FixedMeasure.advance("wait", 20.0 * GLOSS_SCALE), "{along}");
        assert_eq!((page.start, page.end), (10, 18));
    }
}
