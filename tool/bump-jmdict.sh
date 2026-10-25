#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Copyright (C) 2026 Alessandro La Conca
#
# Points xtask at the newest English jmdict-simplified release, with its checksum.
# Prints the new version, or nothing when the pin is already the newest.
set -euo pipefail
cd "$(dirname "$0")/.."
main=xtask/src/main.rs

auth=()
if [[ -n "${GITHUB_TOKEN:-}" ]]; then auth=(-H "Authorization: Bearer $GITHUB_TOKEN"); fi
release=$(curl -fsSL "${auth[@]}" https://api.github.com/repos/scriptin/jmdict-simplified/releases/latest)
version=$(jq -r .tag_name <<<"$release")
url=$(jq -r --arg name "jmdict-eng-$version.json.tgz" '.assets[] | select(.name == $name) | .browser_download_url' <<<"$release")
[[ -n "$url" ]] || { echo "no jmdict-eng asset in release $version" >&2; exit 1; }

current=$(sed -n 's/^const JMDICT_VERSION: &str = "\(.*\)";/\1/p' "$main")
[[ "$current" != "$version" ]] || exit 0

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
curl -fsSL --retry 3 -o "$tmp/jmdict.tgz" "$url"
sum=$(sha256sum "$tmp/jmdict.tgz" | cut -d' ' -f1)

sed -i \
    -e "s|^const JMDICT_VERSION: &str = \".*\";|const JMDICT_VERSION: \&str = \"$version\";|" \
    -e "s|^    \"https://github.com/scriptin/jmdict-simplified/releases/download/.*\";|    \"$url\";|" \
    -e "s|^const JMDICT_SHA256: &str = \".*\";|const JMDICT_SHA256: \&str = \"$sum\";|" \
    "$main"
grep -q "$sum" "$main" && grep -q "$version\";" "$main" || { echo "couldn't rewrite $main" >&2; exit 1; }
echo "$version"
