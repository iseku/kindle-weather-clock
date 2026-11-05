#!/bin/sh
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
        killall -CONT awesome 2>/dev/null
        killall -CONT cvm 2>/dev/null
        lipc-set-prop com.lab126.powerd preventScreenSaver 0 2>/dev/null
        lipc-set-prop com.lab126.pillow disableEnablePillow enable 2>/dev/null
        # 运行期间每 10 分钟开关一次 WiFi，每次都是一条 lipc 状态变更；cvm 被 STOP
        # 着，这些事件全积在队列里，一 CONT 就集体回放 —— 状态栏 WiFi 图标狂闪、
        # 电量数字乱跳，要缓好一阵才停。退出时先把 WiFi 恢复成开（书库要用网），
        # 再让它消化完这些积压事件，最后才拉书库。
        lipc-set-prop com.lab126.cmd wirelessEnable 1 2>/dev/null
        # 屏幕此时已经是程序画好的「正在退出…」全屏提示，不用再刷白
        # （刷白会把提示刷掉，用户面对的又是一片白）。
        sleep 30
        # cvm 被 STOP 太久时 upstart 会判定 framework 已停并把它 KILL，
        # 那种情况下书库没有任何进程能重绘，必须先重启 framework。
        if ! /sbin/initctl status framework 2>/dev/null | grep -q "start/running"; then
            /sbin/initctl start framework 2>/dev/null
            sleep 8
        fi
        lipc-set-prop com.lab126.appmgrd start app://com.lab126.booklet.home 2>/dev/null
        sleep 4
        /usr/sbin/eips -f >/dev/null 2>&1
        echo "$(date) user quit" >> /mnt/us/kwc-restart.log
        break
    fi
    echo "$(date) kwc exited($rc), restarting in 2s" >> /mnt/us/kwc-restart.log
    sleep 2
done
