#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemKind {
    Text,
    Image,
    Html,
}

#[derive(Debug, Clone)]
pub struct Item {
    pub id: u64,
    pub kind: ItemKind,
    pub data: Vec<u8>,
    pub pinned: bool,
    pub ts_ms: i64,
    pub mime: Option<String>,
    pub file_path: Option<String>,
}

impl Item {
    pub fn title(&self) -> String {
        match self.kind {
            ItemKind::Text => String::from_utf8_lossy(&self.data)
                .chars()
                .take(40)
                .collect(),
            ItemKind::Image => String::from("[image]"),
            ItemKind::Html => String::from("[html]"),
        }
    }
}

pub mod history;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_text_truncates() {
        let s = "hello".repeat(20);
        let item = Item {
            id: 1,
            kind: ItemKind::Text,
            data: s.as_bytes().to_vec(),
            pinned: false,
            ts_ms: 0,
            mime: None,
            file_path: None,
        };
        let t = item.title();
        assert!(t.len() <= 40);
    }
}
