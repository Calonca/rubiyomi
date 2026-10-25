// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Alessandro La Conca

const NO_START: &str = "、。，．,.）］｝〕〉》」』】〗〙〛｠)]}｣’”»ヽヾゝゞ々〻ー‐゠–〜～？！?!‼⁇⁈⁉:;：；・…‥｡､";

const NO_END: &str = "（［｛〔〈《「『【〖〘〚｟([{｢‘“«";

const SMALL_KANA: &str = "ぁぃぅぇぉっゃゅょゎゕゖァィゥェォッャュョヮヵヶㇰㇱㇲㇳㇴㇵㇶㇷㇸㇹㇺㇻㇼㇽㇾㇿ";

const HANGING: &str = "、。，．,.､｡";

const ROTATED_IN_COLUMN: &str = "ー－―‐-–—〜～…‥（）「」『』【】〔〕〈〉《》［］｛｝()[]{}<>＜＞＝=→←⇒⇔：:;；｜|／/〖〗〘〙〚〛｟｠｢｣‘’“”";

const CORNER_IN_COLUMN: &str = "、。，．､｡";

pub fn no_start(c: char) -> bool {
    NO_START.contains(c) || SMALL_KANA.contains(c)
}

pub fn no_end(c: char) -> bool {
    NO_END.contains(c)
}

pub fn hangs(c: char) -> bool {
    HANGING.contains(c)
}

pub fn is_small_kana(c: char) -> bool {
    SMALL_KANA.contains(c)
}

pub fn rotated_in_column(c: char) -> bool {
    ROTATED_IN_COLUMN.contains(c)
}

pub fn corner_in_column(c: char) -> bool {
    CORNER_IN_COLUMN.contains(c)
}

pub fn is_latin(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(c, '&' | '@' | '#' | '%' | '+' | '*' | '_' | '\'' | '"' | '^')
        || ('\u{c0}'..='\u{24f}').contains(&c)
}

pub fn is_space(c: char) -> bool {
    c == ' ' || c == '\u{a0}'
}
