# SPDX-License-Identifier: MIT
# Sourced: points the desktop build at GTK 4's headers, the system's or those tool/gtk-dev.sh unpacks,
# and gives it a target directory of its own, so its flags don't rebuild the rest.
if ! pkg-config --exists gtk4 2>/dev/null; then
  tool/gtk-dev.sh
  G="$PWD/target/gtk-dev/usr"
  export PKG_CONFIG_PATH=$G/lib64/pkgconfig LIBRARY_PATH=$G/lib64
  export RUSTFLAGS="-L native=$G/lib64 -C link-arg=-Wl,--allow-shlib-undefined -C link-arg=-fuse-ld=mold"
fi
export CARGO_TARGET_DIR=target/desktop
