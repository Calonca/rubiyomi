#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Serves a pretend GitHub release, to try the update flow without publishing one.

    tool/update-feed.py FILE [VERSION]

FILE is an AppImage or .exe; it is offered as Rubiyomi VERSION (default 9.9.9).
Then start a build made with `update-check` with
RUBIYOMI_UPDATE_URL=http://127.0.0.1:8765/release.json.
"""
import hashlib
import http.server
import json
import shutil
import sys
from pathlib import Path

PORT = 8765

src = Path(sys.argv[1])
version = sys.argv[2] if len(sys.argv) > 2 else "9.9.9"
suffix = {".AppImage": "-x86_64.AppImage", ".exe": "-x86_64.exe"}[src.suffix]
name = f"Rubiyomi-{version}{suffix}"
root = Path(__file__).resolve().parent.parent / "dist" / "update-feed"
shutil.rmtree(root, ignore_errors=True)
root.mkdir(parents=True)
shutil.copy(src, root / name)
digest = hashlib.sha256((root / name).read_bytes()).hexdigest()
(root / f"{name}.sha256").write_text(f"{digest}  {name}\n")
base = f"http://127.0.0.1:{PORT}"
release = {
    "tag_name": f"v{version}",
    "html_url": f"{base}/",
    "body": f"A pretend release to try the update flow.\n\n- {name} is {src.name}\n- served from {root}",
    "assets": [{"name": n, "browser_download_url": f"{base}/{n}"} for n in (name, f"{name}.sha256")],
}
(root / "release.json").write_text(json.dumps(release, indent=2))
print(f"RUBIYOMI_UPDATE_URL={base}/release.json")
handler = lambda *a, **k: http.server.SimpleHTTPRequestHandler(*a, directory=root, **k)
http.server.ThreadingHTTPServer(("127.0.0.1", PORT), handler).serve_forever()
