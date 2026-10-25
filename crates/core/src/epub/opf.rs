// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::collections::HashMap;
use std::io::{Read, Seek};

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use super::{Archive, EpubError, entities, resolve};

#[derive(Debug, Default)]
pub(super) struct Package {
    pub title: String,
    pub author: String,
    pub language: String,
    pub spine: Vec<String>,
    pub cover: Option<String>,
    pub nav: Option<String>,
    pub ncx: Option<String>,
}

struct Item {
    href: String,
    media_type: String,
    properties: String,
}

pub(super) fn attr(e: &BytesStart<'_>, name: &str) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.local_name().as_ref() == name)
        .and_then(|a| a.normalized_value_with(quick_xml::XmlVersion::Implicit1_0, 16, entities::resolve).ok().map(|v| v.into_owned()))
}

fn rootfile<R: Read + Seek>(archive: &mut Archive<R>) -> Result<String, EpubError> {
    let Some(xml) = archive.read_string("META-INF/container.xml") else {
        return Err(EpubError::NotEpub);
    };
    let mut reader = Reader::from_str(&xml);
    loop {
        match reader.read_event() {
            Ok(Event::Start(e) | Event::Empty(e)) if e.local_name().as_ref() == "rootfile" => {
                if let Some(path) = attr(&e, "full-path") {
                    return Ok(path);
                }
            }
            Ok(Event::Eof) => return Err(EpubError::Malformed("container.xml names no package".into())),
            Err(e) => return Err(EpubError::Malformed(format!("container.xml: {e}"))),
            _ => {}
        }
    }
}

pub(super) fn read<R: Read + Seek>(archive: &mut Archive<R>) -> Result<Package, EpubError> {
    let opf_path = rootfile(archive)?;
    let Some(xml) = archive.read_string(&opf_path) else {
        return Err(EpubError::Malformed(format!("the package {opf_path} is missing")));
    };
    let mut reader = Reader::from_str(&xml);
    let mut package = Package::default();
    let mut items: HashMap<String, Item> = HashMap::new();
    let mut spine_ids = Vec::new();
    let mut toc_id = None;
    let mut cover_id = None;
    let mut reading: Option<&'static str> = None;
    let mut text = String::new();
    loop {
        let event = reader.read_event().map_err(|e| EpubError::Malformed(format!("{opf_path}: {e}")))?;
        match event {
            Event::Start(e) | Event::Empty(e) => match e.local_name().as_ref() {
                "title" if package.title.is_empty() => (reading, text) = (Some("title"), String::new()),
                "creator" if package.author.is_empty() => (reading, text) = (Some("creator"), String::new()),
                "language" if package.language.is_empty() => (reading, text) = (Some("language"), String::new()),
                "meta" if attr(&e, "name").as_deref() == Some("cover") => cover_id = attr(&e, "content"),
                "item" => {
                    if let (Some(id), Some(href)) = (attr(&e, "id"), attr(&e, "href")) {
                        let item = Item {
                            href: resolve(&opf_path, &href),
                            media_type: attr(&e, "media-type").unwrap_or_default(),
                            properties: attr(&e, "properties").unwrap_or_default(),
                        };
                        items.insert(id, item);
                    }
                }
                "spine" => toc_id = attr(&e, "toc"),
                "itemref" => {
                    if let Some(id) = attr(&e, "idref") {
                        spine_ids.push(id);
                    }
                }
                _ => {}
            },
            Event::Text(t) if reading.is_some() => text.push_str(&t.xml10_content()),
            Event::GeneralRef(r) if reading.is_some() => {
                if let Ok(Some(c)) = r.resolve_char_ref() {
                    text.push(c);
                } else if let Some(s) = entities::resolve(&r) {
                    text.push_str(s);
                }
            }
            Event::End(_) if reading.is_some() => {
                let value = text.split_whitespace().collect::<Vec<_>>().join(" ");
                match reading.take() {
                    Some("title") => package.title = value,
                    Some("creator") => package.author = value,
                    Some("language") => package.language = value,
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }

    for id in &spine_ids {
        if let Some(item) = items.get(id) {
            package.spine.push(item.href.clone());
        }
    }
    if package.spine.is_empty() {
        return Err(EpubError::Malformed("the spine is empty".into()));
    }
    package.nav = items.values().find(|i| i.properties.split_whitespace().any(|p| p == "nav")).map(|i| i.href.clone());
    package.ncx = toc_id
        .and_then(|id| items.get(&id))
        .or_else(|| items.values().find(|i| i.media_type == "application/x-dtbncx+xml"))
        .map(|i| i.href.clone());
    package.cover = items
        .values()
        .find(|i| i.properties.split_whitespace().any(|p| p == "cover-image"))
        .or_else(|| cover_id.and_then(|id| items.get(&id)))
        .filter(|i| i.media_type.starts_with("image/"))
        .map(|i| i.href.clone());
    if package.title.is_empty() {
        package.title = opf_path.rsplit('/').next().unwrap_or("Untitled").trim_end_matches(".opf").to_owned();
    }
    Ok(package)
}
