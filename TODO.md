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

## IPC (D-Bus)
- 接口定义 + zbus 测试总线验证（后续）

## UI (GTK4)
- 列表/搜索/选中状态机用例（后续）

## 工程
- systemd --user 单元与 .desktop（后续）

---
说明：当前测试均标注 `#[ignore]` 以避免在未实现阶段破坏构建。落地实现时应逐一去除 `#[ignore]`，确保 Red → Green → Refactor 节奏。

