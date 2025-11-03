use clipdash_core::{Item, ItemKind};
use clipdash_store::Store;

fn mk(n: &str) -> Item {
    Item { id: 0, kind: ItemKind::Text, data: n.as_bytes().to_vec(), pinned: false }
}

#[test]
#[ignore]
fn store_roundtrip_preserves_item_ordering() {
    let mut s = Store::new_in_memory();
    s.put(mk("a"));
    s.put(mk("b"));
    s.put(mk("c"));
    let titles: Vec<String> = s.all().iter().map(|i| i.title()).collect();
    assert_eq!(titles, vec!["a".into(), "b".into(), "c".into()]);
}

