# Clipdash

轻量、高性能的 Linux 剪贴板历史工具（Rust 实现），体验接近 Windows 的 Win+V：按下热键，呼出原生 GTK 弹窗，搜索/回车即贴。

核心特性
- 历史与搜索：收集文本、HTML、图片，并支持快速搜索、上下键导航、Enter 粘贴
- 原生 UI：GTK3 列表 + 预览（文本/Markdown 渲染；图片支持“适应窗口/100%”切换；Pin/删除/清空）
- 系统集成：.desktop 启动器、systemd --user、自带 GNOME 快捷键脚本（可绑定 <Super>v）
- Wayland/X11：Wayland 通过 wl-clipboard，X11 通过 xclip；守护自动轮询并做格式判定/去重
- 配置灵活：~/.config/clipdash/config.toml 可调 UI 外观、预览阈值、采集开关、缓存配额、历史上限/TTL
- 外观：默认“伪亚克力”半透明卡片（稳定、通用）；Xorg 可启用 RGBA 背景；真实模糊可配合 picom/KWin（可选）

支持范围（建议）
- 二进制（x86_64, gnu）：Ubuntu 20.04/22.04/24.04、Debian 11/12（glibc ≥ 2.31）
- 二进制（aarch64, gnu）：Ubuntu 22.04/24.04（glibc ≥ 2.35）
- 静态 MUSL（x86_64/aarch64）：仅 CLI/Daemon，适配更广
- UI 运行时：GTK3 ≥ 3.22（GNOME/KDE/Xfce 等均可）
- 透明性：Wayland 多数 DE 会忽略整窗透明，建议使用“伪亚克力”；Xorg 可设置 ui.opacity 并配合 picom/KWin 模糊

—

快速上手（开发/试用）
1) 依赖
- Rust stable
- GTK3 开发包（Ubuntu/Debian：`sudo apt install -y libgtk-3-dev`）
- 剪贴板工具：Wayland → `wl-clipboard`；X11 → `xclip`

2) 一键安装（含 UI、systemd）：
- `CLIPDASH_WITH_GTK=1 bash scripts/install_dev.sh`
- 运行 UI：`clipdash-ui`
- 绑定 GNOME 快捷键（可选）：`bash scripts/gnome_switch_to_ui.sh`（或 `bash scripts/gnome_bind_super_v.sh` 绑定 `clipdash menu`）

3) 手动构建
- 全工作区：`cargo build --release --workspace`
- 守护/CLI：`cargo build --release -p clipdash-daemon -p clipdash-cli`
- UI（GTK3）：`cargo build --release -p clipdash-ui --features gtk-ui`
- 守护服务：`systemctl --user enable --now clipdashd.service`

常用命令
- `clipdash-ui`：原生 UI（搜索、预览、回车粘贴；空格开/关预览；p Pin；Delete 删除；Ctrl+L 清空）
- `clipdash menu`：zenity/rofi/wofi/dmenu 弹窗菜单
- `clipdash add-text <text>`、`clipdash list|get|copy|pin|delete|clear`

—

手动编译（简）
- 依赖安装（Ubuntu/Debian）：
  - `sudo apt update && sudo apt install -y build-essential pkg-config libgtk-3-dev`
  - Wayland：`sudo apt install -y wl-clipboard`；X11：`sudo apt install -y xclip`
- 构建：`cargo build --release --workspace --features gtk-ui`
- 运行：
  - 守护：`~/.local/bin/clipdash-daemon` 或 `systemctl --user restart clipdashd.service`
  - UI：`~/.local/bin/clipdash-ui`

—

配置文件（~/.config/clipdash/config.toml）

UI（已实现）
- `ui.dark = true|false` 初始主题
- `ui.opacity = 1.0` 整窗不透明度（Xorg 生效；Wayland 多数忽略）
- `ui.acrylic = off|fake|auto` 伪亚克力（默认 fake）
- `ui.blur_strength = 0.0..1.0` 伪亚克力“强度”，越大越通透（默认 0.4）
- `ui.preview_height = 360` 预览初始高度；`ui.preview_min_height = 180` 最小高度
- `ui.max_preview_chars = 200000` 文本/HTML 预览长度上限
- `ui.max_image_preview_bytes = 10000000` 图片预览字节上限（超限仅提示，不解码）

守护/采集（已实现）
- `watch.text = true|false` 是否采集文本（默认 true）
- `watch.html = true|false` 是否采集 HTML（默认 true；UI 以纯文本渲染）
- `watch.image = true|false` 是否采集图片（默认 true）
- `history.max_items = 200`、`history.ttl_secs = 0`（0 表示无限）
- `history.max_text_bytes = 100000`、`history.max_image_bytes = 2000000`
- `cache.images.max_bytes = 104857600`、`cache.html.max_bytes = 52428800`

示例：
```
ui.dark = true
ui.opacity = 0.92
ui.acrylic = "fake"
ui.blur_strength = 0.6
ui.preview_height = 380
ui.preview_min_height = 180
ui.max_preview_chars = 200000
ui.max_image_preview_bytes = 10000000

watch.text = true
watch.html = true
watch.image = true

history.max_items = 200
history.max_text_bytes = 100000
history.max_image_bytes = 2000000
history.ttl_secs = 0

cache.images.max_bytes = 104857600
cache.html.max_bytes = 52428800
```

—

兼容性提示 & 常见问题
- 透明无效：检查 `echo $XDG_SESSION_TYPE` 是否 x11；Wayland 环境推荐用“伪亚克力”（ui.acrylic=fake）
- 真毛玻璃：Xorg + picom/KWin 可启用模糊；GNOME Wayland 无统一接口
- 剪贴板无效：安装 `wl-clipboard` 或 `xclip` 并确认命令可用
- 快捷键冲突：GNOME 可改 `<Super><Shift>v` 或用脚本重新绑定

