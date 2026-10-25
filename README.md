# Rubiyomi

![Rubiyomi](art/icon-512.png)

A simple app for reading Japanese books in EPUB.

Load your EPUBs and start reading. Words are coloured by your cards on
Anki or jpdb, so you see at a glance what you already know.

Read anywhere, even on a plane: the dictionary is already included, so
with the offline sources the app needs no internet.

Early version: the app's sections are in place, and books come next.

## Building

```sh
cargo xtask dict                  # the dictionaries, into dist/dict (needs curl, tar, xz)
cargo run --profile fast          # desktop
tool/android-build.sh             # APK, in android/app/build/outputs/apk/debug/
```

The APK needs the Android SDK, NDK 28 and `cargo-ndk`.

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
