# Kindle 天气时钟 · 设计文档

> 把一台闲置的 Kindle Paperwhite 2 改造成墨水屏天气时钟。
> 文档记录 2026-10-31 定稿版的设计、关键决策与踩过的坑。

---

## 1. 项目概述

**目标**：Kindle PW2 开机后显示一屏「日期 + 月历 + 黄历 + 时钟 + 实时天气 + 三日预报」，左侧中间有亮度调节和退出按钮，退出后回到原生书库。

**形态**：

```
┌──────────────────────────┬──────────────────┐
│ 2026年10月31日 星期六      │      01:14       │
│ 农历 九月廿二              │                  │
├──────────────────────────┤   ☀ 晴 11.9℃     │
│  日 一 二 三 四 五 六      │                  │
│  （黑底白字表头）          │  温度    湿度      │
│   1  2  3                │  11.9℃   34%     │
│   4  5  6  7  8  9 10     │                  │
│  …… 今天带方框            │  风力    风向  ┃+┃│
│                          │  2级    东南风 ┃0┃│
├──────────────────────────┤                ┃-┃│
│ 宜 祭祀 平治道涂 馀事勿取  │  三天预报      ┃退┃│
│ 忌 嫁娶 开市              │  星期五 ☀ 12~23℃ │
│ 济南            电量 99%  │  星期六 ☁ 10~21℃ │
└──────────────────────────┴──────────────────┘
        ← 60% 左栏 →        ← 40% 右栏 →  ← 控制条
```

**技术栈**：Rust + [Slint](https://slint.dev) 1.17（软件渲染器）+ 自研 Kindle framebuffer 后端。

---

## 2. 硬件与系统环境

| 项 | 值 |
|---|---|
| 设备 | Kindle Paperwhite 2（PW2） |
| 屏幕 | 758 × 1024 墨水屏，8 bit 灰度 |
| CPU | ARMv7（armv7-unknown-linux-musleabihf） |
| 系统 | Kindle FW 5.x，busybox 工具集 |
| 越狱 | 已装 KUAL / MRPI / USBNetwork |

### framebuffer 原生参数（重要）

```
xres = 758        yres = 1024
xres_virtual = 768    yres_virtual = 4096   ← 4 重缓冲
stride = 768
rotate = 3（驱动默认）
modes: 758x1024p-0 / 1024x758p-0
```

驱动是 `mxc_epdc_fb`。**这几个数字是后面所有显示问题的根源**，详见 §4.1。

---

## 3. 整体架构

```
      ┌───────────────┐
天气 API ─→│               │
农历库  ─→│  app (Rust)   │──→ Slint 属性 ──→ 软件渲染器 ──→ backend ──→ /dev/fb0
系统时间 ─→│               │                                    │
      └───────────────┘                                    ↓
                                                     /dev/input/event*
                                                         （触摸）
```

| 层 | 位置 | 职责 |
|---|---|---|
| 业务层 | `app/src/main.rs` | 取数、格式化、农历、定时器、回调 |
| 界面层 | `app/ui/app.slint` | 声明式布局（60/40 分屏、日历网格、控制条） |
| 平台后端 | `crates/slint-backend-kindle/` | framebuffer 写入/刷新、触摸读入、开机唤醒 |

**为什么用 Slint**：墨水屏刷新极慢，但 Slint 的软件渲染器只重绘脏区域，`refresh_region()` 只需刷变化的矩形，比整屏刷快一个数量级。

---

## 4. 显示层

### 4.1 关键决策：绝不修改 framebuffer 的 vinfo

原生 fb 是竖屏 758 × 1024。要让 Slint 画 1024 × 758 的横屏画布，最直觉的做法是程序启动时把 vinfo 改成横屏：

```c
v.xres = 1024; v.yres = 758;      // ← 曾经这么干过
ioctl(fd, FBIOPUT_VSCREENINFO, &v);
```

**能改过去，但改不回来。**

`mxc_epdc_fb` 的内存布局是固定高度的多重缓冲（`yres_virtual = 4096`）。把尺寸改成横屏后，再想把 vinfo 设回 758 × 1024 时：

- `fb_check_var()` 静默拒绝
- `ioctl()` **仍然返回 0**（驱动对不认识的字段是忽略而非报错）

结果：程序一退出，framework 按错误的 vinfo 往屏外画，**书库彻底看不见，只能重启设备**。这个坑花了整整一晚。

> **教训**：`ioctl` 返回 0 不等于生效，改 fb 几何参数前必须读回校验。

### 4.2 现行方案：内部旋转

fb 一个字节都**不动**，保持原生 758 × 1024。后端在写像素时把 1024 × 758 的画布旋转后落盘：

```rust
// crates/slint-backend-kindle/src/framebuffer/mod.rs
pub(crate) enum Rotation { None, R180, R90, R270 }

// write_line: 画布的一行 (y, x_range) 映射成 fb 上的一列
Rotation::R270 => {
    let fx = y;
    for (i, px) in pixels.iter().enumerate() {
        let x = x_range.start + i;
        let fy = self.fb_height as usize - 1 - x;
        // fb[fy][fx] = px
    }
}
```

- 由环境变量 `KWC_ROTATE` 选择方向，未设则回退到旧的 `KWC_ROTATE_180`
- **本机正确值是 `KWC_ROTATE=270`**（设备向左转 90° 横放）

`refresh_region()` 要把画布上的脏矩形映射成 fb 上的脏矩形（90/270 时宽高互换）：

```rust
Rotation::R270 => Rect { x: y, y: fb_height - x - w, width: h, height: w }
```

### 4.3 两个容易踩的细节

**几何校验用 fb 宽度，不是画布宽度**

```rust
// 画布 1024 > stride 768，用 width 校验会误报 "invalid framebuffer geometry"
if fb_width == 0 || fb_height == 0 || stride < fb_width as usize { ... }
```

**缩放因子**

`dashboard_scale_factor(1024, 758)` 会算出 0.74，导致画面往中间缩、四周留黑边。用 `KWC_SCALE=1` 强制满屏。

---

## 5. 触摸

### 5.1 坐标转置（必读）

触摸控制器上报的是**竖屏坐标**（`max_x=757, max_y=1023`），但屏幕是按横屏渲染的。实测标定：

| 点击位置 | 原始上报 |
|---|---|
| 屏幕左上 | raw(65, 979) |
| 屏幕右下 | raw(707, 43) |

推出映射：

```rust
screen_x = (max_y - raw_y) * screen_width  / max_y
screen_y =  raw_x          * screen_height / max_x
```

实现在 `crates/slint-backend-kindle/src/touch.rs`，用 `transposed: bool` 标记（`max_x < max_y` 时判定为竖屏控制器）：

```rust
if transposed {
    self.x = (max_y - raw_y) * W / max_y;
    self.y = raw_x * H / max_x;
}
```

### 5.2 事件

`EVIOCGRAB` 独占设备 → 查 `EVIOCGABS` 拿轴范围 → `EV_KEY BTN_TOUCH` + `EV_ABS` 累积 → `SYN_REPORT` 时发 Slint `PointerPressed` / `PointerMoved`，抬起时发 `PointerReleased`。

> `EVIOCGRAB` 意味着同一时刻只能有一个进程占触摸设备。多开实例会 panic：`failed to open touch input: Resource busy (os error 16)`。启动前必须确认没有残留进程。

---

## 6. 业务层

### 6.1 数据源

| 数据 | 来源 | 备注 |
|---|---|---|
| 实时天气 | `http://d1.weather.com.cn/sk_2d/{城市代码}.html` | 需带 `Referer` + 桌面 UA，返回 `var dataSK={...}` |
| 三日预报 | `http://t.weather.itboy.net/api/weather/city/{城市代码}` | `data.forecast` 取前 3 天 |
| 农历/黄历 | `lunar-rs` crate | 宜忌、节日、节气 |
| 地理位置 | `http://cip.cc/` | 解析含「城市」的那一行，失败回退「济南」 |
| 电量 | `/sys/class/power_supply/max77696-battery/capacity` | PW2 的实际路径，需遍历目录兼容其他机型 |
| 亮度 | `lipc-set-prop com.lab126.powerd flIntensity` | 范围 0–24，但 **0 档关不干净**，见 §7 |

### 6.2 刷新节奏

| 内容 | 周期 |
|---|---|
| 时钟 `HH:MM` | 1 秒检查，跨分钟才重绘 |
| 日历 / 农历 / 黄历 | 跨天才重建 |
| 天气 | 600 秒 |

**踩过的坑**：`update_clock()` 原本每秒都 `set_calendar(ModelRc::new(...))`，新建的 `ModelRc` 让 Slint 判定属性变更，于是墨水屏**每秒刷一次整块日历**，实测一晚上掉 33% 电（CPU 8%）。改成传入 `day_changed` 参数、只在跨天时重建后，CPU 降到 0.95%。

网络请求同理会卡住主线程：`ureq` 默认**无超时**，连接半死时 `read()` 永久阻塞在 `sk_wait_data`，1 秒时钟定时器跟着停，屏幕从此不再刷新。显式设 `timeout_connect/read/write` 只挡住了一半。

**唯一可靠的做法是把取数整个搬出 UI 线程**（`fetch_snapshot` / `apply_snapshot` / `spawn_weather_update`）。2026-11-01 又卡死一次：日志停在 `实况取数成功` 之后、`关闭 WiFi` 之前，主线程 `wchan=sk_wait_data` 挂了 2.5 小时 —— `fetch_location`（cip.cc）的 DNS 解析不受 ureq 超时覆盖，musl 的 resolver 会一直等下去。现在定时器只负责 spawn 线程，取完用 `slint::invoke_from_event_loop` 回主线程写属性；`AtomicU64` 记本轮开始时间，超过 300 秒视为卡死、允许重开一轮（卡住的线程救不回来，但时钟不会跟着停）。

### 6.3 时区（必读）

**Kindle 的系统时钟是 UTC**，直接用 `Local::now()` 会显示 UTC 时间。所有时间统一走固定东八区：

```rust
fn now_east8() -> DateTime<FixedOffset> {
    Utc::now().with_timezone(&FixedOffset::east_opt(8 * 3600).unwrap())
}
```

`update_clock()` 和 `build_calendar()` 都用它。

### 6.4 日历数据

Slint 1.17 的 `GridLayout` **没有 `columns` 属性**，也没有 `grid-column` / `grid-row`。固定宽度子元素不会自动折行，会排成一整行把布局撑爆。

解法：Rust 侧预分块成周行。

```rust
pub struct CalendarRow { cells: [CalendarDay] }   // 每周一行，补齐到 7 的倍数再 chunks(7)
```

界面上就是 `for row in root.calendar { HorizontalLayout { for cell in row.cells { ... } } }`。

### 6.5 排版要点

- 日历表头黑底白字：`Rectangle { background: #000000; Text { color: #ffffff } }`
- 嵌套 `HorizontalLayout` 默认平分剩余高度，需要 `vertical-stretch: 0` 才不会把行距拉大
- 今天用 2px 黑框标出
- 温度单位用单字符 **℃（U+2103）**，别用 `°C` —— STHEITI 字体里 `°` 的侧边距很大，会在中间露出空隙

---

## 7. 亮度控制

右侧浮层控制条（`app.slint` 末尾）：

```
  ┌──┐  +    亮度 +
  ├──┤  16   当前值
  ├──┤  −    亮度 −
  │  │
  ┌──┐
  │退│      退出
  └──┘
```

每个按钮内嵌 `TouchArea`，触发 `brightness-up()` / `brightness-down()` / `quit()` 回调。亮度按 4 步进，钳在 0–24，同时 `app.set_brightness()` 更新显示值。

### 7.2 按需 WiFi（省电模式）

Kindle 的 WiFi 常开是耗电大户。程序改成**只在取数据时开网**：

- `has_default_route()` 读 `/proc/net/route` 看有没有默认路由（比 ping 便宜且不产生流量）
- `set_wifi(on)` 走 `lipc-set-prop com.lab126.cmd wirelessEnable 0|1`
- `with_network(f)` 包裹所有联网取数：没网就开、轮询最多 15 秒等到默认路由（实测关飞行模式后约 6 秒可用），就绪后再稳 2 秒；取完**总是关网**
- 等网超时就跳过这次联网更新，但照样跑一次 `f()`，让电量等本地数据仍能刷新

屏幕上「电量」右边显示当前状态并**可点击切换**：`WiFi` 表示无线开着，`飞行` 表示关了。点击后后台线程调 lipc，2 秒后回 UI 线程更新文字（lipc 会阻塞几秒，不能放 UI 线程）。

**代价**：设备平时离线，SSH 连不上。想远程调试要么等它下次取数的窗口（10 分钟一次，窗口约 15 秒），要么在屏幕上点一下开关开网。

### 7.1 亮度 0 档必须直写 sysfs

`lipc-set-prop flIntensity 0` 并不是关灯，实测映射：

| lipc flIntensity | `/sys/class/backlight/max77696-bl/brightness` |
|---|---|
| 16 | 969 |
| 1 | 2 |
| **0** | **1** ← PWM 最低档，暗处仍有微光 |
| 直写 sysfs `0` | 0 ← 真正关断 |

所以 0 档单独走 sysfs：

```rust
if v == 0 {
    std::fs::write("/sys/class/backlight/max77696-bl/brightness", "0")?;
    return;   // 其余档位继续走 lipc，保留系统原有亮度曲线
}
```

程序启动时也会把 UI 亮度值同步写一次硬件，否则重启后 0 档不生效。

---

## 8. 启停与系统集成

### 8.1 `run-kwc.sh`（/mnt/us/run-kwc.sh）

```sh
#!/bin/sh
killall -STOP awesome cvm              # 冻住原生 UI
lipc-set-prop com.lab126.powerd preventScreenSaver 1
lipc-set-prop com.lab126.pillow disableEnablePillow disable
export KWC_SCALE=1
export KWC_ROTATE=270
while true; do
    /mnt/us/kindle-weather-clock
    rc=$?
    [ "$rc" -eq 42 ] && { 恢复系统; break; }   # 用户点了「退」
    记录日志; sleep 2                          # 崩溃自动重启
done
```

退出码约定：**42 = 用户主动退出**，其他非零 = 崩溃（自动重启）。

### 8.2 退出恢复流程（关键）

```sh
killall -CONT awesome cvm
lipc-set-prop com.lab126.powerd preventScreenSaver 0
lipc-set-prop com.lab126.pillow disableEnablePillow enable

# cvm 被 STOP 太久时 upstart 会判定 framework 已停并把它 KILL，
# 此时没有任何进程能重绘屏幕，必须先重启 framework
if ! /sbin/initctl status framework | grep -q "start/running"; then
    /sbin/initctl start framework; sleep 8
fi

/usr/sbin/eips -f -c      # ① 先刷白：墨水屏是局部刷新，
sleep 2                   #    时钟留下的像素不会被判定成"变化"
lipc-set-prop com.lab126.appmgrd start app://com.lab126.booklet.home
sleep 4
/usr/sbin/eips -f         # ② 再全刷，把书库真正推上屏
```

**绝对不要做**：`pkill cvm` 或长时间 `killall -STOP cvm`。upstart 会判定 framework 停止并 KILL cvm，之后屏幕再也刷不出来。

### 8.2.1 退出时为什么要先等 30 秒

程序每 10 分钟开关一次 WiFi，每次都是一条 `lipc-set-prop com.lab126.cmd wirelessEnable` 状态变更。运行时 `cvm`（画状态栏的 framework 进程）被 `STOP` 住了，这些变更事件全积在队列里 —— 跑一整天就是几百条。

点退出后 `killall -CONT cvm` 一恢复，cvm 就开始**集体回放**这些积压事件：状态栏 WiFi 图标一会儿开一会儿关，电量数字一点点往下跳，要缓很久才停。

所以退出流程是：程序先把 UI 切成 `quitting` 状态（全屏「正在退出…」提示）并全刷 → `exit(42)` → 脚本恢复 WiFi 为开（书库要联网）→ **`sleep 30`，此时 cvm 仍停着，屏幕只有提示、状态栏不会盖上来** → 才 `killall -CONT` 放 cvm 出来消化积压事件 → 拉书库 → `eips -f`。用户看到的是「时钟 → 正在退出… 半分钟 → 书库」。

**不要再 `eips -f -c` 刷白**：提示层已经覆盖全屏，刷白会把提示刷掉，用户面对的又是一片白，更以为死机。

治本的做法是运行期间不碰 framework 的 WiFi 状态（改用 `wpa_cli disconnect`），但那样系统里会显示「WiFi 已连接」而实际没网，状态不一致反而更容易让人困惑，所以暂时用等待。

### 8.3 KUAL 扩展（/mnt/us/extensions/kwc/）

三个文件：`config.xml`、`menu.json`、`bin/kwc.sh`。

**菜单名必须是纯 ASCII**。用中文会触发 KUAL 的路径解析 bug：

```
KUAL: directory 'nsions/kwc' not found
```

`extensions/kwc` 被截断成 `nsions/kwc`（少了前 4 个字符），chdir 失败 → 直接放弃执行 → 脚本一次都不跑，连日志都没有。

排查时看这两个文件：

| 路径 | 用途 |
|---|---|
| `/var/tmp/KUAL.log` | 执行日志，路径 bug 直接写在这里 |
| `/var/tmp/KUAL.cache` | 菜单缓存，改完 menu.json 要删掉 |

正确的 `menu.json` 结构（对标能工作的 usbnet 扩展）：

```json
{
  "items": [{
    "name": "Weather Clock",
    "priority": 0,
    "items": [{
      "name": "Start Weather Clock",
      "action": "./bin/kwc.sh",
      "params": "start",
      "priority": 1,
      "exitmenu": true,
      "refresh": false,
      "status": false,
      "internal": "status Starting weather clock"
    }]
  }]
}
```

要点：**顶层必须是分组（有 items 子数组），不能平铺 action 项**；字段要齐（`refresh` / `status` / `priority`）；菜单名用英文。

---

## 9. 构建与部署

### 9.1 交叉编译

本机没有系统级 Rust 工具链，用工作区自带的：

```bash
export CARGO_HOME=$PWD/../.toolchain/cargo
export RUSTUP_HOME=$PWD/../.toolchain/rustup
export CARGO_ZIGBUILD_CACHE_DIR=$PWD/../.toolchain/cargo-zigbuild-cache
export ZIG_GLOBAL_CACHE_DIR=$PWD/../.toolchain/zig-cache
export TMPDIR=$PWD/../.toolchain/tmp
export ZIG_PATH=$PWD/../.toolchain/zig-aarch64-macos-0.14.1/zig
export PATH=$PWD/../.toolchain/cargo/bin:$PWD/../.toolchain/zig-aarch64-macos-0.14.1:$PATH

cargo zigbuild --release --target armv7-unknown-linux-musleabihf   # 首次约 2 分钟，增量约 28 秒
```

产物：`target/armv7-unknown-linux-musleabihf/release/kindle-weather-clock`（约 19 MB）。

### 9.2 部署

```bash
scp target/.../kindle-weather-clock root@172.16.71.17:/mnt/us/kwc.new
ssh root@172.16.71.17 '
  sync                                           # ← 必做
  md5sum /mnt/us/kwc.new                         # ← 必做，见下方警告
  pkill -9 -f "[r]un-kwc.sh"; pkill -9 -f "[k]indle-weather-clock"
  mv /mnt/us/kwc.new /mnt/us/kindle-weather-clock
  setsid nohup /mnt/us/run-kwc.sh > /mnt/us/kwc-nohup.log 2>&1 < /dev/null &
'
```

**三个必守的坑**：

1. **`sync` + md5 校验**：`/mnt/us` 是 FUSE 挂载的 FAT32，不 sync 直接 scp 会**截断文件**（曾导致脚本少 20 行，报 `syntax error: unexpected end of file`）。
2. **`pkill -f` 会杀掉自己的 ssh**：命令行里含 `run-kwc.sh` 字符串时，`pkill -f run-kwc.sh` 会匹配到这条 ssh 命令本身。用 `[r]un-kwc.sh` 这种正则写法避开。
3. **`setsid`**：普通 `nohup ... &` 在 ssh 会话结束时可能被杀，加 `setsid` 才能脱离。

### 9.3 验证画面（不需要有人在设备旁）

```bash
ssh root@172.16.71.17 'dd if=/dev/fb0 bs=1024 count=1024 2>/dev/null' > /tmp/fb.raw
python3 -c "
from PIL import Image
d = open('/tmp/fb.raw','rb').read()
Image.frombytes('L',(768,1024),d[:768*1024]).save('/tmp/fb.png')"
```

用工作区自带的 python（含 Pillow）：`/Users/tony/.dsh/dsh-runtimes/dsh-next-primary-runtime/dependencies/python/bin/python3`。系统 python3 没有 PIL。

---

## 10. 已知限制

| 限制 | 说明 |
|---|---|
| 不支持真正的多缓冲 | 只映射第一个缓冲（768 × 4096 里前 768 × 1024），够用但没有双缓冲的平滑 |
| 字体固定 | 只加载 STHEITI，缺符号时会渲染成空白块（如 `✕` 换成「退」） |
| 触摸独占 | 同一时刻只能跑一个实例 |
| 依赖网络 | 天气 API 无缓存，断网时显示上一次的旧数据 |
| 磁盘吃紧 | `/mnt/us` 总 1.3 GB，`documents` 占 963 MB，只剩 55 MB 可用 |

---

## 11. 后续可做

- [ ] 开机自启（现在需要手动从 KUAL 启动）
- [ ] 天气数据本地缓存，断网时降级显示
- [ ] 亮度值持久化（现在每次启动回到 16）
- [ ] 时钟按分钟对齐重绘，而不是每秒重绘（省电）
- [ ] 深色模式（夜间自动反转）

---

## 附录：文件清单（Kindle 侧）

```
/mnt/us/
├── kindle-weather-clock          19.1 MB   主程序
├── run-kwc.sh                     1.6 KB   启动/守护脚本
├── kwc-restart.log                         运行日志（异常与退出）
└── extensions/kwc/                         KUAL 扩展
    ├── config.xml
    ├── menu.json
    └── bin/kwc.sh                          菜单入口
```

## 附录：环境变量

| 变量 | 值 | 作用 |
|---|---|---|
| `KWC_SCALE` | `1` | 强制满屏（否则 0.74 倍留黑边） |
| `KWC_ROTATE` | `270` | 内部旋转方向（90/180/270/0） |
| `KWC_WIDTH` | `1024` | 画布宽（默认值） |
| `KWC_HEIGHT` | `758` | 画布高（默认值） |
