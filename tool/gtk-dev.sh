#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Unpacks GTK 4's development files into target/gtk-dev without installing anything, so the
# desktop build links on a machine that has only the GTK runtime on its host.
set -euo pipefail
cd "$(dirname "$0")/.."
root="$PWD/target/gtk-dev"
if [ -f "$root/usr/lib64/pkgconfig/gtk4.pc" ] && grep -q "^prefix=$root/usr" "$root/usr/lib64/pkgconfig/gtk4.pc"; then exit 0; fi
rpms="$root.rpm"
rm -rf "${root:?}" "${rpms:?}"; mkdir -p "$root" "$rpms"
dnf download --destdir "$rpms" --arch x86_64 --arch noarch gtk4 gtk4-devel graphene-devel
rm -f "$rpms"/*.i686.rpm
for rpm in "$rpms"/*.rpm; do (cd "$root" && rpm2cpio "$rpm" | cpio -idmu --quiet); done
for pc in "$root"/usr/lib64/pkgconfig/*.pc; do sed -i "s|^prefix=/usr\$|prefix=$root/usr|" "$pc"; done
ln -sf /usr/lib64/libgraphene-1.0.so.0 "$root/usr/lib64/libgraphene-1.0.so"
