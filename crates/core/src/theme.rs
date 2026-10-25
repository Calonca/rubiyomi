// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use material_colors::{color::Rgb, theme::ThemeBuilder};

pub const DEFAULT_SEED: u32 = 0xE8A33D;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const fn from_rgb(rgb: u32) -> Self {
        Self { r: (rgb >> 16) as u8, g: (rgb >> 8) as u8, b: rgb as u8 }
    }

    pub const fn to_rgb(self) -> u32 {
        ((self.r as u32) << 16) | ((self.g as u32) << 8) | self.b as u32
    }

    pub fn luminance(self) -> f64 {
        fn channel(c: u8) -> f64 {
            let c = c as f64 / 255.0;
            if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        }
        0.2126 * channel(self.r) + 0.7152 * channel(self.g) + 0.0722 * channel(self.b)
    }

    pub fn contrast(self, other: Color) -> f64 {
        let (a, b) = (self.luminance(), other.luminance());
        let (hi, lo) = if a > b { (a, b) } else { (b, a) };
        (hi + 0.05) / (lo + 0.05)
    }
}

impl From<Rgb> for Color {
    fn from(c: Rgb) -> Self {
        Self { r: c.red, g: c.green, b: c.blue }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scheme {
    pub dark: bool,
    pub primary: Color,
    pub on_primary: Color,
    pub primary_container: Color,
    pub on_primary_container: Color,
    pub secondary_container: Color,
    pub on_secondary_container: Color,
    pub tertiary: Color,
    pub error: Color,
    pub on_error: Color,
    pub surface: Color,
    pub on_surface: Color,
    pub on_surface_variant: Color,
    pub surface_container_low: Color,
    pub surface_container: Color,
    pub surface_container_high: Color,
    pub surface_container_highest: Color,
    pub outline: Color,
    pub outline_variant: Color,
    pub inverse_surface: Color,
    pub inverse_on_surface: Color,
    pub scrim: Color,
}

pub fn scheme_from_seed(seed: u32, dark: bool) -> Scheme {
    let theme = ThemeBuilder::with_source(Rgb::from_u32(seed & 0xFF_FFFF)).build();
    let s = if dark { theme.schemes.dark } else { theme.schemes.light };
    Scheme {
        dark,
        primary: s.primary.into(),
        on_primary: s.on_primary.into(),
        primary_container: s.primary_container.into(),
        on_primary_container: s.on_primary_container.into(),
        secondary_container: s.secondary_container.into(),
        on_secondary_container: s.on_secondary_container.into(),
        tertiary: s.tertiary.into(),
        error: s.error.into(),
        on_error: s.on_error.into(),
        surface: s.surface.into(),
        on_surface: s.on_surface.into(),
        on_surface_variant: s.on_surface_variant.into(),
        surface_container_low: s.surface_container_low.into(),
        surface_container: s.surface_container.into(),
        surface_container_high: s.surface_container_high.into(),
        surface_container_highest: s.surface_container_highest.into(),
        outline: s.outline.into(),
        outline_variant: s.outline_variant.into(),
        inverse_surface: s.inverse_surface.into(),
        inverse_on_surface: s.inverse_on_surface.into(),
        scrim: s.scrim.into(),
    }
}
