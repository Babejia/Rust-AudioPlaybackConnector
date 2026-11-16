//! 手机音轨自动静音门。
//!
//! Windows 把手机经蓝牙传来的音频挂成「默认输出端点上的一个普通音频会话」。
//! iPhone 这类手机调音量时**不会**动会话音量（它恒为 100%），而是在手机内部
//! 先把 PCM 压小再编码，所以电脑这边只能从「信号电平」看出手机被调小了。
//!
//! 本模块持续采样该会话的峰值电平，持续低于门槛就把这条会话静音（真 0），
//! 电平回升再自动恢复。只影响手机这一路，不动电脑总音量。

use std::sync::Mutex;
use std::time::Instant;
use windows::core::*;
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::*;

use crate::config::GateSettings;

/// 已绑定的目标会话
struct Bound {
    /// 持有它保证会话对象存活
    _ctl: IAudioSessionControl,
    meter: IAudioMeterInformation,
    vol: ISimpleAudioVolume,
    name: String,
}

struct Gate {
    bound: Option<Bound>,
    /// 当前是否由我们静音
    muted: bool,
    /// 手动钉住静音（热键切换）
    manual_mute: bool,
    quiet_since: Option<Instant>,
    last_try: Option<Instant>,
    ticks: u32,
}

static GATE: Mutex<Option<Gate>> = Mutex::new(None);

pub fn init() {
    let mut g = GATE.lock().unwrap();
    *g = Some(Gate {
        bound: None,
        muted: false,
        manual_mute: false,
        quiet_since: None,
        last_try: None,
        ticks: 0,
    });
}

fn db_to_linear(db: f32) -> f32 { 10f32.powf(db / 20.0) }

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
            let s = p.to_string().unwrap_or_default();
            CoTaskMemFree(Some(p.0 as *const _));
            s
        }
        Err(_) => String::new(),
    }
}

/// 严格匹配：用户指定关键字，或当前连接的 A2DP 设备名出现在音轨名里。
/// 换手机、改设备名都不用改代码。
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
        let dev = enumr
            .GetDefaultAudioEndpoint(EDataFlow::eRender, ERole::eConsole)
            .ok()?;
        let mgr: IAudioSessionManager2 = dev.Activate(CLSCTX_ALL, None).ok()?;
        let sessions = mgr.GetSessionEnumerator().ok()?;
        let count = sessions.GetCount().ok()?;

        // 先找精确命中的，找不到再退回宽松匹配（避免认错成别的手机的残留音轨）
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
    let meter: IAudioMeterInformation = unsafe { ctl.cast() }.ok()?;
    let vol: ISimpleAudioVolume = unsafe { ctl.cast() }.ok()?;
    Some(Bound { _ctl: ctl, meter, vol, name })
}

fn set_mute(bound: &Bound, mute: bool) {
    let _ = unsafe { bound.vol.SetMute(BOOL(if mute { 1 } else { 0 }), None) };
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

    // 功能关闭：恢复原状
    if !s.enabled {
        if gate.muted {
            if let Some(b) = &gate.bound {
                set_mute(b, false);
            }
            gate.muted = false;
        }
        gate.manual_mute = false;
        gate.bound = None;
        return;
    }

    gate.ticks = gate.ticks.wrapping_add(1);

    // 绑定：未绑定时重试（最多 2 秒一次），并定期重新枚举以适配换设备/换手机
    let need_bind = gate.bound.is_none() || gate.ticks % 300 == 0;
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
            }
            None => {
                gate.bound = None;
                return;
            }
        }
    }

    let peak = {
        let b = gate.bound.as_ref().unwrap();
        match unsafe { b.meter.GetPeakValue() } {
            Ok(p) => p,
            Err(_) => {
                gate.bound = None;
                return;
            }
        }
    };

    // 手动钉住静音时不参与自动判断
    if gate.manual_mute {
        return;
    }

    let threshold = db_to_linear(s.threshold_db);
    let release = db_to_linear(s.release_db);
    let hold_ms = s.hold_ms as u128;

    if peak < threshold {
        let now = Instant::now();
        if gate.quiet_since.is_none() {
            gate.quiet_since = Some(now);
        }
        let quiet_ms = gate.quiet_since.unwrap().elapsed().as_millis();
        if !gate.muted && quiet_ms >= hold_ms {
            if let Some(b) = &gate.bound {
                set_mute(b, true);
            }
            gate.muted = true;
        }
    } else {
        gate.quiet_since = None;
        if gate.muted && peak > release {
            if let Some(b) = &gate.bound {
                set_mute(b, false);
            }
            gate.muted = false;
        }
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
        set_mute(b, want);
    }
    gate.muted = want;
    if !want {
        gate.quiet_since = None;
    }
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

/// 断开 / 退出前的清理：解除静音并解绑
pub fn release_all() {
    let mut guard = match GATE.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    if let Some(gate) = guard.as_mut() {
        if gate.muted || gate.manual_mute {
            if let Some(b) = &gate.bound {
                set_mute(b, false);
            }
        }
        gate.muted = false;
        gate.manual_mute = false;
        gate.bound = None;
        gate.quiet_since = None;
    }
}
