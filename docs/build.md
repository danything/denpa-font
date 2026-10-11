# 作り方

取ってくる・確かめるのは [fetch.sh](../fetch.sh) (curl・sha256sum・openssl・base64・tar)、フォントを作るのは Rust の道具 1 本 ([tools/](../tools))。
道具はネットに触りません。依存は fontations の skrifa・write-fonts と ttf2woff2 だけで、版は [rust-toolchain.toml](../rust-toolchain.toml) と `tools/Cargo.lock` で固定してあります。

```sh
sh fetch.sh                                   # build/src に取ってくる
cargo build --release --locked --manifest-path tools/Cargo.toml
tools/target/release/denpa-font build 3.0     # → dist/denpa-font.ttf・.woff2
tools/target/release/denpa-font verify dist/denpa-font.ttf [--prev 前の版の ttf] [--woff2 woff2 を解いた ttf]
tools/target/release/denpa-font repertoire    # 収める字の一覧を見る
DENPA_ONLY='永あ①' tools/target/release/denpa-font build 3.0   # 書いた字だけ作る (試すとき)
```

- 字ごとに並べて作ります (スレッドの数は `DENPA_THREADS`、無ければ CPU の数)。出力先は `DENPA_OUT` (既定はここ)
- 同じ入力から同じバイト列ができます (日付は `SOURCE_DATE_EPOCH`)
- 元に無い字をどう作ったかは `build/extras.txt` に出ます

## verify

止めるのは機械で決まるものだけです。

- denpa の表の字が全部ある ([missing.txt](../missing.txt) の字を除く。いまは空)、空白のほかに形の無い字が無い、送り幅が 512 か 1024
- 名前・em
- palt: 全角の英数字と記号 (U+FF01–FF5E) だけを墨の幅から決めた量で詰め、どの用字系からも引ける。その字の既定の送り幅は全角 1024 (半角の同じ字は 512) のまま
- woff2 を解いたものが ttf と同じ字・同じ GPOS になる
- 前の版から描き方が変わった字の一覧と、前と今を並べた見本 (`build/changes.svg`) を出す。**止めない** (変わってよいかは PR を見る人が決める)

## CI ([build.yml](../.github/workflows/build.yml))

PR・main・タグのたびに、fetch.sh → `cargo test` → build → verify を回します。

- 前の版は `denpa-font.ttf` を持つ一番新しいリリースから取る
- woff2 は Google の `woff2_decompress` で解いて verify に渡す (ttf2woff2 の出力を確かめる)
- 同じ入力から 2 度作り、SHA256SUMS が合うことを確かめる
- verify の結果はジョブの要約、見本は artifact の `changes`

## リリース

タグ `vX.Y` を push すると、CI が作って確かめ、Releases に ttf・woff2・SHA256SUMS を置きます。フォントの版 (nameID 5) は `Version X.YYY`。

- X: 元を替える・字を減らす・字の幅や行の高さを変える
- Y: 字を足す・直す
