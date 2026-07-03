//! System tray icon management.
//! Uses Shell_NotifyIconW for notification area icon.

use windows::Win32::Foundation::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub const WM_NOTIFYICON: u32 = WM_APP + 1;

pub struct TrayIcon {
    nid: NOTIFYICONDATAW,
    niid: NOTIFYICONIDENTIFIER,
    added: bool,
}

impl TrayIcon {
    pub fn new(hwnd: HWND, tooltip: &str) -> Self {
        let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = WM_NOTIFYICON;
        nid.szTip = encode_tooltip(tooltip);
        // Set version to NOTIFYICON_VERSION_4 for proper event codes
        nid.Anonymous = NOTIFYICONDATAW_0 { uVersion: 4 };

        let mut niid: NOTIFYICONIDENTIFIER = unsafe { std::mem::zeroed() };
        niid.cbSize = std::mem::size_of::<NOTIFYICONIDENTIFIER>() as u32;
        niid.hWnd = hwnd;
        niid.uID = 1;

        TrayIcon { nid, niid, added: false }
    }

    pub fn set_icon(&mut self, icon: HICON) {
        self.nid.hIcon = icon;
    }

    pub fn set_tooltip(&mut self, text: &str) {
        self.nid.szTip = encode_tooltip(text);
        self.nid.uFlags |= NIF_TIP;
    }

    /// Add the tray icon for the first time (NIM_ADD),
    /// then set notification version for proper event routing.
    pub fn add(&mut self) -> bool {
        unsafe {
            let ok = Shell_NotifyIconW(NIM_ADD, &self.nid).as_bool();
            if ok {
                self.added = true;
                // uVersion is already set in constructor; NIM_SETVERSION uses it
                let _ = Shell_NotifyIconW(NIM_SETVERSION, &self.nid);
            }
            ok
        }
    }

    /// Update (modify) the existing tray icon.
    pub fn update(&mut self) -> bool {
        if !self.added {
            return self.add();
        }
        unsafe {
            Shell_NotifyIconW(NIM_MODIFY, &self.nid).as_bool()
        }
    }

    /// Force re-add on next update (e.g. after TaskbarCreated).
    pub fn mark_not_added(&mut self) {
        self.added = false;
    }

    pub fn get_icon_rect(&self) -> Option<RECT> {
        unsafe {
            Shell_NotifyIconGetRect(&self.niid).ok()
        }
    }

    pub fn remove(&self) {
        unsafe { let _ = Shell_NotifyIconW(NIM_DELETE, &self.nid); }
    }
}

fn encode_tooltip(s: &str) -> [u16; 128] {
    let mut buf = [0u16; 128];
    let enc: Vec<u16> = s.encode_utf16().collect();
    let len = enc.len().min(127);
    buf[..len].copy_from_slice(&enc[..len]);
    buf[len] = 0;
    buf
}
