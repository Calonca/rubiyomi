// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Morph {
    pub surface: String,
    pub start: usize,
    pub len: usize,
    pub pos: [String; 3],
    pub base: String,
    pub reading: String,
}

impl Morph {
    fn is(&self, pos0: &str) -> bool {
        self.pos[0] == pos0
    }

    fn sub(&self, pos1: &str) -> bool {
        self.pos[1] == pos1
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Merged {
    pub start: usize,
    pub len: usize,
    pub base: String,
    pub reading: String,
    pub pos: String,
}

fn bends(head: &Morph, suru: bool) -> bool {
    suru || head.is("動詞") || head.is("形容詞") || head.is("助動詞")
}

fn joins(head: &Morph, last: &Morph, suru: bool, m: &Morph) -> bool {
    let bending = bends(head, suru);
    match m.pos[0].as_str() {
        "助動詞" => bending && !(matches!(m.base.as_str(), "だ" | "です") && head.is("名詞") && !suru),
        "動詞" => bending && (m.sub("非自立") || m.sub("接尾")),
        "助詞" => bending && m.sub("接続助詞") && matches!(m.base.as_str(), "て" | "で" | "ば") && !last.is("助詞"),
        "形容詞" => bending && (m.sub("非自立") || m.sub("接尾")),
        "名詞" => bending && m.sub("接尾") && m.pos[2] == "助動詞語幹",
        _ => false,
    }
}

pub fn merge(morphs: &[Morph]) -> Vec<Merged> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < morphs.len() {
        let head = &morphs[i];
        if head.is("記号") || head.surface.trim().is_empty() {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        let mut suru = false;
        while j < morphs.len() {
            let m = &morphs[j];
            if !suru && j == i + 1 && head.is("名詞") && head.sub("サ変接続") && m.is("動詞") && m.base == "する" {
                suru = true;
                j += 1;
                continue;
            }
            if !joins(head, &morphs[j - 1], suru, m) {
                break;
            }
            j += 1;
        }
        let group = &morphs[i..j];
        let base = if suru {
            format!("{}する", head.surface)
        } else if head.base.is_empty() || head.base == "*" {
            head.surface.clone()
        } else {
            head.base.clone()
        };
        let reading =
            if group.iter().all(|m| !m.reading.is_empty()) { group.iter().map(|m| m.reading.as_str()).collect() } else { String::new() };
        let last = &group[group.len() - 1];
        out.push(Merged { start: head.start, len: last.start + last.len - head.start, base, reading, pos: head.pos[0].clone() });
        i = j;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(ms: &[(&str, [&str; 3], &str)]) -> Vec<(String, String)> {
        let mut at = 0;
        let morphs: Vec<Morph> = ms
            .iter()
            .map(|(surface, pos, base)| {
                let len = surface.chars().count();
                at += len;
                Morph {
                    surface: (*surface).into(),
                    start: at - len,
                    len,
                    pos: pos.map(str::to_owned),
                    base: (*base).into(),
                    reading: String::new(),
                }
            })
            .collect();
        let text: Vec<char> = morphs.iter().flat_map(|m| m.surface.chars()).collect();
        merge(&morphs).into_iter().map(|w| (text[w.start..w.start + w.len].iter().collect(), w.base)).collect()
    }

    fn pairs(p: &[(&str, &str)]) -> Vec<(String, String)> {
        p.iter().map(|(a, b)| ((*a).into(), (*b).into())).collect()
    }

    // Phrases from 芥川龍之介「羅生門」 and 夏目漱石「吾輩は猫である」, public domain, on Aozora Bunko:
    // https://www.aozora.gr.jp/cards/000879/files/127_15260.html
    // https://www.aozora.gr.jp/cards/000148/files/789_14547.html
    #[test]
    fn bent_verbs_join_and_the_copula_after_a_noun_stays_apart() {
        let waiting = words(&[
            ("雨やみ", ["名詞", "一般", "*"], "雨やみ"),
            ("を", ["助詞", "格助詞", "一般"], "を"),
            ("待っ", ["動詞", "自立", "*"], "待つ"),
            ("て", ["助詞", "接続助詞", "*"], "て"),
            ("い", ["動詞", "非自立", "*"], "いる"),
            ("た", ["助動詞", "*", "*"], "た"),
            ("。", ["記号", "句点", "*"], "。"),
        ]);
        assert_eq!(waiting, pairs(&[("雨やみ", "雨やみ"), ("を", "を"), ("待っていた", "待つ")]));
        let cat = words(&[
            ("吾輩", ["名詞", "代名詞", "一般"], "吾輩"),
            ("は", ["助詞", "係助詞", "*"], "は"),
            ("猫", ["名詞", "一般", "*"], "猫"),
            ("で", ["助動詞", "*", "*"], "だ"),
            ("ある", ["助動詞", "*", "*"], "ある"),
            ("。", ["記号", "句点", "*"], "。"),
        ]);
        assert_eq!(cat, pairs(&[("吾輩", "吾輩"), ("は", "は"), ("猫", "猫"), ("である", "だ")]));
        let suru = words(&[
            ("記憶", ["名詞", "サ変接続", "*"], "記憶"),
            ("し", ["動詞", "自立", "*"], "する"),
            ("て", ["助詞", "接続助詞", "*"], "て"),
            ("いる", ["動詞", "非自立", "*"], "いる"),
        ]);
        assert_eq!(suru, pairs(&[("記憶している", "記憶する")]));
    }
}
