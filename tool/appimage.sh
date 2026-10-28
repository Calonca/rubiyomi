#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# tool/appimage.sh BINARY OUT_DIR: packs the binary and dist/dict into
# Rubiyomi-<version>-x86_64.AppImage, with its .sha256, in OUT_DIR.
set -euo pipefail
cd "$(dirname "$0")/.."
binary="$1"
out="$2"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
# appimagetool ships only as a rolling "continuous" release; when it moves, check the new
# build and put its digest (shown on the release page) here.
tool_url=https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
tool_sha256=95cbe7cce9717fce90c484e34052ee7c7f1d7635b33c12525b4776826a7d29b6

if [ ! -f dist/dict/version.json ]; then
  echo "no dictionaries in dist/dict: run cargo xtask dict first" >&2
  exit 1
fi
work="dist/appimage"
rm -rf "$work"
mkdir -p "$work/AppDir/usr/bin" "$work/AppDir/usr/share/rubiyomi/dict" "$out"
tool="dist/appimagetool-$tool_sha256"
if [ ! -x "$tool" ]; then
  curl -fL --retry 3 -o "$tool.part" "$tool_url"
  echo "$tool_sha256  $tool.part" | sha256sum -c - >/dev/null || {
    echo "appimagetool's continuous build changed: check it and update tool_sha256 in $0" >&2
    rm -f "$tool.part"
    exit 1
  }
  chmod +x "$tool.part"
  mv "$tool.part" "$tool"
fi

app="$work/AppDir"
install -m 755 "$binary" "$app/usr/bin/rubiyomi"
cp dist/dict/jmdict.sqlite.xz dist/dict/ipadic.tar.xz dist/dict/version.json dist/dict/DATA-LICENCE.txt "$app/usr/share/rubiyomi/dict/"
cp packaging/linux/dev.calonca.rubiyomi.desktop "$app/"
cp art/icon-512.png "$app/dev.calonca.rubiyomi.png"
# The binary finds the dictionaries beside itself, in ../share/rubiyomi/dict.
ln -s usr/bin/rubiyomi "$app/AppRun"

name="Rubiyomi-$version-x86_64.AppImage"
# Extracting it first runs it without FUSE, which CI runners lack.
ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$tool" --no-appstream "$app" "$out/$name"
(cd "$out" && sha256sum "$name" > "$name.sha256")
ls -la "$out/$name"
