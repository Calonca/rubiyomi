# Rubiyomi

![Rubiyomi](art/icon-512.png)

A simple app for reading Japanese books in EPUB, on Android, Linux and
Windows.

## Download

From the [latest release](https://github.com/Calonca/rubiyomi/releases/latest):

- **Android:** from Google Play, or `Rubiyomi-<version>.apk` to install it
  yourself (Android asks you to allow installing apps from wherever you opened
  it). The APK doesn't update itself; Google Play and the APK are signed with
  different keys, so one can't update the other: to switch, uninstall first
  (which removes your books and stats from the device).
- **Linux:** `Rubiyomi-<version>-x86_64.AppImage`. Make it executable
  (`chmod +x`, or Properties → Executable in your file manager) and run it.
- **Windows:** `Rubiyomi-<version>-x86_64.exe`. One file, nothing to install:
  run it. It isn't signed yet, so Windows SmartScreen warns about an unknown
  publisher the first time; choose *More info* → *Run anyway*.

The AppImage and the `.exe` look for a new version when they start and offer
it in a pop-up: Update downloads it beside the running file, checks its
SHA-256 and restarts into it, removing the old file. The check can be turned
off under Settings → Updates.

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

Nothing is collected: see [PRIVACY.md](PRIVACY.md). The desktop downloads ask
GitHub for the latest release when they start, which you can turn off.

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
the `RUBIYOMI_KEYSTORE*` variables.

The desktop downloads:

```sh
cargo build --release --features update-check
tool/appimage.sh target/release/rubiyomi out         # Linux AppImage
cargo build --release --features update-check,embed-dict --target x86_64-pc-windows-msvc
```

`embed-dict` puts the dictionaries inside the binary, for the single-file
Windows `.exe`. Builds without `update-check` (`cargo run`, distro packages)
never look for updates. `tool/update-feed.py FILE` serves a pretend release to
try the update flow against.

### macOS

There is no macOS download; build it from source:

1. Install the Xcode command line tools (`xcode-select --install`), Rust
   (<https://rustup.rs>) and `xz` (`brew install xz`; the dictionary build
   uses it).
2. In a clone of this repository:

   ```sh
   cargo xtask dict          # the dictionaries, into dist/dict
   cargo build --release
   ```

3. Run `target/release/rubiyomi`. It finds the dictionaries in the clone's
   `dist/dict`; to move the binary elsewhere, put a copy of `dist/dict` beside
   it as `dict`.

A source build, on any system, has no update check: to update, `git pull` and
build again. Your jpdb.io and AnkiConnect keys are kept in a file only your
user can read, as the macOS Keychain isn't used yet.

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
