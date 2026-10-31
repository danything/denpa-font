# Denpa Font

放送の字幕・データ放送・番組表の字 (JIS X 0213 の1面・2面と ARIB の外字など) を収めた、等幅の丸ゴシックです。
BIZ UDゴシック (モリサワ) の角を機械で丸め、元に無い ARIB の記号などは BIZ UDゴシックの部品で描き足しています。

- ファミリ名 `Denpa Font` (PostScript 名 `DenpaFont-Regular`)、em 1024、送り幅は半角 512・全角 1024 だけ
- 収める字は [danything/denpa](https://github.com/danything/denpa) の表から決め、全部入っています (v3.6 で 11,709 字。web フォントは基本の 8,128 字と JIS X 0213 で足された漢字 3,581 字に分けてある)。縦書き・ヒンティングは持ちません。全角の英数字を詰める palt と、か゚ のような2つのコードポイントの字を1字に組む ccmp を持ちます ([docs/design.md](docs/design.md#palt-全角の英数字を詰める))

## 入手

[Releases](https://github.com/danything/denpa-font/releases) から。

| ファイル | 用途 |
| --- | --- |
| `denpa-font.ttf` | TrueType (fontconfig・Android など)。全部の字で 1 つ |
| `denpa-font.woff2` | web フォント (ブラウザ)。基本の字 (JIS X 0208・ARIB の外字・JIS X 0213 の記号やかななど漢字でない字・か゚ など組む字) |
| `denpa-font-jisx0213.woff2` | web フォント (ブラウザ)。JIS X 0213 で足された漢字 (第3・第4水準)。基本のファイルと同じファミリ名で、`unicode-range` を付けて名乗らせる |
| `denpa-font-jisx0213.txt` | `denpa-font-jisx0213.woff2` の字の範囲 (CSS の `unicode-range` の値。1 行) |
| `SHA256SUMS` | 上の 4 つの sha256 |

web フォントは 2 つに分けてあり、ブラウザは JIS X 0213 で足された漢字が画面に出たときだけ `denpa-font-jisx0213.woff2` を取りに行きます。

```css
@font-face { font-family: "Denpa Font"; src: url(denpa-font.woff2) format("woff2"); }
@font-face { font-family: "Denpa Font"; src: url(denpa-font-jisx0213.woff2) format("woff2"); unicode-range: /* denpa-font-jisx0213.txt の中身 */; }
```

見本 (v2.1 と v3.0 を交互に):

![字幕の大きさの見本 (黒地)](docs/v3/caption-white.png)
![字幕の大きさの見本 (白地)](docs/v3/caption-black.png)

## ライセンス

[SIL Open Font License 1.1](LICENSE) (フォントと作る道具。元の BIZ UDゴシックと同じ)。

字形は BIZ UDゴシック (Copyright 2022 The BIZ UDGothic Project Authors) によります。
使うのは [googlefonts/morisawa-biz-ud-gothic](https://github.com/googlefonts/morisawa-biz-ud-gothic) が OFL で配るものだけです (Windows 付属のものは別の許諾)。
予約フォント名はありませんが、元と取り違えないよう BIZ・UD の名は使っていません。v2.x は源柔ゴシック等幅が元でした (v3.0 には入っていません)。

## 詳しく

- [docs/build.md](docs/build.md): 作り方・確かめ方・リリース
- [docs/design.md](docs/design.md): 丸め方・元に無い字・幅・ヒンティング
- [docs/repertoire.md](docs/repertoire.md): 収める字の決め方
- [docs/upstream.md](docs/upstream.md): 元フォントと denpa の表の版上げ
