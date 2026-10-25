// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use rubiyomi_core::settings::PageTheme;
use rubiyomi_core::theme::{Color, Scheme, scheme_from_seed};

struct Palette {
    dark: bool,
    background: u32,
    surface: u32,
    text: u32,
    dim: u32,
    accent: u32,
    border: u32,
}

fn palette(theme: PageTheme) -> Option<Palette> {
    let p = |dark, background, surface, text, dim, accent, border| Palette { dark, background, surface, text, dim, accent, border };
    Some(match theme {
        PageTheme::System => return None,
        PageTheme::Light => p(false, 0xfdfdfd, 0xf2f2f3, 0x1b1b1d, 0x65676b, 0x2f6fed, 0xdfe0e3),
        PageTheme::Sepia => p(false, 0xf6efe0, 0xeee4d1, 0x3b332a, 0x7d6f5d, 0x9a6b3f, 0xddd0b8),
        PageTheme::Grey => p(true, 0x3b3d40, 0x45484c, 0xe3e4e6, 0xa9acb1, 0x8ab4f8, 0x54585d),
        PageTheme::Dark => p(true, 0x16181c, 0x1f2227, 0xdfe2e6, 0x9aa0a8, 0x7aa2f7, 0x2b2f36),
        PageTheme::Black => p(true, 0x000000, 0x0d0d0f, 0xcfd2d6, 0x85898f, 0x6f9bf0, 0x1c1d20),
        PageTheme::Ocean => p(true, 0x0f2b33, 0x14363f, 0xd7e8e6, 0x8fb0ad, 0x54c1b5, 0x1d4650),
    })
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color { r: m(a.r, b.r), g: m(a.g, b.g), b: m(a.b, b.b) }
}

// The colours of the page, its bars and its word card.
pub fn scheme(theme: PageTheme, system: &Scheme) -> Scheme {
    let Some(p) = palette(theme) else { return *system };
    let mut s = scheme_from_seed(p.accent, p.dark);
    let (background, surface, text) = (Color::from_rgb(p.background), Color::from_rgb(p.surface), Color::from_rgb(p.text));
    s.primary = Color::from_rgb(p.accent);
    s.surface = background;
    s.surface_container_low = mix(background, surface, 0.5);
    s.surface_container = surface;
    s.surface_container_high = mix(surface, text, 0.05);
    s.surface_container_highest = mix(surface, text, 0.1);
    s.on_surface = text;
    s.on_surface_variant = Color::from_rgb(p.dim);
    s.outline_variant = Color::from_rgb(p.border);
    s
}

pub fn swatch(theme: PageTheme, system: &Scheme) -> (Color, Color) {
    let s = scheme(theme, system);
    (s.surface, s.on_surface)
}
