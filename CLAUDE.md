# Rubiyomi

Rust + egui reader for Japanese books, Linux and Android.

## Layout

- `crates/core` — no egui, no JNI: platform trait, colour schemes. Unit tests
  live next to the code.
- `crates/app` — egui UI, desktop binary and Android cdylib.
- `android/` — Gradle project; `MainActivity` extends NativeActivity.
- `art/` — the icon source (`icon.svg`) and the 512 px store icon.

## Building

Run builds through `slotq` (APK: `--cpus 6 --mem 6G`).

- **Use `--profile fast` for every build you run to test**:
  `cargo build --profile fast`, `cargo run --profile fast`, and
  `tool/android-build.sh fast` for the APK (the default; cargo-ndk, then
  Gradle; APK in `android/app/build/outputs/apk/debug/`). `--release` only for
  a build to hand out or for performance measurements.
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
- UI text is English; only book content is Japanese.
