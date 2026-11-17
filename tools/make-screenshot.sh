#!/bin/sh
# 从设备抓一屏，遮掉左下角城市名，存成 docs/screenshot.png
#
# 用法：
#   tools/make-screenshot.sh [设备地址]
#   默认 root@172.16.71.17
#
# 为什么要脚本：城市名要遮（截图会发布到仓库），而它的位置会随布局改动
# 上下浮动 —— 手写遮罩矩形漏过一次（状态栏重排版后整体上移了 9 像素，
# 旧的 y 起点把上面几行留下了）。这里改成自动找墨迹包围盒。
#
# 依赖：ssh、scp、python3 + Pillow。

set -e

DEVICE=${1:-root@172.16.71.17}
ROOT=$(cd "$(dirname "$0")/.." && pwd)
PY=${PYTHON:-python3}
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

echo "抓屏：$DEVICE"
ssh "$DEVICE" 'dd if=/dev/fb0 of=/mnt/us/shot.raw bs=1024 count=1024 2>/dev/null' >/dev/null
scp -q "$DEVICE":/mnt/us/shot.raw "$TMP/shot.raw"
ssh "$DEVICE" 'rm -f /mnt/us/shot.raw' || true

"$PY" - "$TMP/shot.raw" "$ROOT/docs/screenshot.png" <<'PY'
import sys
from PIL import Image, ImageDraw

raw = open(sys.argv[1], 'rb').read()
# fb0 是 768x1024 竖屏，界面在里面按 270° 旋转绘制
img = Image.new('L', (768, 1024))
img.putdata(raw[:768 * 1024])
img = img.crop((0, 0, 758, 1024)).rotate(270, expand=True)

# 左下角状态栏区域里只有城市名一处墨迹，取它的包围盒再外扩一圈
px = img.load()
ink = [(x, y)
       for y in range(img.height - 80, img.height)
       for x in range(0, 140)
       if px[x, y] < 160]

if ink:
    x0 = min(p[0] for p in ink) - 6
    y0 = min(p[1] for p in ink) - 6
    x1 = max(p[0] for p in ink) + 6
    y1 = max(p[1] for p in ink) + 6
    ImageDraw.Draw(img).rectangle([x0, y0, x1, y1], fill=255)
    print(f'遮住城市名：({x0},{y0})-({x1},{y1})，覆盖 {len(ink)} 个墨迹像素')
else:
    print('左下角没检测到墨迹，原样保存')

img.save(sys.argv[2])
print(f'写入 {sys.argv[2]}')
PY
