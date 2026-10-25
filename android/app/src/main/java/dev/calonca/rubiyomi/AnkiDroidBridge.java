// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

package dev.calonca.rubiyomi;

import android.app.Activity;
import android.content.ContentResolver;
import android.content.ContentValues;
import android.content.pm.PackageManager;
import android.database.Cursor;
import android.net.Uri;

import org.json.JSONArray;
import org.json.JSONObject;

final class AnkiDroidBridge {
    static final String PACKAGE = "com.ichi2.anki";
    static final String PERMISSION = "com.ichi2.anki.permission.READ_WRITE_DATABASE";
    private static final Uri BASE = Uri.parse("content://com.ichi2.anki.flashcards");
    private static final Uri NOTES = Uri.withAppendedPath(BASE, "notes");
    private static final Uri MODELS = Uri.withAppendedPath(BASE, "models");
    private static final Uri DECKS = Uri.withAppendedPath(BASE, "decks");
    private static final Uri SCHEDULE = Uri.withAppendedPath(BASE, "schedule");
    private static final String SEP = "\u001f";

    private final Activity activity;

    AnkiDroidBridge(Activity activity) {
        this.activity = activity;
    }

    boolean installed() {
        try {
            activity.getPackageManager().getPackageInfo(PACKAGE, 0);
            return true;
        } catch (PackageManager.NameNotFoundException e) {
            return false;
        }
    }

    boolean permitted() {
        return activity.checkSelfPermission(PERMISSION) == PackageManager.PERMISSION_GRANTED;
    }

    String call(String method, String argsJson) {
        try {
            JSONObject args = new JSONObject(argsJson);
            Object result;
            switch (method) {
                case "status":
                    result = new JSONObject().put("installed", installed()).put("permission", permitted());
                    break;
                case "decks":
                    result = names(DECKS, "deck_name");
                    break;
                case "noteTypes":
                    result = names(MODELS, "name");
                    break;
                case "fields":
                    result = fields(args.getString("noteType"));
                    break;
                case "findNotes":
                    result = findNotes(args.getString("query"));
                    break;
                case "notes":
                    result = notes(args.getJSONArray("ids"));
                    break;
                case "answer":
                    result = answer(args.getLong("note"), args.getInt("ease"));
                    break;
                case "suspend":
                    result = suspend(args.getLong("note"));
                    break;
                case "addNote":
                    result = addNote(args);
                    break;
                default:
                    throw new IllegalArgumentException("unknown call " + method);
            }
            return new JSONObject().put("ok", result).toString();
        } catch (SecurityException e) {
            return error("permission", String.valueOf(e.getMessage()));
        } catch (Exception e) {
            return error(installed() ? "other" : "missing", e.toString());
        }
    }

    private static String error(String kind, String message) {
        try {
            return new JSONObject().put("error", kind).put("message", message).toString();
        } catch (Exception e) {
            return "{\"error\":\"other\"}";
        }
    }

    private ContentResolver resolver() {
        return activity.getContentResolver();
    }

    private JSONArray names(Uri uri, String column) {
        JSONArray out = new JSONArray();
        try (Cursor c = resolver().query(uri, new String[] {column}, null, null, null)) {
            while (c != null && c.moveToNext()) {
                out.put(c.getString(0));
            }
        }
        return out;
    }

    private Object[] model(String name) {
        try (Cursor c = resolver().query(MODELS, new String[] {"_id", "name", "field_names"}, null, null, null)) {
            while (c != null && c.moveToNext()) {
                if (name.equals(c.getString(1))) {
                    return new Object[] {c.getLong(0), c.getString(2).split(SEP, -1)};
                }
            }
        }
        throw new IllegalArgumentException("no note type " + name);
    }

    private JSONArray fields(String noteType) {
        JSONArray out = new JSONArray();
        for (String f : (String[]) model(noteType)[1]) {
            out.put(f);
        }
        return out;
    }

    private JSONArray findNotes(String query) {
        JSONArray out = new JSONArray();
        try (Cursor c = resolver().query(NOTES, new String[] {"_id"}, query, null, null)) {
            while (c != null && c.moveToNext()) {
                out.put(c.getLong(0));
            }
        }
        return out;
    }

    private JSONArray notes(JSONArray ids) throws Exception {
        JSONArray out = new JSONArray();
        if (ids.length() == 0) {
            return out;
        }
        StringBuilder query = new StringBuilder("nid:");
        for (int i = 0; i < ids.length(); i++) {
            if (i > 0) {
                query.append(',');
            }
            query.append(ids.getLong(i));
        }
        try (Cursor c = resolver().query(NOTES, new String[] {"_id", "flds"}, query.toString(), null, null)) {
            while (c != null && c.moveToNext()) {
                JSONArray fields = new JSONArray();
                for (String f : c.getString(1).split(SEP, -1)) {
                    fields.put(f);
                }
                out.put(new JSONObject().put("id", c.getLong(0)).put("fields", fields));
            }
        }
        return out;
    }

    private int answer(long note, int ease) {
        ContentValues v = new ContentValues();
        v.put("note_id", note);
        v.put("ord", 0);
        v.put("answer_ease", ease);
        v.put("time_taken", 5000L);
        return resolver().update(SCHEDULE, v, null, null);
    }

    private int suspend(long note) {
        ContentValues v = new ContentValues();
        v.put("note_id", note);
        v.put("ord", 0);
        v.put("suspended", 1);
        return resolver().update(SCHEDULE, v, null, null);
    }

    private long deckId(String name) {
        try (Cursor c = resolver().query(DECKS, new String[] {"deck_id", "deck_name"}, null, null, null)) {
            while (c != null && c.moveToNext()) {
                if (name.equals(c.getString(1))) {
                    return c.getLong(0);
                }
            }
        }
        throw new IllegalArgumentException("no deck " + name);
    }

    private long addNote(JSONObject args) throws Exception {
        Object[] model = model(args.getString("noteType"));
        String[] names = (String[]) model[1];
        JSONObject given = args.getJSONObject("fields");
        StringBuilder flds = new StringBuilder();
        for (int i = 0; i < names.length; i++) {
            if (i > 0) {
                flds.append(SEP);
            }
            flds.append(given.optString(names[i], ""));
        }
        ContentValues v = new ContentValues();
        v.put("mid", (Long) model[0]);
        v.put("flds", flds.toString());
        v.put("tags", args.optString("tags", ""));
        Uri note = resolver().insert(NOTES, v);
        if (note == null) {
            throw new IllegalStateException("AnkiDroid didn't add the note");
        }
        long id = Long.parseLong(note.getLastPathSegment());
        long deck = deckId(args.getString("deck"));
        Uri cards = Uri.withAppendedPath(note, "cards");
        try (Cursor c = resolver().query(cards, new String[] {"ord"}, null, null, null)) {
            while (c != null && c.moveToNext()) {
                ContentValues d = new ContentValues();
                d.put("deck_id", deck);
                resolver().update(Uri.withAppendedPath(cards, String.valueOf(c.getInt(0))), d, null, null);
            }
        }
        return id;
    }
}
