# Denpa Font

放送の字幕・データ放送・番組表の字 (JIS X 0208 と ARIB の外字など) を収めた、等幅の丸ゴシックです。
BIZ UDゴシック (モリサワ) の角を機械で丸め、元に無い ARIB の記号などは BIZ UDゴシックの部品で描き足しています。

- ファミリ名 `Denpa Font` (PostScript 名 `DenpaFont-Regular`)、em 1024、送り幅は半角 512・全角 1024 だけ
- 収める字は [danything/denpa](https://github.com/danything/denpa) の表から決め、全部入っています (v3.0 で 7,755 字)。縦書き・ヒンティングは持ちません。全角の英数字を詰める palt を持ちます ([docs/design.md](docs/design.md#palt-全角の英数字を詰める))

## 入手

[Releases](https://github.com/danything/denpa-font/releases) から。

| ファイル | 用途 |
| --- | --- |
| `denpa-font.ttf` | TrueType (fontconfig・Android など) |
| `denpa-font.woff2` | web フォント (ブラウザ) |
| `SHA256SUMS` | 上の 2 つの sha256 |

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
