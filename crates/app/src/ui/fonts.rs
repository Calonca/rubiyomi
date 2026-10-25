// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use egui::{FontData, FontDefinitions, FontFamily};

pub const BOOK_SANS: &str = "book-sans";
pub const BOOK_SERIF: &str = "book-serif";

#[derive(Clone, Copy, Debug)]
pub struct Face {
    bytes: &'static [u8],
    index: u32,
}

fn load(path: &str, index: u32) -> Option<Face> {
    static CACHE: OnceLock<Mutex<HashMap<String, &'static [u8]>>> = OnceLock::new();
    let mut cache = CACHE.get_or_init(Default::default).lock().ok()?;
    if let Some(bytes) = cache.get(path) {
        return Some(Face { bytes, index });
    }
    let bytes: &'static [u8] = Box::leak(std::fs::read(path).ok()?.into_boxed_slice());
    cache.insert(path.to_owned(), bytes);
    Some(Face { bytes, index })
}

#[cfg(not(target_os = "android"))]
mod find {
    use super::{Face, load};

    fn fc_match(format: &str, pattern: &str) -> Option<String> {
        let out = std::process::Command::new("fc-match").args(["-f", format, pattern]).output().ok()?;
        String::from_utf8(out.stdout).ok()
    }

    pub fn pattern(pattern: &str) -> Option<Face> {
        let text = fc_match("%{file}\n%{index}", pattern)?;
        let mut lines = text.lines();
        let file = lines.next()?.trim().to_owned();
        let index = lines.next().and_then(|i| i.trim().parse::<u32>().ok()).unwrap_or(0) & 0xffff;
        load(&file, index)
    }

    fn installed(family: &str) -> bool {
        fc_match("%{family}", family).is_some_and(|got| got.to_lowercase().contains(&family.to_lowercase()))
    }

    pub fn ui() -> Option<Face> {
        ["Roboto", "Noto Sans", "Adwaita Sans", "Cantarell", "Ubuntu", "DejaVu Sans", "Liberation Sans"]
            .iter()
            .find(|f| installed(f))
            .and_then(|f| pattern(f))
            .or_else(|| pattern("sans-serif"))
    }

    pub fn japanese_sans() -> Option<Face> {
        pattern("sans-serif:lang=ja")
    }

    pub fn japanese_serif() -> Option<Face> {
        let family = fc_match("%{family}", "serif:lang=ja")?.to_lowercase();
        (family.contains("serif") || family.contains("mincho") || family.contains("明朝")).then(|| pattern("serif:lang=ja")).flatten()
    }
}

#[cfg(target_os = "android")]
mod find {
    use super::{Face, load};

    fn first(names: &[&str]) -> Option<Face> {
        names.iter().find_map(|n| load(&format!("/system/fonts/{n}"), 0))
    }

    pub fn ui() -> Option<Face> {
        first(&["Roboto-Regular.ttf", "RobotoStatic-Regular.ttf", "Roboto.ttf", "DroidSans.ttf"])
    }

    pub fn japanese_sans() -> Option<Face> {
        first(&["NotoSansCJK-Regular.ttc", "NotoSansJP-Regular.otf", "DroidSansJapanese.ttf", "DroidSansFallback.ttf"])
    }

    pub fn japanese_serif() -> Option<Face> {
        first(&["NotoSerifCJK-Regular.ttc", "NotoSerifJP-Regular.otf"])
    }
}

struct Found {
    ui: Option<Face>,
    sans: Option<Face>,
    serif: Option<Face>,
}

fn found() -> &'static Found {
    static FOUND: OnceLock<Found> = OnceLock::new();
    FOUND.get_or_init(|| {
        let found = Found { ui: find::ui(), sans: find::japanese_sans(), serif: find::japanese_serif() };
        log::info!("fonts: ui {}, japanese sans {}, japanese serif {}", found.ui.is_some(), found.sans.is_some(), found.serif.is_some());
        found
    })
}

fn data(face: Face) -> Arc<FontData> {
    let mut d = FontData::from_static(face.bytes);
    d.index = face.index;
    Arc::new(d)
}

pub fn has_serif() -> bool {
    found().serif.is_some()
}

pub fn install(ctx: &egui::Context) {
    let f = found();
    let mut defs = FontDefinitions::default();
    let mut put = |name: &str, face: Option<Face>| {
        if let Some(face) = face {
            defs.font_data.insert(name.to_owned(), data(face));
            true
        } else {
            false
        }
    };
    let ui = put("device-ui", f.ui);
    let sans = put("device-ja-sans", f.sans);
    let serif = put("device-ja-serif", f.serif);
    let proportional = defs.families.entry(FontFamily::Proportional).or_default();
    let mut front = Vec::new();
    if ui {
        front.push("device-ui".to_owned());
    }
    if sans {
        front.push("device-ja-sans".to_owned());
    }
    proportional.splice(0..0, front);
    let fallback = defs.families[&FontFamily::Proportional].clone();
    let book_sans: Vec<String> = sans.then(|| "device-ja-sans".to_owned()).into_iter().chain(fallback.clone()).collect();
    let book_serif: Vec<String> = serif.then(|| "device-ja-serif".to_owned()).into_iter().chain(book_sans.iter().cloned()).collect();
    defs.families.insert(FontFamily::Name(BOOK_SANS.into()), book_sans);
    defs.families.insert(FontFamily::Name(BOOK_SERIF.into()), book_serif);
    ctx.set_fonts(defs);
    egui_material_icons::initialize(ctx);
}
