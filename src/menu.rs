//! Win32 popup menu management.

use std::sync::atomic::{AtomicU32, Ordering};
use windows::core::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const IDM_DEVICE_BASE: u32 = 1000;
pub const IDM_SETTINGS: u32 = 1;
pub const IDM_SOUND: u32 = 3;
pub const IDM_EXIT: u32 = 2;

/// 静音门相关命令
pub const IDM_GATE_TOGGLE: u32 = 10;
pub const IDM_GATE_TH_BASE: u32 = 20; // 20..24
/// 可选门槛（dBFS）。实测可用区间在 −36 ~ −39 之间，
/// 所以选项围着它铺开，不往 −50 那种永远触发不了的地方走。
pub const THRESHOLD_PRESETS: [f32; 4] = [-34.0, -38.0, -40.0, -42.0];
pub const IDM_GATE_TH_COUNT: u32 = THRESHOLD_PRESETS.len() as u32;
pub const IDM_GATE_MUTE: u32 = 30;
pub const IDM_GATE_CALIB: u32 = 40;

static NEXT_DEVICE_ID: AtomicU32 = AtomicU32::new(IDM_DEVICE_BASE);

pub fn reset_device_counter() { NEXT_DEVICE_ID.store(IDM_DEVICE_BASE, Ordering::SeqCst); }
pub fn alloc_device_id() -> u32 { NEXT_DEVICE_ID.fetch_add(1, Ordering::SeqCst) }
pub fn is_device_command(id: u32) -> bool { id >= IDM_DEVICE_BASE }

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub struct MenuState<'a> {
    pub enabled: bool,
    pub threshold_db: f32,
    pub manual_muted: bool,
    pub hotkey: &'a str,
    pub status: &'a str,
    /// 当前有可用的音轨电平，才能做标定
    pub can_calibrate: bool,
}

pub fn create_context_menu(st: &MenuState) -> HMENU {
    unsafe {
        let menu = CreatePopupMenu().expect("CreatePopupMenu failed");

        let _ = AppendMenuW(menu, MF_STRING, IDM_SETTINGS as usize, PCWSTR(wide("蓝牙设置").as_ptr()));
        let _ = AppendMenuW(menu, MF_STRING, IDM_SOUND as usize, PCWSTR(wide("声音设置").as_ptr()));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());

        // —— 手机音轨自动静音 ——
        let gate_label = if st.hotkey.trim().is_empty() {
            "手机小音量自动静音".to_string()
        } else {
            format!("手机小音量自动静音（{}）", st.hotkey.trim())
        };
        let _ = AppendMenuW(menu, MF_STRING, IDM_GATE_TOGGLE as usize, PCWSTR(wide(&gate_label).as_ptr()));
        let _ = CheckMenuItem(
            menu,
            IDM_GATE_TOGGLE,
            (MF_BYCOMMAND | if st.enabled { MF_CHECKED } else { MF_UNCHECKED }).0,
        );

        // 门槛子菜单
        let sub = CreatePopupMenu().expect("CreatePopupMenu failed");
        for (i, preset) in THRESHOLD_PRESETS.iter().enumerate() {
            let id = IDM_GATE_TH_BASE + i as u32;
            let label = format!("{} dB", *preset as i32);
            let _ = AppendMenuW(sub, MF_STRING, id as usize, PCWSTR(wide(&label).as_ptr()));
            let _ = CheckMenuItem(
                sub,
                id,
                (MF_BYCOMMAND
                    | if (*preset - st.threshold_db).abs() < 0.5 { MF_CHECKED } else { MF_UNCHECKED })
                    .0,
            );
        }
        let sub_label = format!("安静门槛（{} dB）", st.threshold_db as i32);
        let _ = AppendMenuW(menu, MF_POPUP, sub.0 as usize, PCWSTR(wide(&sub_label).as_ptr()));

        // 一键标定：把手机调到想要静音的那一档，点这里
        let calib_flags = if st.can_calibrate { MF_STRING } else { MF_STRING | MF_GRAYED };
        let _ = AppendMenuW(menu, calib_flags, IDM_GATE_CALIB as usize, PCWSTR(wide("以当前音量作为静音点").as_ptr()));

        let mute_label = if st.manual_muted { "取消静音" } else { "立即静音" };
        let _ = AppendMenuW(menu, MF_STRING, IDM_GATE_MUTE as usize, PCWSTR(wide(mute_label).as_ptr()));

        let _ = AppendMenuW(menu, MF_STRING | MF_GRAYED, 0, PCWSTR(wide(st.status).as_ptr()));

        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(menu, MF_STRING, IDM_EXIT as usize, PCWSTR(wide("退出").as_ptr()));
        menu
    }
}

pub fn create_device_menu(devices: &[crate::device::DeviceEntry]) -> (HMENU, Vec<(u32, String)>) {
    unsafe {
        let menu = CreatePopupMenu().expect("CreatePopupMenu failed");
        let mut mapping = Vec::new();
        reset_device_counter();

        if devices.is_empty() {
            let _ = AppendMenuW(menu, MF_STRING | MF_GRAYED, 999 as usize, w!("(未找到蓝牙设备)"));
        } else {
            for dev in devices {
                let cid = alloc_device_id();
                let label = match &dev.state {
                    crate::device::DeviceState::Connected => format!("{} ✓ 已连接", dev.name),
                    crate::device::DeviceState::Connecting => format!("{} …", dev.name),
                    crate::device::DeviceState::Failed(e) => format!("{} ⚠ {}", dev.name, e),
                    crate::device::DeviceState::Available => dev.name.clone(),
                };
                let wide_null = label.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
                let flags = match dev.state {
                    crate::device::DeviceState::Connecting => MF_STRING | MF_GRAYED,
                    _ => MF_STRING,
                };
                let _ = AppendMenuW(menu, flags, cid as usize, PCWSTR(wide_null.as_ptr()));
                mapping.push((cid, dev.id.clone()));
            }
        }
        (menu, mapping)
    }
}
