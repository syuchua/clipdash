use crate::{Item, ItemKind};

#[derive(Debug, Clone)]
pub struct HistoryConfig {
    pub max_items: usize,
    pub max_text_bytes: usize,
    pub max_image_bytes: usize,
    pub ttl_secs: u64,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            max_items: 200,
            max_text_bytes: 100_000,
            max_image_bytes: 2_000_000,
            ttl_secs: 0,
        }
    }
}

#[derive(Default)]
pub struct History {
    pub(crate) cfg: HistoryConfig,
    pub(crate) items: Vec<Item>,
    next_id: u64,
}

impl History {
    pub fn with_config(cfg: HistoryConfig) -> Self {
        Self {
            cfg,
            items: Vec::new(),
            next_id: 1,
        }
    }
    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn all(&self) -> &[Item] {
        &self.items
    }

    /// Push with validation; returns Some(id) on success, None if rejected by constraints
    pub fn try_push(&mut self, mut item: Item) -> Option<u64> {
        // Dedup first: if equal kind+data exists, move it to the back and keep id
        if let Some(pos) = self
            .items
            .iter()
            .position(|it| it.kind == item.kind && it.data == item.data)
        {
            let mut existing = self.items.remove(pos);
            existing.pinned = existing.pinned || item.pinned;
            existing.ts_ms = now_ms();
            if existing.mime.is_none() {
                existing.mime = item.mime.take();
            }
            if existing.file_path.is_none() {
                existing.file_path = item.file_path.take();
            }
            let id = existing.id;
            self.items.push(existing);
            return Some(id);
        }

        // Size constraints by kind
        match item.kind {
            ItemKind::Text if item.data.len() > self.cfg.max_text_bytes => return None,
            ItemKind::Image if item.data.len() > self.cfg.max_image_bytes => return None,
            _ => {}
        }

        // Assign id and insert
        item.id = self.next_id;
        self.next_id += 1;
        item.ts_ms = now_ms();
        let id = item.id;
        self.items.push(item);
        self.prune_ttl();
        self.trim();
        Some(id)
    }

    pub fn push(&mut self, item: Item) -> u64 {
        self.try_push(item)
            .expect("push() should be used only for items within limits")
    }

    pub fn trim(&mut self) {
        // 保留 pinned，优先从最旧的未 pinned 开始裁剪
        if self.items.len() <= self.cfg.max_items {
            return;
        }
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
        if let Some(it) = self.items.iter_mut().find(|it| it.id == id) {
            it.pinned = pinned;
        }
    }

    pub fn delete(&mut self, id: u64) -> bool {
        let before = self.items.len();
        self.items.retain(|i| i.id != id);
        self.items.len() < before
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn rebuild_from(&mut self, mut items: Vec<Item>) {
        // ensure order is preserved and next_id is max+1
        let next = items
            .iter()
            .map(|i| i.id)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        self.items = items.drain(..).collect();
        self.next_id = if next == 0 { 1 } else { next };
    }

    pub fn prune_ttl(&mut self) {
        if self.cfg.ttl_secs == 0 {
            return;
        }
        let now = now_ms();
        let ttl_ms = (self.cfg.ttl_secs as i64) * 1000;
        self.items
            .retain(|it| it.pinned || now - it.ts_ms <= ttl_ms);
    }
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    (dur.as_secs() as i64) * 1000 + (dur.subsec_millis() as i64)
}
