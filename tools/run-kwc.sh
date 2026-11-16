#!/bin/sh
# 运行期间 cvm 会一直被 STOP，冻结过久时系统看门狗会抓线程栈、让 dmcc 把诊断包
# 归档到 /mnt/us/documents（书库目录），于是书里会多出一个 cvm_*_crash_* 条目。
# 启动和退出各清一次，避免这两类残留堆积。
rm -rf /mnt/us/documents/cvm_*_crash_*.sdr \
       /mnt/us/documents/cvm_*_crash_*.txt \
       /mnt/us/documents/cvm_*_crash_*.tgz 2>/dev/null
# 同名崩溃还会在 /mnt/us 根下留一份 core dump（单个 60~100 MB，攒两次就把分区占满）
rm -f /mnt/us/cvm_*.core 2>/dev/null
killall -STOP awesome 2>/dev/null
killall -STOP cvm 2>/dev/null
lipc-set-prop com.lab126.powerd preventScreenSaver 1 2>/dev/null
lipc-set-prop com.lab126.pillow disableEnablePillow disable 2>/dev/null
export KWC_SCALE=1
export KWC_ROTATE=270
sleep 1
while true; do
    /mnt/us/kindle-weather-clock
    rc=$?
    if [ "$rc" -eq 42 ]; then
        # 屏幕上是程序画好的「正在退出…」全屏提示。此时 cvm 还 STOP 着，
        # 状态栏不会盖上来 —— 先让它保持干净地停 30 秒。
        # 运行期间每 10 分钟开关一次 WiFi，这些 lipc 状态变更全积在 cvm 的队列里，
        # 一旦 CONT 就集体回放（状态栏 WiFi 图标狂闪、电量乱跳），所以拖到最后再放它出来。
        lipc-set-prop com.lab126.cmd wirelessEnable 1 2>/dev/null
        sleep 30
        killall -CONT awesome 2>/dev/null
        killall -CONT cvm 2>/dev/null
        lipc-set-prop com.lab126.powerd preventScreenSaver 0 2>/dev/null
        lipc-set-prop com.lab126.pillow disableEnablePillow enable 2>/dev/null
        # cvm 被 STOP 太久时 upstart 会判定 framework 已停并把它 KILL，
        # 那种情况下书库没有任何进程能重绘，必须先重启 framework。
        if ! /sbin/initctl status framework 2>/dev/null | grep -q "start/running"; then
            /sbin/initctl start framework 2>/dev/null
            sleep 8
        fi
        # 退出时再清一次：这次会话里被看门狗归档的诊断包，不该留在书库里。
        # 放在启动主页之前，让框架扫描到的是干净的目录。
        rm -rf /mnt/us/documents/cvm_*_crash_*.sdr \
               /mnt/us/documents/cvm_*_crash_*.txt \
               /mnt/us/documents/cvm_*_crash_*.tgz 2>/dev/null
        rm -f /mnt/us/cvm_*.core 2>/dev/null
        lipc-set-prop com.lab126.appmgrd start app://com.lab126.booklet.home 2>/dev/null
        sleep 6
        /usr/sbin/eips -f >/dev/null 2>&1
        echo "$(date) user quit" >> /mnt/us/kwc-restart.log
        break
    fi
    echo "$(date) kwc exited($rc), restarting in 2s" >> /mnt/us/kwc-restart.log
    sleep 2
done
