# Rubiyomi

![Rubiyomi](art/icon-512.png)

A simple app for reading Japanese books in EPUB, on Android and Linux.

**[Download the APK](https://github.com/Calonca/rubiyomi/releases/latest)**

## Features

- **Your cards on the page.** Words are coloured by your cards on Anki
  (AnkiDroid or AnkiConnect) or jpdb.io, so you see at a glance what you
  already know.
- **Tap a word** for its reading and meanings, and grade, add or suspend its
  card without leaving the page. The card is placed so it doesn't cover the
  sentence you're reading; tap another word to move it there, or the same
  word to close it.
- **Works offline.** The dictionary (JMdict, English) is included, and words
  are found on the device. You can also have jpdb.io split the text, under
  Settings → Parsing.
- **Pages your way.** Vertical or horizontal text, serif or sans, text size
  and line height, light, sepia, grey, dark, black and ocean themes, and
  furigana shown, hidden or faint. Pages slide under your finger as you swipe;
  on a computer the arrow keys turn them.
- **Where you are.** A tap shows how far you are in the chapter and in the
  book, and the library shows each book's cover with its progress. The app
  reopens the last book where you left it.
- **Reading stats.** Characters read, reading speed, words looked up,
  chapters finished and unknown words, by day, week or month. A skip through a
  chapter isn't counted as reading unless you say so.
- **History.** The words you looked up are kept with their sentences.

Nothing is collected: see [PRIVACY.md](PRIVACY.md).

## Building

```sh
cargo xtask dict                  # the dictionaries, into dist/dict (needs curl, tar, xz)
cargo run --profile fast          # desktop
tool/android-build.sh             # APK, in android/app/build/outputs/apk/debug/
```

The APK needs the Android SDK, NDK 28 and `cargo-ndk`.

`tool/android-build.sh release` builds the signed APK and the Play Store
bundle. The upload key stays outside the repository: point
`android/keystore.properties` at it (see `keystore.properties.example`) or set
the `RUBIYOMI_KEYSTORE*` variables. A `v*` tag builds both on GitHub, with
the key from the repository's secrets, into a draft release.

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
