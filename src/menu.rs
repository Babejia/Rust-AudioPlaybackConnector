//! Win32 popup menu management.

use std::sync::atomic::{AtomicU32, Ordering};
use windows::core::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const IDM_DEVICE_BASE: u32 = 1000;
const IDM_SETTINGS: u32 = 1;
const IDM_SOUND: u32 = 3;
const IDM_EXIT: u32 = 2;

static NEXT_DEVICE_ID: AtomicU32 = AtomicU32::new(IDM_DEVICE_BASE);

pub fn reset_device_counter() { NEXT_DEVICE_ID.store(IDM_DEVICE_BASE, Ordering::SeqCst); }
pub fn alloc_device_id() -> u32 { NEXT_DEVICE_ID.fetch_add(1, Ordering::SeqCst) }
pub fn is_device_command(id: u32) -> bool { id >= IDM_DEVICE_BASE }

pub fn create_context_menu() -> HMENU {
    unsafe {
        let menu = CreatePopupMenu().expect("CreatePopupMenu failed");
        let _ = AppendMenuW(menu, MF_STRING, IDM_SETTINGS as usize, w!("蓝牙设置"));
        let _ = AppendMenuW(menu, MF_STRING, IDM_SOUND as usize, w!("声音设置"));
        let _ = AppendMenuW(menu, MF_STRING, IDM_EXIT as usize, w!("退出"));
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
