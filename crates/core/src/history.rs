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
        // TDD: 去重、容量控制、Pin 保留 等逻辑稍后实现
        item.id = self.next_id;
        self.next_id += 1;
        self.items.push(item);
        // 先返回分配的 id；trim() 与去重稍后补充
        self.next_id - 1
    }

    pub fn trim(&mut self) {
        // TDD: 仅示意，后续实现为“保留 pinned，裁剪未 pinned 的旧项”
        if self.items.len() > self.cfg.max_items {
            self.items.drain(0..self.items.len() - self.cfg.max_items);
        }
    }

    pub fn pin(&mut self, id: u64, pinned: bool) {
        if let Some(it) = self.items.iter_mut().find(|it| it.id == id) { it.pinned = pinned; }
    }
}

