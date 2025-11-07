use crate::{Item, ItemKind};

#[derive(Debug, Clone)]
pub struct HistoryConfig {
    pub max_items: usize,
    pub max_text_bytes: usize,
    pub max_image_bytes: usize,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self { max_items: 200, max_text_bytes: 100_000, max_image_bytes: 2_000_000 }
    }
}

#[derive(Default)]
pub struct History {
    pub(crate) cfg: HistoryConfig,
    pub(crate) items: Vec<Item>,
    next_id: u64,
}

impl History {
    pub fn with_config(cfg: HistoryConfig) -> Self { Self { cfg, items: Vec::new(), next_id: 1 } }
    pub fn len(&self) -> usize { self.items.len() }
    pub fn all(&self) -> &[Item] { &self.items }

    pub fn push(&mut self, mut item: Item) -> u64 {
        // 去重：相同 kind+data 视为同一条，更新“最近性”（移到末尾），保持原 id
        if let Some(pos) = self.items.iter().position(|it| it.kind == item.kind && it.data == item.data) {
            let mut existing = self.items.remove(pos);
            // 保留 pinned（若新数据为 pinned 也保持）
            existing.pinned = existing.pinned || item.pinned;
            let id = existing.id;
            self.items.push(existing);
            return id;
        }

        // 新条目：分配 id 并追加
        item.id = self.next_id;
        self.next_id += 1;
        let id = item.id;
        self.items.push(item);
        self.trim();
        id
    }

    pub fn trim(&mut self) {
        // 保留 pinned，优先从最旧的未 pinned 开始裁剪
        if self.items.len() <= self.cfg.max_items { return; }
        let mut to_remove = self.items.len() - self.cfg.max_items;
        let mut i = 0;
        while i < self.items.len() && to_remove > 0 {
            if !self.items[i].pinned {
                self.items.remove(i);
                to_remove -= 1;
                // 不自增 i，因为移除了当前位置
            } else {
                i += 1;
            }
        }
        // 若仍有超额且全为 pinned，则保留（允许临时超过上限）
    }

    pub fn pin(&mut self, id: u64, pinned: bool) {
        if let Some(it) = self.items.iter_mut().find(|it| it.id == id) { it.pinned = pinned; }
    }
}
