#!/bin/sh
# MRPI 安装脚本 —— 由 MRPI 在 /var/tmp/mrpi/staging 下执行
# 注意：这是包里「唯一」的顶层 .sh，MRPI 会执行 workdir 里所有顶层 *.sh，
#       守护脚本必须放在 payload/ 子目录里，否则会被当成第二个安装脚本执行。

set -e

echo "[kwc] === Installing kindle-weather-clock ==="

if pgrep -f '^/mnt/us/kindle-weather-clock' >/dev/null 2>&1; then
    echo "[kwc] WARNING: kindle-weather-clock is currently running."
    echo "[kwc] The on-disk binary will be replaced; the running instance"
    echo "[kwc] keeps the old code until it is restarted."
fi

# 1. 主程序
cp -f ./kindle-weather-clock /mnt/us/kindle-weather-clock
chmod 755 /mnt/us/kindle-weather-clock
echo "[kwc] Installed binary: /mnt/us/kindle-weather-clock"

# 2. 守护脚本
cp -f ./payload/run-kwc.sh /mnt/us/run-kwc.sh
chmod 755 /mnt/us/run-kwc.sh
echo "[kwc] Installed daemon: /mnt/us/run-kwc.sh"

# 3. KUAL 扩展（这台机器的 KUAL 扫描根是 /mnt/us/extensions，不是 /extensions）
rm -rf /extensions/kwc 2>/dev/null || true
mkdir -p /mnt/us/extensions/kwc/bin
cp -f ./extensions/kwc/bin/kwc.sh /mnt/us/extensions/kwc/bin/kwc.sh
cp -f ./extensions/kwc/config.xml /mnt/us/extensions/kwc/config.xml
cp -f ./extensions/kwc/menu.json /mnt/us/extensions/kwc/menu.json
chmod 755 /mnt/us/extensions/kwc/bin/kwc.sh
# 清掉 KUAL 的扫描缓存，否则新菜单要等下次重启才出现
rm -f /var/tmp/KUAL.cache 2>/dev/null || true
echo "[kwc] Installed KUAL extension: /mnt/us/extensions/kwc/"

echo "[kwc] === Installation complete ==="
echo "[kwc] Use the KUAL menu (Weather Clock) to start the app."
