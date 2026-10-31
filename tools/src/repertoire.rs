//! 収める字の一覧を、denpa の表から作る。フォント側に表の写しは持たない。
//! 表は fetch.sh が build/src に取ってくる (denpa の版は fetch.sh の DENPA_COMMIT)。ここは読むだけ。
//!
//! 読む表 (build/src/denpa/):
//!   - b24-tables.ts      字幕と番組表の文字の表 (かな・英数・半角記号・JIS の差分・追加記号・DRCS の置き換え先・JIS X 0213 の2つのコードポイントの字・1面の 85〜94区・2面)
//!   - aribtext-gaiji.ts  番組表などの ARIB 外字 (GAIJI。番組表だけで違える EPG_ONLY)
//!   - fold.ts            検索の寄せの表 (FOLD。寄せた先の字)
//!   - ../index-jis0208.txt      EUC-JP の表 (WHATWG。ブラウザの TextDecoder('euc-jp') と同じ字)
//! 表はいずれ 1 つのファイルにまとまるので、表の名前 (HIRAGANA・GAIJI など) をどのファイルからでも探す。
//!
//! 入れるもの: データ放送 (ASCII と、EUC-JP の 1〜84 区)、字幕と番組表 (1〜84 区は EUC-JP と JIS の差分で、
//! JIS X 0213 の1面と同じ字。外字の 85・86・90〜94 区、JIS X 0213 の1面の 85〜94 区と2面、かな・英数・記号)、
//! DRCS の置き換え先、外字の表と寄せた先の字。
//! データ放送は denpa のエンジン (src/lib/bml/euc.ts) が 1〜84 区を TextDecoder('euc-jp') で読み、
//! 85 区から先は字幕と同じ追加記号 (ADDITIONAL) に開く。
//! 2つのコードポイントで 1 字の字 (か゚ など。KANJI_SEQUENCES) は、元の字を入れたうえで 1 字に組む (sequences。GSUB の ccmp)。
//! web フォントは JIS X 0213 にだけある字 (extra) を別のファイルに分ける (docs/repertoire.md)。
//! 追加記号に私用領域の字があれば (92区56〜85点の楽器の略記)、同じ区点の外字の表の字へつなぐ (pua)。
//!
//! TS・JS は実行せず、表のリテラル (文字列・数・配列、// と /* */ の注釈) だけを読む。

use crate::err::{bail, Ctx, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// 私用領域の外字を何で描くか
#[derive(Clone, Debug, PartialEq)]
pub enum Pua {
    /// 同じ区点の Unicode の字と同じ字形
    Char(u32),
    /// Unicode に字が無いもの (92区56〜85点の楽器の略記)。この文字列を 1 マスに並べる
    Text(String),
}

pub struct Repertoire {
    pub codepoints: BTreeSet<u32>,
    pub pua: BTreeMap<u32, Pua>,
    /// 2つ以上のコードポイントで 1 字の字 (JIS X 0213 の か゚・æ̀・˩˥ など)。並びのコードポイントは codepoints にもある
    pub sequences: BTreeSet<Vec<u32>>,
    /// web フォントで別のファイルに分ける字 (JIS X 0213 にだけある字)。codepoints にもある。ほかの字は基本のファイル
    pub extra: BTreeSet<u32>,
}

// ---------------------------------------------------------------- JS のリテラルの読み手

#[derive(Clone, Debug, PartialEq)]
enum Val {
    Str(String),
    Num(i64),
    Arr(Vec<Val>),
}

/// 空白と注釈を飛ばす
fn skip(mut s: &str) -> &str {
    loop {
        s = s.trim_start();
        if let Some(r) = s.strip_prefix("//") {
            s = &r[r.find('\n').unwrap_or(r.len())..];
        } else if let Some(r) = s.strip_prefix("/*") {
            s = &r[r.find("*/").map_or(r.len(), |i| i + 2)..];
        } else {
            return s;
        }
    }
}

/// JS の文字列リテラル (' か ") を読む。返すのは (中身, 残り)
fn js_string(s: &str) -> Option<(String, &str)> {
    let q = s.chars().next()?;
    if q != '\'' && q != '"' {
        return None;
    }
    let mut out = String::new();
    let mut it = s[1..].char_indices();
    while let Some((i, c)) = it.next() {
        match c {
            '\\' => {
                let (_, e) = it.next()?;
                match e {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'u' => {
                        // \uXXXX か \u{X…}
                        let mut hex = String::new();
                        let (_, first) = it.next()?;
                        if first == '{' {
                            for (_, h) in it.by_ref() {
                                if h == '}' {
                                    break;
                                }
                                hex.push(h);
                            }
                        } else {
                            hex.push(first);
                            hex.extend((0..3).filter_map(|_| it.next().map(|x| x.1)));
                        }
                        out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                    }
                    other => out.push(other),
                }
            }
            c if c == q => return Some((out, &s[1 + i + 1..])),
            c => out.push(c),
        }
    }
    None
}

fn num(s: &str) -> Option<i64> {
    let s = s.trim();
    match s.strip_prefix("0x") {
        Some(h) => i64::from_str_radix(h, 16).ok(),
        None => s.parse().ok(),
    }
}

/// 値を 1 つ読む (文字列・数・配列)。返すのは (値, 残り)
fn js_value(s: &str) -> Result<(Val, &str)> {
    let s = skip(s);
    if let Some((t, rest)) = js_string(s) {
        return Ok((Val::Str(t), rest));
    }
    if let Some(mut r) = s.strip_prefix('[') {
        let mut items = vec![];
        loop {
            r = skip(r);
            if let Some(rest) = r.strip_prefix(']') {
                return Ok((Val::Arr(items), rest));
            }
            let (v, rest) = js_value(r)?;
            items.push(v);
            r = skip(rest);
            r = r.strip_prefix(',').unwrap_or(r);
        }
    }
    let end = s.find(|c: char| !(c.is_ascii_alphanumeric() || c == '-')).unwrap_or(s.len());
    match num(&s[..end]) {
        Some(n) => Ok((Val::Num(n), &s[end..])),
        None => bail!("読めない値: {}", &s[..s.len().min(40)]),
    }
}

/// `[export ]const NAME[: 型] = <値>` の <値> の文字列 (先頭から)。
/// denpa はファイルの中でしか使わない表の export を外すので、export の有無は問わない
fn raw_value<'a>(src: &'a str, name: &str) -> Option<&'a str> {
    let key = format!("const {name}");
    let mut from = 0;
    while let Some(i) = src[from..].find(&key) {
        let at = from + i;
        let rest = &src[at + key.len()..];
        // 行の頭 (export を除く) に無いもの・名前の続き (NAME_ROWS など) は別の名前
        let line = &src[src[..at].rfind('\n').map_or(0, |n| n + 1)..at];
        if !matches!(line.trim(), "" | "export") || rest.starts_with(|c: char| c.is_alphanumeric() || c == '_') {
            from += i + key.len();
            continue;
        }
        let eq = rest.find('=')?;
        return Some(skip(&rest[eq + 1..]));
    }
    None
}

/// 表の値。文字列・配列のほか、`new Map<…>([…])` と `ROWS.join('')` を読む
fn table(src: &str, name: &str) -> Result<Option<Val>> {
    let Some(v) = raw_value(src, name) else { return Ok(None) };
    if let Some(r) = v.strip_prefix("new Map") {
        // `new Map<…>()` (空で作ってあとで足す) は空として読む
        let end = r.find(';').unwrap_or(r.len());
        return match r[..end].find("([") {
            Some(i) => Ok(Some(js_value(&r[i + 1..])?.0)),
            None => Ok(Some(Val::Arr(vec![]))),
        };
    }
    if let Some((ident, rest)) = v.split_once(".join(") {
        let ident = ident.trim();
        if ident.chars().all(|c| c.is_alphanumeric() || c == '_') {
            let (sep, _) = js_string(skip(rest)).ctx(|| format!("{name}: join の区切りが読めません"))?;
            let Some(Val::Arr(rows)) = table(src, ident)? else { bail!("{name}: {ident} がありません") };
            let parts: Vec<String> = rows.into_iter().map(|r| if let Val::Str(s) = r { s } else { String::new() }).collect();
            return Ok(Some(Val::Str(parts.join(&sep))));
        }
    }
    Ok(Some(js_value(v)?.0))
}

fn need(src: &str, name: &str) -> Result<Val> {
    table(src, name)?.ctx(|| format!("denpa の表に {name} がありません"))
}

fn as_str(v: &Val) -> Option<&str> {
    if let Val::Str(s) = v { Some(s) } else { None }
}

/// Map の組 ([キー, 値]) を並べる
fn pairs(v: &Val) -> Vec<(Val, Val)> {
    let Val::Arr(items) = v else { return vec![] };
    items
        .iter()
        .filter_map(|p| match p {
            Val::Arr(kv) if kv.len() == 2 => Some((kv[0].clone(), kv[1].clone())),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------- 一覧

pub fn load(dir: &Path) -> Result<Repertoire> {
    let mut src = String::new();
    let denpa = dir.join("denpa");
    for f in ["b24-tables.ts", "aribtext-gaiji.ts", "fold.ts"] {
        if let Ok(t) = std::fs::read_to_string(denpa.join(f)) {
            src.push_str(&t);
            src.push('\n');
        }
    }
    if src.is_empty() {
        bail!("{} に denpa の表がありません (先に fetch.sh)", denpa.display());
    }
    // EUC-JP の 1〜84 区の区点 (0 始まり) と字 (13 区は NEC 特殊文字。JIS X 0213 の1面もほぼ同じ字)
    let mut jis0208: BTreeMap<usize, u32> = BTreeMap::new();
    for line in std::fs::read_to_string(dir.join("index-jis0208.txt")).ctx(|| "index-jis0208.txt (先に fetch.sh)".into())?.lines() {
        let mut f = line.split('\t');
        if let (Some(p), Some(c)) = (f.next(), f.next()) {
            if let (Ok(p), Some(c)) = (p.trim().parse::<usize>(), num(c)) {
                if p < 84 * 94 {
                    jis0208.entry(p).or_insert(c as u32);
                }
            }
        }
    }

    let bad = |c: u32| c == 0xFFFD || c < 0x20;
    // out: web フォントの基本のファイルに入れる字。x0213: JIS X 0213 にだけある字 (web フォントでは別のファイル。下の extra)
    let mut out = BTreeSet::new();
    let mut x0213 = BTreeSet::new();

    // データ放送: ASCII と EUC-JP の 1〜84 区 (85 区から先は字幕と同じ追加記号)
    out.extend(0x20..0x7F);
    out.extend(jis0208.values().copied().filter(|&c| !bad(c)));

    // 字幕と番組表
    for name in ["HIRAGANA", "KATAKANA", "KANA_SYMBOLS_HALF", "JISX0201", "JISX0201_HALF", "ALNUM_FULL", "ALNUM_HALF", "KANJI_SYMBOLS_HALF", "ADDITIONAL"] {
        let v = need(&src, name)?;
        out.extend(as_str(&v).ctx(|| format!("{name} が文字列でない"))?.chars().map(|c| c as u32).filter(|&c| !bad(c)));
    }
    // JIS X 0213 の1面の 85〜94 区
    x0213.extend(as_str(&need(&src, "PLANE1_HIGH")?).ctx(|| "PLANE1_HIGH が文字列でない".into())?.chars().map(|c| c as u32).filter(|&c| !bad(c)));
    let additional: Vec<u32> = as_str(&need(&src, "ADDITIONAL")?).unwrap_or_default().chars().map(|c| c as u32).collect();
    let mut diff: BTreeMap<usize, u32> = BTreeMap::new();
    let Val::Arr(kd) = need(&src, "KANJI_DIFF")? else { bail!("KANJI_DIFF が配列でない") };
    for p in &kd {
        let Val::Arr(kv) = p else { bail!("KANJI_DIFF の組が読めません") };
        let (Some(Val::Num(start)), Some(Val::Str(chars))) = (kv.first(), kv.get(1)) else { bail!("KANJI_DIFF の組が読めません") };
        for (k, ch) in chars.chars().enumerate() {
            diff.insert(*start as usize + k, ch as u32);
        }
    }
    // 字幕で 1〜84 区の区点 i が出す字: JIS の差分 → EUC-JP (85 区から先は ADDITIONAL・PLANE1_HIGH で入れた)。
    // JIS X 0208 に字のある区点 (13 区を除く) は基本のファイル、ほか (JIS X 0213 で足された点) は別のファイル
    for i in 0..84 * 94 {
        let c = diff.get(&i).or(jis0208.get(&i)).copied().unwrap_or(0xFFFD);
        if bad(c) {
            continue;
        }
        if jis0208.contains_key(&i) && i / 94 != 12 {
            out.insert(c);
        } else {
            x0213.insert(c);
        }
    }
    out.insert(0x3000);
    // JIS X 0213 の2面 ([区, 94 字])
    let Val::Arr(plane2) = need(&src, "PLANE2")? else { bail!("PLANE2 が配列でない") };
    for row in &plane2 {
        let Val::Arr(kv) = row else { bail!("PLANE2 の組が読めません") };
        let Some(Val::Str(chars)) = kv.get(1) else { bail!("PLANE2 の組が読めません") };
        x0213.extend(chars.chars().map(|c| c as u32).filter(|&c| !bad(c)));
    }
    // 2つのコードポイントで 1 字の字 ([区点, 字])。元の字も入れる。ccmp はフォントの中でしか組めないので、
    // 元の字・組んだ字・ccmp は基本のファイル (web フォントで、並びの字が別々のファイルに分かれないように)
    let mut sequences = BTreeSet::new();
    for (_, v) in pairs(&need(&src, "KANJI_SEQUENCES")?) {
        let Val::Str(s) = v else { bail!("KANJI_SEQUENCES の字が文字列でない") };
        let seq: Vec<u32> = s.chars().map(|c| c as u32).collect();
        if seq.len() < 2 {
            bail!("KANJI_SEQUENCES の {s} が 1 字");
        }
        out.extend(seq.iter().copied());
        sequences.insert(seq);
    }

    // DRCS の置き換え先
    for (_, v) in pairs(&need(&src, "DRCS_REPLACE")?) {
        let Val::Num(n) = v else { bail!("DRCS_REPLACE の値が数でない") };
        out.insert(n as u32);
    }

    // 外字の表 (番組表など)。GAIJI がリテラルならそれ、計算で作るなら番組表だけの差分 (EPG_ONLY)
    let mut gaiji: BTreeMap<usize, String> = BTreeMap::new();
    for name in ["GAIJI", "EPG_ONLY"] {
        for (k, v) in table(&src, name)?.map(|v| pairs(&v)).unwrap_or_default() {
            let (Val::Num(code), Val::Str(s)) = (k, v) else { bail!("{name} の組が読めません") };
            let (hi, lo) = ((code >> 8) as usize, (code & 0xff) as usize);
            if !(0x21..=0x7e).contains(&hi) || !(0x21..=0x7e).contains(&lo) {
                bail!("{name} のキーが区点でない: {code:#x}");
            }
            out.extend(s.chars().map(|c| c as u32).filter(|&c| !bad(c)));
            gaiji.insert((hi - 0x21) * 94 + (lo - 0x21), s);
        }
    }
    // 検索の寄せた先
    if let Some(v) = table(&src, "FOLD")? {
        for (_, s) in pairs(&v) {
            if let Val::Str(s) = s {
                out.extend(s.chars().map(|c| c as u32).filter(|&c| !bad(c)));
            }
        }
    }

    // 追加記号の私用領域の字 (92区56〜85点の楽器の略記) → 同じ区点の外字の表の字
    let mut pua: BTreeMap<u32, Pua> = BTreeMap::new();
    for (k, &p) in additional.iter().enumerate() {
        if !(0xE000..=0xF8FF).contains(&p) || pua.contains_key(&p) {
            continue;
        }
        let i = 84 * 94 + k;
        let target = if let Some(s) = gaiji.get(&i) {
            let mut cs = s.chars();
            match (cs.next(), cs.next()) {
                (Some(one), None) => Pua::Char(one as u32),
                _ => Pua::Text(s.clone()),
            }
        } else {
            bail!("U+{p:04X} ({}区{}点) をつなぐ字が denpa の表にありません", i / 94 + 1, i % 94 + 1);
        };
        pua.insert(p, target);
    }
    let extra: BTreeSet<u32> = x0213.difference(&out).copied().collect();
    out.extend(extra.iter().copied());
    Ok(Repertoire { codepoints: out, pua, sequences, extra })
}

/// 一覧を書き出す (見るため。ビルドは load を直に使う)
pub fn run(dir: &Path) -> Result<()> {
    let r = load(dir)?;
    for c in &r.codepoints {
        match r.pua.get(c) {
            Some(Pua::Char(t)) => println!("U+{c:04X}  # → U+{t:04X} {}", char::from_u32(*t).unwrap_or('?')),
            Some(Pua::Text(t)) => println!("U+{c:04X}  # → {t}"),
            None => println!("U+{c:04X}"),
        }
    }
    for s in &r.sequences {
        let hex: Vec<String> = s.iter().map(|c| format!("U+{c:04X}")).collect();
        let text: String = s.iter().filter_map(|&c| char::from_u32(c)).collect();
        println!("{}  # 1 字に組む {text}", hex.join(" "));
    }
    eprintln!("{} code points, {} sequences", r.codepoints.len(), r.sequences.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings() {
        assert_eq!(js_string("'a\\'b' rest"), Some(("a'b".to_string(), " rest")));
        assert_eq!(js_string("\"\\u3042\""), Some(("あ".to_string(), "")));
        assert_eq!(js_string("'\\u{242CE}'"), Some(("𤋎".to_string(), "")));
        assert_eq!(js_string("x"), None);
    }

    #[test]
    fn values() {
        let (v, rest) = js_value(" [ // 注釈\n [32, \"〜‖\"], // 1区33点から\n /* x */ [0x7c, '\\\\'], ];x").unwrap();
        assert_eq!(rest, ";x");
        assert_eq!(
            v,
            Val::Arr(vec![
                Val::Arr(vec![Val::Num(32), Val::Str("〜‖".into())]),
                Val::Arr(vec![Val::Num(0x7c), Val::Str("\\".into())]),
            ])
        );
        assert_eq!(js_value("-1,").unwrap().0, Val::Num(-1));
    }

    #[test]
    fn tables() {
        let src = "const ADDITIONAL_ROWS: string[] = [\n  \"ab\", // 85区\n  \"cd\",\n];\n\
                   export const ADDITIONAL = ADDITIONAL_ROWS.join('');\n\
                   export const M = new Map<number, string>([\n  [0x7c58, '(vn)'], // 楽器\n]);\n\
                   /**\n * const S = \"comment\" は読まない\n */\n\
                   export const S = \"x\\\"y\";";
        assert_eq!(table(src, "ADDITIONAL").unwrap(), Some(Val::Str("abcd".into())));
        assert_eq!(
            table(src, "M").unwrap(),
            Some(Val::Arr(vec![Val::Arr(vec![Val::Num(0x7c58), Val::Str("(vn)".into())])]))
        );
        assert_eq!(table(src, "S").unwrap(), Some(Val::Str("x\"y".into())));
        assert_eq!(table(src, "NONE").unwrap(), None);
    }
}
