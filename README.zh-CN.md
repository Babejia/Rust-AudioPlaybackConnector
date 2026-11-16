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
- **手机小音量自动静音**（本项目新增）— 手机音量调小时自动压低电脑侧这一路，解决"手机已经调到最小、电脑还听得见"的问题，详见下文
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

从 [Releases](https://github.com/Babejia/Rust-AudioPlaybackConnector/releases) 页面下载最新版本。

### 从源码构建

```powershell
git clone https://github.com/Babejia/Rust-AudioPlaybackConnector.git
cd Rust-AudioPlaybackConnector
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

## 手机小音量自动静音（本项目新增）

### 它解决什么问题

用 iPhone 当音频源时，有一个很别扭的现象：**手机音量已经调到最小，电脑耳机里却还能清楚听到声音**。

原因不是手机调得不够小，而是 **iPhone 调音量根本不会改变 Windows 这一路的音量**——它在手机内部先把 PCM 压小再编码传过来，Windows 音量合成器里那条音轨始终显示 100%。手机音量档位这个信息，压根没有传给电脑。

所以只能从**信号电平**间接判断：声音整体变弱了，就认为手机被调小了。

### 它怎么工作

程序持续读取这条音轨的峰值电平（`IAudioMeterInformation`）：

- 电平**持续低于门槛**（默认 4 秒）→ 把这一路的会话音量压到约 **−50 dB**（听不见）
- 电平**回升** → 立刻还原

只影响手机这一路，**不动电脑总音量**，也不影响你打游戏时的其它声音。

> **为什么不是直接静音？**
> 一旦调用 `SetMute(true)`，这条音轨的峰值电表会立刻变成 0，程序就再也读不到真实电平，也就永远无法自动恢复（会一直哑着）。所以这里用"把音量压到 −50 dB"代替真静音，读回数据时再把衰减量补偿回去，从而保持全程连续测量。

### 使用方法

右键托盘图标：

| 菜单项 | 作用 |
|---|---|
| **手机小音量自动静音（Pause）** | 总开关；括号里是当前热键 |
| **安静门槛** | −34 / −38 / −40 / −42 dB 四档，菜单标题显示当前值 |
| **以当前音量作为静音点** | 一键标定，见下 |
| **立即静音 / 取消静音** | 手动开关（热键 `Pause` 同效） |
| 状态行 | `设备名 · 实际电平 · 已静音`，实时电平会随时刷新 |

**标定（推荐）**：把手机音量拨到**你希望电脑开始没声的那一档** → 右键 → 点「以当前音量作为静音点」。
程序会取**最近 10 秒的峰值**再 +2 dB 作为门槛——用峰值而不是瞬时值，是为了避免你点的那一瞬间正好是音乐里的安静段落。

**为什么门槛要标定**：门槛太低（如 −50 dB）会永远触发不了，太高（如 −30 dB）会误静音；而可用区间往往只有几 dB 宽，靠猜很难踩准。

### 配置项

配置文件 `AudioPlaybackConnector.json` 与 exe 同目录，**只在启动时读取**，所以改完要重启程序：

| 键 | 默认 | 说明 |
|---|---|---|
| `enabled` | `true` | 总开关 |
| `threshold_db` | `-34.0` | 静音门槛（dBFS），低于它就压低音量 |
| `release_db` | `-30.0` | 恢复门槛，高于它才还原。保持 `threshold + 4` 的迟滞，防止边界抖动 |
| `hold_ms` | `4000` | 连续安静多久才静音。看直播/语音嫌被打断就调大（如 6000） |
| `match_key` | `""` | 手动指定音轨名关键字；留空则自动识别（推荐） |
| `hotkey` | `"Pause"` | 手动静音热键，如 `"Ctrl+Shift+F12"`；留空表示不注册 |

### 已知限制

- **判据是"电平"，不是"手机档位"。** 手机档位这条信息不会传到 Windows，所以无法直接读取。
- 因此"内容本身很安静"和"手机被调小"在程序看来是同一件事：直播里主播长时间不说话、音乐长留白，也会被判定为安静。默认 4 秒的保持时间就是为了让正常语气停顿不被打断。
- 门槛窗口很窄，换手机、换耳机后建议重新标定一次。
- 关闭总开关后程序**完全不轮询**音频（CPU 归零），仅在打开菜单/点标定时按需读一次。

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
    ├── volume.rs              # 手机音轨电平门（自动静音）
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
