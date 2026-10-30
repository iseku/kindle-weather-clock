use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use chrono::{Datelike, Local, Timelike};
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
        app.set_temp(SharedString::from(format!("{}°C", sk.temp)));
        app.set_wind(SharedString::from(format!("{} {}", sk.WD, sk.WS)));
        app.set_humidity(SharedString::from(format!("湿度 {}", sk.SD)));
        app.set_pressure(SharedString::from(format!("气压 {}", sk.qy)));
        app.set_aqi(SharedString::from(format!("AQI {}", sk.aqi)));
    }

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

fn update_clock(app: &AppWindow) {
    let now = Local::now();
    app.set_time(SharedString::from(format!(
        "{:02}:{:02}",
        now.hour(),
        now.minute()
    )));
    app.set_date(SharedString::from(now.format("%Y-%m-%d").to_string()));

    let weekday = match now.weekday() {
        chrono::Weekday::Mon => "星期一",
        chrono::Weekday::Tue => "星期二",
        chrono::Weekday::Wed => "星期三",
        chrono::Weekday::Thu => "星期四",
        chrono::Weekday::Fri => "星期五",
        chrono::Weekday::Sat => "星期六",
        chrono::Weekday::Sun => "星期日",
    };
    app.set_weekday(SharedString::from(weekday));

    // 农历
    let solar = Solar::from_ymd(now.year() as i32, now.month() as i32, now.day() as i32).unwrap();
    let lunar = solar.lunar();
    app.set_lunar(SharedString::from(format!(
        "{}{}",
        lunar.month_in_chinese(),
        lunar.day_in_chinese()
    )));

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

// ---------- 主入口 ----------

fn main() {
    // 横屏：设 framebuffer rotate=1（90°），后端 install 后读到 1024x758
    #[cfg(all(target_arch = "arm", target_os = "linux", target_env = "musl"))]
    {
        use std::os::unix::io::AsRawFd;
        #[repr(C)]
        #[derive(Default)]
        struct FbVar {
            xres: u32, yres: u32, xres_virtual: u32, yres_virtual: u32,
            xoffset: u32, yoffset: u32, bits_per_pixel: u32, grayscale: u32,
            red: [u32; 3], green: [u32; 3], blue: [u32; 3], transp: [u32; 3],
            nonstd: u32, activate: u32, height: u32, width: u32, accel_flags: u32,
            pixclock: u32, left_margin: u32, right_margin: u32, upper_margin: u32,
            lower_margin: u32, hsync_len: u32, vsync_len: u32, sync: u32,
            vmode: u32, rotate: u32, colorspace: u32, reserved: [u32; 4],
        }
        const FBIOSET_VSCREENINFO: libc::c_ulong = 0x4601;
        const FBIOGET_VSCREENINFO: libc::c_ulong = 0x4600;
        // 保持 fd 打开，防止驱动在关闭时重置 vinfo
        static mut FB_FILE: *mut std::fs::File = std::ptr::null_mut();
        if let Ok(fb) = std::fs::OpenOptions::new().read(true).write(true).open("/dev/fb0") {
            let mut v = FbVar::default();
            unsafe { libc::ioctl(fb.as_raw_fd(), FBIOGET_VSCREENINFO as _, &mut v as *mut _ as *mut libc::c_void); }
            v.rotate = 2;
            // 横屏：互换宽高，让 Slint 后端读到 1024x758
            let tmp = v.xres; v.xres = v.yres; v.yres = tmp;
            let tmp = v.xres_virtual; v.xres_virtual = v.yres_virtual; v.yres_virtual = tmp;
            unsafe { libc::ioctl(fb.as_raw_fd(), FBIOSET_VSCREENINFO as _, &mut v as *mut _ as *mut libc::c_void); }
            // 读回驱动实际设的值
            let mut v2 = FbVar::default();
            unsafe { libc::ioctl(fb.as_raw_fd(), FBIOGET_VSCREENINFO as _, &mut v2 as *mut _ as *mut libc::c_void); }
            let _ = std::fs::write("/mnt/us/kwc-fb-debug.txt",
                format!("before: xres={} yres={} xv={} yv={} rotate={}\nafter:  xres={} yres={} xv={} yv={} rotate={}\n",
                    v.xres, v.yres, v.xres_virtual, v.yres_virtual, v.rotate,
                    v2.xres, v2.yres, v2.xres_virtual, v2.yres_virtual, v2.rotate));
            // 不关闭 fd——驱动在 fd 关闭时会重置 vinfo
            unsafe {
                let old = FB_FILE;
                FB_FILE = Box::into_raw(Box::new(fb));
                if !old.is_null() { drop(Box::from_raw(old)); }
            }
        }
    }

    // 横屏：告诉后端用 1024x758 渲染（驱动内部仍用 758x1024，rotate=2 负责旋转）
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
    let agent = ureq::Agent::new();

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
    update_clock(&app);
    update_weather(&app, &agent);

    // 时钟定时器（1s）+ 唤醒标志检查
    {
        let weak = app.as_weak();
        #[cfg(all(target_arch = "arm", target_os = "linux", target_env = "musl"))]
        let wake_flag = Arc::clone(&wake_flag);
        let clock_timer = Timer::default();
        clock_timer.start(TimerMode::Repeated, CLOCK_INTERVAL, move || {
            if let Some(app) = weak.upgrade() {
                update_clock(&app);
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
