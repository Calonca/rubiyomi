// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::io::{Cursor, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

pub struct EpubBuilder {
    title: String,
    author: String,
    chapters: Vec<(String, String)>,
    images: Vec<(String, Vec<u8>)>,
    extra: Vec<(String, Vec<u8>)>,
}

impl EpubBuilder {
    pub fn new() -> Self {
        EpubBuilder { title: "本".into(), author: String::new(), chapters: Vec::new(), images: Vec::new(), extra: Vec::new() }
    }

    pub fn title(mut self, title: &str) -> Self {
        self.title = title.into();
        self
    }

    pub fn author(mut self, author: &str) -> Self {
        self.author = author.into();
        self
    }

    pub fn chapter(mut self, title: &str, body: &str) -> Self {
        self.chapters.push((title.into(), body.into()));
        self
    }

    pub fn image(mut self, name: &str, bytes: Vec<u8>) -> Self {
        self.images.push((name.into(), bytes));
        self
    }

    pub fn file(mut self, path: &str, bytes: impl Into<Vec<u8>>) -> Self {
        self.extra.push((path.into(), bytes.into()));
        self
    }

    pub fn build(self) -> Vec<u8> {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        let put = |zip: &mut ZipWriter<Cursor<Vec<u8>>>, path: &str, bytes: &[u8]| {
            zip.start_file(path, stored).unwrap();
            zip.write_all(bytes).unwrap();
        };
        put(&mut zip, "mimetype", b"application/epub+zip");
        put(
            &mut zip,
            "META-INF/container.xml",
            br#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#,
        );
        let (mut manifest, mut spine, mut nav) = (String::new(), String::new(), String::new());
        for (i, (title, body)) in self.chapters.iter().enumerate() {
            let file = format!("text/ch{i}.xhtml");
            manifest.push_str(&format!("<item id=\"c{i}\" href=\"{file}\" media-type=\"application/xhtml+xml\"/>\n"));
            spine.push_str(&format!("<itemref idref=\"c{i}\"/>\n"));
            nav.push_str(&format!("<li><a href=\"{file}\">{title}</a></li>\n"));
            let xhtml = format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<html xmlns=\"http://www.w3.org/1999/xhtml\" xml:lang=\"ja\">\n<head><title>{title}</title></head>\n<body>\n{body}\n</body>\n</html>\n"
            );
            put(&mut zip, &format!("OEBPS/{file}"), xhtml.as_bytes());
        }
        for (i, (name, bytes)) in self.images.iter().enumerate() {
            manifest.push_str(&format!("<item id=\"i{i}\" href=\"images/{name}\" media-type=\"image/png\"/>\n"));
            put(&mut zip, &format!("OEBPS/images/{name}"), bytes);
        }
        manifest.push_str("<item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>\n");
        let nav = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\">\n<head><title>目次</title></head>\n<body><nav epub:type=\"toc\"><ol>\n{nav}</ol></nav></body>\n</html>\n"
        );
        put(&mut zip, "OEBPS/nav.xhtml", nav.as_bytes());
        let opf = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"id\">\n<metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n<dc:identifier id=\"id\">urn:uuid:00000000-0000-4000-8000-000000000000</dc:identifier>\n<dc:title>{}</dc:title>\n<dc:creator>{}</dc:creator>\n<dc:language>ja</dc:language>\n</metadata>\n<manifest>\n{manifest}</manifest>\n<spine page-progression-direction=\"rtl\">\n{spine}</spine>\n</package>\n",
            self.title, self.author
        );
        put(&mut zip, "OEBPS/content.opf", opf.as_bytes());
        for (path, bytes) in &self.extra {
            put(&mut zip, path, bytes);
        }
        zip.finish().unwrap().into_inner()
    }
}

pub fn png(width: u32, height: u32) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    image::RgbImage::from_pixel(width, height, image::Rgb([200, 120, 40])).write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

pub fn tempdir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("rubiyomi-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// 芥川龍之介「羅生門」(1915), public domain (author d. 1927); text and ruby as on Aozora Bunko:
// https://www.aozora.gr.jp/cards/000879/files/127_15260.html
pub const RASHOMON: [&str; 2] = [
    "ある日の暮方の事である。一人の<ruby><rb>下人</rb><rp>（</rp><rt>げにん</rt><rp>）</rp></ruby>が、<ruby><rb>羅生門</rb><rp>（</rp><rt>らしょうもん</rt><rp>）</rp></ruby>の下で雨やみを待っていた。",
    "広い門の下には、この男のほかに誰もいない。",
];
