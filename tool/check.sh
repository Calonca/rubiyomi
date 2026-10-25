#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all --check
cargo clippy --workspace --all-targets --profile fast -- -D warnings
cargo clippy -p rubiyomi --lib --target aarch64-linux-android --profile fast -- -D warnings
cargo test --workspace --profile fast
cargo deny check licenses bans sources
