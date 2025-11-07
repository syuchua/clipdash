use clipdash_core::{
    history::{History, HistoryConfig},
    Item, ItemKind,
};

fn text_item(s: &str) -> Item {
    Item {
        id: 0,
        kind: ItemKind::Text,
        data: s.as_bytes().to_vec(),
        pinned: false,
        ts_ms: 0,
        mime: None,
        file_path: None,
    }
}

fn image_item(n: usize) -> Item {
    Item {
        id: 0,
        kind: ItemKind::Image,
        data: vec![0u8; n],
        pinned: false,
        ts_ms: 0,
        mime: Some("image/png".into()),
        file_path: None,
    }
}

#[test]
fn core_ring_buffer_keeps_pinned_on_trim() {
    let mut h = History::with_config(HistoryConfig {
        max_items: 3,
        ..Default::default()
    });
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
fn dedup_updates_timestamp_instead_of_growing() {
    let mut h = History::with_config(HistoryConfig {
        max_items: 3,
        ..Default::default()
    });
    let _a1 = h.push(text_item("same"));
    let before = h.len();
    let _a2 = h.push(text_item("same"));
    assert_eq!(before, h.len(), "dedup should not increase length");
}

#[test]
fn large_text_is_rejected_by_config() {
    let mut h = History::with_config(HistoryConfig {
        max_items: 10,
        max_text_bytes: 5,
        max_image_bytes: 10,
        ttl_secs: 0,
    });
    let before = h.len();
    let too_long = "abcdef"; // 6 > 5
    let id = h.try_push(text_item(too_long));
    assert!(id.is_none(), "oversized text should be rejected");
    assert_eq!(before, h.len(), "history length should not grow");
}

#[test]
fn large_image_is_rejected_by_config() {
    let mut h = History::with_config(HistoryConfig {
        max_items: 10,
        max_text_bytes: 100,
        max_image_bytes: 3,
        ttl_secs: 0,
    });
    let before = h.len();
    let id = h.try_push(image_item(5)); // 5 > 3
    assert!(id.is_none(), "oversized image should be rejected");
    assert_eq!(before, h.len(), "history length should not grow");
}

#[test]
fn ttl_prunes_unpinned_but_keeps_pinned() {
    let mut h = History::with_config(HistoryConfig {
        max_items: 10,
        max_text_bytes: 100,
        max_image_bytes: 100,
        ttl_secs: 1,
    });
    let ida = h.push(text_item("old"));
    let idb = h.push(text_item("old pinned"));
    h.pin(idb, true);
    let idc = h.push(text_item("fresh"));
    // make a/b very old, keep c fresh
    let mut items = h.all().to_vec();
    for it in &mut items {
        if it.id != idc {
            it.ts_ms = 0;
        }
    }
    h.rebuild_from(items);
    h.prune_ttl();
    let titles: Vec<String> = h.all().iter().map(|i| i.title()).collect();
    assert!(titles.iter().any(|t| t.contains("fresh")));
    // pinned old should remain
    assert!(h.all().iter().any(|i| i.id == idb));
    // unpinned old should be gone
    assert!(!titles.iter().any(|t| t == "old"));
}
