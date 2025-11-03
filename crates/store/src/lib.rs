use clipdash-core::{Item, ItemKind};

#[derive(Default)]
pub struct Store;

impl Store {
    pub fn new_in_memory() -> Self {
        Store
    }

    pub fn put(&self, _item: Item) {
        // placeholder
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_store() {
        let _s = Store::new_in_memory();
    }
}

