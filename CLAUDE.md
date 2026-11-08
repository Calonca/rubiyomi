# Rubiyomi

Rust reader for Japanese books, on native views through NSH: Linux (GTK 4), Windows and Android.

## Layout

- `crates/core` — no UI, no JNI: EPUB, library store, layout, dictionaries
  (JMdict index, unpacking), word finder (lindera), furigana, reviews (jpdb.io and
  AnkiConnect clients, graders, card states), settings, platform trait,
  colour schemes. HTTP goes through `http::Transport`, so tests answer with
  hand-written JSON. Unit tests live next to the code; test EPUBs are built in
  code (`testing.rs`).
- `crates/app` — the app, on NSH (native-shell, pinned by git rev in its Cargo.toml). Desktop
  is feature `desktop` (GTK 4 on Linux), Android a cdylib with Java glue in
  `crates/app/android/` (`Bridge`, `AnkiDroidBridge` for AnkiDroid, `Secrets` keeping keys
  encrypted with the Android Keystore), its manifest and resources. Data in
  `~/.local/share/rubiyomi`, keys under the keyring service "Rubiyomi" (Secret Service) or
  the Android Keystore, never a file, except under `dev-hooks`.
- `xtask` — `cargo xtask dict` builds the bundled dictionaries into
  `dist/dict/` (JMdict release pinned with its checksum in `xtask/src/main.rs`;
  `tool/bump-jmdict.sh` moves it to the newest jmdict-simplified English
  release, and the monthly `jmdict.yml` workflow opens a PR doing that). The
  APK build needs them; desktop builds find them there.
- `fastlane/metadata/android/en-US` — the Play Store texts; `PRIVACY.md` the
  privacy policy. Keep them true to what the app does.
- `art/` — the icon source (`icon.svg`) and the 512 px store icon.

## Building

Run builds through `slotq` (`--cpus 6 --mem 6G` for the APK and the desktop build).

- **Use `--profile fast` for every build you run to test.** `--release` only for a build to
  hand out or for performance measurements.
- `tool/desktop.sh` builds the GTK window into `target/desktop/fast/rubiyomi` with the
  `dev-hooks` feature (`tool/gtk-env.sh` points it at GTK 4's headers, unpacked by
  `tool/gtk-dev.sh` into `target/gtk-dev` when the system has none);
  `tool/headless.sh OUT.png [ENV=VALUE...]` runs that binary in a private headless weston and
  saves a screenshot. Never run the desktop app on the user's display.
- `tool/android.sh` builds Rubiyomi dev (`dev.calonca.rubiyomi.dev`, debug key),
  `target/apk/rubiyomi-dev.apk`; `tool/android.sh release` the signed APK plus AAB of
  `dev.calonca.rubiyomi`, with the upload key from `keystore.properties` or
  `RUBIYOMI_KEYSTORE*`; never commit a key. No Gradle; needs `cargo xtask dict`. Bump
  `version_code` in it for every Play upload. A `v*` tag runs `release.yml` (APK, AAB,
  AppImage, `.exe`, into a draft release) with the key from the `release` environment's secrets.
- Feature `update-check` (the desktop downloads only: AppImage, `.exe`) turns
  on the update check; Android and packaged builds never have it.
  `embed-dict` puts the dictionaries in the Windows `.exe`. `tool/appimage.sh`
  packs the AppImage; `tool/update-feed.py` serves a pretend release.
- Feature `dev-hooks` (off by default, never on Android) turns on the `RUBIYOMI_*`
  environment variables that the headless runs use (`SCREEN`, `SCRIPT`, `OPEN_WORD`, `WINDOW`,
  `SECRETS_FILE`, ...), all read through `src/hooks.rs`; it also allows keys in a file. Release
  builds don't have it. `RUBIYOMI_DICT_DIR` is a real setting and is always read.
- `tool/icon-font.py` cuts the Material Symbols subset in `crates/app/assets` down to the icons listed
  in it (needs fonttools).
- `cargo test --workspace --profile fast` — the tests; `tool/check.sh` runs
  all of CI's checks locally.
- `tool/icon.py ZenOldMincho-Bold.ttf` regenerates the launcher icon layers and
  `art/icon.svg`; then render `art/icon-512.png` from the SVG.
- `adb` is in `~/Android/Sdk/platform-tools`. Ask before installing on the
  phone. Commit locally only: never push without the user's OK.

## Rules

- MIT. Every source file starts with the SPDX line and the
  copyright line. Every new dependency must pass `cargo deny check licenses`
  and, when it ships data or fonts, get an entry in THIRD_PARTY_NOTICES.md.
- No copyrighted book text, recorded API responses or real keys anywhere.
  Japanese text in tests comes from public-domain works (Aozora Bunko), with
  the source noted.
- Comments only for a non-obvious why. Tests only where they guard something
  real.
- Dictionary data (JMdict, IPADIC) is never committed. JMdict: English only,
  credited in the app (Settings → About → Dictionary), README and
  THIRD_PARTY_NOTICES.md; the data and our index are CC BY-SA 4.0.
- UI text is English; only book content is Japanese.
