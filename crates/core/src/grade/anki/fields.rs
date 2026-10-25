// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

pub fn plain(field: &str) -> String {
    let mut out = String::with_capacity(field.len());
    let mut in_tag = false;
    let mut in_ruby = false;
    let mut entity = String::new();
    let mut in_entity = false;
    for c in field.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if in_tag => {}
            '[' => in_ruby = true,
            ']' if in_ruby => in_ruby = false,
            _ if in_ruby => {}
            '&' => {
                in_entity = true;
                entity.clear();
            }
            ';' if in_entity => {
                in_entity = false;
                out.push_str(match entity.as_str() {
                    "nbsp" => " ",
                    "amp" => "&",
                    "lt" => "<",
                    "gt" => ">",
                    "quot" => "\"",
                    _ => "",
                });
            }
            _ if in_entity => entity.push(c),
            _ => out.push(c),
        }
    }
    out.chars().filter(|c| !c.is_whitespace()).collect()
}
