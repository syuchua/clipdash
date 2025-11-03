use clipdash_backend::{ClipboardBackend, ClipData, ClipKind};

struct Dummy;
impl ClipboardBackend for Dummy {
    fn read_current(&self) -> Option<ClipData> { None }
}

#[test]
#[ignore]
fn backend_contract_emits_change_on_new_selection() {
    // 将来：使用模拟器订阅事件，断言当 selection 改变时收到一次通知
    let b = Dummy;
    assert!(b.read_current().is_none());
}

