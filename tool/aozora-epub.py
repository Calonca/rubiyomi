#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
# Copyright (C) 2026 Alessandro La Conca
"""Turns an Aozora Bunko XHTML file (public domain works only) into a test EPUB.

usage: aozora-epub.py IN.html OUT.epub [--dense]
--dense adds a made-up reading on every run of kanji without one: the worst case for furigana.
"""
import re
import sys
import zipfile

src, out = sys.argv[1], sys.argv[2]
dense = "--dense" in sys.argv
raw = open(src, "rb").read()
html = raw.decode("cp932", "replace") if b'Shift_JIS' in raw[:400] else raw.decode("utf-8", "replace")
title = re.search(r'<h1 class="title">(.*?)</h1>', html, re.S).group(1)
author = re.search(r'<h2 class="author">(.*?)</h2>', html, re.S).group(1)
body = re.search(r'<div class="main_text">(.*?)<div class="bibliographical_information">', html, re.S).group(1)
body = re.sub(r'<img[^>]*>', '', body)
body = re.sub(r'<a [^>]*>|</a>', '', body)
parts = re.split(r'(<h[1-6][^>]*>.*?</h[1-6]>)', body, flags=re.S)
chapters = []
for piece in parts:
    if piece.startswith('<h'):
        chapters.append([re.sub(r'<[^>]+>', '', piece).strip(), ''])
    elif chapters:
        chapters[-1][1] += piece
    elif piece.strip():
        chapters.append(['', piece])


def paragraphs(text):
    lines = re.split(r'<br\s*/?>', text)
    return ''.join(f'<p>{line.strip()}</p>\n' for line in lines if re.sub(r'<[^>]+>', '', line).strip())


def more_ruby(text):
    def word(m):
        return f'<ruby><rb>{m[0]}</rb><rt>{"かな" * len(m[0])}</rt></ruby>'
    pieces = re.split(r'(<ruby>.*?</ruby>|<[^>]+>)', text, flags=re.S)
    return ''.join(p if i % 2 else re.sub(r'[㐀-鿿々]+', word, p) for i, p in enumerate(pieces))


name = ("Dense " if dense else "") + title
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
    z.writestr(zipfile.ZipInfo("mimetype"), "application/epub+zip", zipfile.ZIP_STORED)
    z.writestr("META-INF/container.xml", '<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>')
    manifest, spine, nav = '', '', ''
    for i, (head, text) in enumerate(chapters):
        text = paragraphs(text)
        if dense:
            text = more_ruby(text)
        z.writestr(f"c{i}.xhtml", f'<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml" lang="ja"><head><title>{head}</title></head><body><h2>{head}</h2>{text}</body></html>')
        manifest += f'<item id="c{i}" href="c{i}.xhtml" media-type="application/xhtml+xml"/>'
        spine += f'<itemref idref="c{i}"/>'
        nav += f'<li><a href="c{i}.xhtml">{head}</a></li>'
    z.writestr("nav.xhtml", f'<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>Contents</title></head><body><nav epub:type="toc"><ol>{nav}</ol></nav></body></html>')
    z.writestr("book.opf", f'<?xml version="1.0" encoding="utf-8"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">aozora-789{"-dense" if dense else ""}</dc:identifier><dc:title>{name}</dc:title><dc:creator>{author}</dc:creator><dc:language>ja</dc:language></metadata><manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>{manifest}</manifest><spine>{spine}</spine></package>')
print(name, len(chapters), [len(re.sub(r'<rt>.*?</rt>|<[^>]+>', '', c[1])) for c in chapters], sum(c[1].count('<ruby') for c in chapters))
