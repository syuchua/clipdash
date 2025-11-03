use clipdash-core::{Item, ItemKind};

#[derive(Default)]
pub struct Store {
    items: Vec<Item>,
}

impl Store {
    pub fn new_in_memory() -> Self { Store { items: Vec::new() } }
    pub fn put(&mut self, item: Item) { self.items.push(item) }
    pub fn all(&self) -> &[Item] { &self.items }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_store() {
        let _s = Store::new_in_memory();
    }
}
