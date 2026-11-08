# Rubiyomi

<img src="art/icon-512.png" width="128" alt="Rubiyomi">

A reader for Japanese EPUB books. Words are coloured by your Anki or jpdb.io
cards, so you see at a glance what you know; tap one for its reading and
meanings, and grade or add its card without leaving the page. The dictionary
is built in and works offline.

https://github.com/user-attachments/assets/31a4f6e5-a92a-47f0-b96e-b2b12c2af77e

<p>
  <img src="https://github.com/user-attachments/assets/8aaf9d79-5f65-4f47-bfcf-4b9fb8b80120" width="200" alt="Tap a word for its reading and meanings">
  <img src="https://github.com/user-attachments/assets/d26624cd-ed7c-41d9-9c6f-490bcce3f4cc" width="200" alt="Words coloured by your Anki or jpdb.io cards">
  <img src="https://github.com/user-attachments/assets/90365757-4352-4104-8c1b-efc69c2305ba" width="200" alt="The library of EPUB books">
  <img src="https://github.com/user-attachments/assets/f3c0c159-cd3f-4289-baa0-e982b71d1a2d" width="200" alt="Vertical text and page themes">
</p>

## Get it

**Android** 9 or later, on Google Play (closed test). Start here:

### [➜ Join the tester group](https://groups.google.com/g/rubiyomi-testers)

Then open the [testing page](https://play.google.com/apps/testing/dev.calonca.rubiyomi),
tap *Become a tester* and install. The testing page works only after you've
joined the group ([all the steps](#google-play)).

- **Windows, Linux**, and the Android APK: the
  [latest release](https://github.com/Calonca/rubiyomi/releases/latest).
- **Mac and iOS:** coming.

## Download

From the [latest release](https://github.com/Calonca/rubiyomi/releases/latest):

- **Android:** from [Google Play](#google-play), or `Rubiyomi-<version>.apk`
  to install it yourself (Android asks you to allow installing apps from
  wherever you opened it). The APK doesn't update itself; Google Play and the
  APK are signed with different keys, so one can't update the other: to
  switch, uninstall first (which removes your books and stats from the
  device).
- **Linux:** `Rubiyomi-<version>-x86_64.AppImage`. Make it executable
  (`chmod +x`, or Properties → Executable in your file manager) and run it.
  It uses your system's GTK, which must be 4.14 or later (Ubuntu 24.04,
  Fedora 40, Debian 13 and newer).
- **Windows:** `Rubiyomi-<version>-x86_64.exe`. One file, nothing to install:
  run it. It isn't signed yet, so Windows SmartScreen warns about an unknown
  publisher the first time; choose *More info* → *Run anyway*.

The AppImage and the `.exe` look for a new version when they start and offer
it in a pop-up: Update downloads it beside the running file, checks its
SHA-256 and restarts into it, removing the old file. The check can be turned
off under Settings → Updates.

### Google Play

The app is in closed testing on Google Play. To join:

1. **[Join the rubiyomi-testers group](https://groups.google.com/g/rubiyomi-testers)**
   with the Google account you use on the phone's Play Store.
2. Open the [testing page](https://play.google.com/apps/testing/dev.calonca.rubiyomi)
   and tap *Become a tester* (it can take a few minutes after joining the
   group).
3. Install it from Google Play through the link on that page.

Please keep it installed for two weeks, and report what breaks in the
[issues](https://github.com/Calonca/rubiyomi/issues).

## Features

- **Your cards on the page.** Words are coloured by your cards on Anki
  (AnkiDroid or AnkiConnect) or jpdb.io, or by their stage on WaniKani, so
  you see at a glance what you already know. Due WaniKani words can be
  reviewed from the word card.
- **Tap a word** for its reading and meanings, and grade, add or suspend its
  card without leaving the page. The card is placed so it doesn't cover the
  sentence you're reading; tap another word to move it there, or the same
  word to close it.
- **Works offline.** The dictionary (JMdict, English) is included, and words
  are found on the device. You can also have jpdb.io split the text, under
  Settings → Parsing.
- **Pages your way.** Vertical or horizontal text, serif or sans, text size
  and line height, light, sepia, grey, dark, black and ocean themes.
- **Furigana over every word.** Readings from the dictionary (or from jpdb.io
  when it splits the text) stand over each word's kanji, in the word's colour;
  or keep the book's own, or hide them. Pages slide under your finger as you swipe;
  on a computer the arrow keys turn them.
- **Where you are.** A tap shows how far you are in the chapter and in the
  book, and the library shows each book's cover with its progress. The app
  reopens the last book where you left it.
- **Reading stats.** Characters read, reading speed, words looked up,
  chapters finished and unknown words, by day, week or month. A skip through a
  chapter isn't counted as reading unless you say so.
- **History.** The words you looked up are kept with their sentences.

Nothing is collected: see [PRIVACY.md](PRIVACY.md). The desktop downloads ask
GitHub for the latest release when they start, which you can turn off.

## Building

```sh
cargo xtask dict                  # the dictionaries, into dist/dict (needs curl, tar, xz)
tool/desktop.sh                   # the desktop app, target/desktop/fast/rubiyomi
tool/android.sh                   # Rubiyomi dev, target/apk/rubiyomi-dev.apk
```

The desktop app is GTK 4 on Linux (it needs GTK 4's development files;
`tool/gtk-dev.sh` unpacks them without installing anything, which
`tool/desktop.sh` does when they're missing). The APK needs the Android SDK
(platform 36, build tools 36), NDK 28, a JDK and `cargo-ndk`; no Gradle.
Rubiyomi dev is an app of its own beside the released one.

`tool/android.sh release` builds the signed APK and the Play Store bundle
(`target/apk/rubiyomi.apk`, `rubiyomi.aab`). The upload key stays outside the
repository: point `keystore.properties` at it (see
`keystore.properties.example`) or set the `RUBIYOMI_KEYSTORE*` variables.

The desktop downloads:

```sh
cargo build --release -p rubiyomi --features desktop,update-check
tool/appimage.sh target/release/rubiyomi out         # Linux AppImage
cargo build --release -p rubiyomi --features desktop,update-check,embed-dict --target x86_64-pc-windows-msvc
```

`embed-dict` puts the dictionaries inside the binary, for the single-file
Windows `.exe`. Builds without `update-check` (distro packages, your own)
never look for updates. `tool/update-feed.py FILE` serves a pretend release to
try the update flow against.

A `v*` tag builds the APK, the Play bundle, the AppImage and the `.exe` on
GitHub, each with its `.sha256`, with the key from the `release` environment's
secrets, into a draft release.

## Licence

MIT, © 2026 Alessandro La Conca. See [LICENSE](LICENSE) and
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

This app uses the [JMdict/EDICT](https://www.edrdg.org/wiki/index.php/JMdict-EDICT_Dictionary_Project)
dictionary files. These files are the property of the
[Electronic Dictionary Research and Development Group](https://www.edrdg.org/),
and are used in conformance with the Group's
[licence](https://www.edrdg.org/edrdg/licence.html). The dictionary data is
CC BY-SA 4.0. Words are found with [lindera](https://github.com/lindera/lindera)
and mecab-ipadic.
