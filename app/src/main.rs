use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use chrono::{Datelike, FixedOffset, NaiveDate, Timelike, Utc};
use serde::Deserialize;
use slint::{ModelRc, SharedString, Timer, TimerMode, VecModel};
use lunar_rs::Solar;

slint::include_modules!();

#[cfg(all(target_arch = "arm", target_os = "linux", target_env = "musl"))]
static STHEITI_BOLD: &[u8] = include_bytes!("../../assets/STHeitiBold.ttf");

#[cfg(all(target_arch = "arm", target_os = "linux", target_env = "musl"))]
static mut KINDLE_BACKEND: *const slint_backend_kindle::KindleBackend = std::ptr::null();

/// 实时天气城市码（历下区）
const CITY_REALTIME: &str = "101120107";
/// 三天预报城市码（济南市级，itboy 只支持市级）
const CITY_FORECAST: &str = "101120101";

const CLOCK_INTERVAL: Duration = Duration::from_secs(1);
const WEATHER_INTERVAL: Duration = Duration::from_secs(600);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(12);

// ---------- 天气数据结构 ----------

#[derive(Deserialize)]
struct Sk2d {
    #[serde(default)]
    temp: String,
    #[serde(default)]
    WD: String,
    #[serde(default)]
    WS: String,
    #[serde(default)]
    SD: String,
    #[serde(default)]
    qy: String,
    #[serde(default)]
    weather: String,
    #[serde(default)]
    aqi: String,
}

#[derive(Deserialize)]
struct ItboyResponse {
    #[serde(default)]
    data: ItboyData,
}

#[derive(Deserialize, Default)]
struct ItboyData {
    #[serde(default)]
    forecast: Vec<ItboyDay>,
}

#[derive(Deserialize)]
struct ItboyDay {
    #[serde(default)]
    week: String,
    #[serde(rename = "type", default)]
    weather_type: String,
    #[serde(default)]
    high: String,
    #[serde(default)]
    low: String,
}

// ---------- 天气获取 ----------

fn fetch_realtime(agent: &ureq::Agent) -> Option<Sk2d> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let url = format!("http://d1.weather.com.cn/sk_2d/{CITY_REALTIME}.html?_={ts}000");
    let resp = agent
        .get(&url)
        .set("User-Agent", "Mozilla/5.0")
        .set("Referer", "http://www.weather.com.cn/")
        .call()
        .ok()?;
    let text = resp.into_string().ok()?;
    let json = text.trim().strip_prefix("var dataSK=")?.trim_end_matches(';');
    serde_json::from_str(json).ok()
}

fn fetch_forecast(agent: &ureq::Agent) -> Vec<ItboyDay> {
    let url = format!("http://t.weather.itboy.net/api/weather/city/{CITY_FORECAST}");
    let resp = match agent.get(&url).call() {
        Ok(r) => r,
        Err(_) => return vec![],
    };
    let body: ItboyResponse = match resp.into_json() {
        Ok(b) => b,
        Err(_) => return vec![],
    };
    body.data.forecast.into_iter().take(3).collect()
}

/// 天气文本 → 图标名
fn weather_kind(weather: &str) -> &'static str {
    if weather.contains("雷") {
        "cloud-lightning"
    } else if weather.contains("雨") || weather.contains("雪") {
        "cloud-rain"
    } else if weather.contains("雾") || weather.contains("霾") {
        "cloud-fog"
    } else if weather.contains("多云") {
        "cloud-sun"
    } else if weather.contains("晴") {
        "sun"
    } else {
        "cloud"
    }
}

fn update_weather(app: &AppWindow, agent: &ureq::Agent) {
    if let Some(sk) = fetch_realtime(agent) {
        app.set_weather_kind(SharedString::from(weather_kind(&sk.weather)));
        app.set_weather_text(SharedString::from(sk.weather.clone()));
        app.set_temp(SharedString::from(format!("{}℃", sk.temp)));
        app.set_wind_dir(SharedString::from(sk.WD));
        app.set_wind_force(SharedString::from(sk.WS));
        app.set_humidity(SharedString::from(format!("{}%", sk.SD.trim_end_matches('%'))));
    }

    app.set_location(SharedString::from(fetch_location(agent)));
    app.set_battery(SharedString::from(fetch_battery()));

    let forecast = fetch_forecast(agent);
    let rows: Vec<ForecastDay> = forecast
        .into_iter()
        .map(|d| ForecastDay {
            week: SharedString::from(d.week),
            icon: SharedString::from(weather_kind(&d.weather_type)),
            text: SharedString::from(d.weather_type),
            low: SharedString::from(d.low.replace('℃', "")),
            high: SharedString::from(d.high.replace('℃', "")),
        })
        .collect();
    app.set_forecast(ModelRc::new(VecModel::from(rows)));
}

// ---------- 时钟 + 农历 ----------

/// day_changed 为 false 时只更新时间，不重建日历。
/// ponytail: 日历是 ModelRc，每次新建对象都会让 Slint 判定属性变更并重绘整块日历，
/// 而本函数被 1s 定时器调用 —— 不拦住的话墨水屏每秒刷一次大区域，实测一晚掉 33% 电。
fn update_clock(app: &AppWindow, now: &chrono::DateTime<FixedOffset>, day_changed: bool) {
    app.set_time(SharedString::from(format!(
        "{:02}:{:02}",
        now.hour(),
        now.minute()
    )));
    if !day_changed {
        return;
    }
    let weekday = match now.weekday() {
        chrono::Weekday::Mon => "星期一",
        chrono::Weekday::Tue => "星期二",
        chrono::Weekday::Wed => "星期三",
        chrono::Weekday::Thu => "星期四",
        chrono::Weekday::Fri => "星期五",
        chrono::Weekday::Sat => "星期六",
        chrono::Weekday::Sun => "星期日",
    };
    app.set_date(SharedString::from(format!(
        "{}年{}月{}日 {}",
        now.year(),
        now.month(),
        now.day(),
        weekday
    )));

    // 农历
    let solar = Solar::from_ymd(now.year() as i32, now.month() as i32, now.day() as i32).unwrap();
    let lunar = solar.lunar();
    app.set_lunar(SharedString::from(format!(
        "农历 {}月{}",
        lunar.month_in_chinese(),
        lunar.day_in_chinese()
    )));

    // 整月日历
    app.set_calendar(ModelRc::new(VecModel::from(build_calendar(&now))));

    let mut festivals: Vec<String> = lunar.festivals().iter().map(|s| s.to_string()).collect();
    if let Some(jq) = lunar.current_jie_qi() {
        festivals.push(jq.name().to_string());
    }
    app.set_festival(SharedString::from(
        festivals.first().cloned().unwrap_or_default(),
    ));

    app.set_yi(SharedString::from(format!("宜 {}", lunar.day_yi().join(" "))));
    app.set_ji(SharedString::from(format!("忌 {}", lunar.day_ji().join(" "))));
}

// Kindle 系统时钟是 UTC，固定东八区
fn now_east8() -> chrono::DateTime<FixedOffset> {
    Utc::now().with_timezone(&FixedOffset::east_opt(8 * 3600).unwrap())
}

// ---------- 整月日历 ----------

fn build_calendar(now: &chrono::DateTime<FixedOffset>) -> Vec<CalendarRow> {
    let year = now.year();
    let month = now.month();
    let today = now.day();
    let first = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    let next = NaiveDate::from_ymd_opt(year, if month == 12 { 1 } else { month + 1 }, 1).unwrap();
    let days_in_month = (next - first).num_days() as usize;
    let first_weekday = first.weekday().num_days_from_sunday() as usize;

    let mut days: Vec<CalendarDay> = Vec::new();
    let blank = || CalendarDay {
        day: SharedString::new(),
        label: SharedString::new(),
        is_today: false,
    };
    for _ in 0..first_weekday {
        days.push(blank());
    }
    for d in 1..=days_in_month {
        let solar = Solar::from_ymd(year, month as i32, d as i32).unwrap();
        let lunar = solar.lunar();
        let label = if let Some(fest) = lunar.festivals().first() {
            SharedString::from(*fest)
        } else {
            SharedString::from(lunar.day_in_chinese())
        };
        days.push(CalendarDay {
            day: SharedString::from(d.to_string()),
            label,
            is_today: d as u32 == today,
        });
    }
    while days.len() % 7 != 0 {
        days.push(blank());
    }
    days.chunks(7)
        .map(|c| CalendarRow {
            cells: ModelRc::new(VecModel::from(c.to_vec())),
        })
        .collect()
}

// ---------- 地理位置（IP 定位） ----------

fn fetch_location(agent: &ureq::Agent) -> String {
    let resp = agent.get("http://cip.cc/").call();
    if let Ok(resp) = resp {
        if let Ok(text) = resp.into_string() {
            for line in text.lines() {
                if line.contains("城市") {
                    if let Some(pos) = line.find(':').or(line.find('：')) {
                        let city = line[pos + 1..].trim().to_string();
                        if !city.is_empty() {
                            return city;
                        }
                    }
                }
            }
        }
    }
    "济南".to_string()
}

// ---------- 电量 ----------

const FL_MAX: i32 = 24;

fn read_frontlight() -> Option<i32> {
    let out = std::process::Command::new("lipc-get-prop")
        .args(["com.lab126.powerd", "flIntensity"])
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

const FL_BRIGHTNESS: &str = "/sys/class/backlight/max77696-bl/brightness";

fn set_frontlight(v: i32) {
    let v = v.clamp(0, FL_MAX);
    // lipc 的 0 档实际写 sysfs brightness=1（PWM 最低档，暗处仍有微光），
    // 只有直写 sysfs 0 才是真正关断前光。
    if v == 0 {
        let _ = std::fs::write(FL_BRIGHTNESS, "0");
        return;
    }
    let _ = std::process::Command::new("lipc-set-prop")
        .args(["com.lab126.powerd", "flIntensity"])
        .arg(v.to_string())
        .status();
}

fn fetch_battery() -> String {
    // Kindle(PW2): max77696-battery/capacity；其他机型也走同目录，遍历取第一个可解析的
    if let Ok(entries) = std::fs::read_dir("/sys/class/power_supply") {
        for e in entries.flatten() {
            let p = e.path().join("capacity");
            if let Ok(s) = std::fs::read_to_string(&p) {
                if let Ok(n) = s.trim().parse::<u32>() {
                    return format!("电量 {}%", n);
                }
            }
        }
    }
    String::new()
}

// ---------- 主入口 ----------

fn main() {
    // 横屏：framebuffer 的 vinfo 保持驱动原生（758x1024），
    // 由后端在写入时旋转 90°，程序退出后书库不受影响。
    #[cfg(all(target_arch = "arm", target_os = "linux", target_env = "musl"))]
    {
        unsafe {
            std::env::set_var("KWC_WIDTH", "1024");
            std::env::set_var("KWC_HEIGHT", "758");
        }
    }

    // Kindle 后端：必须先 install 设置全局 platform，再创建组件
    #[cfg(all(target_arch = "arm", target_os = "linux", target_env = "musl"))]
    let backend = {
        let b = slint_backend_kindle::install(STHEITI_BOLD)
            .expect("failed to install Kindle backend");
        b.set_black_and_white(true);
        Arc::new(b)
    };

    let app = AppWindow::new().expect("failed to create AppWindow");
    // ponytail: 必须带超时。合盖休眠唤醒后 WiFi 未就绪时，无超时的 socket 读会永久阻塞
    // 在 sk_wait_data，主线程冻结 → 时钟定时器一起停，屏幕从此再不刷新。
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(5))
        .timeout_read(std::time::Duration::from_secs(10))
        .timeout_write(std::time::Duration::from_secs(10))
        .build();

    // 后台线程监听输入事件（电源键/触屏），设标志位；主线程定时器检查并全刷
    #[cfg(all(target_arch = "arm", target_os = "linux", target_env = "musl"))]
    let wake_flag = {
        let flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag_clone = Arc::clone(&flag);
        std::thread::spawn(move || {
            use std::io::Read;
            let log = |msg: &str| {
                let _ = std::fs::OpenOptions::new()
                    .create(true).append(true)
                    .open("/mnt/us/kwc-wake.log")
                    .and_then(|mut f| std::io::Write::write_all(&mut f, msg.as_bytes()));
            };
            log("thread started\n");
            for dev in ["/dev/input/event0", "/dev/input/event1"] {
                match std::fs::File::open(dev) {
                    Ok(mut f) => {
                        log(&format!("opened {}\n", dev));
                        let mut buf = [0u8; 16];
                        loop {
                            match f.read(&mut buf) {
                                Ok(16) => {
                                    log("event received!\n");
                                    flag_clone.store(true, std::sync::atomic::Ordering::Relaxed);
                                }
                                Ok(n) => { log(&format!("short read {} bytes, break\n", n)); break; }
                                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                    std::thread::sleep(std::time::Duration::from_millis(100));
                                }
                                Err(e) => { log(&format!("error: {}, break\n", e)); break; }
                            }
                        }
                    }
                    Err(e) => log(&format!("failed to open {}: {}\n", dev, e)),
                }
            }
            log("thread exiting\n");
        });
        flag
    };

    // 设置全局指针（mem::forget 保证永不释放）
    #[cfg(all(target_arch = "arm", target_os = "linux", target_env = "musl"))]
    {
        unsafe { KINDLE_BACKEND = &*backend as *const slint_backend_kindle::KindleBackend; }
    }
    std::mem::forget(backend);

    // 初始刷新
    update_clock(&app, &now_east8(), true);
    update_weather(&app, &agent);

    // 亮度控制 + 退出
    {
        let fl = read_frontlight().unwrap_or(20);
        app.set_brightness(fl);
        set_frontlight(fl); // 启动时把 sysfs 对齐到 UI 值（0 档必须直写 0）

        let weak = app.as_weak();
        app.on_brightness_up(move || {
            if let Some(app) = weak.upgrade() {
                let v = (app.get_brightness() + 4).min(FL_MAX);
                set_frontlight(v);
                app.set_brightness(v);
            }
        });

        let weak = app.as_weak();
        app.on_brightness_down(move || {
            if let Some(app) = weak.upgrade() {
                let v = (app.get_brightness() - 4).max(0);
                set_frontlight(v);
                app.set_brightness(v);
            }
        });

        app.on_quit(|| {
            // 42 = 主动退出；run-kwc.sh 收到后恢复 Kindle 系统并不再重启。
            // framebuffer 的 vinfo 我们一个字都没改，无需还原。
            std::process::exit(42);
        });
    }

    // 时钟定时器（1s）+ 唤醒标志检查
    {
        let weak = app.as_weak();
        #[cfg(all(target_arch = "arm", target_os = "linux", target_env = "musl"))]
        let wake_flag = Arc::clone(&wake_flag);
        let mut last_day = 0i32;
        let clock_timer = Timer::default();
        clock_timer.start(TimerMode::Repeated, CLOCK_INTERVAL, move || {
            if let Some(app) = weak.upgrade() {
                let now = now_east8();
                let day = now.day() as i32;
                let day_changed = day != last_day;
                last_day = day;
                update_clock(&app, &now, day_changed);
                #[cfg(all(target_arch = "arm", target_os = "linux", target_env = "musl"))]
                if wake_flag.swap(false, std::sync::atomic::Ordering::Relaxed) {
                    std::thread::sleep(std::time::Duration::from_millis(300));
                    unsafe { (&*KINDLE_BACKEND).request_full_refresh(); }
                }
            }
        });
        std::mem::forget(clock_timer);
    }

    // 天气定时器（10min）+ 全刷
    {
        let weak = app.as_weak();
        let agent = agent.clone();
        #[cfg(all(target_arch = "arm", target_os = "linux", target_env = "musl"))]
        let weather_timer = {
            let t = Timer::default();
            t.start(TimerMode::Repeated, WEATHER_INTERVAL, move || {
                if let Some(app) = weak.upgrade() {
                    update_weather(&app, &agent);
                    unsafe { (&*KINDLE_BACKEND).request_full_refresh(); }
                }
            });
            t
        };
        std::mem::forget(weather_timer);
    }

    app.run().expect("failed to run event loop");
}
