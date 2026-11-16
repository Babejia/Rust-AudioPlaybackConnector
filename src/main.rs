//! AudioPlaybackConnector (Rust)
//! Bluetooth A2DP Sink connection manager for Windows 10 2004+.
//!
//! Windows GUI application — no console window.
#![windows_subsystem = "windows"]

mod config;
mod device;
mod i18n;
mod icon;
mod menu;
mod tray;
mod volume;

use config::AppConfig;
use device::DeviceState;
use icon::d2d_color;

use std::sync::Mutex;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::System::Registry::*;
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS,
};
use windows::Win32::UI::WindowsAndMessaging::*;

// ─── Global Application State ────────────────────────────────────────

struct AppState {
    hwnd: HWND,
    tray: crate::tray::TrayIcon,
    icon_light: HICON,
    icon_dark: HICON,
    reconnect: bool,
    last_devices: Vec<String>,
    device_mapping: Vec<(u32, String)>,
    settings: config::GateSettings,
}

unsafe impl Send for AppState {}
static APP_STATE: Mutex<Option<AppState>> = Mutex::new(None);

// ─── Custom Window Messages ──────────────────────────────────────────

const WM_NOTIFYICON: u32 = WM_APP + 1;
const WM_CONNECTDEVICE: u32 = WM_APP + 2;
static WM_TASKBAR_CREATED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// 静音门轮询定时器
const TIMER_GATE: usize = 1;
/// 全局热键 ID
const HOTKEY_ID: i32 = 1;

// ─── Window Procedure ────────────────────────────────────────────────

extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_DESTROY => {
                on_destroy(hwnd);
                LRESULT(0)
            }
            WM_TIMER => {
                if wparam.0 == TIMER_GATE {
                    tick_gate();
                }
                LRESULT(0)
            }
            WM_HOTKEY => {
                let _ = volume::toggle_manual_mute();
                LRESULT(0)
            }
            WM_SETTINGCHANGE => {
                if lparam.0 != 0 {
                    let ptr = lparam.0 as *const u16;
                    let len = (0..).take_while(|&i| *ptr.add(i) != 0).count();
                    let s = std::slice::from_raw_parts(ptr, len);
                    if let Ok(str) = String::from_utf16(s) {
                        if str == "ImmersiveColorSet" {
                            update_notify_icon();
                        }
                    }
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_NOTIFYICON => {
                // lParam low word = notification event code
                // NOTIFYICON_VERSION_4:
                //   Left click  → WM_LBUTTONDOWN (0x201)
                //   Right click → WM_CONTEXTMENU (0x7B = 123)
                // Fallback (older version):
                //   Select → NIN_SELECT (0x400)
                let action = (lparam.0 as u32) & 0xFFFF;
                match action {
                    0x0201 | 0x0202 | 0x0400 | 0x0401 => on_tray_left_click(hwnd),
                    0x0204 | 0x0205 => {} // WM_RBUTTONDOWN/UP — WM_CONTEXTMENU follows
                    123 => on_tray_right_click(hwnd),
                    _ => {}
                }
                LRESULT(0)
            }
            WM_COMMAND => {
                let cmd_id = (wparam.0 as u32) & 0xFFFF;
                handle_menu_command(hwnd, cmd_id);
                LRESULT(0)
            }
            WM_CONNECTDEVICE => {
                on_reconnect();
                LRESULT(0)
            }
            _ => {
                let tb = WM_TASKBAR_CREATED.load(std::sync::atomic::Ordering::Relaxed);
                if tb != 0 && msg == tb {
                    // Taskbar was recreated — re-add tray icon
                    let mut guard = APP_STATE.lock().unwrap();
                    if let Some(ref mut s) = *guard {
                        s.tray.mark_not_added();
                        s.tray.update();
                    }
                    LRESULT(0)
                } else {
                    DefWindowProcW(hwnd, msg, wparam, lparam)
                }
            }
        }
    }
}

// ─── 静音门辅助 ──────────────────────────────────────────────────────

fn current_settings() -> Option<config::GateSettings> {
    APP_STATE.lock().unwrap().as_ref().map(|s| s.settings.clone())
}

fn tick_gate() {
    if let Some(s) = current_settings() {
        volume::tick(&s);
    }
}

/// 把当前状态写回配置文件
fn save_state() {
    let snapshot = {
        let guard = APP_STATE.lock().unwrap();
        guard.as_ref().map(|s| (s.reconnect, s.last_devices.clone(), s.settings.clone()))
    };
    if let Some((reconnect, last_devices, gate)) = snapshot {
        config::save_config(&AppConfig { reconnect, last_devices, gate });
    }
}

// ─── Event Handlers ──────────────────────────────────────────────────

fn on_tray_left_click(hwnd: HWND) {
    unsafe {
        let rect = {
            APP_STATE.lock().unwrap()
                .as_ref()
                .and_then(|s| s.tray.get_icon_rect())
        };

        // Make window full-screen so menu can receive foreground focus
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, 0, 0,
            GetSystemMetrics(SM_CXSCREEN),
            GetSystemMetrics(SM_CYSCREEN), SWP_HIDEWINDOW);
        SetForegroundWindow(hwnd);

        let devices = query_devices();
        let (menu, mapping) = menu::create_device_menu(&devices);

        {
            let mut guard = APP_STATE.lock().unwrap();
            if let Some(ref mut s) = *guard { s.device_mapping = mapping; }
        }

        let x = rect.map(|r| r.left).unwrap_or(0);
        let y = rect.map(|r| r.bottom).unwrap_or(0);

        let _ = TrackPopupMenu(menu, TRACK_POPUP_MENU_FLAGS(0), x, y, 0, hwnd, None);
        let _ = DestroyMenu(menu);
    }
}

fn on_tray_right_click(hwnd: HWND) {
    unsafe {
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);

        let _ = SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 1, 1, SWP_SHOWWINDOW);
        SetForegroundWindow(hwnd);

        let (enabled, threshold, hotkey) = APP_STATE
            .lock()
            .unwrap()
            .as_ref()
            .map(|s| (s.settings.enabled, s.settings.threshold_db, s.settings.hotkey.clone()))
            .unwrap_or((false, -34.0, String::new()));

        let status = match volume::bound_name() {
            Some(n) => {
                let lv = volume::current_level_db()
                    .map(|d| format!("{:.1} dB", d))
                    .unwrap_or_else(|| "—".to_string());
                if volume::is_silenced() {
                    format!("{} · {} · 已静音", n, lv)
                } else {
                    format!("{} · {}", n, lv)
                }
            }
            None => "未找到手机音轨（手机未播放？）".to_string(),
        };

        let muted = volume::is_manual_muted();
        let menu = menu::create_context_menu(enabled, threshold, muted, &hotkey, &status);
        let _ = TrackPopupMenu(menu, TRACK_POPUP_MENU_FLAGS(0), pt.x, pt.y, 0, hwnd, None);
        let _ = DestroyMenu(menu);
    }
}

fn on_destroy(hwnd: HWND) {
    unsafe {
        let _ = KillTimer(hwnd, TIMER_GATE);
        let _ = UnregisterHotKey(hwnd, HOTKEY_ID);
        volume::release_all();
        device::close_all_connections();

        save_state();

        let guard = APP_STATE.lock().unwrap();
        if let Some(ref state) = *guard {
            state.tray.remove();
        }
        PostQuitMessage(0);
    }
}

fn on_reconnect() {
    let ids: Vec<String> = {
        APP_STATE.lock().unwrap()
            .as_ref()
            .filter(|s| s.reconnect)
            .map(|s| s.last_devices.clone())
            .unwrap_or_default()
    };
    for id in &ids {
        if !id.is_empty() {
            let _ = device::connect_to_device(id);
        }
    }
}

// ─── Command Handling ────────────────────────────────────────────────

fn handle_menu_command(hwnd: HWND, cmd_id: u32) {
    if menu::is_device_command(cmd_id) {
        handle_device_command(cmd_id);
        return;
    }

    let th_last = menu::IDM_GATE_TH_BASE + menu::IDM_GATE_TH_COUNT - 1;

    match cmd_id {
        menu::IDM_SETTINGS => {
            // Open Bluetooth settings
            let _ = std::process::Command::new("cmd")
                .args(["/c", "start", "ms-settings:bluetooth"])
                .spawn();
        }
        menu::IDM_SOUND => {
            // Open Sound settings so user can select the Bluetooth device
            let _ = std::process::Command::new("cmd")
                .args(["/c", "start", "ms-settings:sound"])
                .spawn();
        }
        menu::IDM_EXIT => on_exit_request(hwnd),
        menu::IDM_GATE_TOGGLE => {
            {
                let mut guard = APP_STATE.lock().unwrap();
                if let Some(ref mut s) = *guard {
                    s.settings.enabled = !s.settings.enabled;
                }
            }
            if !current_settings().map(|s| s.enabled).unwrap_or(false) {
                volume::release_all();
            }
            save_state();
        }
        menu::IDM_GATE_MUTE => {
            let _ = volume::toggle_manual_mute();
        }
        id if id >= menu::IDM_GATE_TH_BASE && id <= th_last => {
            let idx = (id - menu::IDM_GATE_TH_BASE) as usize;
            if let Some(preset) = menu::THRESHOLD_PRESETS.get(idx).copied() {
                let mut guard = APP_STATE.lock().unwrap();
                if let Some(ref mut s) = *guard {
                    s.settings.threshold_db = preset;
                    // 保持 4 dB 迟滞，避免在门槛附近来回跳
                    s.settings.release_db = preset + 4.0;
                }
                drop(guard);
                save_state();
            }
        }
        _ => {}
    }
}

fn handle_device_command(cmd_id: u32) {
    let device_id: Option<String> = {
        APP_STATE.lock().unwrap()
            .as_ref()
            .and_then(|s| s.device_mapping.iter().find(|(id, _)| *id == cmd_id))
            .map(|(_, did)| did.clone())
    };

    if let Some(ref id) = device_id {
        if device::is_device_connected(id) {
            device::disconnect_device(id);
            volume::release_all();
            let mut guard = APP_STATE.lock().unwrap();
            if let Some(ref mut s) = *guard {
                s.tray.set_tooltip("AudioPlaybackConnector");
                s.tray.update();
            }
        } else {
            {
                let mut guard = APP_STATE.lock().unwrap();
                if let Some(ref mut s) = *guard {
                    s.last_devices.push(id.clone());
                    s.last_devices.sort();
                    s.last_devices.dedup();
                }
            }
            match device::connect_to_device(id) {
                Ok(()) => {
                    // 记录本次连接的设备名，供音轨匹配（换手机自动适配，不写死）
                    if let Ok(list) = device::enumerate_a2dp_devices() {
                        if let Some(d) = list.iter().find(|d| &d.id == id) {
                            let mut guard = APP_STATE.lock().unwrap();
                            if let Some(ref mut s) = *guard {
                                s.settings.device_name = d.name.clone();
                            }
                        }
                    }
                    // 强制重新绑定到新音轨
                    volume::release_all();

                    let mut guard = APP_STATE.lock().unwrap();
                    if let Some(ref mut s) = *guard {
                        s.tray.set_tooltip("AudioPlaybackConnector - 已连接");
                        s.tray.update();
                    }
                }
                Err(e) => {
                    let mut guard = APP_STATE.lock().unwrap();
                    if let Some(ref mut s) = *guard {
                        s.last_devices.retain(|d| d != id);
                        s.tray.set_tooltip(&format!("AudioPlaybackConnector - {}", e));
                        s.tray.update();
                    }
                }
            }
        }
    }
}

fn on_exit_request(hwnd: HWND) {
    if device::get_connected_device_ids().is_empty() {
        unsafe { let _ = PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0)); }
        return;
    }

    // 有连接时询问是否退出，不再询问重连 — 自动保存重连状态
    unsafe {
        let r = MessageBoxW(hwnd,
            w!("所有连接将被断开。\n确定要退出吗？"),
            w!("AudioPlaybackConnector"),
            MB_YESNO | MB_ICONQUESTION | MB_DEFBUTTON2);
        if r == IDYES {
            let mut guard = APP_STATE.lock().unwrap();
            if let Some(ref mut s) = *guard {
                s.reconnect = true;
                s.last_devices = device::get_connected_device_ids();
            }
            let _ = PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}

// ─── Device Querying ─────────────────────────────────────────────────

fn query_devices() -> Vec<crate::device::DeviceEntry> {
    match device::enumerate_a2dp_devices() {
        Ok(devices) => {
            let connected = device::get_connected_device_ids();
            devices.into_iter().map(|mut d| {
                if connected.contains(&d.id) && d.state != DeviceState::Connected {
                    d.state = DeviceState::Connected;
                }
                d
            }).collect()
        }
        Err(e) => {
            eprintln!("[device] enum error: {}", e);
            Vec::new()
        }
    }
}

// ─── Icon / Theme ────────────────────────────────────────────────────

fn update_notify_icon() {
    unsafe {
        let mut val: u32 = 0;
        let mut cb: u32 = 4;
        let _ = RegGetValueW(HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD, None,
            Some(&mut val as *mut u32 as *mut std::ffi::c_void),
            Some(&mut cb));

        let icon = {
            let guard = APP_STATE.lock().unwrap();
            guard.as_ref().map(|s|
                if val != 0 { s.icon_light } else { s.icon_dark }
            ).unwrap_or(HICON::default())
        };

        let mut guard = APP_STATE.lock().unwrap();
        if let Some(ref mut s) = *guard {
            s.tray.set_icon(icon);
            s.tray.update();
        }
    }
}

// ─── Entry Point ────────────────────────────────────────────────────

fn main() -> Result<()> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        if !device::is_supported() {
            let _ = TaskDialog(None, None,
                w!("不支持的操作系统"), None,
                w!("当前操作系统版本不支持 AudioPlaybackConnector。"),
                TDCBF_OK_BUTTON, TD_ERROR_ICON, None);
            return Ok(());
        }

        let hmod = GetModuleHandleA(None)?;
        let hinst: HINSTANCE = hmod.into();

        // Load the embedded futaba.ico for the window class (shown in taskbar/Alt+Tab)
        let app_icon = LoadIconW(hinst, PCWSTR(1usize as *const u16))
            .unwrap_or_default();

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: WNDCLASS_STYLES(0),
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0, cbWndExtra: 0,
            hInstance: hinst,
            hIcon: app_icon,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hbrBackground: HBRUSH::default(),
            lpszMenuName: PCWSTR::null(),
            lpszClassName: w!("AudioPlaybackConnector"),
            hIconSm: HICON::default(),
        };
        if RegisterClassExW(&wc) == 0 {
            return Err(Error::from_win32());
        }

        let hwnd = CreateWindowExW(
            WS_EX_NOACTIVATE | WS_EX_LAYERED | WS_EX_TOPMOST,
            w!("AudioPlaybackConnector"),
            PCWSTR::null(),
            WS_POPUP,
            0, 0, 0, 0,
            None, None, hinst, None,
        )?;
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF::default(), 0, LWA_ALPHA);

        let cfg = config::load_config();

        // Build SVG icons — fall back to default system icon on error
        let svg_bytes = include_bytes!("../AudioPlaybackConnector.svg");
        let svg_str = match std::str::from_utf8(svg_bytes) {
            Ok(s) => s,
            Err(_) => {
                let _ = TaskDialog(
                    HWND::default(), HINSTANCE::default(),
                    w!("错误"), None,
                    w!("无法读取 SVG 图标资源。"),
                    TDCBF_OK_BUTTON, TD_ERROR_ICON, None);
                return Ok(());
            }
        };
        let sz = GetSystemMetrics(SM_CXSMICON);

        let il = icon::svg_to_hicon(svg_str, sz, sz, d2d_color(0.0, 0.0, 0.0, 1.0));
        let idk = icon::svg_to_hicon(svg_str, sz, sz, d2d_color(1.0, 1.0, 1.0, 1.0));

        device::init(|_did, _st| { /* state change callback — could refresh menu */ });
        volume::init();

        // Create and add tray icon — must NIM_ADD first
        let mut tray = crate::tray::TrayIcon::new(hwnd, "AudioPlaybackConnector");
        tray.set_icon(il);
        if !tray.add() {
            // If tray icon fails, show error and exit
            let _ = TaskDialog(
                HWND::default(), HINSTANCE::default(),
                w!("错误"), None,
                w!("无法创建系统托盘图标。"),
                TDCBF_OK_BUTTON, TD_ERROR_ICON, None);
            return Ok(());
        }

        let tbc = RegisterWindowMessageW(w!("TaskbarCreated"));
        WM_TASKBAR_CREATED.store(tbc, std::sync::atomic::Ordering::Relaxed);

        {
            let mut guard = APP_STATE.lock().unwrap();
            *guard = Some(AppState {
                hwnd, tray, icon_light: il, icon_dark: idk,
                reconnect: cfg.reconnect,
                settings: cfg.gate.clone(),
                last_devices: cfg.last_devices,
                device_mapping: Vec::new(),
            });
        }

        // 静音门轮询（100ms 一次，开销可忽略）
        let _ = SetTimer(hwnd, TIMER_GATE, 100, None);

        // 全局热键：手动静音/恢复
        let hotkey_text = current_settings().map(|s| s.hotkey).unwrap_or_default();
        if let Some((mods, vk)) = volume::parse_hotkey(&hotkey_text) {
            if RegisterHotKey(hwnd, HOTKEY_ID, HOT_KEY_MODIFIERS(mods), vk).is_err() {
                eprintln!("[hotkey] 注册失败（可能被其它程序占用）: {}", hotkey_text);
            }
        }

        let _ = PostMessageW(hwnd, WM_CONNECTDEVICE, WPARAM(0), LPARAM(0));

        // Message loop
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let guard = APP_STATE.lock().unwrap();
        if let Some(ref s) = *guard {
            let _ = DestroyIcon(s.icon_light);
            let _ = DestroyIcon(s.icon_dark);
        }
        Ok(())
    }
}
