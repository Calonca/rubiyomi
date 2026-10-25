// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ruby {
    pub start: u32,
    pub len: u32,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextBlock {
    pub text: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub heading: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ruby: Vec<Ruby>,
}

impl TextBlock {
    pub fn plain(text: impl Into<String>) -> Self {
        TextBlock { text: text.into(), heading: false, ruby: Vec::new() }
    }

    pub fn char_len(&self) -> usize {
        self.text.chars().count()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageBlock {
    pub file: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Block {
    Text(TextBlock),
    Image(ImageBlock),
}

impl Block {
    pub fn as_text(&self) -> Option<&TextBlock> {
        match self {
            Block::Text(t) => Some(t),
            Block::Image(_) => None,
        }
    }

    pub fn char_len(&self) -> usize {
        match self {
            Block::Text(t) => t.char_len(),
            Block::Image(_) => 1,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chapter {
    pub title: String,
    pub blocks: Vec<Block>,
}

impl Chapter {
    pub fn char_len(&self) -> usize {
        self.blocks.iter().map(Block::char_len).sum()
    }

    pub fn locate(&self, offset: usize) -> (usize, usize) {
        let mut rest = offset;
        for (i, b) in self.blocks.iter().enumerate() {
            let len = b.char_len();
            if rest < len {
                return (i, rest);
            }
            rest -= len;
        }
        match self.blocks.len() {
            0 => (0, 0),
            n => (n - 1, self.blocks[n - 1].char_len()),
        }
    }

    pub fn block_start(&self, block: usize) -> usize {
        self.blocks.iter().take(block).map(Block::char_len).sum()
    }

    pub fn text_chars(&self) -> usize {
        self.blocks.iter().filter_map(Block::as_text).map(|t| t.text.chars().filter(|c| !c.is_whitespace()).count()).sum()
    }
}
