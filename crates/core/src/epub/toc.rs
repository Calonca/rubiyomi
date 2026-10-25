// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::collections::HashMap;
use std::io::{Read, Seek};

use quick_xml::Reader;
use quick_xml::events::Event;

use super::opf::{Package, attr};
use super::{Archive, entities, resolve};

pub(super) fn read<R: Read + Seek>(archive: &mut Archive<R>, package: &Package) -> HashMap<String, String> {
    let from_nav = package.nav.as_ref().and_then(|p| Some((p, archive.read_string(p)?))).map(|(p, x)| nav(&x, p)).unwrap_or_default();
    if !from_nav.is_empty() {
        return from_nav;
    }
    package.ncx.as_ref().and_then(|p| Some((p, archive.read_string(p)?))).map(|(p, x)| ncx(&x, p)).unwrap_or_default()
}

fn push_ref(text: &mut String, r: &quick_xml::events::BytesRef<'_>) {
    if let Ok(Some(c)) = r.resolve_char_ref() {
        text.push(c);
    } else if let Some(s) = entities::resolve(r) {
        text.push_str(s);
    }
}

fn clean(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn nav(xml: &str, path: &str) -> HashMap<String, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().check_end_names = false;
    let mut titles = HashMap::new();
    let mut fallback = HashMap::new();
    let mut depth_in_nav = 0usize;
    let mut is_toc = false;
    let mut link: Option<(String, String)> = None;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => match e.local_name().as_ref() {
                "nav" => {
                    depth_in_nav += 1;
                    if depth_in_nav == 1 {
                        is_toc = attr(&e, "type").is_some_and(|t| t.split_whitespace().any(|w| w == "toc"));
                    }
                }
                "a" if depth_in_nav > 0 => link = attr(&e, "href").map(|h| (resolve(path, &h), String::new())),
                _ => {}
            },
            Ok(Event::Text(t)) => {
                if let Some((_, text)) = &mut link {
                    text.push_str(&t.xml10_content());
                }
            }
            Ok(Event::GeneralRef(r)) => {
                if let Some((_, text)) = &mut link {
                    push_ref(text, &r);
                }
            }
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                "a" => {
                    if let Some((href, text)) = link.take() {
                        let target = if is_toc { &mut titles } else { &mut fallback };
                        target.entry(href).or_insert_with(|| clean(&text));
                    }
                }
                "nav" => depth_in_nav = depth_in_nav.saturating_sub(1),
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    if titles.is_empty() { fallback } else { titles }
}

fn ncx(xml: &str, path: &str) -> HashMap<String, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().check_end_names = false;
    let mut titles = HashMap::new();
    let mut label = String::new();
    let mut in_text = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e) | Event::Empty(e)) => match e.local_name().as_ref() {
                "navPoint" => label.clear(),
                "text" => in_text = true,
                "content" => {
                    if let Some(src) = attr(&e, "src") {
                        titles.entry(resolve(path, &src)).or_insert_with(|| clean(&label));
                    }
                }
                _ => {}
            },
            Ok(Event::Text(t)) if in_text => label.push_str(&t.xml10_content()),
            Ok(Event::GeneralRef(r)) if in_text => push_ref(&mut label, &r),
            Ok(Event::End(e)) if e.local_name().as_ref() == "text" => in_text = false,
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    titles
}
