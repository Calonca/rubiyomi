// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

#[cfg(target_os = "android")]
pub mod android;
#[cfg(not(target_os = "android"))]
pub mod desktop;
