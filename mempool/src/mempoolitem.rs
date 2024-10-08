pub trait MemPoolItem {
    type Identifier;
    fn identifier(&self) -> Self::Identifier;
}
