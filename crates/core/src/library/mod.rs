// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

pub mod fsutil;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::epub::{self, EpubError};
use crate::text::Chapter;
use fsutil::{now, read_json, read_json_or_default, write_atomic, write_json};

pub const EXTRACT_VERSION: u32 = 2;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BookId(pub String);

impl std::fmt::Display for BookId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChapterInfo {
    pub title: String,
    pub chars: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BookMeta {
    pub id: BookId,
    pub title: String,
    pub author: String,
    pub language: String,
    pub chapters: Vec<ChapterInfo>,
    pub added: u64,
    pub extract_version: u32,
    pub has_cover: bool,
}

impl BookMeta {
    pub fn total_chars(&self) -> u64 {
        self.chapters.iter().map(|c| c.chars as u64).sum()
    }

    pub fn fraction_at(&self, chapter: usize, offset: usize) -> f32 {
        let total = self.total_chars();
        if total == 0 {
            return 0.0;
        }
        let before: u64 = self.chapters.iter().take(chapter).map(|c| c.chars as u64).sum();
        let within = self.chapters.get(chapter).map_or(0, |c| (offset as u64).min(c.chars as u64));
        ((before + within) as f64 / total as f64) as f32
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Progress {
    pub chapter: u32,
    pub offset: u32,
    pub fraction: f32,
    pub updated: u64,
    // The word last looked up in that chapter, start and length, to reopen at.
    #[serde(default)]
    pub word: Option<(u32, u32)>,
}

#[derive(Debug)]
pub enum ImportError {
    Epub(EpubError),
    Io(std::io::Error),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::Epub(e) => e.fmt(f),
            ImportError::Io(e) => write!(f, "The book couldn't be saved: {e}"),
        }
    }
}

impl std::error::Error for ImportError {}

impl From<std::io::Error> for ImportError {
    fn from(e: std::io::Error) -> Self {
        ImportError::Io(e)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Imported {
    pub meta: BookMeta,
    pub already: bool,
}

#[derive(Clone, Debug)]
pub struct Library {
    dir: PathBuf,
}

impl Library {
    pub fn open(dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        let _ = std::fs::create_dir_all(dir.join("books"));
        Library { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn book_dir(&self, id: &BookId) -> PathBuf {
        self.dir.join("books").join(&id.0)
    }

    pub fn books(&self) -> Vec<BookMeta> {
        let Ok(entries) = std::fs::read_dir(self.dir.join("books")) else { return Vec::new() };
        let mut books: Vec<(u64, BookMeta)> = entries
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter_map(|e| read_json::<BookMeta>(&e.path().join("meta.json")))
            .map(|m| {
                let p = self.progress(&m.id);
                (p.updated.max(m.added), m)
            })
            .collect();
        books.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.title.cmp(&b.1.title)));
        books.into_iter().map(|(_, m)| m).collect()
    }

    pub fn meta(&self, id: &BookId) -> Option<BookMeta> {
        read_json(&self.book_dir(id).join("meta.json"))
    }

    pub fn import_file(&self, path: &Path) -> Result<Imported, ImportError> {
        self.import_bytes(std::fs::read(path)?)
    }

    pub fn import_bytes(&self, bytes: Vec<u8>) -> Result<Imported, ImportError> {
        self.import_inner(bytes, false)
    }

    fn import_inner(&self, bytes: Vec<u8>, again: bool) -> Result<Imported, ImportError> {
        // Two threads may re-extract the same damaged book.
        static IMPORTING: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _one = IMPORTING.lock().unwrap_or_else(|e| e.into_inner());
        let id = BookId(Sha256::digest(&bytes).iter().take(8).map(|b| format!("{b:02x}")).collect());
        if let Some(meta) = self.meta(&id)
            && meta.extract_version == EXTRACT_VERSION
            && !again
        {
            return Ok(Imported { meta, already: true });
        }
        let book = epub::open(std::io::Cursor::new(&bytes)).map_err(ImportError::Epub)?;
        // Built beside the library and moved in whole, so a book is never half there.
        let staging = self.dir.join("books").join(format!(".{id}.importing"));
        let _ = std::fs::remove_dir_all(&staging);
        let meta = match write_book(&staging, &id, &book, &bytes) {
            Ok(meta) => meta,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&staging);
                return Err(e.into());
            }
        };
        let target = self.book_dir(&id);
        if let Ok(progress) = std::fs::read(target.join("progress.json")) {
            let _ = write_atomic(&staging.join("progress.json"), &progress);
        }
        let _ = std::fs::remove_dir_all(&target);
        std::fs::rename(&staging, &target)?;
        Ok(Imported { meta, already: false })
    }

    pub fn chapter(&self, id: &BookId, index: usize) -> Option<Chapter> {
        let path = chapter_path(&self.book_dir(id), index);
        if let Some(c) = read_json(&path) {
            return Some(c);
        }
        log::warn!("chapter {index} of {id} is unreadable; extracting the book again");
        self.reextract(id).ok()?;
        read_json(&path)
    }

    pub fn reextract(&self, id: &BookId) -> Result<Imported, ImportError> {
        let bytes = std::fs::read(self.book_dir(id).join("book.epub"))?;
        self.import_inner(bytes, true)
    }

    pub fn stale(&self) -> Vec<BookId> {
        self.books().into_iter().filter(|m| m.extract_version != EXTRACT_VERSION).map(|m| m.id).collect()
    }

    pub fn image_path(&self, id: &BookId, file: &str) -> PathBuf {
        self.book_dir(id).join("images").join(file)
    }

    pub fn cover_path(&self, id: &BookId) -> PathBuf {
        self.book_dir(id).join("cover.jpg")
    }

    pub fn delete(&self, id: &BookId) -> std::io::Result<()> {
        std::fs::remove_dir_all(self.book_dir(id))
    }

    pub fn progress(&self, id: &BookId) -> Progress {
        read_json_or_default(&self.book_dir(id).join("progress.json"))
    }

    pub fn save_progress(&self, id: &BookId, progress: &Progress) -> std::io::Result<()> {
        write_json(&self.book_dir(id).join("progress.json"), progress)
    }

    pub fn last_read(&self) -> Option<(BookMeta, Progress)> {
        self.books()
            .into_iter()
            .map(|m| (self.progress(&m.id), m))
            .filter(|(p, _)| p.updated > 0)
            .max_by_key(|(p, _)| p.updated)
            .map(|(p, m)| (m, p))
    }
}

fn chapter_path(book_dir: &Path, index: usize) -> PathBuf {
    book_dir.join("chapters").join(format!("{index:04}.json"))
}

fn write_book(dir: &Path, id: &BookId, book: &epub::Book, bytes: &[u8]) -> std::io::Result<BookMeta> {
    std::fs::create_dir_all(dir.join("chapters"))?;
    write_atomic(&dir.join("book.epub"), bytes)?;
    for (i, chapter) in book.chapters.iter().enumerate() {
        write_json(&chapter_path(dir, i), chapter)?;
    }
    for (file, data) in &book.images {
        write_atomic(&dir.join("images").join(file), data)?;
    }
    let first_image = || {
        book.chapters
            .iter()
            .flat_map(|c| &c.blocks)
            .find_map(|b| if let crate::text::Block::Image(i) = b { book.images.get(&i.file) } else { None })
    };
    let has_cover = book.cover.as_ref().or_else(first_image).is_some_and(|bytes| write_cover(&dir.join("cover.jpg"), bytes).is_ok());
    let meta = BookMeta {
        id: id.clone(),
        title: book.title.clone(),
        author: book.author.clone(),
        language: book.language.clone(),
        chapters: book.chapters.iter().map(|c| ChapterInfo { title: c.title.clone(), chars: c.char_len() as u32 }).collect(),
        added: now(),
        extract_version: EXTRACT_VERSION,
        has_cover,
    };
    write_json(&dir.join("meta.json"), &meta)?;
    Ok(meta)
}

fn write_cover(path: &Path, bytes: &[u8]) -> Result<(), image::ImageError> {
    let img = image::load_from_memory(bytes)?;
    let thumb = img.thumbnail(360, 540).to_rgb8();
    let mut out = std::io::Cursor::new(Vec::new());
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85).encode_image(&thumb)?;
    write_atomic(path, &out.into_inner())?;
    Ok(())
}

#[cfg(test)]
mod tests;
