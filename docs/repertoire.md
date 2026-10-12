# 収める字

**denpa の表だけを元にします。** [fetch.sh](../fetch.sh) が [danything/denpa](https://github.com/danything/denpa) の表を版 (`DENPA_COMMIT`) を固定して取り、
道具 ([repertoire.rs](../tools/src/repertoire.rs)) が読みます。フォント側に表の写しは持ちません。TS・JS は実行せず、表のリテラルだけを読みます。

| 何のための字 | denpa の表 |
| --- | --- |
| 字幕 | `src/lib/ts/b24-tables.ts`: JIS X 0208 の割り当てのある区点、外字 85・86・90〜94 区、かな・英数・記号。漢字は EUC-JP との差分なので、EUC-JP は WHATWG の index-jis0208 (`WHATWG_ENCODING_COMMIT`) で読む |
| DRCS (局が絵で送る字) | 同じファイルの `DRCS_REPLACE` の置き換え先 |
| 番組表などの外字 | `src/lib/ts/aribtext-gaiji.ts` の `GAIJI` (計算で作るなら `EPG_ONLY`) と、検索の寄せた先 (`src/lib/fold.ts` の `FOLD`) |
| データ放送 | denpa のエンジン (`src/lib/bml/euc.ts`) と同じ読み方: 1〜84 区は `TextDecoder('euc-jp')` の字 (WHATWG の index-jis0208。13 区の NEC 特殊文字は ARIB の放送が使わないので除く)、85 区から先は字幕と同じ追加記号、と ASCII |

- 表はいずれ 1 つのファイルにまとまるので、表の名前 (`HIRAGANA`・`GAIJI` など) をどのファイルからでも探す
- 私用領域の字 (字幕の 92区56〜85点の楽器の略記) は、同じ区点の外字の表の字 (`EPG_ONLY` の「(vn)」など) を 1 マスに並べて描く
- 入れられない字は [missing.txt](../missing.txt) に書く (いまは空)

## 字を足す流れ

1. denpa の表に足す → denpa の CI が「フォントに無い字」で落ちる
2. こちらの `DENPA_COMMIT` を上げる (Renovate の PR)
3. 元に無い字なら [data/parts.txt](../data/parts.txt) に組み方を足す (図形で描くなら [draw.rs](../tools/src/draw.rs))
4. タグを打って版を出し、denpa のフォントの版 (と sha256) を上げる
