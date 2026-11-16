#!/bin/sh
# 安全热更新：先停守护循环，再换二进制，最后重新拉起。
#
# 直接 kill 程序再 mv 覆盖会踩坑：守护循环会在 2 秒后把新进程拉起来，
# 而这个进程的代码页正从 /mnt/us 上的旧文件按需装入，文件被替换后
# 再取页就会 SIGBUS（日志里 "Bus error" / dmesg "segfault pc==addr"）。
#
# 用法：tools/deploy.sh [binary] [host]
set -e

ROOT=$(cd "$(dirname "$0")/.." && pwd)
BIN=${1:-$ROOT/target/armv7-unknown-linux-musleabihf/release/kindle-weather-clock}
HOST=${2:-root@172.16.71.17}

[ -f "$BIN" ] || { echo "找不到二进制：$BIN" >&2; exit 1; }

scp -q "$BIN" "$HOST:/mnt/us/kindle-weather-clock.new"

ssh "$HOST" '
set -e
sync
# 1) 停守护循环，避免它在中途把程序拉起来
for p in $(pgrep -f "^/bin/sh /mnt/us/run-kwc.sh"); do kill $p; done
# 2) 停程序本身
for p in $(pgrep -f "^/mnt/us/kindle-weather-clock"); do kill $p; done
sleep 3
# 3) 这时没有进程在用这个文件了，覆盖是安全的
mv /mnt/us/kindle-weather-clock.new /mnt/us/kindle-weather-clock
# 4) 重新拉起守护循环
cd /mnt/us && setsid /mnt/us/run-kwc.sh >/mnt/us/kwc-nohup.log 2>&1 < /dev/null &
sleep 8
pgrep -f "^/mnt/us/kindle-weather-clock" | head -1
'
