//! Internationalization support.
//! Since the Rust version drops XAML Islands, we use a simplified string-table approach.
//! All UI strings are defined here and can be replaced with translations.

use std::collections::HashMap;

// String ID enum for all UI strings
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum StrId {
    AppTitle,
    Connected,
    Connecting,
    Disconnected,
    UnknownError,
    RequestTimedOut,
    DeniedBySystem,
    BluetoothSettings,
    Exit,
    ExitQuestion,
    ExitDetail,
    ReconnectNextStart,
    UnsupportedOS,
    UnsupportedOSDetail,
    NoDevices,
    ClickToConnect,
    ClickToDisconnect,
}

/// String table: maps StrId -> localized text
pub struct I18n {
    strings: HashMap<StrId, &'static str>,
}

impl I18n {
    pub fn new() -> Self {
        let mut s = HashMap::new();
        // English (default)
        s.insert(StrId::AppTitle, "AudioPlaybackConnector");
        s.insert(StrId::Connected, "Connected");
        s.insert(StrId::Connecting, "Connecting…");
        s.insert(StrId::Disconnected, "Disconnected");
        s.insert(StrId::UnknownError, "Unknown error");
        s.insert(StrId::RequestTimedOut, "The request timed out");
        s.insert(StrId::DeniedBySystem, "The operation was denied by the system");
        s.insert(StrId::BluetoothSettings, "Bluetooth Settings");
        s.insert(StrId::Exit, "Exit");
        s.insert(StrId::ExitQuestion, "All connections will be closed.\nExit anyway?");
        s.insert(StrId::ExitDetail, "");
        s.insert(StrId::ReconnectNextStart, "Reconnect on next start");
        s.insert(StrId::UnsupportedOS, "Unsupported Operating System");
        s.insert(StrId::UnsupportedOSDetail, "AudioPlaybackConnector is not supported on this operating system version.");
        s.insert(StrId::NoDevices, "(No Bluetooth A2DP devices found)");
        s.insert(StrId::ClickToConnect, "Connect");
        s.insert(StrId::ClickToDisconnect, "Disconnect");
        I18n { strings: s }
    }

    pub fn get(&self, id: StrId) -> &str {
        self.strings.get(&id).copied().unwrap_or("")
    }
}
