# AudioPlaybackConnector (Rust)

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

[**简体中文**](README.zh-CN.md) | **English**

A Rust rewrite of [AudioPlaybackConnector](https://github.com/ysc3839/AudioPlaybackConnector) — a Bluetooth audio playback (A2DP Sink) connection manager for Windows 10 2004+.

> **Original C++ project**: [ysc3839/AudioPlaybackConnector](https://github.com/ysc3839/AudioPlaybackConnector)
>
> This project ports the original to idiomatic Rust, replacing XAML Islands with pure Win32 popup menus while preserving the same core functionality.


## Features

- **System tray icon** — Runs quietly in the notification area
- **One-click connect** — Left-click the tray icon to list paired Bluetooth A2DP devices and connect
- **Auto reconnect** — Remembers connected devices on exit, auto-reconnects on next start
- **Right-click menu** — Quick access to Bluetooth Settings, Sound Settings, and Exit
- **Theme-aware icon** — Automatically switches between light/dark tray icon based on system theme
- **Custom app icon** — Desktop icon embedded from `desk_ico/futaba.ico`
- **Simplified exit flow** — One confirmation dialog, reconnection preference saved automatically
- **Simplified Chinese UI** — All menus and dialogs localized to Chinese
- **Zero runtime dependencies** — Built entirely on the `windows` crate, no extra frameworks required

## Requirements

- Windows 10 2004 (build 19041) or later
- A Bluetooth adapter supporting A2DP Sink
- Paired Bluetooth audio device (headphones, speaker, etc.)

## Usage

### Download

Download the latest release from the [Releases](https://github.com/l1zmooo/Rust-AudioPlaybackConnector/releases) page.

### Build from source

```powershell
git clone https://github.com/l1zmooo/Rust-AudioPlaybackConnector.git
cd BuleTooth_Audio
cargo run --release
```

### How to use

1. Run `AudioPlaybackConnector` — it appears as an icon in the system tray (bottom-right corner)
2. **Pair a Bluetooth device** first via Windows Bluetooth Settings (right-click the tray icon → "蓝牙设置")
3. **Left-click** the tray icon → select your device from the popup menu
4. The device will connect, and audio will be routed to it
5. If you don't hear sound, right-click → "声音设置" and select your Bluetooth device as the default output

### Connect / Disconnect

- **Connect**: Left-click tray icon → click a device name
- **Disconnect**: Left-click tray icon → click the connected device (marked with ✓)

## Project Structure

```
BuleTooth_Audio/
├── Cargo.toml                 # Rust project manifest
├── build.rs                   # Build script (manifest + icon embedding)
├── app.manifest               # Windows compatibility manifest (PerMonitorV2 DPI)
├── app.rc                     # Icon resource script
├── LICENSE
├── desk_ico/
│   └── futaba.ico             # Desktop icon (embedded into executable)
├── AudioPlaybackConnector.svg # Tray icon source (speaker symbol)
└── src/
    ├── main.rs                # Entry point, window procedure, message loop
    ├── tray.rs                # System tray (Shell_NotifyIconW)
    ├── device.rs              # Bluetooth A2DP enumeration & connection
    ├── config.rs              # JSON configuration (serde_json)
    ├── menu.rs                # Win32 popup menus
    ├── icon.rs                # Direct2D SVG → HICON rendering
    └── i18n.rs                # String table (extensible i18n, currently unused)
```

## Technical Notes

### Key differences from the original C++ version

| Aspect | C++ Original | Rust Rewrite |
|--------|-------------|--------------|
| UI Framework | XAML Islands + DevicePicker | Pure Win32 popup menus |
| Bluetooth API | C++/WinRT coroutines | `windows` crate + `.get()` sync wait |
| Icon Rendering | Direct2D SVG | Direct2D SVG (same approach) |
| Config | WIL + manual JSON | serde_json |
| Error Handling | Exceptions + WIL macros | `Result<T, E>` + `?` operator |
| Dependencies | C++/WinRT, WIL, XAML | `windows` crate only |

### Known Issues

- Device status doesn't refresh in real-time — re-open the menu to see latest state
- Error messages with special characters (⚠) may not render correctly in all fonts

## License

This project is licensed under the MIT License. See [LICENSE](LICENSE) for details.

## Acknowledgments

- [ysc3839](https://github.com/ysc3839) for the original [AudioPlaybackConnector](https://github.com/ysc3839/AudioPlaybackConnector) project
- Microsoft for the [windows-rs](https://github.com/microsoft/windows-rs) crate
