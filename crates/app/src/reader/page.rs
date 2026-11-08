// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::cell::RefCell;
use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;
use std::rc::Rc;

use nsh_core::prelude::*;
use rubiyomi_core::reader::layout::{Form, Measure, Page, PlacedGlyph, PlacedRuby, Rect};
use rubiyomi_core::theme::Color;

use super::words::{Ink, spans};
use crate::ui::c;

#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub text: String,
    pub rect: Rect,
    pub size: f32,
    pub block: usize,
    pub offset: usize,
    pub len: usize,
    pub joins_next: bool,
    pub form: Form,
    // What core's justification adds between characters.
    pub spacing: f32,
}

fn starting(g: &PlacedGlyph) -> Line {
    Line {
        text: g.text.clone(),
        rect: g.rect,
        size: g.size,
        block: g.block,
        offset: g.offset,
        len: g.len,
        joins_next: false,
        form: g.form,
        spacing: 0.0,
    }
}

// Only an unbroken run of the source joins: a hard line break leaves a gap in the offsets.
fn mark_joins(lines: &mut [Line]) {
    for i in 1..lines.len() {
        lines[i - 1].joins_next = lines[i - 1].block == lines[i].block && lines[i - 1].offset + lines[i - 1].len == lines[i].offset;
    }
}

pub fn lines(page: &Page) -> Vec<Line> {
    let mut lines: Vec<Line> = Vec::new();
    let mut natural = 0.0;
    let finish = |line: &mut Line, natural: f32| {
        let gaps = line.text.chars().count().saturating_sub(1);
        if gaps > 0 {
            line.spacing = ((line.rect.w - natural) / gaps as f32).max(0.0);
        }
    };
    for g in &page.glyphs {
        match lines.last_mut() {
            Some(line) if line.block == g.block && line.rect.y == g.rect.y => {
                line.text.push_str(&g.text);
                line.rect.w = g.rect.right() - line.rect.x;
                line.len = g.offset + g.len - line.offset;
            }
            last => {
                if let Some(line) = last {
                    finish(line, natural);
                }
                natural = 0.0;
                lines.push(starting(g));
            }
        }
        natural += g.rect.w;
    }
    if let Some(line) = lines.last_mut() {
        finish(line, natural);
    }
    mark_joins(&mut lines);
    lines
}

// A column has to be set a glyph at a time: each may turn or shift.
pub fn column_lines(page: &Page) -> Vec<Line> {
    let mut lines: Vec<Line> = page.glyphs.iter().map(starting).collect();
    mark_joins(&mut lines);
    lines
}

type TextMeasurer = Box<dyn Fn(&str, &str) -> Points>;

pub struct NshMeasure {
    measure: TextMeasurer,
    family: &'static str,
    // A chapter asks for the same few thousand characters over and over, and each one is a call
    // into the platform's text engine.
    cache: RefCell<HashMap<(char, u32), f32>>,
}

impl NshMeasure {
    // Has to run inside a component: that is where the text engine can be reached.
    pub fn new(family: &'static str) -> Self {
        NshMeasure { measure: Box::new(nsh_core::element::text_measurer()), family, cache: RefCell::default() }
    }
}

impl Measure for NshMeasure {
    fn advance(&self, text: &str, size: f32) -> f32 {
        let mut cache = self.cache.borrow_mut();
        let mut total = 0.0;
        let mut buffer = [0; 4];
        for ch in text.chars() {
            total += *cache
                .entry((ch, size.to_bits()))
                .or_insert_with(|| (self.measure)(&format!("{size}px {}", self.family), ch.encode_utf8(&mut buffer)).0);
        }
        total
    }

    fn ruby_overhangs(&self) -> bool {
        true
    }
}

// What a page looks like, apart from what's on it.
#[derive(Clone)]
pub struct PageStyle {
    pub family: &'static str,
    pub ink: Rgba,
    pub ruby: Rgba,
    pub vertical: bool,
    pub width: f32,
    pub height: f32,
}

// The box of a line is taller than its glyphs, so ascenders stay inside it, and centred on
// them, so a fixed line height puts them where core did.
const LEADING: f32 = 0.15;

const GLYPH_ROOM: f32 = 0.5;

const COLUMN_LINE: f32 = 0.055;

// Where a line's text sits on the page, before any rotation.
#[derive(Clone, Copy, Debug, PartialEq)]
struct LineBox {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    rotated: bool,
}

impl LineBox {
    fn row(line: &Line) -> Self {
        // A hair of room: at exactly its width a rounding error would ellipsize the line.
        LineBox {
            x: line.rect.x,
            y: line.rect.y - line.size * LEADING,
            w: line.rect.w + line.size,
            h: line.size * (1.0 + 2.0 * LEADING),
            rotated: false,
        }
    }

    fn glyph(line: &Line, dx: f32, dy: f32) -> Self {
        let (cx, cy) = (line.rect.x + line.rect.w / 2.0, line.rect.y + line.rect.h / 2.0);
        let rotated = line.form == Form::Rotated;
        // A rotated word is laid out flat, then turned about its middle.
        let (w, h) = if rotated { (line.rect.h, line.rect.w) } else { (line.rect.w, line.rect.h) };
        // Centred in a wider box: at exactly its width a rounding error would ellipsize the glyph.
        let w = w + line.size * GLYPH_ROOM;
        LineBox { x: cx - w / 2.0 + dx * line.size, y: cy - h / 2.0 + dy * line.size, w, h, rotated }
    }

    // A press comes in the coordinates of the text's own view, its transforms undone.
    fn to_page(self, x: f32, y: f32) -> (f32, f32) {
        if self.rotated {
            let (cx, cy) = (self.x + self.w / 2.0, self.y + self.h / 2.0);
            (cx - (y - self.h / 2.0), cy + (x - self.w / 2.0))
        } else {
            (self.x + x, self.y + y)
        }
    }
}

fn row_text(content: String, size: f32, family: &'static str, color: Rgba) -> Text {
    let height = size * (1.0 + 2.0 * LEADING);
    // Core measured at this very size, so a system text scale would only break the lines.
    text(content)
        .text_size(px(size))
        .line_height(px(height))
        .h(px(height))
        .font_family(family)
        .text_color(color)
        .number_of_lines(1)
        .allow_font_scaling(false)
}

// In a selectable area Android's text views keep the touch and hand it only to their own press
// listener, so a tap on a line never gets to the page around it.
fn tappable(out: Text, at: LineBox, on_tap: &PageTap) -> Text {
    let on_tap = on_tap.clone();
    out.on_press(move |e: &PressEvent| {
        let (x, y) = at.to_page(e.position.x.0, e.position.y.0);
        on_tap(x, y);
    })
}

// What colours a char of the chapter, by its block and offset.
pub type InkOf<'a> = &'a dyn Fn(usize, usize) -> Ink;

fn span(run: String, ink: Ink) -> Text {
    let mut out = text(run);
    if let Some(colour) = ink.colour {
        out = out.text_color(c(colour));
    }
    if let Some(line) = ink.line {
        out = out.underline().text_decoration_color(c(line));
    }
    out
}

// One text, whatever its colours: the pieces are spans of it, so it stays a paragraph to copy.
fn row_content(line: Line, style: &PageStyle, ink: InkOf) -> Text {
    let ink_at = |i: usize| ink(line.block, line.offset + i);
    // A glyph that stands for more or fewer chars than it shows can't be told apart by char.
    if line.text.chars().count() != line.len || (0..line.len).all(|i| ink_at(i) == Ink::default()) {
        return row_text(line.text, line.size, style.family, style.ink);
    }
    let runs = spans(&line.text, ink_at);
    runs.into_iter().fold(row_text(String::new(), line.size, style.family, style.ink), |out, (run, ink)| out.child(span(run, ink)))
}

fn row(line: Line, follows: bool, style: &PageStyle, ink: InkOf, on_tap: &PageTap) -> AnyElement {
    let at = LineBox::row(&line);
    let spacing = line.spacing;
    tappable(row_content(line, style, ink), at, on_tap)
        .absolute()
        .left(px(at.x))
        .top(px(at.y))
        .w(px(at.w))
        .letter_spacing(px(spacing))
        .continues_text(follows)
        .into_element()
}

fn glyph(line: Line, follows: bool, style: &PageStyle, ink: InkOf, on_tap: &PageTap) -> AnyElement {
    let (dx, dy, size) = match line.form {
        // Horizontal fonts put 、。 at the bottom left; a column wants them at the top right.
        Form::Corner => (0.55, -0.55, line.size),
        Form::SmallKana => (0.1, -0.1, line.size),
        Form::Combined if line.text.chars().count() > 1 => (0.0, 0.0, line.size * 0.8),
        _ => (0.0, 0.0, line.size),
    };
    let at = LineBox::glyph(&line, dx, dy);
    let ink = ink(line.block, line.offset);
    let out = text(line.text)
        .text_size(px(size))
        .line_height(px(at.h))
        .font_family(style.family)
        .text_color(ink.colour.map_or(style.ink, c))
        .number_of_lines(1)
        .allow_font_scaling(false)
        .text_center();
    let out = tappable(out, at, on_tap).absolute().left(px(at.x)).top(px(at.y)).w(px(at.w)).h(px(at.h)).continues_text(follows);
    if at.rotated { out.rotate(radians(FRAC_PI_2)).into_element() } else { out.into_element() }
}

// A column's underline: the reading has the right side, so it runs down the left.
fn column_line(rect: Rect, size: f32, colour: Color) -> AnyElement {
    let thickness = (size * COLUMN_LINE).max(1.0);
    view()
        .absolute()
        .left(px(rect.x - 2.0 * thickness))
        .top(px(rect.y))
        .w(px(thickness))
        .h(px(rect.h))
        .bg(c(colour))
        .select_none()
        .into_element()
}

fn reading(ruby: &PlacedRuby, style: &PageStyle) -> AnyElement {
    let r = ruby.rect;
    let color = style.ruby;
    if style.vertical {
        let column = ruby.text.chars().map(|ch| {
            row_text(ch.to_string(), ruby.size, style.family, color).text_center().w_full().h(px(ruby.size)).line_height(px(ruby.size))
        });
        let room = ruby.size * GLYPH_ROOM;
        return view().absolute().left(px(r.x - room / 2.0)).top(px(r.y)).w(px(r.w + room)).select_none().children(column).into_element();
    }
    // Wider than the reading, so a line of its own never wraps it; `text_center` keeps it on `r`.
    let room = ruby.size * 2.0;
    row_text(ruby.text.clone(), ruby.size, style.family, color)
        .text_center()
        .select_none()
        .absolute()
        .left(px(r.x - room))
        .top(px(r.y - ruby.size * LEADING))
        .w(px(r.w + 2.0 * room))
        .into_element()
}

// A tap on a line, in page coordinates.
pub type PageTap = Rc<dyn Fn(f32, f32)>;

// One selectable page. `file_of` finds where an image block's picture is on disk.
pub fn page_view(page: &Page, style: &PageStyle, ink: InkOf, on_tap: PageTap, file_of: impl Fn(usize) -> Option<String>) -> View {
    let mut out = Vec::new();
    let lines = if style.vertical { column_lines(page) } else { lines(page) };
    let mut follows = false;
    for line in lines {
        let joins_next = line.joins_next;
        if style.vertical
            && let Some(colour) = ink(line.block, line.offset).line
        {
            out.push(column_line(line.rect, line.size, colour));
        }
        out.push(if style.vertical { glyph(line, follows, style, ink, &on_tap) } else { row(line, follows, style, ink, &on_tap) });
        follows = joins_next;
    }
    out.extend(page.ruby.iter().map(|r| reading(r, style)));
    out.extend(page.images.iter().filter_map(|i| {
        let path = file_of(i.block)?;
        Some(image(path).absolute().left(px(i.rect.x)).top(px(i.rect.y)).w(px(i.rect.w)).h(px(i.rect.h)).into_element())
    }));
    view().absolute().w(px(style.width)).h(px(style.height)).selectable(true).children(out)
}

#[cfg(test)]
mod tests {
    use nsh_core::selection::{SelectionModel, SelectionTexts};
    use rubiyomi_core::reader::layout::{FixedMeasure, LayoutBlock, LayoutConfig, layout, page_for_offset};
    use rubiyomi_core::reader::runs::{Run, book_runs};
    use rubiyomi_core::text::{Ruby, TextBlock};

    use super::*;

    // 吾輩は猫である, 夏目漱石, Aozora Bunko 789.
    const OPENING: &str = "吾輩は猫である。";
    const NAME: &str = "名前はまだ無い。";
    const BORN: &str = "どこで生れたかとんと見当がつかぬ。何でも薄暗いじめじめした所で泣いていた事だけは記憶している。";

    fn config(width: f32) -> LayoutConfig {
        LayoutConfig { width, height: 400.0, font_size: 20.0, show_ruby: true, ..LayoutConfig::default() }
    }

    fn blocks<'a>(texts: &'a [(&'a str, Vec<Run>)]) -> Vec<LayoutBlock<'a>> {
        let mut start = 0;
        texts
            .iter()
            .enumerate()
            .map(|(block, (text, runs))| {
                let here = start;
                start += text.chars().count();
                LayoutBlock::Text { block, start: here, text, runs, heading: false }
            })
            .collect()
    }

    struct Texts(Vec<Line>);

    impl SelectionTexts for Texts {
        fn len(&self) -> usize {
            self.0.len()
        }

        fn content(&self, i: usize) -> &str {
            &self.0[i].text
        }

        fn frame(&self, i: usize) -> Bounds<Points> {
            let r = self.0[i].rect;
            bounds(point(px(r.x), px(r.y)), size(px(r.w), px(r.h)))
        }

        fn offset_at(&self, _: usize, _: Point<Points>) -> u32 {
            0
        }

        fn continues(&self, i: usize) -> bool {
            i > 0 && self.0[i - 1].joins_next
        }
    }

    fn plain(text: &str) -> (&str, Vec<Run>) {
        (text, book_runs(&TextBlock::plain(text)))
    }

    #[test]
    fn lines_group_glyphs_by_row() {
        let texts = [plain(OPENING), plain(BORN)];
        let pages = layout(&blocks(&texts), &config(200.0), &FixedMeasure);
        assert_eq!(pages.len(), 1);
        let lines = lines(&pages[0]);
        assert!(lines.len() > 2);
        assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<String>(), format!("{OPENING}{BORN}"));
        for pair in lines.windows(2) {
            assert!(pair[0].block != pair[1].block || pair[0].rect.y < pair[1].rect.y);
        }
    }

    #[test]
    fn copy_text_of_page() {
        let block = TextBlock {
            text: format!("{OPENING}{NAME}"),
            heading: false,
            ruby: vec![Ruby { start: 0, len: 2, text: "わがはい".to_owned() }, Ruby { start: 8, len: 2, text: "なまえ".to_owned() }],
        };
        let texts = [(block.text.as_str(), book_runs(&block))];
        let pages = layout(&blocks(&texts), &config(120.0), &FixedMeasure);
        let lines = lines(&pages[0]);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines.iter().map(|l| l.joins_next).collect::<Vec<_>>(), [true, true, false]);
        assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<String>(), block.text);
        assert!(!pages[0].ruby.is_empty());
        assert!(lines.iter().all(|l| !l.text.contains("わがはい") && !l.text.contains("なまえ")));
        let texts = Texts(lines);
        let mut selection = SelectionModel::new();
        selection.select_all(&texts);
        assert_eq!(selection.text(&texts), block.text);
    }

    #[test]
    fn a_column_is_set_a_glyph_at_a_time() {
        let texts = [plain(OPENING), plain(NAME)];
        let cfg = LayoutConfig { vertical: true, ..config(200.0) };
        let pages = layout(&blocks(&texts), &cfg, &FixedMeasure);
        let lines = column_lines(&pages[0]);
        assert_eq!(lines.len(), OPENING.chars().count() + NAME.chars().count());
        assert!(lines.iter().all(|l| l.text.chars().count() == 1));
        assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<String>(), format!("{OPENING}{NAME}"));
        let ends: Vec<usize> = lines.iter().enumerate().filter(|(_, l)| !l.joins_next).map(|(i, _)| i).collect();
        assert_eq!(ends, [OPENING.chars().count() - 1, lines.len() - 1]);
        assert_eq!(lines[OPENING.chars().count() - 1].form, Form::Corner);
    }

    fn line(form: Form) -> Line {
        let rect = Rect { x: 100.0, y: 200.0, w: 20.0, h: 60.0 };
        Line { text: "猫".to_owned(), rect, size: 20.0, block: 0, offset: 0, len: 1, joins_next: false, form, spacing: 0.0 }
    }

    #[test]
    fn a_press_on_a_line_lands_on_the_page() {
        let at = LineBox::row(&line(Form::Upright));
        assert_eq!(at.to_page(5.0, 3.0), (105.0, 200.0));
    }

    #[test]
    fn a_column_glyph_has_room_not_to_be_ellipsized() {
        for form in [Form::Upright, Form::Rotated] {
            let line = Line {
                text: "二".to_owned(),
                rect: Rect { x: 100.0, y: 50.0, w: 30.0, h: 30.0 },
                size: 30.0,
                block: 0,
                offset: 0,
                len: 1,
                joins_next: false,
                form,
                spacing: 0.0,
            };
            let at = LineBox::glyph(&line, 0.0, 0.0);
            assert!(at.w > line.rect.w, "{form:?}");
            assert_eq!(at.x + at.w / 2.0, line.rect.x + line.rect.w / 2.0, "{form:?}");
        }
    }

    #[test]
    fn a_press_on_a_turned_word_lands_where_it_shows() {
        let at = LineBox::glyph(&line(Form::Rotated), 0.0, 0.0);
        assert_eq!((at.w, at.h), (70.0, 20.0));
        // The word's first letter, past the room at the left of the flat box, shows at the top of the column.
        assert_eq!(at.to_page(5.0, 10.0), (110.0, 200.0));
        assert_eq!(at.to_page(65.0, 10.0), (110.0, 260.0));
        // Its top edge turns to the right.
        assert_eq!(at.to_page(35.0, 0.0), (120.0, 230.0));
    }

    #[test]
    fn image_only_chapter_has_a_page() {
        let blocks = [LayoutBlock::Image { block: 0, start: 0, width: 300, height: 500 }];
        let pages = layout(&blocks, &config(200.0), &FixedMeasure);
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].images.len(), 1);
        assert!(lines(&pages[0]).is_empty());
    }

    #[test]
    fn offset_survives_a_new_layout() {
        let long = BORN.repeat(30);
        let texts = [plain(&long)];
        let offset = long.chars().count() / 2;
        for width in [160.0, 240.0, 333.0] {
            let pages = layout(&blocks(&texts), &config(width), &FixedMeasure);
            assert!(pages.len() > 1);
            assert!(pages[page_for_offset(&pages, offset)].contains_offset(offset));
        }
    }
}
