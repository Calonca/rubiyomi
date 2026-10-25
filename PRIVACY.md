# Rubiyomi privacy policy

Last updated: 25 October 2026.

Rubiyomi collects no data. It has no account, no analytics, no ads and no
crash reporting, and its author receives nothing from it.

## What stays on your device

The books you add, your reading position, the words you looked up, your
reading stats and your settings are stored in the app's private storage on
your device. Uninstalling the app deletes them. If you have Android's backup
turned on, it may include them in your own Google backup; your keys are kept
out of it.

Your jpdb.io API key and AnkiConnect key are stored encrypted, with a key
held in the Android Keystore (on Linux, in the desktop's secret service).

## What leaves your device, and only if you set it up

- **jpdb.io.** If you choose jpdb.io for reviews, Rubiyomi sends your API key
  and the words of the chapter you are reading to jpdb.io, to colour them by
  your cards, and the word and its sentence when you grade or add a card.
  jpdb.io's own privacy policy applies to what it receives.
- **jpdb.io parsing.** If you choose jpdb.io under Settings → Parsing,
  Rubiyomi sends your API key and the text of each chapter you open to
  jpdb.io, once per chapter, to find its words.
- **Anki.** If you choose Anki, Rubiyomi reads and writes your cards through
  AnkiDroid on the same phone (with the permission you grant), or through
  AnkiConnect at the address you enter on a computer. Nothing goes to a server
  unless that address is one.
- **Links.** "Open on jpdb.io" and other links open in your browser.

The dictionary is bundled with the app, so unless you choose jpdb.io for
parsing, looking words up never uses the internet.

## Permissions

- *Internet*: only for jpdb.io or AnkiConnect, as above.
- *AnkiDroid database*: only if you choose Anki on the phone.

## Contact

Questions: open an issue at https://github.com/Calonca/rubiyomi/issues.
