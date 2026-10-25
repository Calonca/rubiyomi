// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::io::Cursor;

use super::*;
use crate::testing::{EpubBuilder, RASHOMON};
use crate::text::{Block, Ruby};

fn open_bytes(bytes: Vec<u8>) -> Result<Book, EpubError> {
    open(Cursor::new(bytes))
}

fn rashomon() -> EpubBuilder {
    EpubBuilder::new()
        .title("羅生門")
        .author("芥川 龍之介")
        .chapter("一", &format!("<p>{}</p>", RASHOMON[0]))
        .chapter("二", &format!("<p>{}</p>", RASHOMON[1]))
}

#[test]
fn chapters_follow_the_spine_with_ruby_at_char_offsets() {
    let book = open_bytes(rashomon().build()).unwrap();
    assert_eq!((book.title.as_str(), book.author.as_str(), book.language.as_str()), ("羅生門", "芥川 龍之介", "ja"));
    assert_eq!(book.chapters.iter().map(|c| c.title.as_str()).collect::<Vec<_>>(), ["一", "二"]);
    let Block::Text(block) = &book.chapters[0].blocks[0] else { panic!() };
    assert_eq!(block.text, "ある日の暮方の事である。一人の下人が、羅生門の下で雨やみを待っていた。");
    assert_eq!(block.ruby, [Ruby { start: 15, len: 2, text: "げにん".into() }, Ruby { start: 19, len: 3, text: "らしょうもん".into() }]);
}

#[test]
fn drm_is_refused_but_font_obfuscation_is_not() {
    let rights = rashomon().file("META-INF/rights.xml", "<rights/>").build();
    assert_eq!(open_bytes(rights).unwrap_err(), EpubError::Drm);
    let encryption = |algorithm: &str| {
        format!(
            r#"<encryption xmlns="urn:oasis:names:tc:opendocument:xmlns:container" xmlns:enc="http://www.w3.org/2001/04/xmlenc#"><enc:EncryptedData><enc:EncryptionMethod Algorithm="{algorithm}"/></enc:EncryptedData></encryption>"#
        )
    };
    let aes = rashomon().file("META-INF/encryption.xml", encryption("http://www.w3.org/2001/04/xmlenc#aes128-cbc")).build();
    assert_eq!(open_bytes(aes).unwrap_err(), EpubError::Drm);
    let fonts = rashomon().file("META-INF/encryption.xml", encryption("http://www.idpf.org/2008/embedding")).build();
    assert!(open_bytes(fonts).is_ok());
}

#[test]
fn broken_input_is_an_error_not_a_panic() {
    assert_eq!(open_bytes(b"plain text".to_vec()).unwrap_err(), EpubError::NotEpub);
    let empty_zip = zip::ZipWriter::new(Cursor::new(Vec::new())).finish().unwrap().into_inner();
    assert_eq!(open_bytes(empty_zip).unwrap_err(), EpubError::NotEpub);
    let mut truncated = rashomon().build();
    truncated.truncate(truncated.len() / 2);
    assert!(open_bytes(truncated).is_err());
    let book = open_bytes(EpubBuilder::new().chapter("一", "<p>&xxe;<b>閉じない</p>").build()).unwrap();
    assert!(book.chapters[0].blocks.iter().filter_map(Block::as_text).any(|t| t.text.contains("&xxe;")), "entities aren't resolved");
}

#[test]
fn hostile_hrefs_stay_inside_the_book() {
    assert_eq!(resolve("OEBPS/text/ch0.xhtml", "../../../../etc/passwd"), "etc/passwd");
    assert_eq!(resolve("OEBPS/text/ch0.xhtml", "%2e%2e/%2e%2e/%2e%2e/x.png"), "x.png");
    assert_eq!(resolve("OEBPS/text/ch0.xhtml", "/abs/../../y.png#frag"), "y.png");
    for path in ["../../a.png", "OEBPS/../../b.png", "c/../../../d.png", "..", "a/.."] {
        let name = flat_name(&resolve("OEBPS/text/ch0.xhtml", path));
        assert!(!name.contains('/') && name != ".." && name != ".", "{path} -> {name}");
    }
}
