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
        # cvm 被 STOP 太久时 upstart 会判定 framework 已停并把它 KILL，
        # 那种情况下书库没有任何进程能重绘，必须先重启 framework。
        if ! /sbin/initctl status framework 2>/dev/null | grep -q "start/running"; then
            /sbin/initctl start framework 2>/dev/null
            sleep 8
        fi
        # 先把整屏刷白：墨水屏是局部刷新，直接让书库重绘的话，
        # 时钟留下的像素（数字、控制条）不会被认为"变化"，会残留在顶部。
        /usr/sbin/eips -f -c >/dev/null 2>&1
        sleep 2
        lipc-set-prop com.lab126.appmgrd start app://com.lab126.booklet.home 2>/dev/null
        sleep 4
        /usr/sbin/eips -f >/dev/null 2>&1
        echo "$(date) user quit" >> /mnt/us/kwc-restart.log
        break
    fi
    echo "$(date) kwc exited($rc), restarting in 2s" >> /mnt/us/kwc-restart.log
    sleep 2
done
