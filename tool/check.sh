#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# CI's checks.
set -euo pipefail
cd "$(dirname "$0")/.."
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$ANDROID_HOME/ndk/28.2.13676358}"
cargo fmt --all --check
cargo clippy --workspace --all-targets --profile fast -- -D warnings
(. tool/gtk-env.sh && cargo clippy -p rubiyomi --features desktop,dev-hooks --all-targets --profile fast -- -D warnings)
(. tool/gtk-env.sh && cargo clippy -p rubiyomi --features desktop,update-check --all-targets --profile fast -- -D warnings)
# Windows can't be linked from here (NSH needs the MSVC WebView2 loader), but it can be checked.
if rustup target list --installed 2>/dev/null | grep -qx x86_64-pc-windows-gnu; then
  CARGO_TARGET_DIR=target/windows cargo clippy --target x86_64-pc-windows-gnu -p rubiyomi --features desktop,update-check,embed-dict \
    --profile fast -- -D warnings
fi
cargo ndk -t arm64-v8a --platform 28 clippy -p rubiyomi --lib --profile fast -- -D warnings
cargo test --workspace --profile fast
cargo deny check licenses bans sources
cargo deny --all-features check licenses
