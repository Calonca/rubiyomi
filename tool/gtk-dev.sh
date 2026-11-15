#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Unpacks GTK 4's development files into target/gtk-dev without installing anything, so the
# desktop build links on a machine that has only the GTK runtime on its host. Fedora (dnf) and
# Debian/Ubuntu (apt-get) only; elsewhere, install the development packages instead.
set -euo pipefail
cd "$(dirname "$0")/.."
root="$PWD/target/gtk-dev"

need_packages() {
  cat >&2 <<'EOF'
GTK 4's development files are missing, and tool/gtk-dev.sh can't unpack them here. Install them:
  Debian/Ubuntu: sudo apt install libgtk-4-dev libgraphene-1.0-dev libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev
  Fedora:        sudo dnf install gtk4-devel graphene-devel gstreamer1-devel gstreamer1-plugins-base-devel
  Arch:          sudo pacman -S gtk4 graphene gstreamer gst-plugins-base-libs
With them installed, tool/desktop.sh uses the system's files and needs no helper.
EOF
  exit 1
}

# Points the .pc files at the unpacked tree.
relocate() {
  local pc
  for pc in "$1"/pkgconfig/*.pc "$root"/usr/share/pkgconfig/*.pc; do
    [ -f "$pc" ] || continue
    sed -i "s|^\([A-Za-z_]*\)=/usr\b|\1=$root/usr|" "$pc"
  done
}

modules="gtk4 gstreamer-app-1.0 gstreamer-audio-1.0"
# The pkgconfig directory of the unpacked gtk4.pc, if there is one.
unpacked() { dirname "$(ls "$root"/usr/lib*/pkgconfig/gtk4.pc "$root"/usr/lib/*/pkgconfig/gtk4.pc 2>/dev/null | head -n1)"; }
found() { PKG_CONFIG_PATH="$(unpacked):$root/usr/share/pkgconfig" pkg-config --exists $modules 2>/dev/null; }

if [ -f "$(unpacked)/gtk4.pc" ] && grep -q "^prefix=$root/usr" "$(unpacked)/gtk4.pc" && found; then exit 0; fi

if command -v dnf >/dev/null && command -v rpm2cpio >/dev/null && command -v cpio >/dev/null; then
  rpms="$root.rpm"
  rm -rf "${root:?}" "${rpms:?}"; mkdir -p "$root" "$rpms"
  dnf download --destdir "$rpms" --arch "$(uname -m)" --arch noarch gtk4 gtk4-devel graphene-devel
  rm -f "$rpms"/*.i686.rpm
  for rpm in "$rpms"/*.rpm; do (cd "$root" && rpm2cpio "$rpm" | cpio -idmu --quiet); done
  relocate "$root/usr/lib64"
  ln -sf /usr/lib64/libgraphene-1.0.so.0 "$root/usr/lib64/libgraphene-1.0.so"
elif command -v apt-get >/dev/null && command -v dpkg-deb >/dev/null; then
  arch=$(dpkg-architecture -qDEB_HOST_MULTIARCH 2>/dev/null || gcc -print-multiarch)
  debs="$root.deb"
  rm -rf "${root:?}" "${debs:?}"; mkdir -p "$root" "$debs"
  # What installing the dev packages would install, as apt resolves it (a simulation needs no root).
  apt-get -s install --no-install-recommends libgtk-4-dev libgraphene-1.0-dev \
      libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev | awk '/^Inst (lib|.*-dev )/{print $2}' >"$debs/wanted"
  (cd "$debs" && xargs -r apt-get download <wanted)
  for deb in "$debs"/*.deb; do dpkg-deb -x "$deb" "$root"; done
  lib="$root/usr/lib/$arch"
  relocate "$lib"
  # A dev package's libfoo.so points at the runtime library, which is installed, not unpacked.
  for so in "$lib"/*.so; do
    [ -L "$so" ] && [ ! -e "$so" ] || continue
    target=$(readlink "$so")
    case $target in /*) ;; *) target="/usr/lib/$arch/$target" ;; esac
    ln -sf "$target" "$so"
  done
else
  need_packages
fi
found || { PKG_CONFIG_PATH="$(unpacked):$root/usr/share/pkgconfig" pkg-config --print-errors --exists $modules || true; need_packages; }
