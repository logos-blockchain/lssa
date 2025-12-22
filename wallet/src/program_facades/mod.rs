//! This module contains [`WalletCore`](crate::WalletCore) facades for interacting with various
//! on-chain programs.

use serde::{Serialize, ser::SerializeSeq};

pub mod amm;
pub mod native_token_transfer;
pub mod pinata;
pub mod token;

/// Why it is necessary:
///
/// Serialize implemented only for `[u8; N]` where `N<=32` and orphan rules would disallow custom
/// Serialize impls for them.
///
/// Additionally, RISC0 splits instructions into words of 4-byte size which glues bytes for custom
/// structs so we need to expand each byte into `u32` to preserve shape, because AMM awaits
/// `Vec<u8>` as instruction.
struct OrphanHackNBytesInput<const N: usize>([u32; N]);

impl<const N: usize> OrphanHackNBytesInput<N> {
    fn expand(orig: [u8; N]) -> Self {
        let mut res = [0u32; N];

        for (idx, val) in orig.into_iter().enumerate() {
            res[idx] = val as u32;
        }

        Self(res)
    }
}

impl<const N: usize> Serialize for OrphanHackNBytesInput<N> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(N))?;
        for word in self.0 {
            seq.serialize_element(&word)?;
        }
        seq.end()
    }
}

type OrphanHack65BytesInput = OrphanHackNBytesInput<65>;
type OrphanHack49BytesInput = OrphanHackNBytesInput<49>;
