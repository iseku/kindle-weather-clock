#!/bin/sh
# 组装 MRPI 安装包（本地执行，产物不进 git）
#
# 用法：
#   tools/build-mrpi.sh [二进制路径]
#   默认取 target/armv7-unknown-linux-musleabihf/release/kindle-weather-clock
#
# 依赖 kindletool（https://github.com/NiLuJe/KindleTool），
# 可用环境变量 KINDLETOOL 指定路径。

set -e

ROOT=$(cd "$(dirname "$0")/.." && pwd)
BIN=${1:-$ROOT/target/armv7-unknown-linux-musleabihf/release/kindle-weather-clock}
KINDLETOOL=${KINDLETOOL:-kindletool}
DEVICE=${DEVICE:-paperwhite2}
PKG=$ROOT/build/mrpi-package
OUT=$ROOT/update-kwc.bin

[ -f "$BIN" ] || { echo "找不到二进制：$BIN" >&2; exit 1; }
command -v "$KINDLETOOL" >/dev/null 2>&1 || {
    echo "找不到 kindletool（可用 KINDLETOOL=/path/to/kindletool 指定）" >&2
    exit 1
}

rm -rf "$PKG"
mkdir -p "$PKG/payload" "$PKG/extensions/kwc/bin"

cp "$BIN"                                  "$PKG/kindle-weather-clock"
cp "$ROOT/tools/run-kwc.sh"                "$PKG/payload/run-kwc.sh"
cp "$ROOT/tools/extensions/kwc/bin/kwc.sh" "$PKG/extensions/kwc/bin/"
cp "$ROOT/tools/extensions/kwc/config.xml" "$PKG/extensions/kwc/"
cp "$ROOT/tools/extensions/kwc/menu.json"  "$PKG/extensions/kwc/"
cp "$ROOT/tools/mrpi/install.sh"           "$PKG/install.sh"

chmod 755 "$PKG/install.sh" "$PKG/payload/run-kwc.sh" \
          "$PKG/extensions/kwc/bin/kwc.sh"

# 注意：必须在包目录内传 "."，且输出文件名必须以 update 开头
cd "$PKG"
"$KINDLETOOL" create ota2 -d "$DEVICE" . "$OUT"

echo "生成：$OUT"
ls -l "$OUT"
