//! Denpa Font を作る道具。リポジトリの根で動かす:
//!
//!   cargo run --release --manifest-path tools/Cargo.toml -- build 3.0     元から作る (dist/。build/ に途中のもの)
//!   cargo run ... -- verify dist/denpa-font.ttf [--prev ttf] [--woff2 ttf]   確かめる
//!   cargo run ... -- repertoire                                           収める字の一覧を見る
//!
//! 並べて作るスレッドの数は DENPA_THREADS (無ければ CPU の数)。書き出す先は DENPA_OUT (無ければここ。build/ と dist/)。

mod draw;
mod err;
mod fillet;
mod fit;
mod make;
mod palt;
mod parts;
mod repertoire;
mod round;
mod sfnt;
mod verify;

use err::{bail, Ctx, Result};
use make::{Base, Job};
use repertoire::Pua;
use sfnt::{put_i16, put_u16, Contour, Glyphs, Pt};
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::MetadataProvider;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use write_fonts::read::{FontRef, TableProvider};
use write_fonts::types::{GlyphId, NameId, Tag};

const SRC: &str = "build/src";
/// 元にするフォント (fetch.sh が取ってくる)
const BIZ: &str = "BIZUDGothic-Regular.ttf";
const FAMILY: &str = "Denpa Font";
const PS: &str = "DenpaFont-Regular";
const COPYRIGHT: &str = "Copyright 2022 The BIZ UDGothic Project Authors (https://github.com/googlefonts/morisawa-biz-ud-gothic). \
Copyright (c) 2026 danything.";
const LICENSE: &str = "This Font Software is licensed under the SIL Open Font License, Version 1.1. \
This license is available with a FAQ at: https://openfontlicense.org";
const LICENSE_URL: &str = "https://openfontlicense.org";
const VENDOR_URL: &str = "https://github.com/danything/denpa-font";
/// em (半角は半分)
const UPM: u16 = 1024;
/// 行の高さ (元の Rounded M+ 1m for ARIB と同じ。字幕の見え方を変えない)
const WIN_ASCENT: u16 = 981;
const WIN_DESCENT: u16 = 168;
/// OS/2 の字面の上下 (v2.x と同じ)
const TYPO_ASCENT: i16 = 881;
const TYPO_DESCENT: i16 = -143;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("build") => build(args.get(1).map_or("0.0", String::as_str)),
        Some("verify") => {
            // verify <ttf> [--prev 前の版の ttf] [--woff2 woff2 を解いた ttf]
            let opt = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(Path::new);
            if args.len() < 2 {
                bail!("verify <ttf> [--prev ttf] [--woff2 ttf]");
            }
            verify::run(Path::new(&args[1]), opt("--prev"), opt("--woff2"))
        }
        Some("repertoire") => repertoire::run(Path::new(SRC)),
        _ => bail!("build <版> | verify <ttf> [--prev ttf] [--woff2 ttf] | repertoire (先に fetch.sh)"),
    }
}

/// 1行1字の一覧 (U+XXXX。# から後は説明)
pub fn read_codepoints(path: &str) -> Result<Vec<u32>> {
    let text = fs::read_to_string(path).unwrap_or_default();
    let mut out = vec![];
    for line in text.lines() {
        let s = line.split('#').next().unwrap().trim();
        if s.is_empty() {
            continue;
        }
        let h = s.split_whitespace().next().unwrap().trim_start_matches("U+");
        out.push(u32::from_str_radix(h, 16).ctx(|| format!("{path}: {line}"))?);
    }
    Ok(out)
}

/// skrifa の輪郭を TrueType の点の並びに写す
#[derive(Default)]
struct Pen {
    cs: Vec<Contour>,
    cubic: bool,
}

impl OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.cs.push(vec![Pt { x: x as f64, y: y as f64, on: true }]);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        if let Some(c) = self.cs.last_mut() {
            c.push(Pt { x: x as f64, y: y as f64, on: true });
        }
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        if let Some(c) = self.cs.last_mut() {
            c.push(Pt { x: cx as f64, y: cy as f64, on: false });
            c.push(Pt { x: x as f64, y: y as f64, on: true });
        }
    }
    fn curve_to(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: f32) {
        self.cubic = true;
    }
    fn close(&mut self) {
        if let Some(c) = self.cs.last_mut()
            && c.len() > 1
            && c[0] == c[c.len() - 1]
        {
            c.pop();
        }
    }
}

/// 元のフォントの字を em 1024 にして読む (.notdef と、Unicode の割り当てのある字)
fn read_base(font: &FontRef) -> Result<((Vec<Contour>, u16), Base)> {
    let upm = font.head()?.units_per_em() as f32;
    let size = Size::new(UPM as f32);
    let outlines = font.outline_glyphs();
    let metrics = font.glyph_metrics(size, LocationRef::default());
    let get = |gid: GlyphId| -> Result<(Vec<Contour>, u16)> {
        let mut pen = Pen::default();
        if let Some(g) = outlines.get(gid) {
            g.draw(DrawSettings::unhinted(size, LocationRef::default()), &mut pen)?;
        }
        if pen.cubic {
            bail!("glyph {gid} に3次の曲線があります");
        }
        let adv = metrics.advance_width(gid).unwrap_or(0.0);
        if (adv - adv.round()).abs() > 1e-3 {
            bail!("glyph {gid} の送り幅 {adv} が em {upm} の半分で割り切れません");
        }
        Ok((pen.cs, adv.round() as u16))
    };
    let notdef = get(GlyphId::new(0))?;
    let mut base = BTreeMap::new();
    for (c, gid) in font.charmap().mappings() {
        base.insert(c, get(gid)?);
    }
    Ok((notdef, base))
}

fn build(version: &str) -> Result<()> {
    let (major, minor) = version.split_once('.').ctx(|| "版は X.Y".into())?;
    let (major, minor): (u16, u16) = (major.parse()?, minor.parse()?);
    let epoch: i64 = std::env::var("SOURCE_DATE_EPOCH").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
    let src = Path::new(SRC);
    let data = fs::read(src.join(BIZ)).ctx(|| format!("{SRC}/{BIZ} (先に fetch.sh)"))?;
    let font = FontRef::new(&data)?;
    if font.head()?.units_per_em() != 2 * UPM {
        bail!("{BIZ} の em が 2048 でない (半分にして 1024 にする前提)");
    }
    let (notdef, base) = read_base(&font)?;
    let mut repertoire = repertoire::load(src)?;
    let parts = parts::load("data/parts.txt")?;
    // 試すとき: DENPA_ONLY に書いた字だけ作る (parts.txt にあれば、denpa の表にまだ無い字も)
    if let Ok(only) = std::env::var("DENPA_ONLY") {
        repertoire.codepoints.retain(|&c| char::from_u32(c).is_some_and(|ch| only.contains(ch)));
        repertoire.codepoints.extend(only.chars().map(|ch| ch as u32).filter(|c| parts.contains_key(c)));
    }
    let sharp: BTreeSet<u32> = read_codepoints("data/sharp.txt")?.into_iter().collect();

    // 作る字の段取り: 部品を組む (parts.txt) → 元の字 → 描く (draw.rs)。私用領域は同じ区点の字へ
    let mut jobs: Vec<(Job, u16)> = vec![(Job::Keep(notdef.0), notdef.1)];
    let mut cmap: BTreeMap<u32, u32> = BTreeMap::new();
    let mut report = String::from("# BIZ UDゴシックの字をそのまま丸めたもののほかの作り方 (tools の build が書く)\n");
    let ch = |c: u32| char::from_u32(c).unwrap_or('?');
    let mut aliases = vec![];
    let mut missing = vec![];
    for &c in &repertoire.codepoints {
        if repertoire.pua.contains_key(&c) {
            continue;
        }
        let keep = sharp.contains(&c);
        let job = match parts.get(&c) {
            Some(parts::Entry::Alias(t)) => {
                aliases.push((c, *t));
                continue;
            }
            Some(parts::Entry::Parts { advance, parts }) => {
                writeln!(report, "parts   U+{c:04X}  # {}", ch(c))?;
                (Job::Parts(parts.clone(), keep), *advance)
            }
            None => match base.get(&c) {
                Some((s, adv)) if keep || s.is_empty() => (Job::Keep(s.clone()), *adv),
                Some((s, adv)) => (Job::Fillet(s.clone()), *adv),
                None => match draw::draw(c, &base)? {
                    Some(s) => {
                        writeln!(report, "draw    U+{c:04X}  # {}", ch(c))?;
                        (if keep { Job::Keep(s) } else { Job::Raster(s) }, UPM)
                    }
                    None => {
                        missing.push(c);
                        continue;
                    }
                },
            },
        };
        cmap.insert(c, jobs.len() as u32);
        jobs.push(job);
    }
    for (c, t) in aliases {
        match cmap.get(&t) {
            Some(&g) => {
                cmap.insert(c, g);
                writeln!(report, "alias   U+{c:04X} U+{t:04X}  # {}", ch(t))?;
            }
            // 先が収める字に無ければ (DENPA_ONLY で試すときなど)、元の字から作る
            None => match base.get(&t) {
                Some((s, adv)) => {
                    let job = if sharp.contains(&t) || s.is_empty() { Job::Keep(s.clone()) } else { Job::Fillet(s.clone()) };
                    cmap.insert(c, jobs.len() as u32);
                    jobs.push((job, *adv));
                    writeln!(report, "alias   U+{c:04X} U+{t:04X}  # {}", ch(t))?;
                }
                None => bail!("parts.txt: U+{c:04X} の先 U+{t:04X} がフォントにありません"),
            },
        }
    }
    for &c in &repertoire.codepoints {
        match repertoire.pua.get(&c) {
            Some(Pua::Char(t)) => match cmap.get(t) {
                Some(&g) => {
                    cmap.insert(c, g);
                    writeln!(report, "alias   U+{c:04X} U+{t:04X}  # {}", ch(*t))?;
                }
                None => writeln!(report, "# missing U+{c:04X} (→ U+{t:04X} {} も無い)", ch(*t))?,
            },
            Some(Pua::Text(t)) => {
                cmap.insert(c, jobs.len() as u32);
                jobs.push((Job::Parts(make::compose_text(t), false), UPM));
                writeln!(report, "compose U+{c:04X} {t}")?;
            }
            None => {}
        }
    }
    for c in missing {
        writeln!(report, "# missing U+{c:04X} {}", ch(c))?;
    }
    // 書き出す先 (DENPA_OUT。無ければリポジトリの根)。CI は 2 回目を別の所に作って比べる
    let out = std::path::PathBuf::from(std::env::var("DENPA_OUT").unwrap_or_else(|_| ".".into()));
    fs::create_dir_all(out.join("build"))?;
    let rep: Vec<u32> = repertoire.codepoints.iter().copied().collect();
    fs::write(out.join("build/repertoire.txt"), rep.iter().map(|c| format!("U+{c:04X}\n")).collect::<String>())?;

    // 字を作る (並べて)
    let t0 = std::time::Instant::now();
    let threads = std::env::var("DENPA_THREADS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(4, |n| n.get()))
        .max(1);
    let next = std::sync::atomic::AtomicUsize::new(0);
    type Made = std::result::Result<(Vec<u8>, bool), String>;
    let results: Vec<std::sync::Mutex<Option<Made>>> = (0..jobs.len()).map(|_| std::sync::Mutex::new(None)).collect();
    std::thread::scope(|sc| {
        for _ in 0..threads {
            sc.spawn(|| {
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if i >= jobs.len() {
                        break;
                    }
                    let r = make::make(&jobs[i].0, &base).map_err(|e| e.to_string());
                    *results[i].lock().unwrap() = Some(r);
                }
            });
        }
    });
    let mut data_out = vec![];
    let mut rastered = BTreeSet::new();
    for (i, r) in results.into_iter().enumerate() {
        let r = r.into_inner().unwrap().ctx(|| format!("字 {i} が作られていません"))?;
        let (d, fallback) = r.map_err(|e| format!("字 {i}: {e}"))?;
        if fallback {
            rastered.insert(i as u32);
        }
        data_out.push(d);
    }
    for (&c, g) in &cmap {
        if rastered.contains(g) {
            writeln!(report, "raster  U+{c:04X}  # {} (輪郭のまま丸めると輪郭が交わるので格子で丸めた)", ch(c))?;
        }
    }
    fs::write(out.join("build/extras.txt"), &report)?;
    let lsb = data_out.iter().map(|d| if d.is_empty() { 0 } else { sfnt::i16_at(d, 2) }).collect();
    let glyphs = Glyphs { data: data_out, advance: jobs.iter().map(|j| j.1).collect(), lsb };
    eprintln!("{} 字を {:.1} 秒で作った ({threads} スレッド)", glyphs.len(), t0.elapsed().as_secs_f64());

    let ttf = tables(&font, &glyphs, &cmap, major, minor, epoch)?;
    fs::create_dir_all(out.join("dist"))?;
    fs::write(out.join("dist/denpa-font.ttf"), &ttf)?;
    let woff2 = ttf2woff2::encode(&ttf, ttf2woff2::BrotliQuality::from(11u8)).map_err(|e| format!("woff2: {e}"))?;
    fs::write(out.join("dist/denpa-font.woff2"), &woff2)?;
    println!("{}/dist/denpa-font.ttf {} バイト、denpa-font.woff2 {} バイト", out.display(), ttf.len(), woff2.len());
    Ok(())
}

/// 表を組む。元の表から運ぶのは OS/2・post・head・hhea の値だけ (em を半分にした分を直す)。
/// ヒンティングは持たない (fpgm・prep・cvt・gasp を作らない)。縦書きの表と GSUB は持たない。
/// GPOS は palt (全角の英数字と記号を詰める) だけ (palt.rs)
fn tables(base: &FontRef, glyphs: &Glyphs, cmap: &BTreeMap<u32, u32>, major: u16, minor: u16, epoch: i64) -> Result<Vec<u8>> {
    let half = |v: i16| ((v as f64) / 2.0).round() as i16;
    let mut b = write_fonts::FontBuilder::new();
    let gpos = palt::gpos(glyphs, cmap)?;
    if let Some(g) = &gpos {
        b.add_table(g)?;
    }
    let (glyf, loca, hmtx) = glyphs.tables();
    b.add_raw(Tag::new(b"glyf"), glyf);
    b.add_raw(Tag::new(b"loca"), loca);
    b.add_raw(Tag::new(b"hmtx"), hmtx);
    let n = glyphs.len() as u16;

    // 外枠と余白の最小・最大
    let (mut x0, mut y0, mut x1, mut y1) = (i16::MAX, i16::MAX, i16::MIN, i16::MIN);
    let (mut min_lsb, mut min_rsb, mut max_ext) = (i16::MAX, i16::MAX, i16::MIN);
    for (i, d) in glyphs.data.iter().enumerate() {
        if d.is_empty() {
            continue;
        }
        let (a, bb, c, e) = (sfnt::i16_at(d, 2), sfnt::i16_at(d, 4), sfnt::i16_at(d, 6), sfnt::i16_at(d, 8));
        (x0, y0, x1, y1) = (x0.min(a), y0.min(bb), x1.max(c), y1.max(e));
        let ext = glyphs.lsb[i] + (c - a);
        min_lsb = min_lsb.min(glyphs.lsb[i]);
        min_rsb = min_rsb.min(glyphs.advance[i] as i16 - ext);
        max_ext = max_ext.max(ext);
    }

    // maxp 1.0 (命令は無い)
    let (mp, mc) = glyphs.max_points_contours();
    let mut maxp = vec![0u8; 32];
    maxp[0..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    put_u16(&mut maxp, 4, n);
    put_u16(&mut maxp, 6, mp);
    put_u16(&mut maxp, 8, mc);
    put_u16(&mut maxp, 14, 1); // maxZones
    b.add_raw(Tag::new(b"maxp"), maxp);

    let mut head = sfnt::table(base, b"head")?.to_vec();
    let rev = ((major as f64 + minor as f64 / 1000.0) * 65536.0).round() as i32;
    head[4..8].copy_from_slice(&rev.to_be_bytes());
    head[8..12].copy_from_slice(&0u32.to_be_bytes()); // checkSumAdjustment は組むときに入る
    put_u16(&mut head, 16, 0x000B); // flags: y=0 が基線・x=0 が左の余白・ppem は整数 (命令は無い)
    put_u16(&mut head, 18, UPM);
    let mac = epoch + 2_082_844_800; // 1904 年起点
    head[20..28].copy_from_slice(&mac.to_be_bytes());
    head[28..36].copy_from_slice(&mac.to_be_bytes());
    for (o, v) in [(36, x0), (38, y0), (40, x1), (42, y1)] {
        put_i16(&mut head, o, v);
    }
    put_u16(&mut head, 46, 8); // lowestRecPPEM
    put_i16(&mut head, 50, 1); // loca は長い形
    b.add_raw(Tag::new(b"head"), head);

    let mut hhea = sfnt::table(base, b"hhea")?.to_vec();
    put_i16(&mut hhea, 4, WIN_ASCENT as i16);
    put_i16(&mut hhea, 6, -(WIN_DESCENT as i16));
    put_i16(&mut hhea, 8, 0);
    put_u16(&mut hhea, 10, glyphs.advance.iter().copied().max().unwrap_or(UPM));
    for (o, v) in [(12, min_lsb), (14, min_rsb), (16, max_ext)] {
        put_i16(&mut hhea, o, v);
    }
    put_u16(&mut hhea, 34, n);
    b.add_raw(Tag::new(b"hhea"), hhea);

    let mut os2 = sfnt::table(base, b"OS/2")?.to_vec();
    if sfnt::u16_at(&os2, 0) < 2 {
        bail!("OS/2 の版が 2 より前");
    }
    // 平均の字幅は半角の幅 (v2.x と同じ。Windows の GDI はこれを字の幅の目安にする)
    put_i16(&mut os2, 2, (UPM / 2) as i16);
    // 上付き・下付き・打ち消し線・x の高さ・大文字の高さは em を半分にする
    for o in [10, 12, 14, 16, 18, 20, 22, 24, 26, 28, 86, 88] {
        let v = sfnt::i16_at(&os2, o);
        put_i16(&mut os2, o, half(v));
    }
    os2[58..62].copy_from_slice(b"NONE"); // achVendID (元の Morisawa のものは使わない)
    put_i16(&mut os2, 68, TYPO_ASCENT);
    put_i16(&mut os2, 70, TYPO_DESCENT);
    put_i16(&mut os2, 72, 0);
    put_u16(&mut os2, 74, WIN_ASCENT);
    put_u16(&mut os2, 76, WIN_DESCENT);
    let (first, last) = (cmap.keys().next().copied().unwrap_or(0), cmap.keys().last().copied().unwrap_or(0));
    put_u16(&mut os2, 64, first.min(0xFFFF) as u16);
    put_u16(&mut os2, 66, last.min(0xFFFF) as u16);
    put_u16(&mut os2, 94, u16::from(gpos.is_some())); // usMaxContext (palt は 1 字ずつ)
    b.add_raw(Tag::new(b"OS/2"), os2);

    let mut post = sfnt::table(base, b"post")?[..32].to_vec();
    post[0..4].copy_from_slice(&0x0003_0000u32.to_be_bytes()); // 字の名前は持たない
    for o in [8, 10] {
        let v = sfnt::i16_at(&post, o);
        put_i16(&mut post, o, half(v));
    }
    post[12..16].copy_from_slice(&0u32.to_be_bytes()); // isFixedPitch: 全角と半角があるので等幅とは言わない (v2.x と同じ)
    b.add_raw(Tag::new(b"post"), post);

    let cm = write_fonts::tables::cmap::Cmap::from_mappings(
        cmap.iter().filter_map(|(&c, &g)| char::from_u32(c).map(|ch| (ch, GlyphId::new(g)))),
    )
    .map_err(|e| format!("cmap: {e:?}"))?;
    b.add_table(&cm)?;
    let names = [
        (0, COPYRIGHT.to_string()),
        (1, FAMILY.to_string()),
        (2, "Regular".to_string()),
        (3, format!("{PS};{major}.{minor:03}")),
        (4, FAMILY.to_string()),
        (5, format!("Version {major}.{minor:03}")),
        (6, PS.to_string()),
        (11, VENDOR_URL.to_string()),
        (13, LICENSE.to_string()),
        (14, LICENSE_URL.to_string()),
    ];
    let records = names
        .into_iter()
        .map(|(id, s)| write_fonts::tables::name::NameRecord::new(3, 1, 0x409, NameId::new(id), s.into()))
        .collect();
    b.add_table(&write_fonts::tables::name::Name::new(records))?;
    Ok(b.build())
}
