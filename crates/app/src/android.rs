// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

// The Java side of Android: dev.calonca.rubiyomi.Bridge, called with JNI.

use std::sync::OnceLock;

use nsh::jni::objects::{GlobalRef, JClass, JFloatArray, JObject, JString};
use nsh::jni::sys::jlong;
use nsh::jni::{JNIEnv, JavaVM, errors};
use rubiyomi_core::grade::GradeError;
use rubiyomi_core::grade::anki::{AnkiBackend, Note};
use serde_json::{Value, json};

use crate::App;

nsh::ui_sets!(vec![nsh_base::registry(), crate::__nsh_native_module()]);

struct Bridge {
    vm: JavaVM,
    class: GlobalRef,
}

static BRIDGE: OnceLock<Bridge> = OnceLock::new();

fn capture(env: &mut JNIEnv<'_>) -> errors::Result<Bridge> {
    let class = env.find_class("dev/calonca/rubiyomi/Bridge")?;
    Ok(Bridge { vm: env.get_java_vm()?, class: env.new_global_ref(class)? })
}

// `nsh::main!`, and the Bridge class looked up here: on any other thread than Java's, FindClass
// sees only the system's classes.
#[unsafe(no_mangle)]
extern "system" fn Java_com_longbridge_nsh_Native_runRust(mut env: JNIEnv<'_>, _: JClass<'_>, handle: jlong) {
    if BRIDGE.get().is_none() {
        match capture(&mut env) {
            Ok(bridge) => {
                let _ = BRIDGE.set(bridge);
            }
            Err(e) => {
                let _ = env.exception_clear();
                log::error!("the Java bridge isn't there: {e}");
            }
        }
    }
    nsh::run_root(handle, nsh_core::element::Root::of::<App>().app_id("dev.calonca.rubiyomi"));
}

fn with_bridge<T>(what: &str, f: impl FnOnce(&mut JNIEnv<'_>, &JClass<'_>) -> errors::Result<T>) -> Option<T> {
    let bridge = BRIDGE.get()?;
    let mut env = bridge.vm.attach_current_thread().ok()?;
    let class: &JClass<'_> = bridge.class.as_obj().into();
    match env.with_local_frame(16, |env| f(env, class)) {
        Ok(v) => Some(v),
        Err(e) => {
            let _ = env.exception_clear();
            log::warn!("{what}: {e}");
            None
        }
    }
}

fn optional_string(env: &mut JNIEnv<'_>, v: JObject<'_>) -> errors::Result<Option<String>> {
    if v.is_null() {
        return Ok(None);
    }
    Ok(Some(env.get_string(&JString::from(v))?.into()))
}

// Java returns null when there is no such key and throws when it can't read one.
pub fn secret(name: &str) -> Result<Option<String>, String> {
    with_bridge("secret", |env, class| {
        let name = env.new_string(name)?;
        let v = env.call_static_method(class, "secret", "(Ljava/lang/String;)Ljava/lang/String;", &[(&name).into()])?.l()?;
        optional_string(env, v)
    })
    .ok_or_else(|| "the Android Keystore didn't answer".to_owned())
}

pub fn set_secret(name: &str, value: &str) -> Result<(), String> {
    let stored = with_bridge("setSecret", |env, class| {
        let (name, value) = (env.new_string(name)?, env.new_string(value)?);
        env.call_static_method(class, "setSecret", "(Ljava/lang/String;Ljava/lang/String;)Z", &[(&name).into(), (&value).into()])?.z()
    });
    if stored == Some(true) { Ok(()) } else { Err("the Android Keystore refused it".into()) }
}

pub fn keep_screen_on(on: bool) {
    with_bridge("keepScreenOn", |env, class| env.call_static_method(class, "keepScreenOn", "(Z)V", &[on.into()]).map(|_| ()));
}

pub fn accent_color() -> Option<u32> {
    let colour = with_bridge("accentColor", |env, class| env.call_static_method(class, "accentColor", "()I", &[])?.i())?;
    (colour >= 0).then_some(colour as u32)
}

fn floats<const N: usize>(method: &str) -> [f32; N] {
    let read = with_bridge(method, |env, class| {
        let values = env.call_static_method(class, method, "()[F", &[])?.l()?;
        let values = JFloatArray::from(values);
        let mut out = [0.0; N];
        env.get_float_array_region(&values, 0, &mut out)?;
        Ok(out)
    });
    read.unwrap_or([0.0; N])
}

pub fn cutout() -> [f32; 4] {
    floats("cutout")
}

// What the pages have: the window, less the cutout.
pub fn page_area() -> Option<(f32, f32)> {
    let ([width, height], [left, top, right, bottom]) = (floats::<2>("windowSize"), cutout());
    (width > 0.0).then_some((width - left - right, height - top - bottom))
}

pub fn full_screen(on: bool) {
    with_bridge("fullScreen", |env, class| env.call_static_method(class, "fullScreen", "(Z)V", &[on.into()]).map(|_| ()));
}

pub fn open_url(url: &str) -> bool {
    with_bridge("openUrl", |env, class| {
        let url = env.new_string(url)?;
        env.call_static_method(class, "openUrl", "(Ljava/lang/String;)Z", &[(&url).into()])?.z()
    })
    .unwrap_or(false)
}

pub fn request_anki_permission() {
    with_bridge("requestAnkiPermission", |env, class| env.call_static_method(class, "requestAnkiPermission", "()V", &[]).map(|_| ()));
}

pub fn permission_answer() -> Option<bool> {
    match with_bridge("takePermissionAnswer", |env, class| env.call_static_method(class, "takePermissionAnswer", "()I", &[])?.i()) {
        Some(1) => Some(true),
        Some(2) => Some(false),
        _ => None,
    }
}

fn anki_call(method: &str, args: &Value) -> Result<Value, GradeError> {
    let answer = with_bridge("ankiCall", |env, class| {
        let (m, j) = (env.new_string(method)?, env.new_string(args.to_string())?);
        let v = env
            .call_static_method(class, "ankiCall", "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;", &[(&m).into(), (&j).into()])?
            .l()?;
        Ok(String::from(env.get_string(&JString::from(v))?))
    })
    .ok_or_else(|| GradeError::Other("AnkiDroid couldn't be asked".into()))?;
    let json: Value = serde_json::from_str(&answer).map_err(|_| GradeError::Other("AnkiDroid's answer wasn't JSON".into()))?;
    match json.get("error").and_then(Value::as_str) {
        Some("permission") => Err(GradeError::PermissionDenied),
        Some("missing") => Err(GradeError::NotFound("AnkiDroid isn't installed".into())),
        Some(_) => Err(GradeError::Other(format!("AnkiDroid: {}", json.get("message").and_then(Value::as_str).unwrap_or("failed")))),
        None => Ok(json.get("ok").cloned().unwrap_or(Value::Null)),
    }
}

pub fn anki_status() -> Option<(bool, bool)> {
    let s = anki_call("status", &json!({})).ok()?;
    Some((s["installed"].as_bool().unwrap_or(false), s["permission"].as_bool().unwrap_or(false)))
}

pub struct AnkiDroid;

fn strings(v: &Value) -> Vec<String> {
    v.as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_owned)).collect()).unwrap_or_default()
}

impl AnkiBackend for AnkiDroid {
    fn decks(&self) -> Result<Vec<String>, GradeError> {
        Ok(strings(&anki_call("decks", &json!({}))?))
    }

    fn note_types(&self) -> Result<Vec<String>, GradeError> {
        Ok(strings(&anki_call("noteTypes", &json!({}))?))
    }

    fn fields(&self, note_type: &str) -> Result<Vec<String>, GradeError> {
        Ok(strings(&anki_call("fields", &json!({ "noteType": note_type }))?))
    }

    fn find_notes(&self, query: &str) -> Result<Vec<u64>, GradeError> {
        let v = anki_call("findNotes", &json!({ "query": query }))?;
        Ok(v.as_array().map(|a| a.iter().filter_map(Value::as_u64).collect()).unwrap_or_default())
    }

    fn notes(&self, ids: &[u64]) -> Result<Vec<Note>, GradeError> {
        let v = anki_call("notes", &json!({ "ids": ids }))?;
        Ok(v.as_array().into_iter().flatten().filter_map(|n| Some(Note { id: n["id"].as_u64()?, fields: strings(&n["fields"]) })).collect())
    }

    fn suspend(&self, note: u64) -> Result<(), GradeError> {
        anki_call("suspend", &json!({ "note": note }))?;
        Ok(())
    }

    fn answer(&self, note: u64, ease: u8) -> Result<(), GradeError> {
        let changed = anki_call("answer", &json!({ "note": note, "ease": ease }))?;
        // AnkiDroid ignores a grade for a card that isn't due.
        if changed.as_u64() == Some(0) {
            return Err(GradeError::Other("AnkiDroid didn't take the grade (is the card due?)".into()));
        }
        Ok(())
    }

    fn add_note(&self, deck: &str, note_type: &str, fields: &[(String, String)], tags: &[&str]) -> Result<u64, GradeError> {
        let map: serde_json::Map<String, Value> = fields.iter().map(|(k, v)| (k.clone(), v.clone().into())).collect();
        let v = anki_call("addNote", &json!({ "deck": deck, "noteType": note_type, "fields": map, "tags": tags.join(" ") }))?;
        v.as_u64().ok_or_else(|| GradeError::Other("AnkiDroid didn't add the note".into()))
    }
}
