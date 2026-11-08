#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
# Copyright (C) 2026 Alessandro La Conca
# Cuts Material Symbols Rounded, as egui_material_icons 0.8.0 ships it, down to the icons Rubiyomi uses.
# Needs fonttools, in a venv outside the repo. Icons are drawn by code point, so ligatures go too.
# usage: tool/icon-font.py [CRATE_DIR]
import sys
from glob import glob
from os.path import expanduser
from pathlib import Path

from fontTools import subset
from fontTools.ttLib import TTFont

ICONS = """
add all_inclusive arrow_back auto_stories block close content_copy content_paste delete description
dictionary gavel info insights list local_library more_horiz open_in_new palette pause privacy_tip school
segment settings text_fields update
""".split()

crate = Path(sys.argv[1] if len(sys.argv) > 1 else glob(expanduser("~/.cargo/registry/src/*/egui_material_icons-0.8.0"))[0])
out = Path(__file__).resolve().parent.parent / "crates/app/assets/MaterialSymbolsRounded-subset.ttf"

codes = dict(line.split() for line in (crate / "MaterialSymbolsRounded-Regular.codepoints").read_text().splitlines())
options = subset.Options(layout_features=[], hinting=False, notdef_outline=True, recalc_timestamp=False)
font = TTFont(crate / "MaterialSymbolsRounded-Regular.ttf", recalcTimestamp=False)
subsetter = subset.Subsetter(options)
subsetter.populate(unicodes=[int(codes[name], 16) for name in ICONS])
subsetter.subset(font)
out.parent.mkdir(exist_ok=True)
font.save(out)
print(out, out.stat().st_size)
