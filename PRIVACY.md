# Rubiyomi privacy policy

Last updated: 28 October 2026.

Rubiyomi collects no data. It has no account, no analytics, no ads and no
crash reporting, and its author receives nothing from it.

## What stays on your device

The books you add, your reading position, the words you looked up, your
reading stats and your settings are stored in the app's private storage on
your device. Uninstalling the app deletes them. If you have Android's backup
turned on, it may include them in your own Google backup; your keys are kept
out of it.

Your jpdb.io API key and AnkiConnect key are stored encrypted, with a key
held in the Android Keystore (on Linux, in the desktop's secret service; on
Windows, in the Credential Manager).

## The update check

The desktop downloads from GitHub (the Linux AppImage and the Windows .exe)
look for a newer version when they start. This is the one call Rubiyomi makes
on its own:

- **What it does.** It asks GitHub's public API for the latest Rubiyomi
  release (`api.github.com/repos/Calonca/rubiyomi/releases/latest`).
- **What it sends.** Nothing but a normal request: your IP address, as with
  any connection, and `Rubiyomi/<version>` as the user agent. No identifier,
  nothing about your books, words, stats or settings. GitHub's privacy
  statement applies to the request.
- **Updating.** Only when you choose Update does the app download the new
  version from GitHub, and check it against its published SHA-256.
- **Turning it off.** Settings → Updates → "Check for updates when Rubiyomi
  starts". The "Check for updates" button there still works when you want it.
- **Not on Android.** The Android app has no update check: the Play Store
  updates it.

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

Settings → Privacy policy opens this page.

## Permissions

- *Internet*: only for jpdb.io or AnkiConnect, as above.
- *AnkiDroid database*: only if you choose Anki on the phone.

## Contact

Questions: open an issue at https://github.com/Calonca/rubiyomi/issues.
