// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

mod drm;
mod entities;
mod opf;
mod toc;
mod xhtml;

use std::collections::BTreeMap;
use std::io::{Read, Seek};

use crate::text::{Block, Chapter};

#[derive(Debug)]
pub struct Book {
    pub title: String,
    pub author: String,
    pub language: String,
    pub chapters: Vec<Chapter>,
    pub images: BTreeMap<String, Vec<u8>>,
    pub cover: Option<Vec<u8>>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum EpubError {
    Drm,
    NotEpub,
    Malformed(String),
}

impl std::fmt::Display for EpubError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EpubError::Drm => write!(f, "This book is DRM-protected and can't be opened."),
            EpubError::NotEpub => write!(f, "This file isn't an EPUB."),
            EpubError::Malformed(why) => write!(f, "This EPUB can't be read: {why}"),
        }
    }
}

impl std::error::Error for EpubError {}

struct Archive<R: Read + Seek> {
    zip: zip::ZipArchive<R>,
}

impl<R: Read + Seek> Archive<R> {
    fn has(&self, path: &str) -> bool {
        self.zip.index_for_name(path).is_some()
    }

    fn read(&mut self, path: &str) -> Option<Vec<u8>> {
        let index = self.zip.index_for_name(path).or_else(|| {
            // Some books differ in case between the manifest and the zip.
            let lower = path.to_lowercase();
            (0..self.zip.len()).find(|&i| self.zip.name_for_index(i).and_then(|n| n.ok()).is_some_and(|n| n.to_lowercase() == lower))
        })?;
        let mut file = self.zip.by_index(index).ok()?;
        let mut bytes = Vec::with_capacity(file.size() as usize);
        file.read_to_end(&mut bytes).ok()?;
        Some(bytes)
    }

    fn read_string(&mut self, path: &str) -> Option<String> {
        let bytes = self.read(path)?;
        Some(match String::from_utf8(bytes) {
            Ok(s) => s,
            Err(e) => String::from_utf8_lossy(e.as_bytes()).into_owned(),
        })
    }
}

pub fn open<R: Read + Seek>(reader: R) -> Result<Book, EpubError> {
    let zip = zip::ZipArchive::new(reader).map_err(|_| EpubError::NotEpub)?;
    let mut archive = Archive { zip };
    drm::check(&mut archive)?;
    let package = opf::read(&mut archive)?;
    let titles = toc::read(&mut archive, &package);

    let mut chapters = Vec::with_capacity(package.spine.len());
    let mut images = BTreeMap::new();
    for (n, path) in package.spine.iter().enumerate() {
        if is_image(path) {
            let blocks = image_block(&mut archive, path, &mut images).map(Block::Image).into_iter().collect();
            chapters.push(Chapter { title: titles.get(path).cloned().unwrap_or_else(|| format!("Chapter {}", n + 1)), blocks });
            continue;
        }
        let Some(source) = archive.read_string(path) else {
            log::warn!("spine item {path} is missing");
            chapters.push(Chapter { title: format!("Chapter {}", n + 1), blocks: Vec::new() });
            continue;
        };
        let extracted = xhtml::extract(&source, path);
        let mut blocks = Vec::with_capacity(extracted.blocks.len());
        for block in extracted.blocks {
            match block {
                xhtml::Extracted::Text(t) => blocks.push(Block::Text(t)),
                xhtml::Extracted::Image(src) => {
                    if let Some(image) = image_block(&mut archive, &src, &mut images) {
                        blocks.push(Block::Image(image));
                    }
                }
            }
        }
        let title = titles
            .get(path)
            .cloned()
            .or(extracted.first_heading)
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| format!("Chapter {}", n + 1));
        chapters.push(Chapter { title, blocks });
    }

    let cover = package.cover.as_deref().and_then(|p| archive.read(p));
    Ok(Book { title: package.title, author: package.author, language: package.language, chapters, images, cover })
}

fn image_block<R: Read + Seek>(
    archive: &mut Archive<R>,
    path: &str,
    images: &mut BTreeMap<String, Vec<u8>>,
) -> Option<crate::text::ImageBlock> {
    let file = flat_name(path);
    if !images.contains_key(&file) {
        let bytes = archive.read(path)?;
        images.insert(file.clone(), bytes);
    }
    let bytes = &images[&file];
    let (width, height) = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().ok()?.into_dimensions().ok()?;
    Some(crate::text::ImageBlock { file, width, height })
}

fn is_image(path: &str) -> bool {
    let lower = path.to_lowercase();
    [".png", ".jpg", ".jpeg", ".gif", ".webp"].iter().any(|e| lower.ends_with(e))
}

fn flat_name(path: &str) -> String {
    let name: String = path
        .trim_start_matches('/')
        .split('/')
        .filter(|s| !s.is_empty() && *s != "OEBPS" && *s != "OPS" && *s != "item")
        .collect::<Vec<_>>()
        .join("_")
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' })
        .collect();
    if name.is_empty() { "image".into() } else { name }
}

pub(crate) fn resolve(base_file: &str, href: &str) -> String {
    let href = href.split(['#', '?']).next().unwrap_or("");
    let href = percent_decode(href);
    let mut parts: Vec<&str> = if href.starts_with('/') {
        Vec::new()
    } else {
        let mut p: Vec<&str> = base_file.split('/').collect();
        p.pop();
        p
    };
    for seg in href.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.retain(|s| !s.is_empty());
    parts.join("/")
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(v) = u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("zz"), 16)
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests;
