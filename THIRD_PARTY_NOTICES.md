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

## JMdict (dictionary data, bundled)

This app uses the [JMdict/EDICT](https://www.edrdg.org/wiki/index.php/JMdict-EDICT_Dictionary_Project)
dictionary files. These files are the property of the
[Electronic Dictionary Research and Development Group](https://www.edrdg.org/),
and are used in conformance with the Group's
[licence](https://www.edrdg.org/edrdg/licence.html).

Only the English JMdict is used, as released by
[jmdict-simplified](https://github.com/scriptin/jmdict-simplified). The data,
and the index `cargo xtask dict` builds from it (`jmdict.sqlite`), are
licensed under the
[Creative Commons Attribution-ShareAlike 4.0](https://creativecommons.org/licenses/by-sa/4.0/)
licence; no copyright is claimed over them. The release used is pinned, with
its checksum, in `xtask/src/main.rs` and moved to the newest one before each
Rubiyomi release; the data is fetched at build time and never committed. In
the app: Settings → About → Dictionary.

## IPADIC (word finder data, bundled)

The word finder uses mecab-ipadic, compiled by lindera (MIT). Its notice,
also shown in the app under Settings → About → Word finder:

```
===========================================================================
Lindera Morphological Analyzer
===========================================================================

This software includes a binary and/or source version of data from

  mecab-ipadic-2.7.0-20070801

which can be obtained from

  http://jaist.dl.sourceforge.net/project/mecab/mecab-ipadic/2.7.0-20070801/mecab-ipadic-2.7.0-20070801.tar.gz

===========================================================================
mecab-ipadic-2.7.0-20070801 Notice
===========================================================================

Nara Institute of Science and Technology (NAIST),
the copyright holders, disclaims all warranties with regard to this
software, including all implied warranties of merchantability and
fitness, in no event shall NAIST be liable for
any special, indirect or consequential damages or any damages
whatsoever resulting from loss of use, data or profits, whether in an
action of contract, negligence or other tortuous action, arising out
of or in connection with the use or performance of this software.

A large portion of the dictionary entries
originate from ICOT Free Software.  The following conditions for ICOT
Free Software applies to the current dictionary as well.

Each User may also freely distribute the Program, whether in its
original form or modified, to any third party or parties, PROVIDED
that the provisions of Section 3 ("NO WARRANTY") will ALWAYS appear
on, or be attached to, the Program, which is distributed substantially
in the same form as set out herein and that such intended
distribution, if actually made, will neither violate or otherwise
contravene any of the laws and regulations of the countries having
jurisdiction over the User or the intended distribution itself.

NO WARRANTY

The program was produced on an experimental basis in the course of the
research and development conducted during the project and is provided
to users as so produced on an experimental basis.  Accordingly, the
program is provided without any warranty whatsoever, whether express,
implied, statutory or otherwise.  The term "warranty" used herein
includes, but is not limited to, any warranty of the quality,
performance, merchantability and fitness for a particular purpose of
the program and the nonexistence of any infringement or violation of
any right of any third party.

Each user of the program will agree and understand, and be deemed to
have agreed and understood, that there is no warranty whatsoever for
the program and, accordingly, the entire risk arising from or
otherwise connected with the program is assumed by the user.

Therefore, neither ICOT, the copyright holder, or any other
organization that participated in or was otherwise related to the
development of the program and their respective officials, directors,
officers and other employees shall be held liable for any and all
damages, including, without limitation, general, special, incidental
and consequential damages, arising out of or otherwise in connection
with the use or inability to use the program or any product, material
or result produced or otherwise obtained by using the program,
regardless of whether they have been advised of, or otherwise had
knowledge of, the possibility of such damages at any time during the
project or thereafter.  Each user will be deemed to have agreed to the
foregoing by his or her commencement of use of the program.  The term
"use" as used herein includes, but is not limited to, the use,
modification, copying and distribution of the program and the
production of secondary products from the program.

In the case where the program, whether in its original form or
modified, was distributed or delivered to or received by a user from
any person, organization or entity other than ICOT, unless it makes or
grants independently of ICOT any specific warranty to the user in
writing, such person, organization or entity, will also be exempted
from and not be held liable to the user for any such damages as noted
above as far as the program is concerned.
```

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

## Root certificates (jpdb.io and GitHub connections)

HTTPS to jpdb.io, and to GitHub for the update check, is checked against Mozilla's list of root certificates,
bundled through the `webpki-roots` crate (CDLA-Permissive-2.0,
<https://github.com/rustls/webpki-roots>; the list itself comes from
Mozilla's CA Certificate Program through the Common CA Database,
<https://www.ccadb.org/>).

## egui default fonts

egui's built-in fonts are included through `epaint_default_fonts`:

- Ubuntu Light — Ubuntu Font Licence 1.0, © Canonical Ltd.
- Hack — MIT, © Source Foundry Authors; Bitstream Vera licence for the parts
  derived from Bitstream Vera; DejaVu changes in the public domain.
- Noto Emoji — SIL Open Font License 1.1, © Google Inc.
- emoji-icon-font — MIT, © 2014 John Slegers.

## AppImage runtime (Linux download)

The Linux AppImage starts through the AppImage type 2 runtime
(<https://github.com/AppImage/type2-runtime>, MIT), added by appimagetool. It
is statically linked with libfuse 3 (LGPL-2.1-or-later,
<https://github.com/libfuse/libfuse>), squashfuse (BSD-2-Clause,
<https://github.com/vasi/squashfuse>), zstd (BSD-3-Clause,
<https://github.com/facebook/zstd>) and zlib (Zlib). The runtime's source,
and the means to relink it, are at those addresses; the app itself can be
taken out of the AppImage with `--appimage-extract`.

## Rust crates

Every crate in `Cargo.lock` is under a licence on the allow-list in
`deny.toml`, checked by `cargo deny check licenses` in CI.
