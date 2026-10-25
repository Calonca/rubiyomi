// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use quick_xml::Reader;
use quick_xml::events::Event;

use super::opf::attr;
use super::{entities, resolve};
use crate::text::{Ruby, TextBlock};

pub(super) enum Extracted {
    Text(TextBlock),
    Image(String),
}

pub(super) struct Chapter {
    pub blocks: Vec<Extracted>,
    pub first_heading: Option<String>,
}

const BLOCKS: &[&str] = &[
    "p",
    "div",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "li",
    "ul",
    "ol",
    "blockquote",
    "dd",
    "dt",
    "dl",
    "td",
    "th",
    "tr",
    "table",
    "section",
    "article",
    "header",
    "footer",
    "aside",
    "main",
    "figure",
    "figcaption",
    "pre",
    "body",
    "hr",
    "nav",
];
const HEADINGS: &[&str] = &["h1", "h2", "h3", "h4", "h5", "h6"];
const SKIPPED: &[&str] = &["head", "script", "style", "title", "rp", "rtc", "noscript", "template"];

#[derive(Clone, Copy, PartialEq)]
enum Space {
    Spaces,
    Newline,
}

#[derive(Default)]
struct Builder {
    blocks: Vec<Extracted>,
    first_heading: Option<String>,
    text: String,
    chars: u32,
    ruby: Vec<Ruby>,
    space: Option<Space>,
    heading_depth: usize,
    skip_depth: usize,
    in_ruby: bool,
    base_start: u32,
    rt: Option<String>,
}

impl Builder {
    fn push_str(&mut self, s: &str) {
        for c in s.chars() {
            self.push_char(c);
        }
    }

    fn push_char(&mut self, c: char) {
        if self.skip_depth > 0 {
            return;
        }
        if let Some(rt) = &mut self.rt {
            if !c.is_ascii_whitespace() {
                rt.push(c);
            }
            return;
        }
        if c.is_ascii_whitespace() {
            if !self.text.is_empty() {
                let kind = if c == '\n' || c == '\r' { Space::Newline } else { Space::Spaces };
                self.space = Some(match self.space {
                    Some(Space::Newline) => Space::Newline,
                    _ => kind,
                });
            }
            return;
        }
        if let Some(space) = self.space.take() {
            let last = self.text.chars().last();
            let latin = |x: char| x.is_ascii_graphic() || x.is_alphabetic() && (x as u32) < 0x2E80;
            let keep = match space {
                Space::Spaces => true,
                // A wrapped line of Latin text keeps a space; Japanese doesn't.
                Space::Newline => last.is_some_and(latin) && latin(c),
            };
            if keep && last != Some('\n') {
                self.text.push(' ');
                self.chars += 1;
            }
        }
        self.text.push(c);
        self.chars += 1;
    }

    fn line_break(&mut self) {
        if self.skip_depth > 0 || self.rt.is_some() || self.text.is_empty() {
            return;
        }
        self.space = None;
        self.text.push('\n');
        self.chars += 1;
    }

    fn flush(&mut self) {
        self.space = None;
        while self.text.ends_with(|c: char| c.is_ascii_whitespace()) {
            self.text.pop();
            self.chars -= 1;
        }
        let chars = self.chars;
        self.ruby.retain(|r| r.start + r.len <= chars);
        let text = std::mem::take(&mut self.text);
        let ruby = std::mem::take(&mut self.ruby);
        self.chars = 0;
        self.base_start = 0;
        if text.chars().all(|c| c.is_whitespace()) {
            return;
        }
        let heading = self.heading_depth > 0;
        if heading && self.first_heading.is_none() {
            self.first_heading = Some(text.replace('\n', " "));
        }
        self.blocks.push(Extracted::Text(TextBlock { text, heading, ruby }));
    }

    fn start(&mut self, name: &str, e: &quick_xml::events::BytesStart<'_>, path: &str) {
        if SKIPPED.contains(&name) {
            self.skip_depth += 1;
            return;
        }
        if self.skip_depth > 0 {
            return;
        }
        if BLOCKS.contains(&name) {
            self.flush();
        }
        match name {
            n if HEADINGS.contains(&n) => self.heading_depth += 1,
            "br" => self.line_break(),
            "ruby" => {
                self.in_ruby = true;
                self.flush_space_into_text();
                self.base_start = self.chars;
            }
            "rt" => self.rt = Some(String::new()),
            "img" | "image" => {
                let src = attr(e, "src").or_else(|| attr(e, "href"));
                if let Some(src) = src.filter(|s| !s.starts_with("data:")) {
                    self.flush();
                    self.blocks.push(Extracted::Image(resolve(path, &src)));
                }
            }
            _ => {}
        }
    }

    fn flush_space_into_text(&mut self) {
        if let Some(Space::Spaces) = self.space {
            self.space = None;
            self.text.push(' ');
            self.chars += 1;
        }
    }

    fn end(&mut self, name: &str) {
        if SKIPPED.contains(&name) {
            self.skip_depth = self.skip_depth.saturating_sub(1);
            return;
        }
        if self.skip_depth > 0 {
            return;
        }
        match name {
            "rt" => {
                if let Some(reading) = self.rt.take() {
                    if self.in_ruby && self.chars > self.base_start && !reading.is_empty() {
                        self.ruby.push(Ruby { start: self.base_start, len: self.chars - self.base_start, text: reading });
                    }
                    self.base_start = self.chars;
                }
            }
            "ruby" => self.in_ruby = false,
            _ => {}
        }
        if BLOCKS.contains(&name) {
            self.flush();
        }
        if HEADINGS.contains(&name) {
            self.heading_depth = self.heading_depth.saturating_sub(1);
        }
    }
}

pub(super) fn extract(source: &str, path: &str) -> Chapter {
    let mut reader = Reader::from_str(source);
    let config = reader.config_mut();
    config.check_end_names = false;
    config.allow_unmatched_ends = true;
    config.expand_empty_elements = true;
    config.allow_dangling_amp = true;
    let mut b = Builder::default();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = e.local_name().as_ref().to_ascii_lowercase();
                b.start(&name, &e, path);
            }
            Ok(Event::End(e)) => {
                let name = e.local_name().as_ref().to_ascii_lowercase();
                b.end(&name);
            }
            Ok(Event::Text(t)) => b.push_str(&t.xml10_content()),
            Ok(Event::CData(t)) => b.push_str(&t.into_inner()),
            Ok(Event::GeneralRef(r)) => {
                if let Ok(Some(c)) = r.resolve_char_ref() {
                    b.push_char(c);
                } else if let Some(s) = entities::resolve(&r) {
                    b.push_str(s);
                } else {
                    b.push_str(&format!("&{};", &*r));
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                log::warn!("{path}: {e}; keeping what was read");
                break;
            }
            _ => {}
        }
    }
    b.flush();
    Chapter { blocks: b.blocks, first_heading: b.first_heading }
}
