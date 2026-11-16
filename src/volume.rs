//! 手机音轨自动静音门。
//!
//! 背景：iPhone 走 A2DP Sink 时调音量**不会**改 Windows 的会话音量（会话恒为
//! 100%），它在手机内部先把 PCM 压小再编码。所以电脑这边只能从「信号电平」
//! 判断手机音量高低。
//!
//! 关键难点：会话一旦被 `SetMute(true)`，它的峰值电表会变成 0，从此再也看不到
//! 真实电平，判定就会永久卡在“很安静”。
//!
//! 解决办法：**不用真静音，改用把会话音量压到极低（约 −50 dB）**。
//! 这样电表依然跟着信号走，只要把读数按衰减量补偿回去，就能持续得到真实电平：
//!   - 手机调大 → 补偿后的电平上升 → 立刻还原音量
//!   - 手机很小 → 电平低于门槛 → 压到 −50 dB（听不见）
//! 全程连续测量，没有任何“开窗探测”，所以不会有周期性漏音。

use std::sync::Mutex;
use std::time::Instant;
use windows::core::*;
use windows::Win32::Foundation::BOOL;
use windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation;
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::*;

use crate::config::GateSettings;

/// 自动静音时施加的会话增益（约 −50 dB，听不见但电表仍可测）
const SILENCE_GAIN: f32 = 0.0032;

struct Bound {
    /// 持有它保证会话对象存活
    _ctl: IAudioSessionControl,
    meter: IAudioMeterInformation,
    vol: ISimpleAudioVolume,
    name: String,
}

// COM 接口指针本身不是 Send；这里只从 UI 线程（定时器/菜单）访问，
// unsafe impl 只是为了让静态 Mutex 能成立。
unsafe impl Send for Bound {}

/// 衰减是否反映在电表读数上（用于把读数补偿回真实电平）
#[derive(Clone, Copy, PartialEq)]
enum Calib {
    /// 还没验证过
    Unknown,
    /// 电表读数会被会话增益缩放 → 需要除以增益
    Reflected,
    /// 电表读数是施加增益之前的原始电平 → 不用补偿
    Raw,
}

struct Gate {
    bound: Option<Bound>,
    /// 当前是否由我们压低音量
    silenced: bool,
    /// 手动钉住静音（热键切换，真静音）
    manual_mute: bool,
    quiet_since: Option<Instant>,
    last_try: Option<Instant>,
    ticks: u32,
    calib: Calib,
    /// 压低音量那一刻读到的原始电平，用来判断电表是否被缩放
    ref_before: f32,
    /// 是否已用这一刻的读数做过校准
    calib_probed: bool,
    /// 最近十几秒的真实电平峰值（dB，按 1.8 dB/秒 衰减）——标定时用它，
    /// 避免正好赶上音乐里一个安静的瞬间导致门槛设低
    recent_db: f32,
    /// 上面这个值最后一次更新的时间
    max_at: Option<Instant>,
}

static GATE: Mutex<Option<Gate>> = Mutex::new(None);

pub fn init() {
    let mut g = GATE.lock().unwrap();
    *g = Some(Gate {
        bound: None,
        silenced: false,
        manual_mute: false,
        quiet_since: None,
        last_try: None,
        ticks: 0,
        calib: Calib::Unknown,
        ref_before: 0.0,
        calib_probed: false,
        recent_db: -120.0,
        max_at: None,
    });
}

/// 把一次电平读数记入“近期峰值”（带时间衰减，约 10 秒窗口）
fn push_level(gate: &mut Gate, real: f32) {
    let cur = linear_to_db(real);
    if cur <= -119.0 {
        return;
    }
    let now = Instant::now();
    let decayed = match gate.max_at {
        Some(t) => {
            let dt = t.elapsed().as_secs_f32();
            if dt > 5.0 {
                // 太久没更新，重新开始
                cur
            } else {
                gate.recent_db - 1.8 * dt
            }
        }
        None => cur,
    };
    gate.recent_db = if cur > decayed { cur } else { decayed };
    gate.max_at = Some(now);
}

fn db_to_linear(db: f32) -> f32 { 10f32.powf(db / 20.0) }

fn linear_to_db(v: f32) -> f32 {
    if v <= 0.0 { -120.0 } else { 20.0 * v.log10() }
}

/// 解析形如 "Pause" / "Ctrl+Alt+F12" / "Shift+F9" 的热键描述
pub fn parse_hotkey(s: &str) -> Option<(u32, u32)> {
    const MOD_ALT: u32 = 0x0001;
    const MOD_CONTROL: u32 = 0x0002;
    const MOD_SHIFT: u32 = 0x0004;
    const MOD_WIN: u32 = 0x0008;

    let text = s.trim();
    if text.is_empty() {
        return None;
    }

    let mut mods: u32 = 0;
    let mut vk: u32 = 0;

    for part in text.split('+') {
        let p = part.trim().to_ascii_lowercase();
        match p.as_str() {
            "" => {}
            "ctrl" | "control" => mods |= MOD_CONTROL,
            "alt" => mods |= MOD_ALT,
            "shift" => mods |= MOD_SHIFT,
            "win" | "super" => mods |= MOD_WIN,
            "pause" | "break" => vk = 0x13,
            "scrolllock" | "scroll" => vk = 0x91,
            "insert" | "ins" => vk = 0x2D,
            "home" => vk = 0x24,
            "end" => vk = 0x23,
            "space" => vk = 0x20,
            other => {
                if other.len() == 1 {
                    let c = other.chars().next().unwrap().to_ascii_uppercase();
                    if c.is_ascii_alphanumeric() {
                        vk = c as u32;
                    }
                } else if let Some(rest) = other.strip_prefix('f') {
                    if let Ok(n) = rest.parse::<u32>() {
                        if (1..=24).contains(&n) {
                            vk = 0x70 + n - 1;
                        }
                    }
                }
            }
        }
    }

    if vk == 0 { None } else { Some((mods, vk)) }
}

unsafe fn session_name(ctl: &IAudioSessionControl) -> String {
    match ctl.GetDisplayName() {
        Ok(p) => {
            let s = pwstr_to_string(p);
            CoTaskMemFree(Some(p.0 as *const _));
            s
        }
        Err(_) => String::new(),
    }
}

/// PWSTR → String（只用最基础的指针运算，避免依赖不同版本的辅助方法）
unsafe fn pwstr_to_string(p: PWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut len = 0usize;
    while *p.0.add(len) != 0 {
        len += 1;
    }
    String::from_utf16_lossy(std::slice::from_raw_parts(p.0, len))
}

/// 严格匹配：用户指定关键字，或当前连接的 A2DP 设备名出现在音轨名里。
fn is_target_strict(name: &str, s: &GateSettings) -> bool {
    if name.is_empty() {
        return false;
    }
    let n = name.to_lowercase();

    let key = s.match_key.trim().to_lowercase();
    if !key.is_empty() {
        return n.contains(&key);
    }

    let dev = s.device_name.trim().to_lowercase();
    !dev.is_empty() && n.contains(&dev)
}

/// 宽松匹配：驱动给 A2DP Sink 音轨打的标记，任何手机都在
fn is_target_loose(name: &str) -> bool {
    name.to_lowercase().contains("a2dp")
}

fn try_bind(s: &GateSettings) -> Option<Bound> {
    unsafe {
        let enumr: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let dev = enumr.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
        let mgr: IAudioSessionManager2 = dev.Activate(CLSCTX_ALL, None).ok()?;
        let sessions = mgr.GetSessionEnumerator().ok()?;
        let count = sessions.GetCount().ok()?;

        let mut loose: Option<(IAudioSessionControl, String)> = None;

        for i in 0..count {
            let ctl = match sessions.GetSession(i) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let name = session_name(&ctl);

            if is_target_strict(&name, s) {
                if let Some(b) = make_bound(ctl, name) {
                    return Some(b);
                }
                continue;
            }
            if loose.is_none() && is_target_loose(&name) {
                loose = Some((ctl, name));
            }
        }

        if let Some((ctl, name)) = loose {
            return make_bound(ctl, name);
        }
        None
    }
}

fn make_bound(ctl: IAudioSessionControl, name: String) -> Option<Bound> {
    let meter: IAudioMeterInformation = ctl.cast().ok()?;
    let vol: ISimpleAudioVolume = ctl.cast().ok()?;
    Some(Bound { _ctl: ctl, meter, vol, name })
}

/// 设置会话增益（0.0 = 真静音，1.0 = 原始音量）
fn set_gain(bound: &Bound, gain: f32) {
    let _ = unsafe { bound.vol.SetMasterVolume(gain, std::ptr::null()) };
}

fn set_mute(bound: &Bound, mute: bool) {
    let flag = if mute { BOOL(1) } else { BOOL(0) };
    let _ = unsafe { bound.vol.SetMute(flag, std::ptr::null()) };
}

fn read_peak(bound: &Bound) -> Option<f32> {
    unsafe { bound.meter.GetPeakValue() }.ok()
}

/// 供菜单显示：把当前读数换算成手机实际电平（dBFS）。
/// 即使处于压低状态也能显示真实电平，方便标定门槛。
pub fn current_level_db() -> Option<f32> {
    let g = GATE.lock().ok()?;
    let gate = g.as_ref()?;
    let b = gate.bound.as_ref()?;
    let raw = read_peak(b)?;
    // 只有在“压低音量”状态下才需要补偿；否则读数本身就是真实电平
    let source = if gate.silenced && gate.calib == Calib::Reflected {
        raw / SILENCE_GAIN
    } else {
        raw
    };
    Some(linear_to_db(source))
}

pub fn is_silenced() -> bool {
    match GATE.lock() {
        Ok(g) => g.as_ref().map(|x| x.silenced || x.manual_mute).unwrap_or(false),
        Err(_) => false,
    }
}

/// 供标定使用：最近约 10 秒的真实电平峰值（比瞬时值稳，不会刚好赶上安静瞬间）
pub fn recent_max_db() -> Option<f32> {
    let g = GATE.lock().ok()?;
    let gate = g.as_ref()?;
    gate.bound.as_ref()?;
    if gate.recent_db <= -119.0 {
        return None;
    }
    Some(gate.recent_db)
}

/// 按需刷新（打开菜单 / 点标定时调用）：绑定并读一次电平。
/// 功能关闭时不会在后台轮询，就靠这个保证菜单里仍能看到当前电平。
pub fn sync_now(s: &GateSettings) {
    let mut guard = match GATE.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    let gate = match guard.as_mut() {
        Some(g) => g,
        None => return,
    };

    if gate.bound.is_none() {
        match try_bind(s) {
            Some(b) => {
                gate.bound = Some(b);
                gate.recent_db = -120.0;
                gate.max_at = None;
            }
            None => return,
        }
    }

    let raw = match gate.bound.as_ref().and_then(read_peak) {
        Some(v) => v,
        None => {
            gate.bound = None;
            return;
        }
    };

    let real = if gate.silenced && gate.calib == Calib::Reflected {
        raw / SILENCE_GAIN
    } else {
        raw
    };
    push_level(gate, real);
}

/// 每 100ms 由 UI 线程定时器调用
pub fn tick(s: &GateSettings) {
    let mut guard = match GATE.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    let gate = match guard.as_mut() {
        Some(g) => g,
        None => return,
    };

    gate.ticks = gate.ticks.wrapping_add(1);

    // 功能关闭：完全不轮询（不枚举会话、不读电表）。
    // 菜单里的电平与标定走 sync_now()，只在用户操作时读一次。
    if !s.enabled {
        if gate.silenced || gate.manual_mute {
            if let Some(b) = &gate.bound {
                set_mute(b, false);
                set_gain(b, 1.0);
            }
            gate.silenced = false;
            gate.manual_mute = false;
        }
        gate.quiet_since = None;
        return;
    }

    // 绑定：未绑定时重试（最多 2 秒一次），并定期重新枚举以适配换设备/换手机
    let need_bind = gate.bound.is_none() || gate.ticks % 1000 == 0;
    if need_bind {
        if gate.bound.is_none() {
            if let Some(t) = gate.last_try {
                if t.elapsed().as_millis() < 2000 {
                    return;
                }
            }
        }
        gate.last_try = Some(Instant::now());
        match try_bind(s) {
            Some(b) => {
                gate.bound = Some(b);
                gate.quiet_since = None;
                gate.calib = Calib::Unknown;
                gate.calib_probed = false;
                gate.recent_db = -120.0;
                gate.max_at = None;
            }
            None => {
                gate.bound = None;
                return;
            }
        }
    }

    let raw = {
        let b = gate.bound.as_ref().unwrap();
        match read_peak(b) {
            Some(p) => p,
            None => {
                gate.bound = None;
                return;
            }
        }
    };

    // 手动静音：真静音，不参与自动判定
    if gate.manual_mute {
        return;
    }

    // 校准：压低音量后第一次读到的电平，用来判断电表是否跟着缩放
    if gate.silenced && !gate.calib_probed && gate.ref_before > 0.0 {
        gate.calib_probed = true;
        gate.calib = if raw <= gate.ref_before * 0.3 {
            Calib::Reflected
        } else {
            Calib::Raw
        };
    }

    let threshold = db_to_linear(s.threshold_db);
    let release = db_to_linear(s.release_db);
    let hold_ms = s.hold_ms as u128;

    // 真实电平：压低音量且电表会被缩放时，把读数补偿回去
    let real = if gate.silenced && gate.calib == Calib::Reflected {
        raw / SILENCE_GAIN
    } else {
        raw
    };

    // 记录近期峰值（供标定使用）
    push_level(gate, real);

    if gate.silenced {
        // 已经压低音量：真实电平回来了就恢复
        if real > release {
            if let Some(b) = &gate.bound {
                set_gain(b, 1.0);
            }
            gate.silenced = false;
            gate.quiet_since = None;
        }
        return;
    }

    // 正常判定：此时 real 就是真实电平
    if real < threshold {
        let now = Instant::now();
        if gate.quiet_since.is_none() {
            gate.quiet_since = Some(now);
        }
        let quiet_ms = gate.quiet_since.unwrap().elapsed().as_millis();
        if quiet_ms >= hold_ms {
            if let Some(b) = &gate.bound {
                set_gain(b, SILENCE_GAIN);
            }
            gate.silenced = true;
            gate.ref_before = raw;
            gate.calib_probed = false;
            gate.quiet_since = None;
        }
    } else {
        gate.quiet_since = None;
    }
}

/// 热键：切换手动静音，返回切换后的状态
pub fn toggle_manual_mute() -> bool {
    let mut guard = match GATE.lock() {
        Ok(g) => g,
        Err(_) => return false,
    };
    let gate = match guard.as_mut() {
        Some(g) => g,
        None => return false,
    };

    gate.manual_mute = !gate.manual_mute;
    let want = gate.manual_mute;
    if let Some(b) = &gate.bound {
        if want {
            set_gain(b, 1.0);
            set_mute(b, true);
        } else {
            set_mute(b, false);
            set_gain(b, 1.0);
        }
    }
    gate.silenced = false;
    gate.quiet_since = None;
    want
}

pub fn is_manual_muted() -> bool {
    match GATE.lock() {
        Ok(g) => g.as_ref().map(|x| x.manual_mute).unwrap_or(false),
        Err(_) => false,
    }
}

pub fn bound_name() -> Option<String> {
    match GATE.lock() {
        Ok(g) => g.as_ref().and_then(|x| x.bound.as_ref().map(|b| b.name.clone())),
        Err(_) => None,
    }
}

/// 断开 / 退出前的清理：解除静音、还原音量并清空状态。
/// 即使当前不是由我们压低的，也把音量还原到 100%，避免上次异常退出留下哑音轨。
pub fn release_all() {
    let mut guard = match GATE.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    if let Some(gate) = guard.as_mut() {
        if let Some(b) = &gate.bound {
            set_mute(b, false);
            set_gain(b, 1.0);
        }
        gate.silenced = false;
        gate.manual_mute = false;
        gate.bound = None;
        gate.quiet_since = None;
        gate.calib = Calib::Unknown;
        gate.calib_probed = false;
    }
}
