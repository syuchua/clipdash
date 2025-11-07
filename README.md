# Clipdash

一键呼出、快速搜索、即贴即用的 Linux 剪贴板历史工具（目标体验类似 Win+V）。

当前状态：架构与工作区已搭建（skeleton），将以 TDD 逐步实现核心功能。

## 快速开始

1) Ubuntu 20.04 一键脚本（推荐）
- `bash scripts/setup_ubuntu_20_04.sh`

2) 手动安装 Rust（若不使用脚本）
- 推荐：`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- 安装后执行：`source "$HOME/.cargo/env"`
- 验证：`rustc --version && cargo --version && rustup --version`
- 建议组件：`rustup component add clippy rustfmt`

3) Linux 依赖（为后续 GUI/后端做准备，可稍后安装）
- Debian/Ubuntu：
  - 基础：`sudo apt update && sudo apt install -y build-essential pkg-config` 
  - SQLite（可选，若不使用 rusqlite bundled 特性）：`sudo apt install -y libsqlite3-dev`
  - GTK4（UI 后续启用时）：`sudo apt install -y libgtk-4-dev libadwaita-1-dev`
  - X11（可选，X11 后端启用时）：`sudo apt install -y libx11-dev libxfixes-dev`
- Fedora：
  - `sudo dnf install -y @development-tools gcc pkgconfig sqlite-devel gtk4-devel libadwaita-devel libX11-devel libXfixes-devel`
- Arch：
  - `sudo pacman -S --needed base-devel pkgconf sqlite gtk4 libadwaita libx11 libxfixes`

4) 构建与测试（含 demo 守护/CLI/菜单/GTK 原生）
- 拉取依赖并构建工作区：
  - `cd clipdash`
  - `cargo build`  （首次会下载工具链与依赖）
- 运行测试：`cargo test`
- 运行示例二进制：
  - 守护（Unix Socket 服务）：`cargo run -p clipdash-daemon`
  - CLI（连接守护的 demo 命令）：
    - `cargo run -p clipdash-cli -- daemon`（也可直接运行守护）
    - `cargo run -p clipdash-cli -- add-text "hello world"`
    - `cargo run -p clipdash-cli -- list 10`（或带查询：`list 10 hello`）
    - `cargo run -p clipdash-cli -- get <id>`
    - `cargo run -p clipdash-cli -- paste <id>`（仅输出原始文本，可与 xclip 等组合）
    - `cargo run -p clipdash-cli -- copy <id>`（将文本写入系统剪贴板，需安装 wl-clipboard 或 xclip）
    - `cargo run -p clipdash-cli -- pin <id> 1` / `0`
    - `cargo run -p clipdash-cli -- delete <id>`
    - `cargo run -p clipdash-cli -- clear`
    - `cargo run -p clipdash-cli -- menu`（优先 zenity 弹窗，缺省回退 rofi/wofi/dmenu）
  - GTK 原生 UI：
    - 依赖（Ubuntu 20.04）：`sudo apt install -y libgtk-3-dev`
    - 运行（一次性构建特性）：`cargo run -p clipdash-ui --features gtk-ui`
    - 开发安装（带 UI）：`CLIPDASH_WITH_GTK=1 bash scripts/install_dev.sh`（安装后可直接运行 `clipdash-ui`）
  - 脚本：`bash scripts/clipdash_menu.sh`

Socket 路径：`$HOME/.cache/clipdash/daemon.sock`。
持久化文件：`$HOME/.local/share/clipdash/history.v1`（十六进制编码的轻量文本格式）。

依赖提示（用于剪贴板集成）：
- Wayland：安装 `wl-clipboard`（提供 `wl-copy` / `wl-paste`）
- X11：安装 `xclip`
守护会自动优先使用 Wayland 工具，其次回退到 xclip；若都不可用，将不进行自动监听，`copy` 命令也会返回错误。

桌面集成与快捷键
- `.desktop` 启动器：`packaging/clipdash-menu.desktop`（可复制到 `~/.local/share/applications/`）
- 快捷键绑定：
  - 使用弹窗菜单（zenity/rofi/wofi/dmenu）：将 Super+V 绑定为 `clipdash menu`
  - 使用 GTK 原生窗口：将 Super+V 绑定为 `clipdash-ui`（需要启用 GTK 构建）

5) 一键集成（开发环境）
- 安装（构建 release、创建 `~/.local/bin` 链接、安装 systemd --user 与 autostart）
  - `bash scripts/install_dev.sh`
- 卸载：`bash scripts/uninstall_dev.sh`
- GNOME 快捷键（可选）：
  - `bash scripts/gnome_bind_super_v.sh`（将 `<Super>v` 绑定为 `clipdash menu`）
- systemd --user（手动说明）：
  - 单元文件：`packaging/clipdashd.service`（安装后位于 `~/.config/systemd/user/`）
  - 启用并启动：`systemctl --user enable --now clipdashd.service`

## 目录结构（skeleton）

```
clipdash/
├─ crates/
│  ├─ core/      # 领域模型/规则（lib）
│  ├─ store/     # 存储抽象与实现（lib）
│  ├─ backend/   # 剪贴板后端 trait（lib）
│  ├─ daemon/    # D-Bus 服务/守护（bin）
│  ├─ ui/        # GTK4 弹窗（bin）
│  └─ cli/       # 命令行工具（bin）
├─ Cargo.toml
├─ README.md
└─ 架构.md
```

## 贡献与开发约定
- TDD：先列测试用例名与契约，再实现最小功能。
- 代码风格：`cargo fmt`；静态检查：`cargo clippy -- -D warnings`。
- 提交粒度：功能/重构/依赖独立提交；避免混合大提交。

## 里程碑（摘自架构.md）
1) core/store/backend mock + D-Bus + CLI（无 UI）
2) GTK4 弹窗（列表+搜索+粘贴）+ X11/Portal 基础后端
3) 托盘、自启动、Pin/删除、配置
4) Wayland/wlroots 扩展、图片/HTML 预览、性能收尾

---
更多细节请参见 `架构.md`。

## Acrylic / 模糊背景（可选）

Clipdash 默认使用“伪亚克力”（半透明 + 阴影）以确保通用性与稳定性。

配置项（~/.config/clipdash/config.toml）：
- `ui.acrylic = off|fake|auto`（默认 fake）
  - off：基本不透明（轻微透明）
  - fake：仅 CSS 半透明与阴影（通用）
  - auto：与 fake 等价；真实模糊需用户在合成器配置中启用
- `ui.blur_strength = 0.0..1.0`（默认 0.4）：伪亚克力的“强度”，值越大越透明

注意：真实“毛玻璃”模糊效果依赖桌面合成器，应用端无法在所有 DE 中强制启用。

### X11 + picom 示例
1) 安装并启用 picom（不同发行版命令略有差异）。
2) 在 picom 配置中添加匹配 Clipdash UI 的规则（类名多为 `clipdash-ui` 或窗口标题 `Clipdash`）：
```
blur:
  method = "dual_kawase";
  strength = 6;
  background = false;
  background-frame = false;

blur-background-exclude = [
  "class_g = 'clipdash-ui' && argb"
];

wintypes:
  {
    normal = { blur-background = true; }
  };
```
具体匹配表达式可根据 `xprop` 查询的 `WM_CLASS` 调整。

### KDE Plasma（KWin）
在“系统设置 → 特殊应用程序设置/窗口规则”中，按窗口类或标题为 Clipdash 创建规则，启用“背景模糊”。Wayland/X11 均可，但需 KDE 特性支持。

### GNOME（Wayland）
官方无统一窗口模糊接口。建议使用默认“伪亚克力”模式，或查找第三方扩展（兼容性不保证）。
