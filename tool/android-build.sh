#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
set -euo pipefail
cd "$(dirname "$0")/.."
profile="${1:-fast}"
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$ANDROID_HOME/ndk/28.2.13676358}"
case "$profile" in
  debug) cargo_profile=(); gradle_task=assembleDebug ;;
  fast) cargo_profile=(--profile fast); gradle_task=assembleDebug ;;
  # The APK to hand out and the bundle for the Play Store.
  release) cargo_profile=(--release); gradle_task="assembleRelease bundleRelease" ;;
  *) echo "usage: $0 [debug|fast|release]" >&2; exit 2 ;;
esac
cargo ndk -t arm64-v8a --platform 26 -o android/app/src/main/jniLibs build -p rubiyomi --lib "${cargo_profile[@]}"
if [ ! -f dist/dict/version.json ]; then
  echo "no dictionaries in dist/dict: run cargo xtask dict first" >&2
  exit 1
fi
rm -rf android/app/src/main/assets/dict
mkdir -p android/app/src/main/assets/dict
cp dist/dict/jmdict.sqlite.xz dist/dict/ipadic.tar.xz dist/dict/version.json dist/dict/DATA-LICENCE.txt android/app/src/main/assets/dict/
echo "sdk.dir=$ANDROID_HOME" > android/local.properties
# shellcheck disable=SC2086
(cd android && ./gradlew --quiet $gradle_task)
ls -la android/app/build/outputs/apk/*/*.apk
if [ "$profile" = release ]; then ls -la android/app/build/outputs/bundle/release/*.aab; fi
