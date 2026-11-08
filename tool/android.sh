#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Builds the Android app without Gradle: the library, NSH's Java runtime and ours, the manifest, the
# dictionaries from dist/dict (cargo xtask dict builds them).
#   tool/android.sh [fast]   Rubiyomi dev (dev.calonca.rubiyomi.dev), debug key: target/apk/rubiyomi-dev.apk
#   tool/android.sh release  Rubiyomi (dev.calonca.rubiyomi): target/apk/rubiyomi.apk and rubiyomi.aab, signed
#                            with the upload key from keystore.properties (storeFile, storePassword, keyAlias,
#                            keyPassword) or RUBIYOMI_KEYSTORE, _PASSWORD, RUBIYOMI_KEY_ALIAS, _PASSWORD.
set -euo pipefail
cd "$(dirname "$0")/.."
mode=${1:-fast}
case $mode in fast|release) ;; *) echo "usage: tool/android.sh [fast|release]" >&2; exit 2;; esac
# One more for every build that goes to the Play Store.
version_code=8
min_sdk=28
target_sdk=36
bundletool_version=1.18.3
bundletool_sha256=a099cfa1543f55593bc2ed16a70a7c67fe54b1747bb7301f37fdfd6d91028e29

export ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$ANDROID_HOME/ndk/28.2.13676358}"
export JAVA_HOME="${JAVA_HOME:-$HOME/.jdks/jdk-21.0.12+8}"
export PATH="$JAVA_HOME/bin:$PATH"
build_tools="$ANDROID_HOME/build-tools/36.0.0"
android_jar="$ANDROID_HOME/platforms/android-$target_sdk/android.jar"
strip="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-strip"
out=target/apk
[ -f dist/dict/version.json ] || { echo "dist/dict is missing: run cargo xtask dict" >&2; exit 1; }
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)

if [ "$mode" = release ]; then
  id=dev.calonca.rubiyomi label=Rubiyomi name=rubiyomi profile=release version_name=$version
  prop() { [ ! -f keystore.properties ] || sed -n "s/^$1[[:space:]]*=[[:space:]]*//p" keystore.properties | head -1; }
  store=${RUBIYOMI_KEYSTORE:-$(prop storeFile)}
  store_pass=${RUBIYOMI_KEYSTORE_PASSWORD:-$(prop storePassword)}
  alias=${RUBIYOMI_KEY_ALIAS:-$(prop keyAlias)}
  key_pass=${RUBIYOMI_KEY_PASSWORD:-$(prop keyPassword)}
  if [ -z "$store" ]; then
    echo "No upload key set up: signed with the debug key, which the Play Store won't take." >&2
    store=$HOME/.android/debug.keystore store_pass=android alias=androiddebugkey key_pass=android
  fi
else
  id=dev.calonca.rubiyomi.dev label="Rubiyomi dev" name=rubiyomi-dev profile=fast version_name=$version-dev
  store=$HOME/.android/debug.keystore store_pass=android alias=androiddebugkey key_pass=android
fi

rm -rf "${out:?}/lib" "${out:?}/assets"
mkdir -p "$out/lib/arm64-v8a" "$out/assets/nsh" "$out/assets/dict" "$out/classes"
cp dist/dict/* "$out/assets/dict/"

cargo ndk -t arm64-v8a --platform "$min_sdk" build -p rubiyomi --lib --profile "$profile"
"$strip" -o "$out/lib/arm64-v8a/libnsh.so" "target/aarch64-linux-android/$profile/librubiyomi.so"
printf '{"id":"%s"}' "$id" > "$out/assets/nsh/app.json"

java_root="$(cargo metadata --format-version 1 --filter-platform aarch64-linux-android | python3 -c "
import json, os, sys
for p in json.load(sys.stdin)['packages']:
    if p['name'] == 'nsh-platform-android':
        print(os.path.join(os.path.dirname(p['manifest_path']), 'java'))")"
bridge=crates/app/android/java
if [ ! -f "$out/classes.dex" ] || [ "$java_root" -nt "$out/classes.dex" ] || [ "$(cat "$out/java-root" 2>/dev/null)" != "$java_root" ] \
  || [ -n "$(find "$bridge" -newer "$out/classes.dex" -print -quit)" ]; then
  rm -rf "${out:?}/classes" "${out:?}/classes.dex"; mkdir -p "$out/classes"
  echo "$java_root" > "$out/java-root"
  find "$java_root" "$bridge" -name '*.java' > "$out/sources"
  javac --release 17 -Xlint:-options -encoding UTF-8 -classpath "$android_jar" -d "$out/classes" "@$out/sources"
  jar cf "$out/classes.jar" -C "$out/classes" .
  "$build_tools/d8" --min-api "$min_sdk" --lib "$android_jar" --output "$out" "$out/classes.jar"
fi

# The dev build is an app of its own, beside the released one.
sed -e "s/package=\"dev.calonca.rubiyomi\"/package=\"$id\"/" -e "s/dev.calonca.rubiyomi.files/$id.files/" \
  -e "s/android:label=\"Rubiyomi\"/android:label=\"$label\"/" crates/app/android/AndroidManifest.xml > "$out/AndroidManifest.xml"
"$build_tools/aapt2" compile --dir crates/app/android/res -o "$out/res.zip"
link() {
  "$build_tools/aapt2" link -I "$android_jar" --manifest "$out/AndroidManifest.xml" "$out/res.zip" --min-sdk-version "$min_sdk" \
    --target-sdk-version "$target_sdk" --version-code "$version_code" --version-name "$version_name" "$@"
}

link -o "$out/in.apk"
(cd "$out" && zip -q in.apk classes.dex assets/nsh/app.json && zip -q -0 in.apk assets/dict/* && zip -q -0 in.apk lib/arm64-v8a/libnsh.so)
"$build_tools/zipalign" -f -P 16 4 "$out/in.apk" "$out/$name.apk"
"$build_tools/apksigner" sign --ks "$store" --ks-pass "pass:$store_pass" --ks-key-alias "$alias" --key-pass "pass:$key_pass" "$out/$name.apk"
ls -l "$out/$name.apk"
[ "$mode" = release ] || exit 0

# The Play Store's bundle: the same pieces, laid out as a module, with resources in protobuf form.
bundletool="$HOME/.cache/rubiyomi/bundletool-all-$bundletool_version.jar"
if ! echo "$bundletool_sha256  $bundletool" | sha256sum -c --status 2>/dev/null; then
  mkdir -p "$(dirname "$bundletool")"
  curl -fsSL -o "$bundletool.part" "https://github.com/google/bundletool/releases/download/$bundletool_version/bundletool-all-$bundletool_version.jar"
  echo "$bundletool_sha256  $bundletool.part" | sha256sum -c --status || { echo "bundletool's checksum doesn't match" >&2; exit 1; }
  mv "$bundletool.part" "$bundletool"
fi
rm -rf "${out:?}/module" "$out/proto.apk" "$out/base.zip" "$out/$name.aab"
link --proto-format -o "$out/proto.apk"
mkdir -p "$out/module/manifest" "$out/module/dex"
(cd "$out/module" && unzip -q ../proto.apk)
mv "$out/module/AndroidManifest.xml" "$out/module/manifest/"
cp "$out/classes.dex" "$out/module/dex/"
cp -r "$out/assets" "$out/lib" "$out/module/"
(cd "$out/module" && zip -q -r ../base.zip .)
printf '{"compression":{"uncompressedGlob":["assets/dict/**"]}}' > "$out/bundle-config.json"
java -jar "$bundletool" build-bundle --modules="$out/base.zip" --config="$out/bundle-config.json" --output="$out/$name.aab"
jarsigner -keystore "$store" -storepass "$store_pass" -keypass "$key_pass" -sigalg SHA256withRSA -digestalg SHA-256 \
  "$out/$name.aab" "$alias"
ls -l "$out/$name.aab"
