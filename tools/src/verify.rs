//! 出来たフォントを確かめる。どれかに引っかかったら終了コード 1。
//!
//! 止めるのは機械で決まるものだけ (1・2・4・5)。3 は一覧と見本を出すだけで、変わってよいかは PR を見る人が決める。
//!
//! 1. 字の揃い: denpa の表から作った字 (build/repertoire.txt) が全部あり (missing.txt の字を除く)、空白のほかは形があること
//!    (カラー絵文字が既定の字も白黒の形を持つ)。missing.txt の字が入ったら missing.txt から消す。送り幅は半角 512 か全角 1024
//! 2. 名前・em
//! 3. 前の版と比べて描き方が変わった字: ヒンティング無し・フォント自身のヒンティング (TrueType の命令)・
//!    自動ヒンティング (FreeType の light 相当) で描いた輪郭 (24・36px) と送り幅で比べ、
//!    一覧を出し、前と今の字形を並べた見本 (build/changes.svg) を書く (止めない)
//! 4. woff2 (基本の字と JIS X 0213 で足された漢字の 2 つ) を解いたもの (CI が woff2_decompress で解く) が、2 つで ttf と同じ字になり
//!    (重ならず、分け方は build の一覧のとおり)、palt は基本のファイルで引け、ccmp の並びがどれかのファイルの中で組めること
//! 5. palt: 全角の英数字と記号 (U+FF01–FF5E) だけを墨の幅から決めた量で詰め、どの用字系からも引けること。
//!    その字の既定の送り幅は全角 1024 (半角の同じ字は 512) のまま

use crate::read_codepoints;
use crate::err::Result;
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, Engine, HintingInstance, HintingOptions, OutlinePen, SmoothMode};
use skrifa::raw::TableProvider;
use skrifa::{FontRef, GlyphId, MetadataProvider};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

fn label(cps: &BTreeSet<u32>) -> String {
    let limit = 200;
    let mut s: Vec<String> = cps
        .iter()
        .take(limit)
        .map(|&c| format!("U+{c:04X}({})", char::from_u32(c).unwrap_or('?')))
        .collect();
    if cps.len() > limit {
        s.push(format!("ほか {} 字", cps.len() - limit));
    }
    s.join(" ")
}

#[derive(Default)]
struct Rec(Vec<i64>);
impl OutlinePen for Rec {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.extend([0, (x * 64.0) as i64, (y * 64.0) as i64]);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.extend([1, (x * 64.0) as i64, (y * 64.0) as i64]);
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        self.0.extend([2, (cx * 64.0) as i64, (cy * 64.0) as i64, (x * 64.0) as i64, (y * 64.0) as i64]);
    }
    fn curve_to(&mut self, a: f32, b: f32, c: f32, d: f32, x: f32, y: f32) {
        self.0.extend([3, (a * 64.0) as i64, (b * 64.0) as i64, (c * 64.0) as i64, (d * 64.0) as i64, (x * 64.0) as i64, (y * 64.0) as i64]);
    }
    fn close(&mut self) {
        self.0.push(4);
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Unhinted,
    Interpreter,
    Auto,
}

/// 字ごとの「描いた輪郭 + 送り幅」(大きさ・描き方ごと)
fn drawings(data: &[u8], cps: &[u32], modes: &[Mode]) -> Result<BTreeMap<u32, Vec<Vec<i64>>>> {
    let font = FontRef::new(data)?;
    let charmap = font.charmap();
    let outlines = font.outline_glyphs();
    let metrics = font.glyph_metrics(Size::unscaled(), LocationRef::default());
    let mut out: BTreeMap<u32, Vec<Vec<i64>>> = BTreeMap::new();
    let mut insts = vec![];
    for &ppem in &[24.0f32, 36.0] {
        for &m in modes {
            let inst = match m {
                Mode::Unhinted => None,
                Mode::Interpreter => Some(HintingInstance::new(
                    &outlines,
                    Size::new(ppem),
                    LocationRef::default(),
                    HintingOptions { engine: Engine::Interpreter, target: SmoothMode::Normal.into() },
                )?),
                Mode::Auto => Some(HintingInstance::new(
                    &outlines,
                    Size::new(ppem),
                    LocationRef::default(),
                    HintingOptions { engine: Engine::Auto(None), target: SmoothMode::Light.into() },
                )?),
            };
            insts.push((ppem, inst));
        }
    }
    for &c in cps {
        let Some(gid) = charmap.map(c) else { continue };
        let glyph = outlines.get(gid).ok_or_else(|| format!("glyph {gid} がありません"))?;
        let mut v = vec![vec![metrics.advance_width(gid).unwrap_or(0.0) as i64]];
        for (ppem, inst) in &insts {
            let mut pen = Rec::default();
            let settings = match inst {
                None => DrawSettings::unhinted(Size::new(*ppem), LocationRef::default()),
                Some(i) => DrawSettings::hinted(i, false),
            };
            glyph.draw(settings, &mut pen)?;
            v.push(pen.0);
        }
        out.insert(c, v);
    }
    Ok(out)
}

fn differing(a: &[u8], b: &[u8], cps: &[u32], modes: &[Mode]) -> Result<BTreeSet<u32>> {
    let da = drawings(a, cps, modes)?;
    let db = drawings(b, cps, modes)?;
    Ok(cps.iter().copied().filter(|c| da.get(c) != db.get(c)).collect())
}

pub fn run(font_path: &Path, prev_path: Option<&Path>, woff2_ttf: &[&Path]) -> Result<()> {
    let data = std::fs::read(font_path)?;
    let font = FontRef::new(&data)?;
    let mut errors: Vec<String> = vec![];
    let cmap: BTreeMap<u32, GlyphId> = font.charmap().mappings().collect();
    let have: BTreeSet<u32> = cmap.keys().copied().collect();
    let rep: BTreeSet<u32> = read_codepoints("build/repertoire.txt")?.into_iter().collect();
    let missing: BTreeSet<u32> = read_codepoints("missing.txt")?.into_iter().collect();

    // 1. 字の揃い
    let lack: BTreeSet<u32> = rep.difference(&missing).filter(|c| !have.contains(c)).copied().collect();
    if !lack.is_empty() {
        errors.push(format!("repertoire にあるのに無い字 {}: {}", lack.len(), label(&lack)));
    }
    let stale: BTreeSet<u32> = missing.intersection(&have).copied().collect();
    if !stale.is_empty() {
        errors.push(format!("missing.txt にあるのに入っている字 (missing.txt から消す): {}", label(&stale)));
    }
    let extra: BTreeSet<u32> = have.difference(&rep).copied().collect();
    if !extra.is_empty() {
        errors.push(format!("repertoire に無い字が入っている {}: {}", extra.len(), label(&extra)));
    }
    println!("字: repertoire {}、入っている {}、無い (missing.txt) {}", rep.len(), rep.intersection(&have).count(), missing.len());
    let outlines = font.outline_glyphs();
    let mut empty = BTreeSet::new();
    for (&c, &gid) in &cmap {
        let space = char::from_u32(c).is_some_and(|ch| ch.is_whitespace());
        let mut pen = Rec::default();
        if let Some(g) = outlines.get(gid) {
            g.draw(DrawSettings::unhinted(Size::unscaled(), LocationRef::default()), &mut pen)?;
        }
        if pen.0.is_empty() && !space {
            empty.insert(c);
        }
    }
    if !empty.is_empty() {
        errors.push(format!("形の無い字: {}", label(&empty)));
    }
    let metrics = font.glyph_metrics(Size::unscaled(), LocationRef::default());
    let odd: BTreeSet<u32> = cmap
        .iter()
        .filter(|&(_, &gid)| !matches!(metrics.advance_width(gid).map(|a| a as i32), Some(512) | Some(1024)))
        .map(|(&c, _)| c)
        .collect();
    if !odd.is_empty() {
        errors.push(format!("送り幅が 512 でも 1024 でもない字 {}: {}", odd.len(), label(&odd)));
    }

    // 2. 名前・em
    let get = |id: skrifa::string::StringId| font.localized_strings(id).english_or_first().map(|s| s.to_string());
    use skrifa::string::StringId as S;
    for (id, want) in [(S::FAMILY_NAME, "Denpa Font"), (S::SUBFAMILY_NAME, "Regular"), (S::FULL_NAME, "Denpa Font"), (S::POSTSCRIPT_NAME, "DenpaFont-Regular")] {
        if get(id).as_deref() != Some(want) {
            errors.push(format!("nameID {} が {want:?} でない: {:?}", id.to_u16(), get(id)));
        }
    }
    for id in [S::COPYRIGHT_NOTICE, S::VERSION_STRING, S::LICENSE_DESCRIPTION, S::LICENSE_URL] {
        if get(id).is_none() {
            errors.push(format!("nameID {} が無い", id.to_u16()));
        }
    }
    let upem = font.head()?.units_per_em();
    if upem != 1024 {
        errors.push(format!("em が 1024 でない: {upem}"));
    }
    println!(
        "名前: {} / {} / {}",
        get(S::FAMILY_NAME).unwrap_or_default(),
        get(S::POSTSCRIPT_NAME).unwrap_or_default(),
        get(S::VERSION_STRING).unwrap_or_default()
    );

    // 3. 前の版
    if let Some(p) = prev_path {
        let prev = std::fs::read(p)?;
        let pfont = FontRef::new(&prev)?;
        let pset: BTreeSet<u32> = pfont.charmap().mappings().map(|(c, _)| c).collect();
        let added: BTreeSet<u32> = have.difference(&pset).copied().collect();
        let removed: BTreeSet<u32> = pset.difference(&have).copied().collect();
        let both: Vec<u32> = have.intersection(&pset).copied().collect();
        let changed = differing(&data, &prev, &both, &[Mode::Unhinted, Mode::Interpreter, Mode::Auto])?;
        println!("前の版から: 増えた字 {}、減った字 {}、描き方が変わった字 {}", added.len(), removed.len(), changed.len());
        for (name, set) in [("増えた", &added), ("減った", &removed), ("変わった", &changed)] {
            if !set.is_empty() {
                println!("  {name}: {}", label(set));
            }
        }
        let shown: BTreeSet<u32> = changed.union(&added).chain(removed.iter()).copied().collect();
        if !shown.is_empty() {
            std::fs::write("build/changes.svg", sample(&prev, &data, &shown)?)?;
            let n = shown.len();
            let part = if n > SAMPLE_LIMIT { format!("{n} 字のうち初めの {SAMPLE_LIMIT} 字") } else { format!("{n} 字") };
            println!("  見本: build/changes.svg (左が前の版、右が今。{part})");
        }
    } else {
        println!("前の版: なし (比べない)");
    }

    // 4. woff2 (解くのは道具の外。CI が woff2_decompress で解いたものを渡す)
    // 基本のファイルと JIS X 0213 で足された漢字のファイル (この順)。2 つで ttf と同じ字になる
    let mut seen: BTreeSet<u32> = BTreeSet::new();
    let mut sequences = BTreeSet::new();
    for (k, p) in woff2_ttf.iter().enumerate() {
        let back = std::fs::read(p)?;
        let part = FontRef::new(&back)?;
        let part_cmap: BTreeMap<u32, GlyphId> = part.charmap().mappings().collect();
        let part_cps: Vec<u32> = part_cmap.keys().copied().collect();
        let twice: BTreeSet<u32> = part_cps.iter().filter(|c| seen.contains(c)).copied().collect();
        if !twice.is_empty() {
            errors.push(format!("woff2 の 2 つのファイルに同じ字 {}: {}", twice.len(), label(&twice)));
        }
        seen.extend(part_cps.iter().copied());
        if k == 1 {
            // 分け方は build が書いた一覧のとおり
            let want: BTreeSet<u32> = read_codepoints(&format!("build/{}.txt", crate::EXTRA))?.into_iter().collect();
            let got: BTreeSet<u32> = part_cps.iter().copied().collect();
            if want != got {
                let d: BTreeSet<u32> = want.symmetric_difference(&got).copied().collect();
                errors.push(format!("{}.woff2 の字が一覧と違う {}: {}", crate::EXTRA, d.len(), label(&d)));
            }
        }
        let d = differing(&data, &back, &part_cps, &[Mode::Unhinted, Mode::Interpreter])?;
        if !d.is_empty() {
            errors.push(format!("woff2 を解くと違う字 {}: {}", d.len(), label(&d)));
        }
        println!("woff2 ({}): 解いて {} 字を比べて違い {}", p.display(), part_cps.len(), d.len());
        // palt は基本のファイルで引ける (詰める字は全部そちら)
        if k == 0 {
            check_palt(&part, &part_cmap, &mut errors)?;
        }
        sequences.extend(crate::ccmp::read(&part)?.into_keys());
    }
    if !woff2_ttf.is_empty() {
        let lack: BTreeSet<u32> = have.difference(&seen).copied().collect();
        if !lack.is_empty() {
            errors.push(format!("woff2 のどちらにも無い字 {}: {}", lack.len(), label(&lack)));
        }
        // ccmp は並びの字と同じファイルの中で組める
        let want: BTreeSet<Vec<u32>> = crate::ccmp::read(&font)?.into_keys().collect();
        if sequences != want {
            errors.push(format!("woff2 で組めない並びがある (ttf は {}、woff2 は {})", want.len(), sequences.len()));
        }
    }

    // 5. palt
    check_palt(&font, &cmap, &mut errors)?;

    for e in &errors {
        println!("NG: {e}");
    }
    if !errors.is_empty() {
        std::process::exit(1);
    }
    Ok(())
}

/// GPOS の palt の詰め (字 → (xPlacement, xAdvance)) と、palt を引けない用字系。palt のほかの機能があれば errors に足す
fn read_palt(font: &FontRef, errors: &mut Vec<String>) -> Result<(BTreeMap<GlyphId, (i32, i32)>, Vec<String>)> {
    use skrifa::raw::tables::gpos::{PositionLookup, SinglePos};
    let mut out = BTreeMap::new();
    let Ok(gpos) = font.gpos() else {
        errors.push("GPOS が無い (palt が無い)".into());
        return Ok((out, vec![]));
    };
    let features = gpos.feature_list()?;
    let lookups = gpos.lookup_list()?;
    let mut palt = BTreeSet::new();
    for (i, rec) in features.feature_records().iter().enumerate() {
        let tag = rec.feature_tag();
        if tag != skrifa::Tag::new(b"palt") {
            errors.push(format!("GPOS に palt のほかの機能がある: {tag}"));
            continue;
        }
        palt.insert(i as u16);
        for li in rec.feature(features.offset_data())?.lookup_list_indices() {
            match lookups.lookups().get(li.get() as usize)? {
                PositionLookup::Single(l) => {
                    for st in l.subtables().iter() {
                        let mut put = |g: GlyphId, v: skrifa::raw::tables::gpos::ValueRecord| {
                            if v.y_placement().unwrap_or(0) != 0 || v.y_advance().unwrap_or(0) != 0 {
                                errors.push(format!("palt で glyph {g} が縦に動く"));
                            }
                            let a = (v.x_placement().unwrap_or(0) as i32, v.x_advance().unwrap_or(0) as i32);
                            if out.insert(g, a).is_some() {
                                errors.push(format!("palt で glyph {g} を 2 度動かす"));
                            }
                        };
                        match st? {
                            SinglePos::Format1(t) => t.coverage()?.iter().for_each(|g| put(g, t.value_record())),
                            SinglePos::Format2(t) => {
                                for (g, v) in t.coverage()?.iter().zip(t.value_records().iter()) {
                                    put(g, v?);
                                }
                            }
                            // Format 3・4 は 1・2 と同じ形で、オフセットだけ 32 ビット
                            SinglePos::Format3(t) => t.coverage()?.iter().for_each(|g| put(g, t.value_record())),
                            SinglePos::Format4(t) => {
                                for (g, v) in t.coverage()?.iter().zip(t.value_records().iter()) {
                                    put(g, v?);
                                }
                            }
                        }
                    }
                }
                _ => errors.push(format!("palt の lookup {} が 1 字ずつの位置合わせ (SinglePos) でない", li.get())),
            }
        }
    }
    let scripts = gpos.script_list()?;
    let mut unreachable = vec![];
    for rec in scripts.script_records() {
        let script = rec.script(scripts.offset_data())?;
        let has = |ls: &skrifa::raw::tables::layout::LangSys| ls.feature_indices().iter().any(|i| palt.contains(&i.get()));
        if !script.default_lang_sys().transpose()?.is_some_and(|ls| has(&ls)) {
            unreachable.push(rec.script_tag().to_string());
        }
        for l in script.lang_sys_records() {
            if !has(&l.lang_sys(script.offset_data())?) {
                unreachable.push(format!("{}/{}", rec.script_tag(), l.lang_sys_tag()));
            }
        }
    }
    if scripts.script_records().iter().all(|r| r.script_tag() != skrifa::Tag::new(b"DFLT")) {
        unreachable.push("DFLT (無い)".into());
    }
    Ok((out, unreachable))
}

/// palt: 全角の英数字と記号 (palt::RANGE) だけを、墨の幅から決めた量 (palt::adjust) で詰めること。
/// 既定の送り幅 (hmtx) は全角 1024・半角 512 のまま (palt を使わない字幕・データ放送の見え方を変えない)
fn check_palt(font: &FontRef, cmap: &BTreeMap<u32, GlyphId>, errors: &mut Vec<String>) -> Result<()> {
    let (got, unreachable) = read_palt(font, errors)?;
    if !unreachable.is_empty() {
        errors.push(format!("palt を引けない用字系: {}", unreachable.join(" ")));
    }
    let metrics = font.glyph_metrics(Size::unscaled(), LocationRef::default());
    let adv = |g: GlyphId| metrics.advance_width(g).unwrap_or(0.0) as i32;
    let mut want = BTreeMap::new();
    let mut odd = BTreeSet::new();
    for c in crate::palt::RANGE {
        let Some(&g) = cmap.get(&c) else { continue };
        let half = cmap.get(&(c - 0xFEE0)).map(|&h| adv(h));
        if adv(g) != crate::palt::FULL || half.is_some_and(|a| a != crate::palt::FULL / 2) {
            odd.insert(c);
        }
        let Some(b) = metrics.bounds(g) else { continue };
        if let Some((p, a)) = crate::palt::adjust(b.x_min as i32, b.x_max as i32, adv(g)) {
            want.insert(g, (p as i32, a as i32));
        }
        // 式 (palt::adjust) とは別に、フォントにある詰めが: 送り幅を下限から全角の間にし、
        // 墨を真ん中に置いて左右に SIDE 以上残すこと
        if let Some(&(p, a)) = got.get(&g) {
            let w = adv(g) + a;
            let (l, r) = (b.x_min as i32 + p, w - (b.x_max as i32 + p));
            let side = crate::palt::SIDE;
            if !(crate::palt::MIN_ADVANCE..crate::palt::FULL).contains(&w) || l < side || r < side || (l - r).abs() > 1 {
                errors.push(format!("palt で U+{c:04X} の詰め方がおかしい (送り幅 {w}・左 {l}・右 {r})"));
            }
        }
    }
    if !odd.is_empty() {
        errors.push(format!("既定の送り幅が全角 1024 (半角の字は 512) でない字: {}", label(&odd)));
    }
    let extra: BTreeSet<u32> = cmap.iter().filter(|&(c, g)| got.contains_key(g) && !crate::palt::RANGE.contains(c)).map(|(&c, _)| c).collect();
    if !extra.is_empty() {
        errors.push(format!("palt で全角の英数字と記号のほかの字も詰める: {}", label(&extra)));
    }
    let unmapped = got.keys().filter(|g| !cmap.values().any(|v| v == *g)).count();
    if unmapped > 0 {
        errors.push(format!("palt で Unicode の割り当ての無い字を {unmapped} 字詰める"));
    }
    let diff: BTreeSet<u32> = cmap
        .iter()
        .filter(|&(c, g)| crate::palt::RANGE.contains(c) && got.get(g) != want.get(g))
        .map(|(&c, _)| c)
        .collect();
    if !diff.is_empty() {
        errors.push(format!("palt の詰めが墨の幅から決めた量と違う字: {}", label(&diff)));
    }
    let width: Vec<i32> = got.values().map(|&(_, a)| crate::palt::FULL + a).collect();
    println!(
        "palt: {} 字を詰める (送り幅 {}〜{})",
        got.len(),
        width.iter().min().unwrap_or(&0),
        width.iter().max().unwrap_or(&0)
    );
    Ok(())
}

const SAMPLE_LIMIT: usize = 600;

/// 輪郭を SVG の path にする (y は上下を返す)
#[derive(Default)]
struct Svg(String);
impl OutlinePen for Svg {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0 += &format!("M{x:.0} {:.0}", -y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0 += &format!("L{x:.0} {:.0}", -y);
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        self.0 += &format!("Q{cx:.0} {:.0} {x:.0} {:.0}", -cy, -y);
    }
    fn curve_to(&mut self, a: f32, b: f32, c: f32, d: f32, x: f32, y: f32) {
        self.0 += &format!("C{a:.0} {:.0} {c:.0} {:.0} {x:.0} {:.0}", -b, -d, -y);
    }
    fn close(&mut self) {
        self.0.push('Z');
    }
}

fn path(data: &[u8], c: u32) -> Result<String> {
    let font = FontRef::new(data)?;
    let Some(gid) = font.charmap().map(c) else { return Ok(String::new()) };
    let mut pen = Svg::default();
    if let Some(g) = font.outline_glyphs().get(gid) {
        g.draw(DrawSettings::unhinted(Size::unscaled(), LocationRef::default()), &mut pen)?;
    }
    Ok(pen.0)
}

/// 前の版と今の字形を並べた見本 (1 行に 8 組、字の下に U+XXXX)
fn sample(prev: &[u8], now: &[u8], cps: &BTreeSet<u32>) -> Result<String> {
    let (cell, cols) = (1100.0, 8usize);
    let n = cps.len().min(SAMPLE_LIMIT);
    let rows = n.div_ceil(cols).max(1);
    let (w, h) = (cell * 2.0 * cols as f64, (cell + 300.0) * rows as f64);
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {h}\" width=\"{}\" height=\"{}\">\n<rect width=\"100%\" height=\"100%\" fill=\"#fff\"/>\n",
        w / 20.0,
        h / 20.0
    );
    for (i, &c) in cps.iter().take(n).enumerate() {
        let (x, y) = ((i % cols) as f64 * cell * 2.0, (i / cols) as f64 * (cell + 300.0));
        for (k, data) in [prev, now].into_iter().enumerate() {
            let fill = if k == 0 { "#888" } else { "#000" };
            out += &format!(
                "<path transform=\"translate({} {})\" fill=\"{fill}\" d=\"{}\"/>\n",
                x + k as f64 * cell + 40.0,
                y + 900.0,
                path(data, c)?
            );
        }
        out += &format!("<text x=\"{}\" y=\"{}\" font-size=\"160\" font-family=\"monospace\">U+{c:04X}</text>\n", x + 40.0, y + 1250.0);
    }
    out += "</svg>\n";
    Ok(out)
}
