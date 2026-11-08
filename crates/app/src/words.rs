// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::rc::Rc;

use nsh_base::alert_dialog;
use nsh_core::prelude::*;
use rubiyomi_core::cards::WordKey;
use rubiyomi_core::history::Looked;
use rubiyomi_core::library::{Progress, fsutil::now};

use crate::app::{Route, Shared, route, shared};
use crate::reader::words::StateColours;
use crate::ui::{self, Ripples, c, scheme};

// More rows than this make every layout slow; a search narrows them down.
const SHOWN: usize = 200;

fn matches(l: &Looked, q: &str) -> bool {
    q.is_empty()
        || l.spelling.contains(q)
        || l.reading.contains(q)
        || l.sentence.contains(q)
        || l.meanings.iter().any(|m| m.to_lowercase().contains(q))
}

// Reopens the book on the page the word was seen on, with the word marked.
fn open_in_book(shared: &Shared, l: &Looked) {
    let Some(meta) = shared.library.meta(&l.book) else { return };
    let (chapter, offset) = (l.chapter as usize, l.offset as usize);
    let len = if l.len > 0 { l.len } else { l.spelling.chars().count() as u32 };
    let progress = Progress {
        chapter: l.chapter,
        offset: l.offset,
        fraction: meta.fraction_at(chapter, offset),
        updated: now(),
        word: Some((l.offset, len)),
    };
    if let Err(e) = shared.library.save_progress(&l.book, &progress) {
        ui::snackbar(format!("Couldn't open it: {e}"));
        return;
    }
    route().set(Route::Reader(l.book.clone()));
}

fn details(shared: &Shared, l: &Looked, version: Signal<u64>, open: Signal<Option<WordKey>>) -> AnyElement {
    let seen = if l.count == 1 { "Looked up once".to_owned() } else { format!("Looked up {} times", l.count) };
    let book = if l.book_title.is_empty() { String::new() } else { format!(" \u{b7} {}", l.book_title) };
    let (key, history) = (l.key(), shared.history.clone());
    let can_open = shared.library.meta(&l.book).is_some();
    let (jump_shared, jump) = (shared.clone(), l.clone());
    view()
        .pl(px(40.0))
        .pr_4()
        .pb_3()
        .gap(px(4.0))
        .child((!l.sentence.is_empty()).then(|| text(l.sentence.clone()).text_size(px(16.0)).text_color(move || c(scheme().on_surface))))
        .child(text(format!("{seen}{book}")).text_size(px(12.0)).text_color(move || c(scheme().on_surface_variant)))
        .child(
            view()
                .flex_row()
                .gap(px(8.0))
                .child(can_open.then(|| ui::tonal_button("Open in the book", move || open_in_book(&jump_shared, &jump))))
                .child(ui::tonal_button("Remove", move || {
                    history.borrow_mut().remove(&key);
                    open.set(None);
                    version.update(|v| *v += 1);
                })),
        )
        .into_element()
}

fn row(
    shared: &Shared,
    l: &Looked,
    dot: Option<rubiyomi_core::theme::Color>,
    version: Signal<u64>,
    open: Signal<Option<WordKey>>,
) -> AnyElement {
    let key = l.key();
    let is_open = {
        let key = key.clone();
        move || open.with(|o| o.as_ref() == Some(&key))
    };
    let toggle = key.clone();
    let meaning = l.meanings.first().cloned().unwrap_or_default();
    let colour = dot.map(c).unwrap_or_else(|| c(scheme().on_surface_variant).alpha(0.3));
    view()
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(px(14.0))
                .px_4()
                .py_3()
                .bg({
                    let is_open = is_open.clone();
                    move || if is_open() { c(scheme().on_surface).alpha(0.05) } else { c(scheme().surface).alpha(0.0) }
                })
                .ripples()
                .on_press(move || open.set(if open.with_untracked(|o| o.as_ref() == Some(&toggle)) { None } else { Some(toggle.clone()) }))
                .child(view().size(px(10.0)).rounded(px(5.0)).bg(colour))
                .child(
                    view()
                        .flex_1()
                        .child(
                            view()
                                .flex_row()
                                .items_center()
                                .gap(px(8.0))
                                .child(text(l.spelling.clone()).text_size(px(20.0)).text_color(move || c(scheme().on_surface)))
                                .child((l.reading != l.spelling).then(|| {
                                    text(l.reading.clone()).text_size(px(14.0)).text_color(move || c(scheme().on_surface_variant))
                                })),
                        )
                        .child(text(meaning).text_size(px(14.0)).number_of_lines(1).text_color(move || c(scheme().on_surface_variant))),
                ),
        )
        .child({
            let (shared, l) = (shared.clone(), l.clone());
            // Only this block follows `open`, so opening a row leaves the list and its scroll alone.
            dynamic(move || if is_open() { details(&shared, &l, version, open) } else { view().into_element() })
        })
        .into_element()
}

fn list(shared: &Shared, query: Signal<String>, version: Signal<u64>, open: Signal<Option<WordKey>>) -> AnyElement {
    version.get();
    let q = query.get().trim().to_lowercase();
    let grader = shared.grader.get();
    let colours = shared.page.with(|p| StateColours::chosen(p, scheme().dark));
    let history = shared.history.borrow();
    let found: Vec<&Looked> = history.items().iter().filter(|l| matches(l, &q)).collect();
    let count = match found.len() {
        1 => "1 word".to_owned(),
        n if n > SHOWN => format!("{SHOWN} of {n} words"),
        n => format!("{n} words"),
    };
    let shown: Vec<&Looked> = found.into_iter().take(SHOWN).collect();
    let states = grader
        .map(|g| g.states(&shown.iter().map(|l| l.word()).collect::<Vec<_>>()))
        .or_else(|| crate::hooks::fake_states(shown.iter().map(|l| l.key()), 5));
    let mut rows = view().pb_4();
    for l in shown {
        let dot = states.as_ref().and_then(|m| m.get(&l.key())).and_then(|st| colours.get(*st));
        rows = rows.child(row(shared, l, dot, version, open));
    }
    let ask = {
        let shared = shared.clone();
        move || confirm_clear(shared.clone(), version, open)
    };
    view()
        .flex_1()
        .child(
            view()
                .flex_row()
                .items_center()
                .justify_between()
                .px_4()
                .child(text(count).text_size(px(14.0)).text_color(move || c(scheme().on_surface_variant)))
                .child(ui::tonal_button("Clear all", ask)),
        )
        .child(scroll().flex_1().child(rows))
        .into_element()
}

fn confirm_clear(shared: Shared, version: Signal<u64>, open: Signal<Option<WordKey>>) {
    let answer = alert_dialog()
        .title("Clear all words?")
        .description("The list of words you looked up is emptied. Your cards and reading stats stay.")
        .cancel("cancel", "Cancel")
        .destructive("clear", "Clear")
        .show();
    spawn(async move {
        if answer.await.is_ok_and(|r| r.action.as_deref() == Some("clear")) {
            shared.history.borrow_mut().clear();
            open.set(None);
            version.update(|v| *v += 1);
        }
    });
}

// Debug hook for the headless runs: `RUBIYOMI_WORDS=open` opens the first row.
#[cfg(feature = "dev-hooks")]
fn debug_open(shared: &Shared) -> Option<WordKey> {
    if crate::hooks::var("RUBIYOMI_WORDS").as_deref() != Some("open") {
        return None;
    }
    shared.history.borrow().items().first().map(Looked::key)
}

#[cfg(not(feature = "dev-hooks"))]
fn debug_open(_: &Shared) -> Option<WordKey> {
    None
}

pub fn screen() -> impl IntoElement {
    let shared = shared();
    let version = signal(0u64);
    let query = signal(String::new());
    let open: Signal<Option<WordKey>> = signal(debug_open(&shared));
    let shared = Rc::new(shared);
    view().flex_1().child(ui::page_bar("Words")).child(dynamic(move || {
        // Only a removal or a clear rebuilds this, so typing keeps the field.
        version.get();
        if shared.history.borrow().items().is_empty() {
            return ui::empty_state(ui::glyph::DICTIONARY, "No words yet", "Words you look up while reading will be collected here.");
        }
        let shared = shared.clone();
        let kept = query.get_untracked();
        view()
            .flex_1()
            .child(
                view().px_4().pb_2().child(
                    nsh_core::element::input()
                        .placeholder("Search words, readings, meanings")
                        .default_value(kept)
                        .text_size(px(16.0))
                        .text_color(move || c(scheme().on_surface))
                        .on_change(move |v: &str| query.set(v.to_owned())),
                ),
            )
            .child(dynamic(move || list(&shared, query, version, open)))
            .into_element()
    }))
}
