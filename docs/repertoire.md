# 収める字

**denpa の表だけを元にします。** [fetch.sh](../fetch.sh) が [danything/denpa](https://github.com/danything/denpa) の表を版 (`DENPA_COMMIT`) を固定して取り、
道具 ([repertoire.rs](../tools/src/repertoire.rs)) が読みます。フォント側に表の写しは持ちません。TS・JS は実行せず、表のリテラルだけを読みます。

| 何のための字 | denpa の表 |
| --- | --- |
| 字幕と番組表 | `packages/arib/src/b24-tables.ts`: 1〜84 区の全部の区点 (JIS X 0213 の1面と同じ字。漢字は EUC-JP との差分 `KANJI_DIFF` なので、EUC-JP は WHATWG の index-jis0208 (`WHATWG_ENCODING_COMMIT`) で読む)、外字 85・86・90〜94 区 (`ADDITIONAL`)、JIS X 0213 の1面の 85〜94 区 (`PLANE1_HIGH`) と2面 (`PLANE2`)、2つのコードポイントで 1 字の字 (`KANJI_SEQUENCES`。ccmp で組む)、かな・英数・記号 |
| DRCS (局が絵で送る字) | 同じファイルの `DRCS_REPLACE` の置き換え先 |
| 番組表などの外字 | `packages/arib/src/aribtext-gaiji.ts` の `GAIJI` (計算で作るなら `EPG_ONLY`) と、検索の寄せた先 (`src/lib/fold.ts` の `FOLD`) |
| データ放送 | denpa のエンジン (`src/lib/bml/euc.ts`) と同じ読み方: 1〜84 区は `TextDecoder('euc-jp')` の字 (WHATWG の index-jis0208)、85 区から先は字幕と同じ追加記号、と ASCII |

- 表はいずれ 1 つのファイルにまとまるので、表の名前 (`HIRAGANA`・`GAIJI` など) をどのファイルからでも探す
- 私用領域の字 (字幕の 92区56〜85点の楽器の略記) は、同じ区点の外字の表の字 (`EPG_ONLY` の「(vn)」など) を 1 マスに並べて描く
- 入れられない字は [missing.txt](../missing.txt) に書く (いまは空)

## web フォントの分け方

ttf は全部の字で 1 つ、web フォントは 2 つに分けます (ブラウザが要るときだけ取りに行けるように)。

- `denpa-font-jisx0213.woff2`: JIS X 0213 で足された漢字 (第3・第4水準)。JIS X 0213 にだけある字 (1〜84 区の JIS X 0208 に字の無い区点と 13 区の字、1面の 85〜94 区 `PLANE1_HIGH`、2面 `PLANE2` のうち、ほかの表に無い字) のうち漢字 (CJK 統合漢字とその拡張・互換漢字) のもの。字幕や番組表にはめったに出ない
- `denpa-font.woff2`: ほかの全部。v3.5 までと同じ字に、JIS X 0213 の漢字でない字 (▷ ⇦ ⓐ ㊤ ㌔ € ✓ などの記号・かな・ラテン文字) と、か゚ など 2 つのコードポイントの字を足したもの。漢字でない字は字幕や番組表によく出るので、別のファイルにすると、たいていの画面がそれを取りに行ってしまう
- 2 つのコードポイントの字は、並びの元の字 (か・U+309A・æ・U+0300 など) と組んだ字と ccmp を基本のファイルに入れる。ccmp は 1 つのフォントの中でしか組めないので、並びの字が 2 つのファイルに分かれると組めない
- 分け方の字の一覧 (CSS の `unicode-range`) は build が出来たファイルの字から書く (`denpa-font-jisx0213.txt`) ので、いつもファイルと合う

## 字を足す流れ

1. denpa の表に足す → denpa の CI が「フォントに無い字」で落ちる
2. こちらの `DENPA_COMMIT` を上げる (Renovate の PR)
3. 元に無い字なら [data/parts.txt](../data/parts.txt) に組み方を足す (図形で描くなら [draw.rs](../tools/src/draw.rs))
4. タグを打って版を出し、denpa のフォントの版 (と sha256) を上げる
