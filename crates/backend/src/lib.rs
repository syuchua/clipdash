// Intentionally keep backend decoupled from core types for now

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipKind {
    Text,
    Image,
    Html,
}

#[derive(Debug, Clone)]
pub struct ClipData {
    pub kind: ClipKind,
    pub bytes: Vec<u8>,
    pub mime: Option<String>,
}

pub trait ClipboardBackend: Send + Sync {
    fn read_current(&self) -> Option<ClipData>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Dummy;
    impl ClipboardBackend for Dummy {
        fn read_current(&self) -> Option<ClipData> {
            None
        }
    }

    #[test]
    fn backend_trait_compiles() {
        let b = Dummy;
        assert!(b.read_current().is_none());
    }
}
