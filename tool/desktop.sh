#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Builds the GTK 4 window (with the dev-hooks feature, which tool/headless.sh needs) with mold, against
# the headers tool/gtk-dev.sh unpacks: target/desktop/fast/rubiyomi.
set -euo pipefail
cd "$(dirname "$0")/.."
. tool/gtk-env.sh
cargo build --profile fast -p rubiyomi --features desktop,dev-hooks --bin rubiyomi "$@"
ls -l target/desktop/fast/rubiyomi
