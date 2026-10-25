// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use super::*;
use crate::testing::{EpubBuilder, RASHOMON, png, tempdir};
use crate::text::Block;

fn book() -> Vec<u8> {
    EpubBuilder::new()
        .title("羅生門")
        .chapter("一", &format!("<p>{}</p>", RASHOMON[0]))
        .chapter("二", &format!("<p>{}</p><p><img src=\"../images/gate.png\"/></p>", RASHOMON[1]))
        .image("gate.png", png(30, 20))
        .build()
}

#[test]
fn importing_twice_finds_the_same_book() {
    let lib = Library::open(tempdir("lib-import"));
    let first = lib.import_bytes(book()).unwrap();
    assert!(!first.already);
    let again = lib.import_bytes(book()).unwrap();
    assert!(again.already);
    assert_eq!(again.meta.id, first.meta.id);
    assert_eq!(lib.books().len(), 1);
    let Block::Image(img) = &lib.chapter(&first.meta.id, 1).unwrap().blocks[1] else { panic!() };
    assert!(lib.image_path(&first.meta.id, &img.file).exists());
}

#[test]
fn a_hostile_image_path_is_written_inside_the_book() {
    let root = tempdir("lib-hostile");
    let lib = Library::open(root.join("library"));
    let bytes =
        EpubBuilder::new().chapter("一", "<p><img src=\"../../../../../../escape.png\"/></p>").file("escape.png", png(4, 4)).build();
    let id = lib.import_bytes(bytes).unwrap().meta.id;
    let mut files = Vec::new();
    let mut dirs = vec![root.clone()];
    while let Some(dir) = dirs.pop() {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            if e.file_type().unwrap().is_dir() { dirs.push(e.path()) } else { files.push(e.path()) }
        }
    }
    let escaped: Vec<_> = files.iter().filter(|f| f.file_name().is_some_and(|n| n == "escape.png")).collect();
    assert!(!escaped.is_empty());
    assert!(escaped.iter().all(|f| f.starts_with(lib.book_dir(&id).join("images"))), "{escaped:?}");
}

#[test]
fn a_damaged_chapter_is_extracted_again_keeping_the_place() {
    let lib = Library::open(tempdir("lib-damaged"));
    let id = lib.import_bytes(book()).unwrap().meta.id;
    lib.save_progress(&id, &Progress { chapter: 1, offset: 2, fraction: 0.6, updated: 5, word: Some((3, 2)) }).unwrap();
    std::fs::write(lib.book_dir(&id).join("chapters/0000.json"), "garbage").unwrap();
    assert_eq!(lib.chapter(&id, 0).unwrap().title, "一");
    assert_eq!(lib.progress(&id).offset, 2);
    assert_eq!(lib.books().len(), 1);
}
