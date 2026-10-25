// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

pub type Insets = [f32; 4];

pub trait Platform: Send + Sync {
    fn accent_color(&self) -> Option<u32>;

    fn dark_mode(&self) -> Option<bool> {
        None
    }

    fn insets(&self) -> Insets;

    fn to_background(&self) -> bool;
}
