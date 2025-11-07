# TODO (TDD 驱动)

优先级按从内到外、从契约到实现推进。标记 `[spec]` 表示已有测试骨架（多数暂时 `#[ignore]`）。

## Core
- [spec] RingBuffer 语义：
  - 去重：相同内容不新增条目，仅更新时间戳/顺序
  - 容量：超过 `max_items` 时，优先裁剪未 pinned 的最旧项
  - Pin：被 pinned 的项在裁剪时保留
- Item 标题/摘要与数据大小上限（文本/图片）

## Store
- [spec] 内存实现的顺序保持与往返一致性
- SQLite DAO 草图与迁移（后续）

## Backend
- [spec] 剪贴板契约：新 selection 触发一次事件；读写多 MIME 类型
- X11/Portal 后端最小骨架（后续）

## 类型扩展（图片/HTML）
- 采集与判定：从系统剪贴板识别 `image/*`、`text/html`、`text/plain` 多种类型，按优先级入库
- 大小与阈值：图片/HTML 自定义上限（过阈值落入缓存文件并在存储中保存路径）
- 持久化：
  - 文件存储：v3 增加 `mime` 与可选 `path` 字段；或图片/HTML 始终外置文件，文本仍内联
  - SQLite：表结构支持 `mime`、`bytes_len`、`file_path`，并建立按时间与 pinned 的索引
- 守护 API：
  - `GET` 返回类型与内容（`TEXT\n...`、`IMAGE\n<path>|<mime>|<w>x<h>`、`HTML\n...`）
  - `PASTE` 支持图片与 HTML（Wayland: `wl-copy --type`；X11: `xclip -selection clipboard -t <mime>`）
- UI：
  - 列表行标识类型（🖼 image / </> html / T text）
  - 预览区：
    - 图片：缩略图 + 原尺寸信息
    - HTML：安全渲染（简化为纯文本或使用 WebKitGTK 可选）
- 测试：
  - 大小限制与落盘路径清理
  - GET/PASTE 对图片/HTML 的端到端用例（可使用数据样本）

## IPC (D-Bus)
- 接口定义 + zbus 测试总线验证（后续）

## UI (GTK4)
- 列表/搜索/选中状态机用例（后续）

## 工程
- systemd --user 单元与 .desktop（后续）

---
说明：当前测试均标注 `#[ignore]` 以避免在未实现阶段破坏构建。落地实现时应逐一去除 `#[ignore]`，确保 Red → Green → Refactor 节奏。
