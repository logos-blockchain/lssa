//! The AMM Program implementation.

pub use amm_core as core;

pub mod add;
pub mod new_definition;
pub mod remove;
pub mod swap;

#[cfg(all(test, feature = "with-nssa"))]
mod full_tests;
#[cfg(test)]
mod tests;
