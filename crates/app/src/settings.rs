// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::cell::RefCell;
use std::rc::Rc;

use nsh_base::alert_dialog;
use nsh_core::prelude::*;
use rubiyomi_core::dict::jmdict;
use rubiyomi_core::jpdb::JpdbClient;
use rubiyomi_core::platform::Platform;
use rubiyomi_core::settings::{Parser, ReviewSource, Settings};

use crate::app::{Shared, shared};
use crate::reviews::{self, Status};
use crate::reviews_form::{ReviewsForm, ask, jpdb_key_field};
use crate::ui::{self, c, glyph, scheme};

const PRIVACY: &str = "https://github.com/Calonca/rubiyomi/blob/main/PRIVACY.md";
const IPADIC_NOTICE: &str = include_str!("../notices/ipadic.txt");

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    Main,
    Reading,
    // `from_about`: reached from About, so Back returns there.
    Dictionary {
        from_about: bool,
    },
    Parsing,
    Reviews,
    About,
    WordFinder,
    // `from_page`: reached from the page settings, so Back returns there.
    WordColours {
        from_page: bool,
    },
    CustomColours {
        from_page: bool,
    },
}

impl Page {
    fn title(self) -> &'static str {
        match self {
            Page::Main => "Settings",
            Page::Reading => "Page",
            Page::Dictionary { .. } => "Dictionary",
            Page::Parsing => "Parsing",
            Page::Reviews => "Reviews",
            Page::About => "About",
            Page::WordFinder => "Word finder",
            Page::WordColours { .. } => "Word colours",
            Page::CustomColours { .. } => "Custom colours",
        }
    }

    fn parent(self) -> Page {
        match self {
            Page::Dictionary { from_about: true } | Page::WordFinder => Page::About,
            Page::WordColours { from_page: true } => Page::Reading,
            Page::CustomColours { from_page } => Page::WordColours { from_page },
            _ => Page::Main,
        }
    }
}

fn item(glyph: &'static str, title: &'static str, detail: impl Into<String>, to: Signal<Page>, page: Page) -> impl IntoElement {
    ui::icon_row(glyph, title, detail, move || to.set(page))
}

fn main(to: Signal<Page>, shared: &Shared) -> AnyElement {
    let platform = shared.platform.clone();
    let (settings, reviews, page) = (shared.settings, shared.reviews, shared.page);
    let palette = computed(move || page.with(|p| p.palette));
    view()
        .child(ui::section_header("Reading"))
        .child(item(glyph::AUTO_STORIES, "Page", "Theme, text size, furigana, vertical text", to, Page::Reading))
        .child(ui::section_header("Words"))
        .child(item(glyph::DICTIONARY, "Dictionary", "JMdict, English, on this device", to, Page::Dictionary { from_about: false }))
        .child(dynamic(move || {
            let detail = match (settings.with(|s| s.parser), settings.with(|s| !s.jpdb_key.trim().is_empty())) {
                (Parser::Device, _) => "On this device",
                (Parser::Jpdb, true) => "jpdb.io",
                (Parser::Jpdb, false) => "jpdb.io: needs a key, on this device until then",
            };
            item(glyph::SEGMENT, "Parsing", detail, to, Page::Parsing).into_element()
        }))
        .child(dynamic(move || {
            item(glyph::SCHOOL, "Reviews", reviews.summary(settings.with(|s| s.reviews)), to, Page::Reviews).into_element()
        }))
        .child(dynamic(move || {
            item(glyph::PALETTE, "Word colours", palette.get().label(), to, Page::WordColours { from_page: false }).into_element()
        }))
        .child(ui::section_header("App"))
        .child(item(
            glyph::INFO,
            "About",
            concat!("Rubiyomi ", env!("CARGO_PKG_VERSION"), " \u{b7} dictionary and licences"),
            to,
            Page::About,
        ))
        .child(ui::icon_row(glyph::PRIVACY_TIP, "Privacy policy", "Opens in your browser", move || {
            if !platform.open_url(PRIVACY) {
                log::warn!("couldn't open {PRIVACY}");
            }
        }))
        .into_element()
}

fn parsing(shared: &Shared) -> AnyElement {
    let settings = shared.settings;
    let parser = computed(move || settings.with(|s| s.parser));
    let (chooser, section) = (shared.clone(), shared.clone());
    view()
        .child(ui::hint("Who finds the words in the text, and their readings and meanings when you tap one."))
        .child(ui::segmented(
            None,
            &["On this device", "jpdb.io"],
            &[Parser::Device, Parser::Jpdb],
            move || parser.get(),
            move |v| {
                let mut changed = settings.get_untracked();
                changed.parser = v;
                reviews::apply_parsing(&chooser, changed);
            },
        ))
        .child(dynamic(move || match parser.get() {
            Parser::Device => {
                ui::hint("Found with lindera and the JMdict dictionary. Nothing leaves the device, and it works offline.").into_element()
            }
            Parser::Jpdb => jpdb_parsing(&section),
        }))
        .into_element()
}

// The key is the reviews' too, which then sync with it.
fn save_key(shared: &Shared, key: &str) {
    if shared.settings.with_untracked(|s| s.jpdb_key == key) {
        return;
    }
    let mut changed = shared.settings.get_untracked();
    changed.jpdb_key = key.to_owned();
    if changed.reviews == ReviewSource::Jpdb {
        reviews::apply(shared, changed);
    } else {
        reviews::apply_parsing(shared, changed);
    }
}

fn jpdb_parsing(shared: &Shared) -> AnyElement {
    let start = shared.settings.with_untracked(|s| s.jpdb_key.clone());
    let (saved, tries) = (signal(start.trim().to_owned()), signal(0u32));
    let answer = signal_with(None, SignalEquals::Never);
    ask(
        move || {
            let key = saved.get();
            (!key.is_empty()).then(|| (key, tries.get()))
        },
        |(key, _)| {
            let client = JpdbClient::new(key);
            Box::new(move || client.parse_words(&["日本語".to_owned()]).map(|_| ()).map_err(|e| e.to_string()))
        },
        answer,
    );
    // Going back from the field may not take the focus from it first.
    let typed = Rc::new(RefCell::new(start.clone()));
    {
        let (shared, typed) = (shared.clone(), typed.clone());
        cleanup(move || save_key(&shared, typed.borrow().trim()));
    }
    let commit = {
        let shared = shared.clone();
        move |key: String| {
            save_key(&shared, &key);
            saved.set(key);
        }
    };
    view()
        .child(ui::hint(
            "Split the way jpdb.io's own reader splits them. The text of each chapter is sent to jpdb.io once, when you \
             first open it, and its words are kept on the device. Offline, they are found on the device.",
        ))
        .child(jpdb_key_field(shared.platform.clone(), start, move |v| *typed.borrow_mut() = v, commit))
        .child(dynamic(move || {
            if saved.with(|k| k.is_empty()) {
                return ui::hint("Until there's a key, the words are found on the device.").into_element();
            }
            match answer.get() {
                None => ui::hint("Asking jpdb.io\u{2026}").into_element(),
                Some(Ok(())) => ui::hint("jpdb.io takes the key.").into_element(),
                Some(Err(e)) => ui::retry(format!("{e}. Until it's fixed, the words are found on the device."), tries),
            }
        }))
        .into_element()
}

fn complete(draft: &Settings) -> bool {
    draft.reviews != ReviewSource::Anki || draft.anki.is_set()
}

// Changes to the reviews that would be lost by leaving their page.
fn unsaved(shared: &Shared) -> Option<Settings> {
    let draft = shared.reviews.form.get_untracked()?.get_untracked();
    (draft.reviews != ReviewSource::None && shared.settings.with_untracked(|s| draft != *s)).then_some(draft)
}

// Leaves the Reviews page, or whatever else is open, asking first about changes not saved yet.
pub fn leave(shared: &Shared, go: impl FnOnce() + 'static) {
    let Some(draft) = unsaved(shared) else {
        go();
        return;
    };
    let mut dialog = alert_dialog()
        .title("Save changes?")
        .description("Your reviews settings have changes that aren't saved yet.")
        .cancel("cancel", "Cancel")
        .action("discard", "Discard");
    if complete(&draft) {
        dialog = dialog.action("save", "Save");
    }
    let answer = dialog.show();
    let shared = shared.clone();
    spawn(async move {
        match answer.await.ok().and_then(|r| r.action).as_deref() {
            Some("save") => {
                reviews::apply(&shared, draft);
                go();
            }
            Some("discard") => go(),
            _ => {}
        }
    });
}

fn review_status(shared: &Shared, draft: Signal<Settings>) -> AnyElement {
    let (settings, reviews) = (shared.settings, shared.reviews);
    let changed = computed(move || draft.with(|d| settings.with(|s| d != s)));
    dynamic(move || {
        let unsaved = changed.get();
        let line = match reviews.status.get() {
            _ if unsaved => ui::hint("Not saved yet").into_element(),
            Status::Syncing => {
                let said = reviews.syncing_text();
                let mut first = said.chars();
                let said: String = first.next().map(|c| c.to_uppercase().chain(first).collect()).unwrap_or_default();
                ui::hint(said).into_element()
            }
            Status::Synced => ui::hint("Up to date").into_element(),
            Status::Failed(e) => ui::error(e).into_element(),
            Status::Off => view().into_element(),
        };
        view()
            .child(line)
            .child(reviews.sync_note.get().filter(|_| !unsaved).map(ui::error))
            .child(reviews.note.get().map(ui::error))
            .into_element()
    })
}

fn reviews_page(shared: &Shared) -> AnyElement {
    let form = ReviewsForm::new(shared);
    let (draft, source, slot) = (form.draft, form.source, shared.reviews.form);
    slot.set(Some(draft));
    cleanup(move || slot.set(None));
    // Turning reviews off needs nothing more, so it happens at once.
    {
        let shared = shared.clone();
        effect(move || {
            if source.get() == ReviewSource::None && shared.settings.with_untracked(|s| s.reviews != ReviewSource::None) {
                let mut off = shared.settings.get_untracked();
                off.reviews = ReviewSource::None;
                untracked(|| reviews::apply(&shared, off));
            }
        });
    }
    let ready = computed(move || draft.with(complete));
    let save = shared.clone();
    view()
        .child(form.view())
        .child(dynamic(move || {
            let shared = save.clone();
            if source.get() == ReviewSource::None {
                view().into_element()
            } else if !ready.get() {
                ui::hint("To save, choose a deck, a note type and the field that holds the word.").into_element()
            } else {
                view()
                    .px_4()
                    .pt_4()
                    .pb_2()
                    .child(ui::primary_button("Save and sync", move || reviews::apply(&shared, draft.get_untracked())))
                    .into_element()
            }
        }))
        .child(review_status(shared, draft))
        .into_element()
}

fn dictionary(shared: &Shared) -> AnyElement {
    let installed = match shared.dicts.installed() {
        Some(d) => format!("Installed: JMdict of {}, jmdict-simplified {}; mecab-ipadic {}.", d.jmdict_date, d.jmdict, d.ipadic),
        None => "Not installed yet.".to_owned(),
    };
    view()
        .child(ui::prose(jmdict::SOURCE))
        .child(ui::prose(format!("The dictionary data, and the index Rubiyomi builds from it, are licensed under {}. Rubiyomi claims no copyright over them.", jmdict::LICENCE)))
        .child(ui::prose("The English JMdict comes through jmdict-simplified, https://github.com/scriptin/jmdict-simplified. Each Rubiyomi release bundles its newest release."))
        .child(ui::prose(installed))
        .into_element()
}

fn about(to: Signal<Page>, shared: &Shared) -> AnyElement {
    let version = match shared.dicts.installed() {
        Some(d) => format!("JMdict {} \u{b7} CC BY-SA 4.0", d.jmdict_date),
        None => "Not installed yet".to_owned(),
    };
    view()
        .child(ui::section_header(concat!("Rubiyomi ", env!("CARGO_PKG_VERSION"))))
        .child(ui::icon_row(glyph::GAVEL, "Licence", "MIT \u{b7} \u{a9} 2026 Alessandro La Conca", || {}))
        .child(ui::icon_row(glyph::GAVEL, "Interface", "NSH (native-shell), Apache-2.0, https://github.com/longbridge/native-shell", || {}))
        .child(ui::icon_row(
            glyph::GAVEL,
            "Icons",
            "Material Symbols Rounded (Google), Apache-2.0, https://github.com/google/material-design-icons",
            || {},
        ))
        .child(item(glyph::DICTIONARY, "Dictionary", version, to, Page::Dictionary { from_about: true }))
        .child(item(glyph::DESCRIPTION, "Word finder", "lindera and mecab-ipadic \u{b7} notice", to, Page::WordFinder))
        .into_element()
}

fn word_finder() -> AnyElement {
    view()
        .child(ui::prose(
            "Rubiyomi finds the words in a sentence with lindera (MIT) and the mecab-ipadic dictionary, whose notice follows.",
        ))
        .child(view().px_4().py_2().child(text(IPADIC_NOTICE).text_size(px(11.0)).text_color(move || c(scheme().on_surface_variant))))
        .into_element()
}

pub fn screen() -> impl IntoElement {
    let shared = shared();
    let to = signal(Page::Main);
    let backing = shared.clone();
    on_back_press(move || {
        let at = to.get_untracked();
        if at == Page::Main {
            return false;
        }
        leave(&backing, move || to.set(at.parent()));
        true
    });
    if let Some(page) = crate::hooks::var("RUBIYOMI_SETTINGS_PAGE").and_then(|p| debug_page(&p)) {
        to.set(page);
    }
    let bar = shared.clone();
    let body = {
        let shared = shared.clone();
        dynamic(move || match to.get() {
            Page::Main => main(to, &shared),
            Page::Reading => crate::page_form::form(shared.page, shared.page_dragging, shared.grader, move || {
                to.set(Page::WordColours { from_page: true })
            }),
            Page::Dictionary { .. } => dictionary(&shared),
            Page::Parsing => parsing(&shared),
            Page::Reviews => reviews_page(&shared),
            Page::About => about(to, &shared),
            Page::WordFinder => word_finder(),
            Page::WordColours { from_page } => crate::colours_page::choose(shared.page, move || to.set(Page::CustomColours { from_page })),
            Page::CustomColours { .. } => crate::colours_page::custom(shared.page),
        })
    };
    view()
        .flex_1()
        .child(dynamic(move || match to.get() {
            Page::Main => ui::page_bar("Settings").into_element(),
            at => {
                let shared = bar.clone();
                ui::back_bar(at.title(), move || leave(&shared, move || to.set(at.parent()))).into_element()
            }
        }))
        .child(scroll().flex_1().child(body))
}

// A debug hook for the headless runs: `RUBIYOMI_SETTINGS_PAGE=page|dictionary|parsing|reviews|about|finder|colours|custom`.
#[cfg(feature = "dev-hooks")]
fn debug_page(name: &str) -> Option<Page> {
    Some(match name {
        "page" => Page::Reading,
        "dictionary" => Page::Dictionary { from_about: false },
        "parsing" => Page::Parsing,
        "reviews" => Page::Reviews,
        "about" => Page::About,
        "finder" => Page::WordFinder,
        "colours" => Page::WordColours { from_page: false },
        "custom" => Page::CustomColours { from_page: false },
        _ => return None,
    })
}

#[cfg(not(feature = "dev-hooks"))]
fn debug_page(_: &str) -> Option<Page> {
    None
}
