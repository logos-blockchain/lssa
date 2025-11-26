pub trait KeyNode {
    fn root(seed: [u8; 64]) -> Self;

    fn nth_child(&self, cci: u32) -> Self;

    fn chain_code(&self) -> &[u8; 32];

    fn child_index(&self) -> Option<u32>;

    fn address(&self) -> nssa::Address;
}
