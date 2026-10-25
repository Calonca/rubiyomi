// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use rubiyomi_core::platform::{Insets, Platform};

pub struct Desktop;

impl Platform for Desktop {
    fn accent_color(&self) -> Option<u32> {
        None
    }

    fn insets(&self) -> Insets {
        [0.0; 4]
    }

    fn to_background(&self) -> bool {
        false
    }
}
