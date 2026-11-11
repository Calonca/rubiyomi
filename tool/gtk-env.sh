# SPDX-License-Identifier: MIT
# Sourced: points the desktop build at GTK 4's headers, the system's or those tool/gtk-dev.sh unpacks,
# and gives it a target directory of its own, so its flags don't rebuild the rest.
if ! pkg-config --exists gtk4 gstreamer-app-1.0 gstreamer-audio-1.0 2>/dev/null; then
  tool/gtk-dev.sh || return 1 2>/dev/null || exit 1
  G="$PWD/target/gtk-dev/usr"
  L=$(dirname "$(ls "$G"/lib*/pkgconfig/gtk4.pc "$G"/lib/*/pkgconfig/gtk4.pc 2>/dev/null | head -n1)")
  L=${L%/pkgconfig}
  export PKG_CONFIG_PATH=$L/pkgconfig:$G/share/pkgconfig LIBRARY_PATH=$L
  export RUSTFLAGS="-L native=$L -C link-arg=-Wl,--allow-shlib-undefined"
  if command -v mold >/dev/null; then RUSTFLAGS="$RUSTFLAGS -C link-arg=-fuse-ld=mold"; fi
fi
export CARGO_TARGET_DIR=target/desktop
