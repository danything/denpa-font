//! palt (字の墨の幅に合わせて詰める OpenType の機能) の GPOS を、出来た字の墨の枠から作る。
//!
//! 番組名・番組説明を表示するときだけ詰めるためのもの (字幕・データ放送では使わない)。
//! 既定の送り幅 (hmtx) は変えない。palt を有効にしたときだけ、墨の左右に SIDE ずつ残して詰める。
//! 詰めるのは全角の英数字と記号 (U+FF01–FF5E) だけ。かな・漢字・和文の約物は詰めない。

use crate::err::Result;
use crate::sfnt::{i16_at, Glyphs};
use std::collections::BTreeMap;
use std::ops::RangeInclusive;
use write_fonts::tables::gpos::{Gpos, PositionLookup, PositionLookupList, SinglePos, ValueRecord};
use write_fonts::tables::layout::{
    CoverageTable, Feature, FeatureList, FeatureRecord, LangSys, Lookup, LookupFlag, Script, ScriptList, ScriptRecord,
};
use write_fonts::types::{GlyphId16, Tag};

/// 詰める字
pub const RANGE: RangeInclusive<u32> = 0xFF01..=0xFF5E;
/// 全角の送り幅
pub const FULL: i32 = 1024;
/// 墨の左右に残す余白 (字の間は 2 倍の 160)。半角の英字の左右の余白 (平均 50 ほど) を、
/// 全角の英字が半角の 1.5 倍ほど幅広い分だけ広げたもの
pub const SIDE: i32 = 80;
/// 詰めたあとの送り幅の下限 (em の 5/16。ｉ ． ： ｜ のような細い字が詰まり過ぎないように)
pub const MIN_ADVANCE: i32 = 320;
/// palt を引ける用字系 (タグの順)。用字系の無い記号は前後の字の用字系になるので、和文の中でも効くようにする
pub const SCRIPTS: [&[u8; 4]; 4] = [b"DFLT", b"hani", b"kana", b"latn"];

/// 1 字の詰め (xPlacement, xAdvance)。墨は新しい送り幅の真ん中に置く。
/// 詰めない (詰めても全角に届く・墨が無い・全角でない) なら None
pub fn adjust(x_min: i32, x_max: i32, advance: i32) -> Option<(i16, i16)> {
    if advance != FULL || x_max <= x_min {
        return None;
    }
    let ink = x_max - x_min;
    let a = (ink + 2 * SIDE).max(MIN_ADVANCE);
    if a >= FULL {
        return None;
    }
    let lsb = (a - ink) / 2;
    Some(((lsb - x_min) as i16, (a - FULL) as i16))
}

/// palt の GPOS (詰める字が無ければ None)。cmap は Unicode → 字の番号
pub fn gpos(glyphs: &Glyphs, cmap: &BTreeMap<u32, u32>) -> Result<Option<Gpos>> {
    let mut adj: BTreeMap<u16, (i16, i16)> = BTreeMap::new();
    for c in RANGE {
        let Some(&g) = cmap.get(&c) else { continue };
        let d = &glyphs.data[g as usize];
        // 形の無い字 (空白) は墨の枠も無い
        let (x0, x1) = if d.is_empty() { (0, 0) } else { (i16_at(d, 2) as i32, i16_at(d, 6) as i32) };
        if let Some(a) = adjust(x0, x1, glyphs.advance[g as usize] as i32) {
            adj.insert(u16::try_from(g).map_err(|_| format!("字の番号 {g} が大き過ぎます"))?, a);
        }
    }
    if adj.is_empty() {
        return Ok(None);
    }
    // 詰めは字ごとなので、同じ字を使うほかの Unicode (parts.txt の -> など) にも効いてしまう。そういう字があれば止める
    let shared: Vec<String> = cmap
        .iter()
        .filter(|&(c, g)| !RANGE.contains(c) && u16::try_from(*g).is_ok_and(|g| adj.contains_key(&g)))
        .map(|(c, _)| format!("U+{c:04X}"))
        .collect();
    if !shared.is_empty() {
        return Err(format!("palt で詰める字を、全角の英数字と記号のほかの字も使っています: {}", shared.join(" ")).into());
    }
    // 字の番号の順 (BTreeMap) に並べる。Coverage も同じ順になる
    let coverage: CoverageTable = adj.keys().map(|&g| GlyphId16::new(g)).collect();
    let records = adj.values().map(|&(p, a)| ValueRecord::new().with_x_placement(p).with_x_advance(a)).collect();
    let lookup = Lookup::new(LookupFlag::empty(), vec![SinglePos::format_2(coverage, records)]);
    let lookups = PositionLookupList::new(vec![PositionLookup::Single(lookup)]);
    let features = FeatureList::new(vec![FeatureRecord::new(Tag::new(b"palt"), Feature::new(None, vec![0]))]);
    let scripts = ScriptList::new(
        SCRIPTS
            .iter()
            .map(|t| ScriptRecord::new(Tag::new(t), Script::new(Some(LangSys::new(vec![0])), vec![])))
            .collect(),
    );
    Ok(Some(Gpos::new(scripts, features, lookups)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ink_is_centered_with_side() {
        // Ｈ (墨 192〜833): 送り幅は墨 641 + 左右 80 ずつ = 801、墨は 80 から
        let (p, a) = adjust(192, 833, FULL).unwrap();
        assert_eq!((p, a), (80 - 192, 801 - 1024));
        // （ (墨 656〜952、右寄り): 左へ寄せて墨の左右に 80
        let (p, a) = adjust(656, 952, FULL).unwrap();
        assert_eq!(656 + p as i32, 80);
        assert_eq!(FULL + a as i32, 296 + 160);
    }

    #[test]
    fn thin_glyphs_keep_min_advance() {
        // ｉ (墨 456〜568) は下限の 320 にして真ん中に置く
        let (p, a) = adjust(456, 568, FULL).unwrap();
        assert_eq!(FULL + a as i32, MIN_ADVANCE);
        let lsb = 456 + p as i32;
        let rsb = MIN_ADVANCE - (568 + p as i32);
        assert!(lsb >= SIDE && (lsb - rsb).abs() <= 1, "{lsb} {rsb}");
    }

    #[test]
    fn wide_or_halfwidth_glyphs_are_left_alone() {
        // 墨が全角いっぱいの ＿ ・墨と余白で全角を超える Ｗ ／ は詰めない。半角の字は相手にしない
        assert_eq!(adjust(0, 1024, FULL), None);
        assert_eq!(adjust(21, 1003, FULL), None);
        assert_eq!(adjust(49, 975, FULL), None);
        assert_eq!(adjust(100, 400, 512), None);
        assert_eq!(adjust(0, 0, FULL), None);
    }
}
