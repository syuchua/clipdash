# Clipdash

一键呼出、快速搜索、即贴即用的 Linux 剪贴板历史工具（目标体验类似 Win+V）。

当前状态：架构与工作区已搭建（skeleton），将以 TDD 逐步实现核心功能。

## 快速开始

1) 安装 Rust（rustup）
- 推荐：`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- 安装后执行：`source "$HOME/.cargo/env"`
- 验证：`rustc --version && cargo --version && rustup --version`
- 建议组件：`rustup component add clippy rustfmt`

2) Linux 依赖（为后续 GUI/后端做准备，可稍后安装）
- Debian/Ubuntu：
  - 基础：`sudo apt update && sudo apt install -y build-essential pkg-config` 
  - SQLite（可选，若不使用 rusqlite bundled 特性）：`sudo apt install -y libsqlite3-dev`
  - GTK4（UI 后续启用时）：`sudo apt install -y libgtk-4-dev libadwaita-1-dev`
  - X11（可选，X11 后端启用时）：`sudo apt install -y libx11-dev libxfixes-dev`
- Fedora：
  - `sudo dnf install -y @development-tools gcc pkgconfig sqlite-devel gtk4-devel libadwaita-devel libX11-devel libXfixes-devel`
- Arch：
  - `sudo pacman -S --needed base-devel pkgconf sqlite gtk4 libadwaita libx11 libxfixes`

3) 构建与测试（当前仅 skeleton）
- 拉取依赖并构建工作区：
  - `cd clipdash`
  - `cargo build`  （首次会下载工具链与依赖）
- 运行测试：`cargo test`
- 运行示例二进制：
  - 守护：`cargo run -p clipdash-daemon`
  - UI：`cargo run -p clipdash-ui`
  - CLI：`cargo run -p clipdash-cli`

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

