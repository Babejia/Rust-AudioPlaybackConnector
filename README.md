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
- **Auto-mute on low phone volume** *(new in this fork)* — Silences the phone stream on the PC when the phone volume is turned down; see below
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

Download the latest release from the [Releases](https://github.com/Babejia/Rust-AudioPlaybackConnector/releases) page.

### Build from source

```powershell
git clone https://github.com/Babejia/Rust-AudioPlaybackConnector.git
cd Rust-AudioPlaybackConnector
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

## Auto-mute on low phone volume (new in this fork)

### The problem

When an iPhone is the audio source, turning its volume down **does not change the volume of that stream on Windows** — the phone attenuates the PCM internally before encoding, so the Windows volume mixer keeps showing 100% for that session. The result: even at the phone's minimum volume, the residual audio is still clearly audible on the PC.

The only signal available on the PC side is therefore **the signal level itself**.

### How it works

The app continuously reads the peak level of that session (`IAudioMeterInformation`):

- Level stays **below the threshold** for a while (4 s by default) → the session volume is driven down to about **−50 dB** (inaudible)
- Level comes back **above** it → the volume is restored immediately

Only that one stream is affected. The PC master volume and your other applications are untouched.

> **Why not simply mute?** After `SetMute(true)`, the peak meter for that session drops to 0 and the real level can no longer be read — the app could never detect that the phone got louder again, and the stream would stay silent forever. So a large attenuation (−50 dB) is used instead of a real mute, and the reading is compensated back to the true level.

### Using it

Right-click the tray icon:

| Item | Purpose |
|---|---|
| **手机小音量自动静音 (Pause)** | Master switch; the hotkey is shown in parentheses |
| **安静门槛** | Threshold presets: −34 / −38 / −40 / −42 dB |
| **以当前音量作为静音点** | One-click calibration |
| **立即静音 / 取消静音** | Manual mute toggle (same as the `Pause` hotkey) |
| status line | `device · actual level · muted` |

**Calibration (recommended):** set the phone to the volume at which you want the PC to go silent, then click **"以当前音量作为静音点"**. The app takes the peak of the last ~10 seconds and adds 2 dB as the threshold.

### Configuration

`AudioPlaybackConnector.json` next to the executable — **read only at startup**, so restart the app after editing:

| Key | Default | Meaning |
|---|---|---|
| `enabled` | `true` | Master switch |
| `threshold_db` | `-34.0` | Mute threshold (dBFS) |
| `release_db` | `-30.0` | Restore threshold; keep it at `threshold + 4` (hysteresis) |
| `hold_ms` | `4000` | How long the stream must stay quiet before muting |
| `match_key` | `""` | Force a session-name keyword; empty = auto-detect |
| `hotkey` | `"Pause"` | Manual mute hotkey, e.g. `"Ctrl+Shift+F12"`; empty = disabled |

### Limitations

- The detector works on **level**, not on the phone's volume setting — that value never reaches Windows.
- Consequently "quiet content" and "phone turned down" look identical: long silences in a livestream or a quiet passage in music also count as quiet. The 4 s hold time is what keeps normal speech pauses from being cut off.
- The usable threshold window is narrow; re-calibrate after changing phone or headphones.
- With the master switch off, the app does not poll audio at all (0% CPU); it reads once on demand when you open the menu.

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
    ├── volume.rs              # Phone-stream level gate (auto-mute)
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
