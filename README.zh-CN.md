# AudioPlaybackConnector (Rust)

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

**简体中文** | [**English**](README.md)

使用 Rust 重写的 [AudioPlaybackConnector](https://github.com/ysc3839/AudioPlaybackConnector) —— Windows 10 2004+ 蓝牙音频接收 (A2DP Sink) 连接工具。

> **原版 C++ 项目**: [ysc3839/AudioPlaybackConnector](https://github.com/ysc3839/AudioPlaybackConnector)
>
> 本项目将原版移植到 Rust，舍弃了 XAML Islands，使用纯 Win32 弹出菜单替代，同时保留了相同的核心功能。


## 功能特点

- **系统托盘运行** — 静默运行于通知区域
- **一键连接** — 左键点击托盘图标即可列出已配对的蓝牙 A2DP 设备并连接
- **自动重连** — 退出时记住已连接的设备，下次启动自动重连
- **右键菜单** — 快速访问蓝牙设置、声音设置和退出
- **主题自适应图标** — 根据系统主题自动切换亮/暗色托盘图标
- **自定义桌面图标** — 嵌入 `desk_ico/futaba.ico`
- **简化退出流程** — 一次确认对话框，自动保存重连设置
- **中文界面** — 所有菜单和对话框均已本地化为中文
- **零运行时依赖** — 完全基于 `windows` crate，无需额外框架

## 系统要求

- Windows 10 2004 (build 19041) 或更高版本
- 支持 A2DP Sink 的蓝牙适配器
- 已配对的蓝牙音频设备（耳机、音箱等）

## 使用方法

### 下载

从 [Releases](https://github.com/l1zmooo/Rust-AudioPlaybackConnector/releases) 页面下载最新版本。

### 从源码构建

```powershell
git clone https://github.com/l1zmooo/Rust-AudioPlaybackConnector.git
cd BuleTooth_Audio
cargo run --release
```

### 使用步骤

1. 运行 `AudioPlaybackConnector` — 程序会在系统托盘（右下角）显示图标
2. **先配对你的蓝牙设备**：右键托盘图标 → "蓝牙设置"
3. **左键点击**托盘图标 → 从弹出菜单中选择你的设备
4. 连接完成后，音频会自动路由到蓝牙设备
5. 如果听不到声音，右键 → "声音设置" → 选择蓝牙设备作为默认输出

### 连接 / 断开

- **连接**：左键托盘图标 → 点击设备名称
- **断开**：左键托盘图标 → 点击已连接的设备（标记有 ✓）

## 项目结构

```
BuleTooth_Audio/
├── Cargo.toml                 # Rust 项目配置
├── build.rs                   # 构建脚本（嵌入 manifest 和图标）
├── app.manifest               # Windows 兼容性清单 (PerMonitorV2 DPI)
├── app.rc                     # 图标资源脚本
├── LICENSE
├── desk_ico/
│   └── futaba.ico             # 桌面图标（嵌入到可执行文件）
├── AudioPlaybackConnector.svg # 托盘图标源文件（喇叭符号）
└── src/
    ├── main.rs                # 入口点、窗口过程、消息循环
    ├── tray.rs                # 系统托盘 (Shell_NotifyIconW)
    ├── device.rs              # 蓝牙 A2DP 设备枚举与连接管理
    ├── config.rs              # JSON 配置持久化 (serde_json)
    ├── menu.rs                # Win32 弹出菜单
    ├── icon.rs                # Direct2D SVG → HICON 渲染
    └── i18n.rs                # 字符串表（可扩展国际化，暂未使用）
```

## 技术说明

### 与原版 C++ 的关键区别

| 方面 | C++ 原版 | Rust 重写 |
|------|---------|-----------|
| UI 框架 | XAML Islands + DevicePicker | 纯 Win32 弹出菜单 |
| 蓝牙 API | C++/WinRT 协程 | `windows` crate + `.get()` 同步等待 |
| 图标渲染 | Direct2D SVG | Direct2D SVG（相同方案） |
| 配置 | WIL + 手动 JSON | serde_json |
| 错误处理 | 异常 + WIL 宏 | `Result<T, E>` + `?` 操作符 |
| 依赖项 | C++/WinRT, WIL, XAML | 仅 `windows` crate |

### 已知问题

- 设备状态不会实时刷新——需要重新打开菜单查看最新状态
- 包含特殊字符（⚠）的错误信息可能在某些字体中显示异常

## 许可证

本项目基于 MIT 许可证开源。详见 [LICENSE](LICENSE)。

## 致谢

- [ysc3839](https://github.com/ysc3839) 的原版 [AudioPlaybackConnector](https://github.com/ysc3839/AudioPlaybackConnector) 项目
- Microsoft 的 [windows-rs](https://github.com/microsoft/windows-rs) crate
