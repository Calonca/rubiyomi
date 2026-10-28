#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
set -euo pipefail
cd "$(dirname "$0")/.."
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$ANDROID_HOME/ndk/28.2.13676358}"
cargo fmt --all --check
cargo clippy --workspace --all-targets --profile fast -- -D warnings
cargo clippy -p rubiyomi --profile fast --features update-check -- -D warnings
cargo ndk -t arm64-v8a --platform 26 clippy -p rubiyomi --lib --profile fast -- -D warnings
cargo test --workspace --profile fast
cargo deny check licenses bans sources
