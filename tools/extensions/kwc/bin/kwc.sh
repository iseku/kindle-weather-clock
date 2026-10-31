#!/bin/sh
HOME_APP="app://com.lab126.booklet.home"

go_home() {
    lipc-set-prop com.lab126.appmgrd start "$HOME_APP" 2>/dev/null
}

case "$1" in
start)
    # 已在运行就不重复启动；注意这里绝不能 go_home，
    # 否则书库会被拉到前台盖住刚起的时钟。
    if pgrep -f 'run-kwc.sh' >/dev/null 2>&1; then
        exit 0
    fi
    nohup /mnt/us/run-kwc.sh > /mnt/us/kwc-nohup.log 2>&1 &
    ;;
stop)
    pkill -f 'run-kwc.sh' 2>/dev/null
    pkill -f '/mnt/us/kindle-weather-clock' 2>/dev/null
    killall -CONT awesome 2>/dev/null
    killall -CONT cvm 2>/dev/null
    lipc-set-prop com.lab126.powerd preventScreenSaver 0 2>/dev/null
    lipc-set-prop com.lab126.pillow disableEnablePillow enable 2>/dev/null
    if ! /sbin/initctl status framework 2>/dev/null | grep -q "start/running"; then
        /sbin/initctl start framework 2>/dev/null
        sleep 8
    fi
    sleep 1
    go_home
    ;;
*)
    echo "usage: kwc.sh start|stop"
    ;;
esac
