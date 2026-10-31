//! ccmp (字を組む OpenType の機能) の GSUB。2つ以上のコードポイントで 1 字の字 (JIS X 0213 の か゚・æ̀・˩˥ など) を、
//! 元のフォント (BIZ UDゴシック) の ccmp の合字と同じ形の 1 字にする。
//!
//! 元の合字は元のフォントの GSUB から読み (read)、出来た字を並べてこちらの GSUB を作る (gsub)。
//! 字幕は 1 区画に並びをまとめて描き、番組名などの文はブラウザが組む。

use crate::err::{Ctx, Result};
use crate::palt::SCRIPTS;
use std::collections::BTreeMap;
use skrifa::MetadataProvider;
use write_fonts::read::tables::gsub::{ExtensionSubtable, LigatureSubst as ReadLigatures, SubstitutionLookup as ReadLookup};
use write_fonts::read::{FontRef, ReadError, TableProvider};
use write_fonts::tables::gsub::{Gsub, Ligature, LigatureSet, LigatureSubst, SubstitutionLookup, SubstitutionLookupList};
use write_fonts::tables::layout::{CoverageTable, Feature, FeatureList, FeatureRecord, LangSys, Lookup, LookupFlag, Script, ScriptList, ScriptRecord};
use write_fonts::types::{GlyphId, GlyphId16, Tag};

/// 元のフォントの ccmp の合字: コードポイントの並び → 字の番号。並びの字がどれも Unicode の割り当てのある字のものだけ
pub fn read(font: &FontRef) -> Result<BTreeMap<Vec<u32>, GlyphId>> {
    let mut code: BTreeMap<GlyphId, u32> = BTreeMap::new();
    for (c, g) in font.charmap().mappings() {
        code.entry(g).or_insert(c);
    }
    let mut out = BTreeMap::new();
    let Ok(gsub) = font.gsub() else { return Ok(out) };
    let features = gsub.feature_list()?;
    let lookups = gsub.lookup_list()?;
    let mut indices = std::collections::BTreeSet::new();
    for rec in features.feature_records() {
        if rec.feature_tag() == Tag::new(b"ccmp") {
            indices.extend(rec.feature(features.offset_data())?.lookup_list_indices().iter().map(|i| i.get()));
        }
    }
    // 24 ビットの字の番号の形 (Format2) は BIZ UDゴシックに無い (字は 65536 より少ない)
    let mut add = |st: ReadLigatures| -> std::result::Result<(), ReadError> {
        let ReadLigatures::Format1(st) = st else { return Ok(()) };
        for (first, set) in st.coverage()?.iter().zip(st.ligature_sets().iter()) {
            for lig in set?.ligatures().iter() {
                let lig = lig?;
                let seq: Option<Vec<u32>> = std::iter::once(first)
                    .chain(lig.component_glyph_ids().iter().map(|g| GlyphId::from(g.get())))
                    .map(|g| code.get(&g).copied())
                    .collect();
                if let Some(seq) = seq {
                    out.entry(seq).or_insert(GlyphId::from(lig.ligature_glyph()));
                }
            }
        }
        Ok(())
    };
    for i in indices {
        match lookups.lookups().get(i as usize)? {
            ReadLookup::Ligature(l) => {
                for st in l.subtables().iter() {
                    add(st?)?;
                }
            }
            ReadLookup::Extension(l) => {
                for st in l.subtables().iter() {
                    if let ExtensionSubtable::Ligature(e) = st? {
                        add(e.extension()?)?;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

/// ccmp の GSUB (組む字が無ければ None)。cmap は Unicode → 字の番号、ligatures は (並び, 組んだ字の番号)
pub fn gsub(cmap: &BTreeMap<u32, u32>, ligatures: &[(Vec<u32>, u32)]) -> Result<Option<Gsub>> {
    if ligatures.is_empty() {
        return Ok(None);
    }
    let gid = |g: u32| u16::try_from(g).map_err(|_| format!("字の番号 {g} が大き過ぎます"));
    // 1 字目 → (残りの字, 組んだ字)。字の番号の順 (BTreeMap) に並べる。Coverage も同じ順になる
    let mut sets: BTreeMap<u16, Vec<(Vec<u16>, u16)>> = BTreeMap::new();
    for (seq, lig) in ligatures {
        let mut gs = vec![];
        for c in seq {
            gs.push(gid(*cmap.get(c).ctx(|| format!("組む字 U+{c:04X} がフォントにありません"))?)?);
        }
        sets.entry(gs[0]).or_default().push((gs[1..].to_vec(), gid(*lig)?));
    }
    // 組の中では先に合ったものを使うので、長い並びを先に置く (短い並びが長い並びの頭と同じでも長いほうが組める)
    for v in sets.values_mut() {
        v.sort_by_key(|(rest, _)| std::cmp::Reverse(rest.len()));
    }
    let coverage: CoverageTable = sets.keys().map(|&g| GlyphId16::new(g)).collect();
    let sets = sets
        .values()
        .map(|v| LigatureSet::new(v.iter().map(|(rest, g)| Ligature::new(GlyphId16::new(*g), rest.iter().map(|&r| GlyphId16::new(r)).collect())).collect()))
        .collect();
    let lookup = Lookup::new(LookupFlag::empty(), vec![LigatureSubst::format_1(coverage, sets)]);
    let lookups = SubstitutionLookupList::new(vec![SubstitutionLookup::Ligature(lookup)]);
    let features = FeatureList::new(vec![FeatureRecord::new(Tag::new(b"ccmp"), Feature::new(None, vec![0]))]);
    let scripts = ScriptList::new(
        SCRIPTS
            .iter()
            .map(|t| ScriptRecord::new(Tag::new(t), Script::new(Some(LangSys::new(vec![0])), vec![])))
            .collect(),
    );
    Ok(Some(Gsub::new(scripts, features, lookups)))
}
