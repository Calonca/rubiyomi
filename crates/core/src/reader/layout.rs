// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use super::kinsoku;
use super::runs::Run;

pub trait Measure {
    fn advance(&self, text: &str, size: f32) -> f32;
}

pub struct FixedMeasure;

impl Measure for FixedMeasure {
    fn advance(&self, text: &str, size: f32) -> f32 {
        text.chars().map(|c| if c.is_ascii() { size * 0.5 } else { size }).sum()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutConfig {
    pub vertical: bool,
    pub paginated: bool,
    pub width: f32,
    pub height: f32,
    pub font_size: f32,
    pub line_height: f32,
    pub show_ruby: bool,
    pub ruby_scale: f32,
    pub max_chars_per_line: u32,
    pub heading_scale: f32,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        LayoutConfig {
            vertical: false,
            paginated: true,
            width: 400.0,
            height: 600.0,
            font_size: 20.0,
            line_height: 1.8,
            show_ruby: false,
            ruby_scale: 0.5,
            max_chars_per_line: 0,
            heading_scale: 1.25,
        }
    }
}

impl LayoutConfig {
    fn main_extent(&self) -> f32 {
        if self.vertical { self.height } else { self.width }
    }

    fn cross_extent(&self) -> f32 {
        if self.vertical { self.width } else { self.height }
    }

    fn line_length(&self, size: f32) -> f32 {
        let main = self.main_extent();
        if self.max_chars_per_line > 0 { main.min(self.max_chars_per_line as f32 * size) } else { main }
    }

    fn ruby_band(&self, size: f32) -> f32 {
        if self.show_ruby { size * self.ruby_scale } else { 0.0 }
    }

    fn line_pitch(&self, size: f32) -> f32 {
        (size * self.line_height).max(size * 1.15 + self.ruby_band(size))
    }
}

#[derive(Clone, Copy, Debug)]
pub enum LayoutBlock<'a> {
    Text { block: usize, start: usize, text: &'a str, runs: &'a [Run], heading: bool },
    Image { block: usize, start: usize, width: u32, height: u32 },
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    Upright,
    Rotated,
    Corner,
    SmallKana,
    Combined,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlacedGlyph {
    pub text: String,
    pub rect: Rect,
    pub size: f32,
    pub form: Form,
    pub block: usize,
    pub offset: usize,
    pub len: usize,
    pub run: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlacedRuby {
    pub text: String,
    pub rect: Rect,
    pub size: f32,
    pub block: usize,
    pub run: usize,
    pub generated: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlacedImage {
    pub block: usize,
    pub offset: usize,
    pub rect: Rect,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Page {
    pub glyphs: Vec<PlacedGlyph>,
    pub ruby: Vec<PlacedRuby>,
    pub images: Vec<PlacedImage>,
    pub start: usize,
    pub end: usize,
    pub extent: f32,
}

impl Page {
    pub fn glyph_at(&self, x: f32, y: f32) -> Option<&PlacedGlyph> {
        self.glyphs.iter().find(|g| g.rect.contains(x, y))
    }

    pub fn glyph_near(&self, x: f32, y: f32, slop: f32) -> Option<&PlacedGlyph> {
        if let Some(g) = self.glyph_at(x, y) {
            return Some(g);
        }
        let distance = |r: &Rect| {
            let dx = (r.x - x).max(0.0).max(x - r.right());
            let dy = (r.y - y).max(0.0).max(y - r.bottom());
            (dx * dx + dy * dy).sqrt()
        };
        self.glyphs.iter().map(|g| (distance(&g.rect), g)).filter(|(d, _)| *d <= slop).min_by(|a, b| a.0.total_cmp(&b.0)).map(|(_, g)| g)
    }

    pub fn contains_offset(&self, offset: usize) -> bool {
        (self.start..self.end.max(self.start + 1)).contains(&offset)
    }
}

#[derive(Clone, Debug)]
struct Unit {
    advance: f32,
    glyphs: Vec<UnitGlyph>,
    ruby: Option<UnitRuby>,
    first: char,
    last: char,
    space: bool,
}

#[derive(Clone, Debug)]
struct UnitGlyph {
    text: String,
    at: f32,
    advance: f32,
    form: Form,
    offset: usize,
    len: usize,
    run: usize,
}

#[derive(Clone, Debug)]
struct UnitRuby {
    text: String,
    at: f32,
    extent: f32,
    run: usize,
    generated: bool,
}

impl Unit {
    fn no_start(&self) -> bool {
        self.ruby.is_none() && kinsoku::no_start(self.first)
    }

    fn no_end(&self) -> bool {
        self.ruby.is_none() && kinsoku::no_end(self.last)
    }

    fn hangs(&self) -> bool {
        self.glyphs.len() == 1 && self.glyphs[0].len == 1 && kinsoku::hangs(self.first)
    }

    fn single(text: String, advance: f32, form: Form, offset: usize, len: usize, run: usize) -> Unit {
        let first = text.chars().next().unwrap_or(' ');
        let last = text.chars().last().unwrap_or(' ');
        Unit {
            advance,
            space: kinsoku::is_space(first) && len == 1,
            glyphs: vec![UnitGlyph { text, at: 0.0, advance, form, offset, len, run }],
            ruby: None,
            first,
            last,
        }
    }
}

fn column_form(c: char) -> Form {
    if kinsoku::corner_in_column(c) {
        Form::Corner
    } else if kinsoku::is_small_kana(c) {
        Form::SmallKana
    } else if kinsoku::rotated_in_column(c) {
        Form::Rotated
    } else {
        Form::Upright
    }
}

#[allow(clippy::too_many_arguments)]
fn units_of(
    chars: &[char],
    base: usize,
    runs: &[Run],
    size: f32,
    cfg: &LayoutConfig,
    measure: &dyn Measure,
    from: usize,
    to: usize,
) -> Vec<Unit> {
    let mut units = Vec::new();
    for (r, run) in runs.iter().enumerate() {
        let (start, end) = (run.start.max(from), run.end().min(to));
        if start >= end {
            continue;
        }
        let whole = start == run.start && end == run.end();
        if let (true, true, Some(ruby)) = (cfg.show_ruby, whole, run.ruby.as_deref().filter(|r| !r.is_empty()))
            && let Some(unit) = ruby_unit(&chars[start..end], base + start, r, ruby, run.generated, size, cfg, measure)
        {
            units.push(unit);
            continue;
        }
        plain_units(&chars[start..end], base + start, r, size, cfg, measure, &mut units);
    }
    units
}

fn plain_units(chars: &[char], base: usize, run: usize, size: f32, cfg: &LayoutConfig, measure: &dyn Measure, units: &mut Vec<Unit>) {
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if kinsoku::is_latin(c) {
            // In a column one or two digits stand upright together.
            let mut j = i + 1;
            while j < chars.len() && kinsoku::is_latin(chars[j]) {
                j += 1;
            }
            let word: String = chars[i..j].iter().collect();
            if cfg.vertical && j - i <= 2 && chars[i..j].iter().all(char::is_ascii_digit) {
                units.push(Unit::single(word, size, Form::Combined, base + i, j - i, run));
            } else {
                let form = if cfg.vertical { Form::Rotated } else { Form::Upright };
                let advance = measure.advance(&word, size);
                let length = cfg.line_length(size);
                if advance <= length {
                    units.push(Unit::single(word, advance, form, base + i, j - i, run));
                } else {
                    for (k, c) in chars.iter().enumerate().take(j).skip(i) {
                        let s = c.to_string();
                        let a = measure.advance(&s, size);
                        units.push(Unit::single(s, a, form, base + k, 1, run));
                    }
                }
            }
            i = j;
            continue;
        }
        let s = c.to_string();
        let (advance, form) = if cfg.vertical {
            let form = column_form(c);
            let a = if c.is_ascii() { measure.advance(&s, size) } else { size };
            (a, form)
        } else {
            (measure.advance(&s, size), Form::Upright)
        };
        units.push(Unit::single(s, advance, form, base + i, 1, run));
        i += 1;
    }
}

#[allow(clippy::too_many_arguments)]
fn ruby_unit(
    chars: &[char],
    base: usize,
    run: usize,
    ruby: &str,
    generated: bool,
    size: f32,
    cfg: &LayoutConfig,
    measure: &dyn Measure,
) -> Option<Unit> {
    let ruby_size = size * cfg.ruby_scale;
    let mut glyphs = Vec::with_capacity(chars.len());
    let mut at = 0.0;
    for (i, &c) in chars.iter().enumerate() {
        let s = c.to_string();
        let (advance, form) = if cfg.vertical { (size, column_form(c)) } else { (measure.advance(&s, size), Form::Upright) };
        glyphs.push(UnitGlyph { text: s, at, advance, form, offset: base + i, len: 1, run });
        at += advance;
    }
    let extent = if cfg.vertical { ruby.chars().count() as f32 * ruby_size } else { measure.advance(ruby, ruby_size) };
    let width = at.max(extent);
    if width > cfg.line_length(size) {
        return None;
    }
    if at < width {
        let gap = (width - at) / chars.len() as f32;
        for (i, g) in glyphs.iter_mut().enumerate() {
            g.at += gap * (i as f32 + 0.5);
        }
    }
    Some(Unit {
        advance: width,
        glyphs,
        ruby: Some(UnitRuby { text: ruby.to_owned(), at: (width - extent) / 2.0, extent, run, generated }),
        first: *chars.first()?,
        last: *chars.last()?,
        space: false,
    })
}

struct Line {
    units: Vec<Unit>,
    at: Vec<f32>,
    size: f32,
    block: usize,
}

fn break_lines(units: Vec<Unit>, length: f32, size: f32, block: usize, lines: &mut Vec<Line>) {
    const EPS: f32 = 0.01;
    let n = units.len();
    let mut units: Vec<Option<Unit>> = units.into_iter().map(Some).collect();
    let mut i = 0;
    let mut first_line = true;
    while i < n {
        if !first_line {
            while i < n && units[i].as_ref().is_some_and(|u| u.space) {
                i += 1;
            }
            if i >= n {
                break;
            }
        }
        first_line = false;
        let mut used = 0.0;
        let mut j = i;
        while j < n {
            let a = units[j].as_ref().map_or(0.0, |u| u.advance);
            if used + a > length + EPS {
                break;
            }
            used += a;
            j += 1;
        }
        let mut hung = false;
        let end = if j >= n {
            n
        } else if j == i {
            i + 1
        } else {
            let unit = |k: usize| units[k].as_ref().expect("unit taken twice");
            if unit(j).hangs() && (j + 1 >= n || !unit(j + 1).no_start()) {
                hung = true;
                j + 1
            } else {
                let ok = |b: usize| !unit(b).no_start() && !unit(b - 1).no_end();
                let mut b = j;
                while b > i + 1 && !ok(b) {
                    b -= 1;
                }
                if ok(b) { b } else { j }
            }
        };
        let taken: Vec<Unit> = (i..end).filter_map(|k| units[k].take()).collect();
        let last_of_paragraph = end >= n;
        lines.push(set_line(taken, length, size, block, !last_of_paragraph, hung));
        i = end;
    }
}

fn set_line(units: Vec<Unit>, length: f32, size: f32, block: usize, justify: bool, hung: bool) -> Line {
    let mut at = Vec::with_capacity(units.len());
    let mut x = 0.0;
    for u in &units {
        at.push(x);
        x += u.advance;
    }
    let body = if hung { units.len().saturating_sub(1) } else { units.len() };
    let natural: f32 = units.iter().take(body).map(|u| u.advance).sum();
    let trailing = units.iter().take(body).rev().take_while(|u| u.space).count();
    let gaps = body.saturating_sub(1 + trailing);
    let slack = length - natural;
    if justify && gaps > 0 && slack > 0.0 && slack / gaps as f32 <= size {
        let step = slack / gaps as f32;
        for (k, a) in at.iter_mut().enumerate() {
            *a += step * k.min(gaps) as f32;
        }
    }
    Line { units, at, size, block }
}

enum Item {
    Line(Line),
    Image { block: usize, offset: usize, width: u32, height: u32 },
}

pub fn layout(blocks: &[LayoutBlock<'_>], cfg: &LayoutConfig, measure: &dyn Measure) -> Vec<Page> {
    let mut items = Vec::new();
    for block in blocks {
        match *block {
            LayoutBlock::Text { block, start, text, runs, heading } => {
                let size = if heading { cfg.font_size * cfg.heading_scale } else { cfg.font_size };
                let chars: Vec<char> = text.chars().collect();
                let length = cfg.line_length(size);
                if length <= 0.0 {
                    continue;
                }
                let mut from = 0;
                let mut lines = Vec::new();
                for (k, _) in chars.iter().enumerate().filter(|(_, c)| **c == '\n').chain(std::iter::once((chars.len(), &'\n'))) {
                    let units = units_of(&chars, start, runs, size, cfg, measure, from, k);
                    if units.is_empty() {
                        lines.push(Line { units: Vec::new(), at: Vec::new(), size, block });
                    } else {
                        break_lines(units, length, size, block, &mut lines);
                    }
                    from = k + 1;
                }
                items.extend(lines.into_iter().map(Item::Line));
            }
            LayoutBlock::Image { block, start, width, height } => items.push(Item::Image { block, offset: start, width, height }),
        }
    }
    paginate(items, cfg)
}

fn paginate(items: Vec<Item>, cfg: &LayoutConfig) -> Vec<Page> {
    let cross_extent = cfg.cross_extent();
    let mut pages = Vec::new();
    let mut page = Page::default();
    let mut cross = 0.0;
    let mut has_any = false;
    let mut last_end = 0;
    let mut flush = |page: &mut Page, cross: &mut f32, has_any: &mut bool, pages: &mut Vec<Page>| {
        if *has_any {
            page.extent = *cross;
            let offsets =
                page.glyphs.iter().map(|g| (g.offset, g.offset + g.len)).chain(page.images.iter().map(|i| (i.offset, i.offset + 1)));
            let (start, end) = offsets.fold((usize::MAX, 0), |(s, e), (a, b)| (s.min(a), e.max(b)));
            (page.start, page.end) = if start == usize::MAX { (last_end, last_end) } else { (start, end) };
            last_end = page.end;
            pages.push(std::mem::take(page));
        }
        *cross = 0.0;
        *has_any = false;
    };
    for item in items {
        match item {
            Item::Line(line) => {
                let pitch = cfg.line_pitch(line.size);
                if cfg.paginated && has_any && cross + pitch > cross_extent + 0.01 {
                    flush(&mut page, &mut cross, &mut has_any, &mut pages);
                }
                place_line(&mut page, &line, cross, pitch, cfg);
                cross += pitch;
                has_any = true;
            }
            Item::Image { block, offset, width, height } => {
                let (w, h) = (width.max(1) as f32, height.max(1) as f32);
                if cfg.paginated {
                    flush(&mut page, &mut cross, &mut has_any, &mut pages);
                    let scale = (cfg.width / w).min(cfg.height / h);
                    let (iw, ih) = (w * scale, h * scale);
                    let rect = Rect { x: (cfg.width - iw) / 2.0, y: (cfg.height - ih) / 2.0, w: iw, h: ih };
                    page.images.push(PlacedImage { block, offset, rect });
                    has_any = true;
                    cross = cross_extent;
                    flush(&mut page, &mut cross, &mut has_any, &mut pages);
                } else {
                    let gap = cfg.font_size;
                    let rect = if cfg.vertical {
                        let ih = cfg.height.min(h);
                        let iw = w * ih / h;
                        Rect { x: cfg.width - cross - gap - iw, y: (cfg.height - ih) / 2.0, w: iw, h: ih }
                    } else {
                        let iw = cfg.width.min(w);
                        let ih = h * iw / w;
                        Rect { x: (cfg.width - iw) / 2.0, y: cross + gap, w: iw, h: ih }
                    };
                    page.images.push(PlacedImage { block, offset, rect });
                    cross += 2.0 * gap + if cfg.vertical { rect.w } else { rect.h };
                    has_any = true;
                }
            }
        }
    }
    flush(&mut page, &mut cross, &mut has_any, &mut pages);
    pages
}

fn place_line(page: &mut Page, line: &Line, cross: f32, pitch: f32, cfg: &LayoutConfig) {
    let size = line.size;
    let band = cfg.ruby_band(size);
    let ruby_size = size * cfg.ruby_scale;
    let body = (pitch - band - size) / 2.0;
    for (unit, at) in line.units.iter().zip(&line.at) {
        for g in &unit.glyphs {
            let main = at + g.at;
            let rect = if cfg.vertical {
                let right = cfg.width - cross;
                Rect { x: right - pitch + body, y: main, w: size, h: g.advance }
            } else {
                Rect { x: main, y: cross + band + body, w: g.advance, h: size }
            };
            page.glyphs.push(PlacedGlyph {
                text: g.text.clone(),
                rect,
                size,
                form: g.form,
                block: line.block,
                offset: g.offset,
                len: g.len,
                run: g.run,
            });
        }
        if let Some(r) = &unit.ruby {
            let main = at + r.at;
            let rect = if cfg.vertical {
                let right = cfg.width - cross;
                Rect { x: right - pitch + body + size, y: main, w: ruby_size, h: r.extent }
            } else {
                Rect { x: main, y: cross + band + body - ruby_size, w: r.extent, h: ruby_size }
            };
            page.ruby.push(PlacedRuby {
                text: r.text.clone(),
                rect,
                size: ruby_size,
                block: line.block,
                run: r.run,
                generated: r.generated,
            });
        }
    }
}

pub fn page_for_offset(pages: &[Page], offset: usize) -> usize {
    pages.iter().position(|p| offset < p.end).unwrap_or(pages.len().saturating_sub(1))
}

#[cfg(test)]
mod tests;
