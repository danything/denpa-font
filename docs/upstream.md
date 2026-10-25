# 版上げ

取ってくるものと依存の版は Renovate ([renovate.json](../renovate.json)) が追います。

| 何 | どこ | 入れ方 |
| --- | --- | --- |
| denpa の表 | fetch.sh の `DENPA_COMMIT` (main の先頭) | CI が通れば自動で入る |
| EUC-JP の表 | `WHATWG_ENCODING_COMMIT` (whatwg/encoding の main の先頭) | 同上 |
| fontations (skrifa・write-fonts と、その下の read-fonts) | `tools/Cargo.toml` | 同じ版どうしでしか組めないので 1 本の PR にまとめ、同上 |
| BIZ UDゴシック | fetch.sh の `BIZ_VERSION` (上流の GitHub リリースのタグ) | **人が見て入れる** (下) |

## 元フォント (BIZ UDゴシック)

いまは 1.051。上流のリリースから出した ttf と、上流のタグの `OFL.txt` を、このリポジトリのリリース `source-bizudgothic-<版>`
(例 [source-bizudgothic-1.051](https://github.com/danything/denpa-font/releases/tag/source-bizudgothic-1.051)) に置き、fetch.sh はそこから取って sha256 で照らします。

sha256 は Renovate では書き換えられないので、Renovate の PR のブランチに push されると [source-bump.yml](../.github/workflows/source-bump.yml) が:

1. 上流のその版の ttf (版によって zip の中か、そのまま) と `OFL.txt` を取って sha256 を測る。`OFL.txt` に予約フォント名があれば止まる
2. リリース `source-bizudgothic-<版>` に置く (もうあって中身が違えば止まる)
3. fetch.sh の sha256 を書き換えてコミットし、同じブランチに push する
4. GITHUB_TOKEN の push では CI が動かないので、そのコミットを build.yml で確かめ、必須チェック (build・claude-review) の status を付ける

GitHub Apps・デプロイキーは使わず GITHUB_TOKEN だけです。動かなかったときは、そのブランチの push で起きた source-bump の実行を Actions で Re-run します (実行が無ければ Renovate の PR で rebase させる)。`workflow_dispatch` は置いていません。main で起こすと PR のブランチのコードが main のキャッシュの範囲で動き、main のキャッシュを汚せてしまうためです。

**字の形が変わるので自動ではマージしません。** CI のジョブの要約 (前の版から描き方が変わった字) と見本 (artifact の `changes`) を見て、人が入れます。
