use clipdash_core::{history::{History, HistoryConfig}, Item, ItemKind};

fn text_item(s: &str) -> Item {
    Item { id: 0, kind: ItemKind::Text, data: s.as_bytes().to_vec(), pinned: false }
}

#[test]
#[ignore]
fn core_ring_buffer_keeps_pinned_on_trim() {
    let mut h = History::with_config(HistoryConfig { max_items: 3, ..Default::default() });
    let a = h.push(text_item("a"));
    let b = h.push(text_item("b"));
    h.pin(b, true); // pin b
    let _c = h.push(text_item("c"));
    let _d = h.push(text_item("d")); // exceed capacity
    h.trim();
    let ids: Vec<u64> = h.all().iter().map(|i| i.id).collect();
    assert!(ids.contains(&b), "pinned should remain after trim");
}

#[test]
#[ignore]
fn dedup_updates_timestamp_instead_of_growing() {
    let mut h = History::with_config(HistoryConfig { max_items: 3, ..Default::default() });
    let _a1 = h.push(text_item("same"));
    let before = h.len();
    let _a2 = h.push(text_item("same"));
    assert_eq!(before, h.len(), "dedup should not increase length");
}

