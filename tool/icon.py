#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
# Copyright (C) 2026 Alessandro La Conca
# Writes the launcher icon layers and art/icon.svg from Zen Old Mincho Bold.
# usage: tool/icon.py ZenOldMincho-Bold.ttf
import sys
from pathlib import Path

from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont

INK, GOLD = "#1F1B16", "#E8A33D"
# The seal is shrunk about the centre so its corners stay in the 66dp safe circle.
SCALE = 0.84


def fit(x, y):
    return 54 + (x - 54) * SCALE, 54 + (y - 54) * SCALE


def rrect(x, y, w, h, r):
    (x, y), (w, h), r = fit(x, y), (w * SCALE, h * SCALE), r * SCALE
    f = lambda v: f"{v:.2f}"
    return (
        f"M{f(x + r)},{f(y)}H{f(x + w - r)}A{f(r)},{f(r)} 0 0 1 {f(x + w)},{f(y + r)}V{f(y + h - r)}"
        f"A{f(r)},{f(r)} 0 0 1 {f(x + w - r)},{f(y + h)}H{f(x + r)}A{f(r)},{f(r)} 0 0 1 {f(x)},{f(y + h - r)}"
        f"V{f(y + r)}A{f(r)},{f(r)} 0 0 1 {f(x + r)},{f(y)}Z"
    )


def letter(font, ch, baseline):
    glyphs = font.getGlyphSet()
    name = font.getBestCmap()[ord(ch)]
    k = 22 / font["head"].unitsPerEm
    left = 54 - glyphs[name].width * k / 2
    pen = SVGPathPen(glyphs, ntos=lambda v: f"{v:.2f}")
    a, d = k * SCALE, -k * SCALE
    dx, dy = fit(left, baseline)
    glyphs[name].draw(TransformPen(pen, (a, 0, 0, d, dx, dy)))
    return pen.getCommands()


def vector(paths):
    body = "\n".join(f'    <path android:fillColor="{c}"{e} android:pathData="{d}" />' for c, d, e in paths)
    return (
        '<?xml version="1.0" encoding="utf-8"?>\n<!-- SPDX-License-Identifier: MIT -->\n'
        '<vector xmlns:android="http://schemas.android.com/apk/res/android"\n'
        '    android:width="108dp" android:height="108dp"\n'
        '    android:viewportWidth="108" android:viewportHeight="108">\n'
        f"{body}\n</vector>\n"
    )


font = TTFont(sys.argv[1])
seal = rrect(29, 23, 50, 62, 6)
w = 1.6
border = rrect(33 - w / 2, 27 - w / 2, 42 + w, 54 + w, 3 + w / 2) + rrect(33 + w / 2, 27 + w / 2, 42 - w, 54 - w, 3 - w / 2)
letters = letter(font, "ル", 52) + letter(font, "ビ", 75)
odd = ' android:fillType="evenOdd"'

res = Path(__file__).resolve().parent.parent / "android/app/src/main/res/drawable"
(res / "ic_launcher_background.xml").write_text(vector([(INK, "M0,0h108v108h-108z", "")]))
(res / "ic_launcher_foreground.xml").write_text(vector([(GOLD, seal, ""), (INK, border, odd), (INK, letters, "")]))
(res / "ic_launcher_monochrome.xml").write_text(vector([("#FFFFFF", seal + border + letters, odd)]))

art = Path(__file__).resolve().parent.parent / "art/icon.svg"
art.parent.mkdir(exist_ok=True)
art.write_text(
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 108 108">\n'
    f'<rect width="108" height="108" fill="{INK}"/>\n<path fill="{GOLD}" d="{seal}"/>\n'
    f'<path fill="{INK}" fill-rule="evenodd" d="{border}"/>\n<path fill="{INK}" d="{letters}"/>\n</svg>\n'
)
