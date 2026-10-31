# Kindle 天气时钟

把闲置的 Kindle Paperwhite 2 改造成墨水屏天气时钟。

一屏显示日期、农历、当月日历、黄历宜忌、时钟、实时天气和三日预报；右侧有亮度调节和退出按钮，退出即回到原生书库。

```
┌──────────────────────────┬──────────────────┐
│ 2026年10月31日 星期六      │      01:14       │
│ 农历 九月廿二              │   ☀ 晴 11.9℃     │
│ 日 一 二 三 四 五 六       │  温度 11.9℃ 湿度 34% │
│  1  2  3                 │  风力 2级   风向 东南风 │
│  4  5  6  7  8  9 10      │                  │
│  … 今天带方框              │  三天预报          │
│ 宜 祭祀 平治道涂 馀事勿取   │  星期五 ☀ 12~23℃  │
│ 忌 嫁娶 开市              │  星期六 ☁ 10~21℃  │
│ 济南          电量 99%     │  星期日 ☁ 12~17℃  │
└──────────────────────────┴──────────────────┘
```

## 技术栈

- **Rust** + **Slint 1.17**（软件渲染器）
- 自研 `slint-backend-kindle` 后端：framebuffer 写入、内部旋转、触摸转置、开机唤醒
- 数据全部来自公开 HTTP 接口，不需要 Home Assistant

## 快速开始

```bash
# 交叉编译（需要工作区的 .toolchain）
cargo zigbuild --release --target armv7-unknown-linux-musleabihf

# 部署（详见 docs/DESIGN.md §9）
scp target/armv7-unknown-linux-musleabihf/release/kindle-weather-clock root@<kindle-ip>:/mnt/us/kwc.new
```

Kindle 上从 **KUAL → Weather Clock → Start Weather Clock** 启动，点屏幕右侧「退」返回书库。

## 文档

完整设计说明见 **[docs/DESIGN.md](docs/DESIGN.md)**：架构、显示层旋转方案、触摸坐标映射、退出恢复流程、构建部署与踩过的坑。

## 环境变量

| 变量 | 值 | 作用 |
|---|---|---|
| `KWC_SCALE` | `1` | 强制满屏 |
| `KWC_ROTATE` | `270` | 内部旋转方向 |

## 许可证

MIT OR Apache-2.0。框架层 `crates/slint-backend-kindle` 源自 [slint-kindle-backend](https://github.com/sverrejb/slint-kindle-backend)。
