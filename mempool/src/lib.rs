use std::collections::VecDeque;

pub struct MemPool<Item> {
    items: VecDeque<Item>,
}

impl<Item> MemPool<Item> {
    pub fn new() -> Self {
        Self {
            items: VecDeque::new(),
        }
    }

    pub fn pop_last(&mut self) -> Option<Item> {
        self.items.pop_front()
    }

    pub fn peek_last(&self) -> Option<&Item> {
        self.items.front()
    }

    pub fn push_item(&mut self, item: Item) {
        self.items.push_back(item);
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn pop_size(&mut self, size: usize) -> Vec<Item> {
        let mut ret_vec = vec![];

        for _ in 0..size {
            let item = self.pop_last();

            match item {
                Some(item) => ret_vec.push(item),
                None => break,
            }
        }

        ret_vec
    }

    pub fn drain_size(&mut self, remainder: usize) -> Vec<Item> {
        self.pop_size(self.len().saturating_sub(remainder))
    }
}

impl<Item> Default for MemPool<Item> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::vec;

    use super::*;

    pub type ItemId = u64;

    #[derive(Debug, PartialEq, Eq)]
    pub struct TestItem {
        id: ItemId,
    }

    fn test_item_with_id(id: u64) -> TestItem {
        TestItem { id }
    }

    #[test]
    fn test_create_empty_mempool() {
        let _: MemPool<TestItem> = MemPool::new();
    }

    #[test]
    fn test_mempool_new() {
        let pool: MemPool<TestItem> = MemPool::new();
        assert!(pool.is_empty());
        assert_eq!(pool.len(), 0);
    }

    #[test]
    fn test_push_item() {
        let mut pool = MemPool::new();
        pool.push_item(test_item_with_id(1));
        assert!(!pool.is_empty());
        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn test_pop_last() {
        let mut pool = MemPool::new();
        pool.push_item(test_item_with_id(1));
        pool.push_item(test_item_with_id(2));
        let item = pool.pop_last();
        assert_eq!(item, Some(test_item_with_id(1)));
        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn test_peek_last() {
        let mut pool = MemPool::new();
        pool.push_item(test_item_with_id(1));
        pool.push_item(test_item_with_id(2));
        let item = pool.peek_last();
        assert_eq!(item, Some(&test_item_with_id(1)));
    }

    #[test]
    fn test_pop_size() {
        let mut pool = MemPool::new();
        pool.push_item(test_item_with_id(1));
        pool.push_item(test_item_with_id(2));
        pool.push_item(test_item_with_id(3));

        let items = pool.pop_size(2);
        assert_eq!(items, vec![test_item_with_id(1), test_item_with_id(2)]);
        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn test_drain_size() {
        let mut pool = MemPool::new();
        pool.push_item(test_item_with_id(1));
        pool.push_item(test_item_with_id(2));
        pool.push_item(test_item_with_id(3));
        pool.push_item(test_item_with_id(4));

        let items = pool.drain_size(2);
        assert_eq!(items, vec![test_item_with_id(1), test_item_with_id(2)]);
        assert_eq!(pool.len(), 2);
    }

    #[test]
    fn test_default() {
        let pool: MemPool<TestItem> = MemPool::default();
        assert!(pool.is_empty());
        assert_eq!(pool.len(), 0);
    }

    #[test]
    fn test_is_empty() {
        let mut pool = MemPool::new();
        assert!(pool.is_empty());
        pool.push_item(test_item_with_id(1));
        assert!(!pool.is_empty());
    }

    #[test]
    fn test_push_pop() {
        let mut mempool: MemPool<TestItem> = MemPool::new();

        let items = vec![
            test_item_with_id(1),
            test_item_with_id(2),
            test_item_with_id(3),
        ];

        for item in items {
            mempool.push_item(item);
        }
        assert_eq!(mempool.len(), 3);

        let item = mempool.pop_last();

        assert_eq!(item, Some(TestItem { id: 1 }));
        assert_eq!(mempool.len(), 2);

        let item = mempool.pop_last();

        assert_eq!(item, Some(TestItem { id: 2 }));
        assert_eq!(mempool.len(), 1);

        let item = mempool.pop_last();

        assert_eq!(item, Some(TestItem { id: 3 }));
        assert_eq!(mempool.len(), 0);

        let item = mempool.pop_last();

        assert_eq!(item, None);
    }

    #[test]
    fn test_pop_many() {
        let mut mempool: MemPool<TestItem> = MemPool::new();

        let mut items = vec![];

        for i in 1..11 {
            items.push(test_item_with_id(i));
        }

        for item in items {
            mempool.push_item(item);
        }

        assert_eq!(mempool.len(), 10);

        let items1 = mempool.pop_size(4);
        assert_eq!(
            items1,
            vec![
                test_item_with_id(1),
                test_item_with_id(2),
                test_item_with_id(3),
                test_item_with_id(4)
            ]
        );
        assert_eq!(mempool.len(), 6);

        let items2 = mempool.drain_size(2);
        assert_eq!(
            items2,
            vec![
                test_item_with_id(5),
                test_item_with_id(6),
                test_item_with_id(7),
                test_item_with_id(8)
            ]
        );
        assert_eq!(mempool.len(), 2);
    }
}
