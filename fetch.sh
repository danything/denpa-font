#!/bin/sh
# 道具 (tools/) が読むものを build/src に取ってきて確かめる。道具はネットにも書庫にも触らない。
#   要るもの: curl・sha256sum (GitHub の ubuntu ランナーには入っている)
set -eu
cd "$(dirname "$0")"
S=build/src
mkdir -p "$S/denpa"

# 字の一覧を作る denpa の表の版 (Renovate が追う)
# renovate: datasource=git-refs depName=https://github.com/danything/denpa branch=main
DENPA_COMMIT=5eae3615b301857861a21e62600e862856128e05
# EUC-JP の表 (WHATWG Encoding Standard の index-jis0208。ブラウザの TextDecoder と同じ字。
# 字幕の漢字 (EUC-JP との差分) と、denpa のデータ放送のエンジン (TextDecoder('euc-jp') で読む) の 1〜84 区)
# renovate: datasource=git-refs depName=https://github.com/whatwg/encoding branch=main
WHATWG_ENCODING_COMMIT=a985b62a9b45c17da3e17a9f0a0b4e30c34c4a8a
# BIZ UDゴシック Regular (googlefonts/morisawa-biz-ud-gothic の GitHub リリース。OFL 1.1)。
# 配布元のリリースの BIZ_UPSTREAM (sha256 は BIZ_UPSTREAM_SHA256) から出した ttf と許諾文を、このリポジトリのリリース
# source-bizudgothic-<版> に置いてある。
# 版は Renovate が追い、上がったら .github/workflows/source-bump.yml がその PR で sha256 を書き換えてリリースを作る (README)
# renovate: datasource=github-releases depName=googlefonts/morisawa-biz-ud-gothic
BIZ_VERSION=v1.051
BIZ_UPSTREAM=BIZUDGothic.zip
BIZ_UPSTREAM_SHA256=30692df621b92df13b88f1360aed1ab6ae50de441bce751a396c6439045cd759
BIZ_SHA256=18956be217e02e08ca18dfe71aeb2110173e81c5f5d5c69d4833d854039e462a
BIZ_OFL_SHA256=e753d7155d53c747d037a445e584c8ecfca6dd79846db610417e282a736b28bc
BIZ_URL=https://github.com/danything/denpa-font/releases/download/source-bizudgothic-${BIZ_VERSION#v}

# get <url> <出力> : 404 なら 1 を返す (ほかの失敗は止める)
get() {
  code=$(curl -sSL --retry 5 --retry-all-errors -o "$2.part" -w '%{http_code}' "$1")
  case "$code" in
    200) mv "$2.part" "$2" ;;
    404) rm -f "$2.part"; return 1 ;;
    *) echo "取れません ($code): $1" >&2; exit 1 ;;
  esac
}

# 元フォント
for f in "BIZUDGothic-Regular.ttf $BIZ_SHA256" "OFL.txt $BIZ_OFL_SHA256"; do
  set -- $f
  echo "$2  $S/$1" | sha256sum -c --status 2>/dev/null || get "$BIZ_URL/$1" "$S/$1"
  echo "$2  $S/$1" | sha256sum -c --quiet
done

# denpa の表 (無いファイルは飛ばす。表はいずれ 1 つのファイルにまとまる)
if [ "$(cat "$S/denpa/COMMIT" 2>/dev/null)" != "$DENPA_COMMIT" ]; then
  rm -f "$S"/denpa/*
  for f in src/lib/ts/b24-tables.ts src/lib/ts/aribtext-gaiji.ts src/lib/fold.ts; do
    get "https://raw.githubusercontent.com/danything/denpa/$DENPA_COMMIT/$f" "$S/denpa/$(basename "$f")" || echo "denpa に $f はありません"
  done
  echo "$DENPA_COMMIT" > "$S/denpa/COMMIT"
fi

# EUC-JP の表
[ -s "$S/index-jis0208.txt" ] && grep -q "$WHATWG_ENCODING_COMMIT" "$S/index-jis0208.commit" 2>/dev/null ||
  { get "https://raw.githubusercontent.com/whatwg/encoding/$WHATWG_ENCODING_COMMIT/index-jis0208.txt" "$S/index-jis0208.txt" &&
    echo "$WHATWG_ENCODING_COMMIT" > "$S/index-jis0208.commit"; }
echo "取ってきた: BIZ UDゴシック $BIZ_VERSION、denpa $DENPA_COMMIT、index-jis0208 $WHATWG_ENCODING_COMMIT"
