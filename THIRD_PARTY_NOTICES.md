# Third-party notices

Rubiyomi is under the MIT licence. It is built from, and ships with, the
components below. Fonts and artwork that ship inside the app, which carry
their own notices, come first.

## Zen Old Mincho (launcher icon)

The ルビ letters in the launcher icon are outlines of *Zen Old Mincho Bold*,
Copyright 2021 The Zen Old Mincho Project Authors
(<https://github.com/googlefonts/zen-oldmincho>), licensed under the SIL Open
Font License, Version 1.1: <https://openfontlicense.org>. The font itself is
not shipped; only the outlines of two glyphs, as part of the icon.

## Material Symbols (icons)

The icon font, *Material Symbols Rounded*, by Google, through the
`egui_material_icons` crate (MIT). Licensed under the Apache License,
Version 2.0: <https://www.apache.org/licenses/LICENSE-2.0>.
Source: <https://github.com/google/material-design-icons>.

## Material Color Utilities

The colour-scheme maths (HCT, tonal palettes, dynamic colour) is Google's
Material Color Utilities (Apache-2.0,
<https://github.com/material-foundation/material-color-utilities>), through
its Rust port `material-colors` (MIT OR Apache-2.0,
<https://github.com/Aiving/material-colors>).

## egui default fonts

egui's built-in fonts are included through `epaint_default_fonts`:

- Ubuntu Light — Ubuntu Font Licence 1.0, © Canonical Ltd.
- Hack — MIT, © Source Foundry Authors; Bitstream Vera licence for the parts
  derived from Bitstream Vera; DejaVu changes in the public domain.
- Noto Emoji — SIL Open Font License 1.1, © Google Inc.
- emoji-icon-font — MIT, © 2014 John Slegers.

## Rust crates

Every crate in `Cargo.lock` is under a licence on the allow-list in
`deny.toml`, checked by `cargo deny check licenses` in CI.
