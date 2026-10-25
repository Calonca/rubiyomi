// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlatformEvent {
    Imported { path: PathBuf, name: String, temporary: bool },
    ImportFailed(String),
}

pub type Insets = [f32; 4];

pub trait Platform: Send + Sync {
    fn data_dir(&self) -> PathBuf;

    fn accent_color(&self) -> Option<u32>;

    fn dark_mode(&self) -> Option<bool> {
        None
    }

    fn pick_epub(&self);

    fn poll_events(&self) -> Vec<PlatformEvent>;

    fn set_waker(&self, wake: Box<dyn Fn() + Send + Sync>);

    fn insets(&self) -> Insets;

    fn to_background(&self) -> bool;
}
