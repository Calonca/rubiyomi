#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Runs the desktop build in a private headless weston (on its own socket, with
# DISPLAY unset, so nothing opens on the user's screen) and saves its screenshot.
#   tool/headless.sh OUT.png [ENV=VALUE...]
# RUBIYOMI_WINDOW=WxH (default 1100x800) is the app's window size; weston is made the same size, so the
# screenshot is the window. WAIT=SECONDS (default 7) is how long the app runs before the screenshot.
# APP_LOG=FILE keeps the app's log. Runs may go in parallel: each has its own weston socket.
# The app runs on the host (the toolbox has no GTK runtime); weston and its screenshooter are here.
set -euo pipefail
cd "$(dirname "$0")/.."
root="$PWD"
out=$(realpath -m "$1"); shift
bin="$root/target/desktop/fast/rubiyomi"
[ -x "$bin" ] || { echo "build it first: slotq run --cpus 6 --mem 4G -- tool/desktop.sh" >&2; exit 1; }
XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR:-/run/user/$(id -u)}
socket=rubiyomi-test-$$
shots="$root/target/headless-shot-$$"
size=1100x800
for kv in "$@"; do case $kv in RUBIYOMI_WINDOW=*) size=${kv#*=};; esac; done
width=${size%x*}; height=${size#*x}
mkdir -p "$shots" "$root/target/headless-data" "$(dirname "$out")"

env -u WAYLAND_DISPLAY -u DISPLAY weston --backend=headless --renderer=gl --width=$width --height=$height \
  --socket=$socket --debug --idle-time=0 --no-config --shell=${WESTON_SHELL:-kiosk} > "$shots/weston.log" 2>&1 &
weston_pid=$!
app_pid=
cleanup() {
  [ -z "$app_pid" ] || kill "$app_pid" 2>/dev/null || true
  kill "$weston_pid" 2>/dev/null || true
  wait 2>/dev/null || true
  rm -rf "${shots:?}"
}
trap cleanup EXIT
for _ in $(seq 1 50); do [ -S "$XDG_RUNTIME_DIR/$socket" ] && break; sleep 0.2; done
[ -S "$XDG_RUNTIME_DIR/$socket" ] || { echo "weston did not start; see $shots/weston.log" >&2; exit 1; }

extra=(--env=RUBIYOMI_WINDOW=$size)
for kv in "$@"; do extra+=("--env=$kv"); done
flatpak-spawn --host --env=WAYLAND_DISPLAY=$socket --env=GDK_BACKEND=wayland --env=DISPLAY= \
  --env=XDG_DATA_HOME="${DATA:-$root/target/headless-data}" --env=RUBIYOMI_SECRETS_FILE=1 "${extra[@]}" timeout $((${WAIT:-7} + 9)) "$bin" > "$shots/app.log" 2>&1 &
app_pid=$!
sleep "${WAIT:-7}"
(cd "$shots" && WAYLAND_DISPLAY=$socket weston-screenshooter > /dev/null 2>&1)
shot=$(ls "$shots"/wayland-screenshot-*.png | head -1)
mv "$shot" "$out"
[ -z "${APP_LOG:-}" ] || cp "$shots/app.log" "$APP_LOG"
echo "$out"
