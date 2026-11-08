// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use nsh_core::prelude::*;
use rubiyomi_core::settings::{Parser, ReviewSource};

use crate::app::{Route, route, shared};
use crate::reviews_form::ReviewsForm;
use crate::ui::{self, c, scheme};

// Shown once, on the first launch; everything in it is in Settings too.
pub fn screen() -> impl IntoElement {
    // A debug hook for the headless runs: `RUBIYOMI_SETUP_STEP=1` starts on the last step.
    let reviewing = signal(crate::hooks::var("RUBIYOMI_SETUP_STEP").is_some());
    on_back_press(move || {
        if !reviewing.get_untracked() {
            return false;
        }
        reviewing.set(false);
        true
    });
    scroll()
        .flex_1()
        .child(view().px_6().pt_8().child(dynamic(move || if reviewing.get() { reviews(reviewing) } else { welcome(reviewing) })))
}

fn heading(title: &'static str) -> impl IntoElement {
    text(title).text_size(px(24.0)).pb_4().text_color(move || c(scheme().on_surface))
}

fn welcome(reviewing: Signal<bool>) -> AnyElement {
    view()
        .child(heading("Welcome to Rubiyomi"))
        .child(
            text("Read Japanese books with their furigana, and tap any word to see what it means. The dictionary is on this device, so it works offline.")
                .text_size(px(16.0))
                .text_color(move || c(scheme().on_surface)),
        )
        .child(view().pt_3().pb_8().child(text("First, a few choices. All of them can be changed later in Settings.").text_size(px(13.0)).text_color(move || c(scheme().on_surface_variant))))
        .child(ui::primary_button("Get started", move || reviewing.set(true)))
        .into_element()
}

fn reviews(reviewing: Signal<bool>) -> AnyElement {
    let (shared, route) = (shared(), route());
    let form = ReviewsForm::new(&shared);
    let (draft, source) = (form.draft, form.source);
    view()
        .child(heading("Reviews"))
        .child(text("Do you study with jpdb.io, Anki or WaniKani?").text_size(px(16.0)).pb_3().text_color(move || c(scheme().on_surface)))
        .child(form.view())
        .child(dynamic(move || {
            if source.get() != ReviewSource::Jpdb {
                return view().into_element();
            }
            ui::hint(
                "jpdb.io will also find the words in the text, as its own reader does. You can change that in Settings \u{2192} Parsing.",
            )
            .into_element()
        }))
        .child(
            view()
                .pt_6()
                .pb_8()
                .gap(px(8.0))
                .child(dynamic(move || {
                    let shared = shared.clone();
                    let label = if source.get() == ReviewSource::None { "Finish" } else { "Save and finish" };
                    ui::primary_button(label, move || {
                        let mut chosen = draft.get_untracked();
                        chosen.setup_done = true;
                        if chosen.reviews == ReviewSource::Jpdb {
                            chosen.parser = Parser::Jpdb;
                        }
                        crate::reviews::apply(&shared, chosen);
                        route.set(Route::Library);
                    })
                    .into_element()
                }))
                .child(view().px_6().py_3().items_center().child(ui::tonal_button("Back", move || reviewing.set(false)))),
        )
        .into_element()
}
