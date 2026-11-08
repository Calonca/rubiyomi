// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::path::PathBuf;

use nsh_base::alert_dialog;
use nsh_core::prelude::*;
use rubiyomi_core::library::{BookMeta, Library, Progress};
use rubiyomi_core::platform::Platform;

use crate::app::{Route, route, shared};
use crate::ui::{self, Ripples, c, glyph, scheme};

// Bumped whenever the books on disk change, so the shelf reads them again.
#[derive(Clone, Copy, Default, PartialEq)]
struct ShelfVersion(u64);

fn version() -> Signal<ShelfVersion> {
    global::<ShelfVersion>()
}

pub fn changed() {
    version().update(|v| v.0 += 1);
}

// Importing parses the whole EPUB, so it runs off the UI thread.
pub fn import(library: Library, path: PathBuf, name: String, temporary: bool) {
    ui::snackbar("Adding…");
    spawn(async move {
        let done = blocking(move || {
            let result = library.import_file(&path);
            if temporary {
                let _ = std::fs::remove_file(&path);
            }
            result
        })
        .await;
        match done {
            Ok(Ok(i)) if i.already => ui::snackbar(format!("{} is already in your library", i.meta.title)),
            Ok(Ok(i)) => ui::snackbar(format!("Added {}", i.meta.title)),
            Ok(Err(e)) => ui::snackbar(format!("{name}: {e}")),
            Err(e) => ui::snackbar(format!("{name}: {e}")),
        }
        changed();
    });
}

// Books extracted by an older version are read again from their stored EPUB.
pub fn reextract_stale(library: Library) {
    spawn(async move {
        let found = blocking(move || {
            let stale = library.stale();
            for id in &stale {
                if let Err(e) = library.reextract(id) {
                    log::warn!("reextracting {id}: {e}");
                }
            }
            !stale.is_empty()
        })
        .await;
        if found.unwrap_or(false) {
            changed();
        }
    });
}

// A cloth colour for a book without a cover, the same for the same title every time.
fn cloth(title: &str) -> Rgba {
    let hash = title.chars().fold(2166136261u32, |h, ch| (h ^ ch as u32).wrapping_mul(16777619));
    let hue = (hash % 360) as f32 / 60.0;
    let (s, v) = (0.45_f32, 0.42_f32);
    let chroma = v * s;
    let x = chroma * (1.0 - (hue % 2.0 - 1.0).abs());
    let (r, g, b) = match hue as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let m = v - chroma;
    let byte = |n: f32| ((n + m) * 255.0).round() as u32;
    rgb(byte(r) << 16 | byte(g) << 8 | byte(b))
}

fn cover(library: &Library, meta: &BookMeta) -> AnyElement {
    let frame = view().w_full().aspect_ratio(2.0 / 3.0).rounded(px(4.0)).overflow_hidden();
    if meta.has_cover {
        let path = library.cover_path(&meta.id).to_string_lossy().into_owned();
        return frame.bg(move || c(scheme().surface_container_high)).child(image(path).w_full().h_full()).into_element();
    }
    let ink = rgb(0xf3ecdc);
    frame
        .bg(cloth(&meta.title))
        .items_center()
        .justify_center()
        .p_2()
        .child(text(meta.title.clone()).text_color(ink).text_size(px(14.0)).text_center().number_of_lines(4))
        .into_element()
}

fn bar(fraction: f32) -> impl IntoElement {
    view()
        .w_full()
        .h(px(4.0))
        .rounded(px(2.0))
        .overflow_hidden()
        .bg(move || c(scheme().surface_container_highest))
        .child(view().h_full().w(percentage(fraction.clamp(0.0, 1.0))).bg(move || c(scheme().primary)))
}

fn confirm_remove(library: Library, meta: BookMeta) {
    let answer = alert_dialog()
        .title("Remove this book?")
        .description(format!("{} and your place in it will be removed from this device.", meta.title))
        .cancel("cancel", "Cancel")
        .destructive("remove", "Remove")
        .show();
    spawn(async move {
        if !answer.await.is_ok_and(|r| r.action.as_deref() == Some("remove")) {
            return;
        }
        match library.delete(&meta.id) {
            Ok(()) => ui::snackbar(format!("Removed {}", meta.title)),
            Err(e) => ui::snackbar(format!("Couldn't remove it: {e}")),
        }
        changed();
    });
}

fn tile(library: &Library, meta: BookMeta, progress: Progress) -> impl IntoElement {
    let (open, ask) = (route(), {
        let (library, meta) = (library.clone(), meta.clone());
        move || confirm_remove(library.clone(), meta.clone())
    });
    let id = meta.id.clone();
    let started = progress.updated != 0;
    let line = if started { format!("{}% · {}", (progress.fraction * 100.0).round() as u32, meta.author) } else { meta.author.clone() };
    view()
        .gap(px(6.0))
        .ripples()
        .on_press(move || open.set(Route::Reader(id.clone())))
        .on_long_press({
            let ask = ask.clone();
            move || ask()
        })
        .on_context_menu(move |_: &ContextMenuEvent| ask())
        .child(cover(library, &meta))
        .child(started.then(|| bar(progress.fraction)))
        .child(text(meta.title).text_size(px(14.0)).number_of_lines(2).text_color(move || c(scheme().on_surface)))
        .child(text(line).text_size(px(12.0)).number_of_lines(1).text_color(move || c(scheme().on_surface_variant)))
}

// Three columns on a phone; a wider window fits as many 104 px cells as it can, as crates/app does.
fn columns() -> u16 {
    let width = ui::window_width();
    if width < 600.0 { 3 } else { (((width - 32.0 + 16.0) / 120.0).floor() as u16).max(3) }
}

fn shelf(library: Library) -> AnyElement {
    let books: Vec<_> = library.books().into_iter().map(|m| (library.progress(&m.id), m)).collect();
    if books.is_empty() {
        return view()
            .flex_1()
            .items_center()
            .justify_center()
            .gap(px(4.0))
            .child(ui::icon(glyph::LOCAL_LIBRARY, 64.0, move || c(scheme().on_surface_variant)).mb(px(12.0)))
            .child(text("No books yet").text_size(px(20.0)).text_color(move || c(scheme().on_surface)))
            .child(text("Add an EPUB to start reading.").text_color(move || c(scheme().on_surface_variant)))
            .into_element();
    }
    let mut grid = view().grid().grid_cols(columns).gap(px(16.0)).px_4().pb(px(96.0));
    for (progress, meta) in books {
        grid = grid.child(tile(&library, meta, progress));
    }
    scroll().flex_1().child(grid).into_element()
}

pub fn screen() -> impl IntoElement {
    let shared = shared();
    let library = shared.library.clone();
    let platform = shared.platform.clone();
    let version = version();
    view()
        .flex_1()
        .child(ui::page_bar("Library"))
        .child(dynamic(move || {
            version.get();
            shelf(library.clone())
        }))
        .child(
            view()
                .absolute()
                .right(px(16.0))
                .bottom(px(16.0))
                .h(px(56.0))
                .pl_4()
                .pr_5()
                .gap(px(8.0))
                .flex_row()
                .rounded(px(16.0))
                .items_center()
                .bg(move || c(scheme().primary_container))
                .ripples()
                .on_press(move || platform.pick_epub())
                .child(ui::icon(glyph::ADD, 24.0, move || c(scheme().on_primary_container)))
                .child(text("Add book").text_size(px(14.0)).text_color(move || c(scheme().on_primary_container))),
        )
}
