// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::rc::Rc;
use std::sync::Arc;

use nsh_core::element::input;
use nsh_core::prelude::*;
use rubiyomi_core::grade::anki::connect::{AnkiConnect, DEFAULT_URL};
use rubiyomi_core::grade::anki::{AnkiMapping, InDeck, guess_roles, inspect};
use rubiyomi_core::http;
use rubiyomi_core::jpdb::{Deck, JpdbClient};
use rubiyomi_core::platform::Platform;
use rubiyomi_core::settings::{ReviewSource, Settings};
use rubiyomi_core::wanikani::{TOKENS_URL, WaniKaniClient};

use crate::app::Shared;
use crate::platform::NshPlatform;
use crate::reviews;
use crate::ui::{self, Ripples, c, scheme};

type AnkiLists = (Vec<String>, Vec<String>);
type Inspected = Result<Vec<InDeck>, String>;
// The note type asked about, and its fields.
type Fields = (String, Result<Vec<String>, String>);
// A note type's fields, and what one of the deck's cards has in them.
type Known = Option<Result<(Vec<String>, Vec<String>), String>>;
type Job<T> = Box<dyn FnOnce() -> T + Send>;

// Whenever `key` gives something new, `load` makes a job that runs aside, and its answer lands
// in `out`; `out` is None until then. Being an effect, a new key cancels the last answer.
pub fn ask<K, T>(key: impl Fn() -> Option<K> + 'static, load: impl Fn(&K) -> Job<T> + 'static, out: Signal<Option<T>>)
where
    K: Clone + PartialEq + 'static,
    T: Send + 'static,
{
    let key = computed(key);
    effect(move || {
        let key = key.get();
        out.set(None);
        let Some(key) = key else { return };
        let job = untracked(|| load(&key));
        spawn(async move {
            match blocking(job).await {
                Ok(answer) => out.set(Some(answer)),
                Err(e) => log::warn!("asking aside: {e}"),
            }
        });
    });
}

// Where reviews come from, as Settings and the first-launch setup both ask it.
// Decks and note types load by themselves once there is enough to ask with.
#[derive(Clone)]
pub struct ReviewsForm {
    pub draft: Signal<Settings>,
    pub source: Memo<ReviewSource>,
    mapping: Memo<AnkiMapping>,
    platform: Arc<NshPlatform>,
    permission: Signal<u64>,
    jpdb_key: Signal<String>,
    decks: Signal<Option<Result<Vec<Deck>, String>>>,
    decks_tries: Signal<u32>,
    token: Signal<String>,
    signed_in: Signal<Option<Result<String, String>>>,
    anki_address: Signal<(String, String)>,
    anki: Signal<Option<Result<AnkiLists, String>>>,
    anki_tries: Signal<u32>,
    // What the chosen deck holds, looked at when it is chosen.
    inspected: Signal<Option<Inspected>>,
    known: Memo<Known>,
}

impl ReviewsForm {
    pub fn new(shared: &Shared) -> Self {
        let settings = shared.settings.get_untracked();
        let (jpdb_key, token) = (signal(settings.jpdb_key.trim().to_owned()), signal(settings.wanikani_token.trim().to_owned()));
        let anki_address = signal((settings.anki_url.trim().to_owned(), settings.anki_key.trim().to_owned()));
        let draft = signal(settings);
        let source = computed(move || draft.with(|d| d.reviews));
        let mapping = computed(move || draft.with(|d| d.anki.clone()));
        let (decks, decks_tries) = (signal_with(None, SignalEquals::Never), signal(0));
        let signed_in = signal_with(None, SignalEquals::Never);
        let (anki, anki_tries) = (signal_with(None, SignalEquals::Never), signal(0));
        let inspected = signal_with(None, SignalEquals::Never);
        let fields = signal_with(None, SignalEquals::Never);
        let permission = shared.reviews.permission;
        let platform = shared.platform.clone();

        ask(
            move || {
                let key = jpdb_key.get();
                (source.get() == ReviewSource::Jpdb && !key.is_empty()).then(|| (key, decks_tries.get()))
            },
            |(key, _)| {
                let client = JpdbClient::new(key);
                Box::new(move || client.decks().map_err(|e| e.to_string()))
            },
            decks,
        );
        ask(
            move || {
                let token = token.get();
                (source.get() == ReviewSource::WaniKani && !token.is_empty()).then_some(token)
            },
            |token| {
                let client = WaniKaniClient::new(token);
                Box::new(move || {
                    client
                        .user()
                        .map(|u| {
                            let free = if u.max_level < u.level {
                                format!(" (your plan opens levels 1\u{2013}{})", u.max_level)
                            } else {
                                String::new()
                            };
                            format!("Signed in as {}, level {}{free}.", u.username, u.level)
                        })
                        .map_err(|e| e.to_string())
                })
            },
            signed_in,
        );
        let asking = platform.clone();
        let asked = platform.clone();
        ask(
            move || {
                if source.get() != ReviewSource::Anki {
                    return None;
                }
                let granted = permission.get();
                matches!(asking.anki_droid_status(), None | Some((true, true))).then(|| (anki_address.get(), anki_tries.get(), granted))
            },
            move |_| {
                let settings = draft.get_untracked();
                let backend = reviews::anki_backend(&*asked, &settings);
                let connect = asked
                    .anki_droid_status()
                    .is_none()
                    .then(|| AnkiConnect::new(&settings.anki_url, &settings.anki_key, http::transport()));
                Box::new(move || {
                    if let Some(connect) = connect
                        && !connect.request_permission().map_err(|e| e.to_string())?
                    {
                        return Err("Anki refused the connection".to_owned());
                    }
                    Ok((backend.decks().map_err(|e| e.to_string())?, backend.note_types().map_err(|e| e.to_string())?))
                })
            },
            anki,
        );
        let deck = computed(move || mapping.with(|m| m.deck.clone()));
        let looking = platform.clone();
        ask(
            move || {
                let deck = deck.get();
                let types = anki.with(|a| a.as_ref().and_then(|a| a.as_ref().ok()).map(|l| l.1.clone()))?;
                (!deck.is_empty()).then_some((deck, types))
            },
            move |(deck, types): &(String, Vec<String>)| {
                let (backend, deck, types) = (reviews::anki_backend(&*looking, &draft.get_untracked()), deck.clone(), types.clone());
                Box::new(move || inspect(&*backend, &deck, &types).map_err(|e| e.to_string()))
            },
            inspected,
        );
        // The note type the deck's cards use is the one to grade, so it is chosen for you.
        effect(move || {
            if let Some(Ok(found)) = inspected.get() {
                draft.update(|d| pick_note_type(&mut d.anki, &found));
            }
        });
        let note_type = computed(move || mapping.with(|m| m.note_type.clone()));
        let in_deck = computed(move || {
            let note_type = note_type.get();
            inspected
                .with(|i| i.as_ref().and_then(|i| i.as_ref().ok()).map(|found| found.iter().find(|t| t.note_type == note_type).cloned()))
        });
        let reading = platform.clone();
        ask(
            move || {
                let note_type = note_type.get();
                (matches!(in_deck.get(), Some(None)) && !note_type.is_empty()).then_some(note_type)
            },
            move |note_type: &String| {
                let (backend, note_type) = (reviews::anki_backend(&*reading, &draft.get_untracked()), note_type.clone());
                Box::new(move || {
                    let fields = backend.fields(&note_type).map_err(|e| e.to_string());
                    (note_type, fields)
                })
            },
            fields,
        );
        let known = computed(move || -> Known {
            match in_deck.get()? {
                Some(t) => Some(Ok((t.fields, t.sample))),
                None => fields.with(|f: &Option<Fields>| {
                    let (_, answer) = f.as_ref().filter(|(t, _)| *t == note_type.get())?;
                    Some(answer.clone().map(|fields| (fields, Vec::new())))
                }),
            }
        });
        effect(move || {
            if let Some(Ok((fields, _))) = known.get()
                && !mapping.with(|m| fits(m, &fields))
            {
                draft.update(|d| set_roles(&mut d.anki, guess_roles(&fields)));
            }
        });
        ReviewsForm {
            draft,
            source,
            mapping,
            platform,
            permission,
            jpdb_key,
            decks,
            decks_tries,
            token,
            signed_in,
            anki_address,
            anki,
            anki_tries,
            inspected,
            known,
        }
    }

    pub fn view(&self) -> AnyElement {
        let (draft, source, form) = (self.draft, self.source, self.clone());
        view()
            .child(ui::hint("Words on the page take the colour of their card, and the word popup can add or grade them. Looking words up stays on this device."))
            .child(ui::segmented(
                None,
                &["None", "jpdb.io", "Anki", "WaniKani"],
                &[ReviewSource::None, ReviewSource::Jpdb, ReviewSource::Anki, ReviewSource::WaniKani],
                move || source.get(),
                move |v| draft.update(|d| d.reviews = v),
            ))
            .child(dynamic(move || match source.get() {
                ReviewSource::None => ui::hint("Words aren't coloured, and there are no review buttons.").into_element(),
                ReviewSource::Jpdb => form.jpdb(),
                ReviewSource::Anki => form.anki(),
                ReviewSource::WaniKani => form.wanikani(),
            }))
            .into_element()
    }

    fn wanikani(&self) -> AnyElement {
        let (draft, committed, signed_in) = (self.draft, self.token, self.signed_in);
        let start = draft.with_untracked(|d| d.wanikani_token.clone());
        view()
            .child(token_field(
                TokenField {
                    label: "API token",
                    before: "Make one on ",
                    link: "WaniKani's Personal Access Tokens page",
                    url: TOKENS_URL,
                    after: ", with reviews:create ticked so the word card can send reviews. It is kept in the system's keyring, never in Rubiyomi's files.",
                    clear: "Clear token",
                },
                self.platform.clone(),
                start,
                move |v| draft.update(|d| d.wanikani_token = v),
                move |v| committed.set(v),
            ))
            .child(dynamic(move || match signed_in.get() {
                _ if committed.get().is_empty() => view().into_element(),
                None => ui::hint("Asking WaniKani\u{2026}").into_element(),
                Some(Ok(who)) => ui::hint(who).into_element(),
                Some(Err(e)) => ui::error(e).into_element(),
            }))
            .child(ui::hint("Words WaniKani teaches take the colour of their stage. A word can be reviewed from its card when it's due; nothing is queued while offline."))
            .into_element()
    }

    fn jpdb(&self) -> AnyElement {
        let (draft, committed, decks, tries) = (self.draft, self.jpdb_key, self.decks, self.decks_tries);
        let start = draft.with_untracked(|d| d.jpdb_key.clone());
        view()
            .child(jpdb_key_field(self.platform.clone(), start, move |v| draft.update(|d| d.jpdb_key = v), move |v| committed.set(v)))
            .child(ui::label("New words go into"))
            .child(dynamic(move || {
                let (id, name) = draft.with(|d| (d.jpdb_deck.clone(), d.jpdb_deck_name.clone()));
                let current = if id.is_empty() { "No deck chosen".to_owned() } else { name };
                match decks.get() {
                    _ if committed.get().is_empty() => ui::hint("Your decks show here once the key is in.").into_element(),
                    None => ui::hint("Asking jpdb.io\u{2026}").into_element(),
                    Some(Ok(decks)) => {
                        let options: Vec<(String, String)> = decks.iter().map(|d| (d.id.clone(), d.name.clone())).collect();
                        ui::choice(current, options, move |chosen| {
                            if let Some(deck) = decks.iter().find(|d| d.id == chosen) {
                                draft.update(|d| {
                                    d.jpdb_deck = deck.id.clone();
                                    d.jpdb_deck_name = deck.name.clone();
                                });
                            }
                        })
                    }
                    Some(Err(e)) => ui::retry(e, tries),
                }
            }))
            .child(ui::hint("Your decks are synced when the app starts, so pages are coloured offline too. Grades and new cards go to jpdb.io straight away; nothing is queued while offline."))
            .into_element()
    }

    fn anki(&self) -> AnyElement {
        let form = self.clone();
        dynamic(move || {
            form.permission.get();
            match form.platform.anki_droid_status() {
                Some((false, _)) => {
                    let platform = form.platform.clone();
                    view()
                        .child(ui::error("AnkiDroid isn't installed."))
                        .child(ui::button_row("Get AnkiDroid", move || {
                            platform.open_url("https://play.google.com/store/apps/details?id=com.ichi2.anki");
                        }))
                        .into_element()
                }
                Some((true, false)) => {
                    let platform = form.platform.clone();
                    view()
                        .child(ui::hint("Rubiyomi needs AnkiDroid's permission to read and add cards."))
                        .child(
                            view()
                                .px_4()
                                .py_2()
                                .child(ui::primary_button("Allow access to AnkiDroid", move || platform.request_anki_permission())),
                        )
                        .into_element()
                }
                Some((true, true)) => view().child(ui::hint("Connected to AnkiDroid.")).child(form.anki_choices()).into_element(),
                None => view()
                    .child(ui::hint("Desktop Anki, through the AnkiConnect add-on. Keep Anki open while you read."))
                    .child(form.anki_address())
                    .child(form.anki_choices())
                    .into_element(),
            }
        })
    }

    fn anki_address(&self) -> AnyElement {
        let (draft, address) = (self.draft, self.anki_address);
        let commit = move || address.set(draft.with_untracked(|d| (d.anki_url.trim().to_owned(), d.anki_key.trim().to_owned())));
        let (url, key) = draft.with_untracked(|d| (d.anki_url.clone(), d.anki_key.clone()));
        view()
            .child(ui::label("Address"))
            .child(plain_field(DEFAULT_URL, false, url, move |v| draft.update(|d| d.anki_url = v), commit))
            .child(ui::label("Key"))
            .child(plain_field("Only if AnkiConnect asks for one", true, key, move |v| draft.update(|d| d.anki_key = v), commit))
            .into_element()
    }

    fn anki_choices(&self) -> AnyElement {
        let form = self.clone();
        dynamic(move || {
            let (draft, tries) = (form.draft, form.anki_tries);
            let answer = form.anki.get();
            let saved = draft.with_untracked(|d| (vec![d.anki.deck.clone()], vec![d.anki.note_type.clone()]));
            let (top, lists) = match answer {
                Some(Ok(lists)) => (view().into_element(), lists),
                None => (ui::hint("Asking Anki\u{2026}").into_element(), saved),
                Some(Err(e)) => (ui::retry(e, tries), saved),
            };
            let connected = form.anki.with_untracked(|a| matches!(a, Some(Ok(_))));
            if !connected && draft.with_untracked(|d| d.anki.deck.is_empty()) {
                return top;
            }
            let (decks, types) = (labelled(&lists.0), lists.1);
            let inner = form.clone();
            view()
                .child(top)
                .child(ui::label("Deck"))
                .child(ui::hint("Where your Japanese cards are. Words you add from the reader go here too."))
                .child(dynamic(move || {
                    let deck = inner.mapping.with(|m| m.deck.clone());
                    let (draft, decks, types) = (inner.draft, decks.clone(), types.clone());
                    let shown = if deck.is_empty() { "Choose a deck\u{2026}".to_owned() } else { deck.clone() };
                    view()
                        .child(ui::choice(shown, decks, move |chosen| draft.update(|d| d.anki.deck = chosen)))
                        .child((!deck.is_empty()).then(|| inner.anki_note_type(deck, types)))
                        .into_element()
                }))
                .into_element()
        })
    }

    fn anki_note_type(&self, deck: String, types: Vec<String>) -> AnyElement {
        let (draft, mapping, inspected, known) = (self.draft, self.mapping, self.inspected, self.known);
        let looking = deck.clone();
        view()
            .child(dynamic(move || match inspected.get() {
                None => ui::hint(format!("Looking in {looking}\u{2026}")).into_element(),
                Some(Err(e)) => ui::error(e).into_element(),
                Some(Ok(_)) => view().into_element(),
            }))
            .child(ui::label("Note type"))
            .child(dynamic(move || {
                let found = inspected.with(|i| i.as_ref().and_then(|i| i.as_ref().ok()).cloned());
                let note_type = mapping.with(|m| m.note_type.clone());
                let labels: Vec<(String, String)> =
                    types.iter().map(|t| (t.clone(), type_label(t, found.as_deref().unwrap_or_default()))).collect();
                let shown = labels.iter().find(|(t, _)| *t == note_type).map_or_else(
                    || if note_type.is_empty() { "Choose a note type\u{2026}".to_owned() } else { note_type.clone() },
                    |(_, label)| label.clone(),
                );
                view()
                    .child(found.as_ref().map(|found| ui::hint(cards_said(found))))
                    .child(ui::choice(shown, labels, move |chosen| draft.update(|d| d.anki.note_type = chosen)))
                    .into_element()
            }))
            .child(dynamic(move || {
                if mapping.with(|m| m.note_type.is_empty()) {
                    return view().into_element();
                }
                match known.get() {
                    None => ui::hint("Asking Anki\u{2026}").into_element(),
                    Some(Err(e)) => ui::error(e).into_element(),
                    Some(Ok((fields, sample))) => field_rows(draft, mapping.get(), &fields, &sample),
                }
            }))
            .into_element()
    }
}

fn labelled(options: &[String]) -> Vec<(String, String)> {
    options.iter().filter(|o| !o.is_empty()).map(|o| (o.clone(), o.clone())).collect()
}

fn type_label(note_type: &str, found: &[InDeck]) -> String {
    match found.iter().find(|f| f.note_type == note_type) {
        Some(f) => format!("{note_type}  \u{b7}  {} cards here", f.notes),
        None => note_type.to_owned(),
    }
}

fn cards_said(found: &[InDeck]) -> String {
    let cards: usize = found.iter().map(|t| t.notes).sum();
    match (cards, found.len()) {
        (0, _) => "This deck has no cards yet: choose the note type for the cards Rubiyomi adds.".to_owned(),
        (n, 1) => format!("All {n} cards in this deck are of this note type."),
        (n, _) => format!("{n} cards in this deck, of {} note types: the most used one is chosen.", found.len()),
    }
}

const ROLES: [&str; 4] = ["Word", "Reading", "Meaning", "Sentence"];

fn roles_of(m: &mut AnkiMapping) -> [&mut String; 4] {
    [&mut m.word_field, &mut m.reading_field, &mut m.meaning_field, &mut m.sentence_field]
}

fn set_roles(m: &mut AnkiMapping, roles: [String; 4]) {
    for (slot, value) in roles_of(m).into_iter().zip(roles) {
        *slot = value;
    }
}

fn role_of(m: &AnkiMapping, field: &str) -> Option<usize> {
    [&m.word_field, &m.reading_field, &m.meaning_field, &m.sentence_field].iter().position(|r| r.as_str() == field)
}

fn set_role(m: &mut AnkiMapping, field: &str, role: Option<usize>) {
    if let Some(old) = role_of(m, field) {
        roles_of(m)[old].clear();
    }
    if let Some(role) = role {
        *roles_of(m)[role] = field.to_owned();
    }
}

// Every role either unused or on a field the note type has, and the word placed.
fn is_mapped(m: &AnkiMapping, fields: &[String]) -> bool {
    !m.word_field.is_empty() && fits(m, fields)
}

// The roles name only fields the note type has, and at least one is in use.
fn fits(m: &AnkiMapping, fields: &[String]) -> bool {
    let all = [&m.word_field, &m.reading_field, &m.meaning_field, &m.sentence_field];
    all.iter().any(|f| !f.is_empty()) && all.iter().all(|f| f.is_empty() || fields.contains(f))
}

fn pick_note_type(m: &mut AnkiMapping, found: &[InDeck]) {
    if let Some(top) = found.first()
        && !found.iter().any(|t| t.note_type == m.note_type && is_mapped(m, &t.fields))
    {
        m.note_type = top.note_type.clone();
        set_roles(m, guess_roles(&top.fields));
    }
}

fn field_rows(draft: Signal<Settings>, m: AnkiMapping, fields: &[String], sample: &[String]) -> AnyElement {
    let mut rows = view().mx_4().my_2().p_3().gap(px(12.0)).rounded(px(12.0)).bg(move || c(scheme().surface_container));
    for (k, field) in fields.iter().enumerate() {
        rows = rows.child(field_row(draft, &m, field.clone(), sample.get(k).map(String::as_str)));
    }
    view()
        .child(ui::label("What goes in each field"))
        .child(ui::hint("As a card for 読み would be filled. Change what a field holds and the preview follows."))
        .child(rows)
        .child(m.word_field.is_empty().then(|| ui::error("Choose the field that holds the word.")))
        .child(ui::hint("New cards get the tag rubiyomi. Grades go to Anki straight away."))
        .into_element()
}

fn field_row(draft: Signal<Settings>, m: &AnkiMapping, field: String, sample: Option<&str>) -> AnyElement {
    let role = role_of(m, &field);
    let mut options: Vec<(String, String)> = ROLES.iter().map(|r| ((*r).to_owned(), (*r).to_owned())).collect();
    options.push(("Nothing".to_owned(), "Nothing".to_owned()));
    let shown = role.map_or("Nothing", |r| ROLES[r]).to_owned();
    let name = field.clone();
    let ink = move || c(scheme().on_surface);
    let preview = match role {
        Some(3) => text(String::new())
            .text_size(px(16.0))
            .text_color(ink)
            .child(text("毎晩、本を"))
            .child(text("読み").text_color(move || c(scheme().primary)).underline())
            .child(text("ながら眠った。"))
            .into_element(),
        Some(r) => text(["読み", "よみ", "reading \u{b7} pronunciation (of a kanji)"][r])
            .text_size(px(if r == 0 { 22.0 } else { 16.0 }))
            .text_color(ink)
            .into_element(),
        None => text("Left empty").text_size(px(13.0)).text_color(move || c(scheme().on_surface_variant)).into_element(),
    };
    let on_card = sample.filter(|t| !t.trim().is_empty()).map(|sample| {
        let shown: String = sample.chars().take(60).collect();
        let more = if sample.chars().count() > 60 { "\u{2026}" } else { "" };
        text(format!("On one of your cards: {shown}{more}")).text_size(px(12.0)).text_color(move || c(scheme().on_surface_variant))
    });
    view()
        .gap(px(4.0))
        .child(text(field).text_size(px(16.0)).text_color(ink))
        .child(view().mx(px(-16.0)).child(ui::choice(shown, options, move |chosen| {
            let role = ROLES.iter().position(|r| *r == chosen);
            draft.update(|d| set_role(&mut d.anki, &name, role));
        })))
        .child(preview)
        .child(on_card)
        .into_element()
}

// A text field: `typing` hears each change, `commit` the text when the field is left.
fn plain_field(
    placeholder: &'static str,
    masked: bool,
    start: String,
    typing: impl Fn(String) + 'static,
    commit: impl Fn() + Clone + 'static,
) -> AnyElement {
    let holder = element_ref();
    holder.on_focus_out(commit.clone());
    ui::field_box(
        input()
            .element_ref(&holder)
            .placeholder(placeholder)
            .masked(masked)
            .auto_correct(false)
            .auto_capitalize(AutoCapitalize::None)
            .default_value(start)
            .text_size(px(16.0))
            .text_color(move || c(scheme().on_surface))
            .on_change(move |v: &str| typing(v.to_owned()))
            .on_submit(move |_: &str| commit()),
    )
    .into_element()
}

// What a service's key field says: its name, and where the key is found.
pub struct TokenField {
    pub label: &'static str,
    pub before: &'static str,
    pub link: &'static str,
    pub url: &'static str,
    pub after: &'static str,
    pub clear: &'static str,
}

// A key pasted from a service's site. `typing` hears each change; `commit` hears the key, trimmed,
// when the field is left or cleared, so a half-typed key isn't tried. Leaving the page isn't always
// heard, so a page that saves as it goes keeps what `typing` heard last.
pub fn token_field(
    t: TokenField,
    platform: Arc<NshPlatform>,
    start: String,
    typing: impl Fn(String) + 'static,
    commit: impl Fn(String) + 'static,
) -> AnyElement {
    let typed = signal(start);
    let (typing, commit) = (Rc::new(typing), Rc::new(commit));
    let holder = element_ref();
    let (left, submitted) = (commit.clone(), commit.clone());
    holder.on_focus_out(move || left(typed.get_untracked().trim().to_owned()));
    let changed = typing.clone();
    // Pasting or clearing sets the field and settles it at once.
    let replace = Rc::new(move |key: String| {
        typed.set(key.clone());
        typing(key.clone());
        commit(key);
    });
    let (pasting, clearing) = (replace.clone(), replace);
    let url = t.url;
    view()
        .child(ui::label(t.label))
        .child(
            view()
                .px_4()
                .py_1()
                .ripples()
                .on_press(move || {
                    if !platform.open_url(url) {
                        log::warn!("couldn't open {url}");
                    }
                })
                .child(
                    text(String::new())
                        .text_size(px(13.0))
                        .text_color(move || c(scheme().on_surface_variant))
                        .child(text(t.before))
                        .child(text(t.link).text_color(move || c(scheme().primary)).underline())
                        .child(text(t.after)),
                ),
        )
        .child(
            view()
                .flex_row()
                .items_center()
                .child(
                    view().flex_1().child(ui::field_box(
                        input()
                            .element_ref(&holder)
                            .placeholder(t.label)
                            .masked(true)
                            .auto_correct(false)
                            .auto_capitalize(AutoCapitalize::None)
                            .value(move || typed.get())
                            .text_size(px(16.0))
                            .text_color(move || c(scheme().on_surface))
                            .on_change(move |v: &str| {
                                typed.set(v.to_owned());
                                changed(v.to_owned());
                            })
                            .on_submit(move |v: &str| submitted(v.trim().to_owned())),
                    )),
                )
                .child(cfg!(target_os = "android").then(|| {
                    view()
                        .w(px(48.0))
                        .h(px(48.0))
                        .items_center()
                        .justify_center()
                        .label("Paste")
                        .ripples()
                        .on_press(move || {
                            let pasting = pasting.clone();
                            spawn(async move {
                                let key = clipboard_string().await;
                                let key = key.trim().to_owned();
                                if !key.is_empty() {
                                    pasting(key);
                                }
                            });
                        })
                        .child(ui::icon(ui::glyph::CONTENT_PASTE, 24.0, move || c(scheme().on_surface_variant)))
                })),
        )
        .child(dynamic(move || {
            let clearing = clearing.clone();
            if typed.with(|k| k.is_empty()) {
                return view().into_element();
            }
            ui::button_row(t.clear, move || clearing(String::new()))
        }))
        .into_element()
}

// The jpdb.io key, with where to find it.
pub fn jpdb_key_field(
    platform: Arc<NshPlatform>,
    start: String,
    typing: impl Fn(String) + 'static,
    commit: impl Fn(String) + 'static,
) -> AnyElement {
    token_field(
        TokenField {
            label: "API key",
            before: "Copy it from the bottom of ",
            link: "jpdb.io's settings page",
            url: "https://jpdb.io/settings",
            after: ". It is kept in the system's keyring, never in Rubiyomi's files.",
            clear: "Clear key",
        },
        platform,
        start,
        typing,
        commit,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(note_type: &str, fields: &[&str], notes: usize) -> InDeck {
        InDeck { note_type: note_type.into(), notes, fields: fields.iter().map(|f| (*f).to_owned()).collect(), sample: Vec::new() }
    }

    #[test]
    fn the_decks_note_type_is_chosen_unless_the_mapping_already_fits_one_of_them() {
        let deck = [found("Mining", &["Expression", "Meaning"], 9), found("Basic", &["Front", "Back"], 1)];

        let mut m = AnkiMapping { note_type: "Other".into(), word_field: "Word".into(), ..AnkiMapping::default() };
        pick_note_type(&mut m, &deck);
        assert_eq!(m.note_type, "Mining");
        assert!(!m.word_field.is_empty());

        let mut kept = AnkiMapping { note_type: "Basic".into(), word_field: "Back".into(), ..AnkiMapping::default() };
        pick_note_type(&mut kept, &deck);
        assert_eq!((kept.note_type.as_str(), kept.word_field.as_str()), ("Basic", "Back"));
    }

    #[test]
    fn a_field_holds_one_role_and_a_role_one_field() {
        let mut m = AnkiMapping { word_field: "A".into(), reading_field: "B".into(), ..AnkiMapping::default() };
        set_role(&mut m, "A", Some(1));
        assert_eq!((m.word_field.as_str(), m.reading_field.as_str()), ("", "A"));
        set_role(&mut m, "A", None);
        assert_eq!(m.reading_field, "");
    }
}
