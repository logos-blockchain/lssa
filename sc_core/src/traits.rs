use serde::{Deserialize, Serialize};
use utxo::utxo_core::UTXO;

/// Marker trait for contract state object
pub trait IContract<'a>: Serialize + Deserialize<'a> {}

/// Trait for input parameters
///
/// To be able to publish public part of input parameters,
/// we need to be able to make them.
pub trait IInputParameters<'a>: Deserialize<'a> {
    fn public_input_parameters_ser(&self) -> Vec<Vec<u8>>;
}

/// Marker trait for public outputs
pub trait IPublicOutput: Serialize {}

/// Trait for private output
///
/// Must produce the list of resulting UTXO for publication
pub trait IPrivateOutput {
    fn make_utxo_list(&self) -> Vec<UTXO>;
}

/// Trait for public execution type
pub trait IPublicExecutor<'a, I: IInputParameters<'a>, PuO: IPublicOutput> {
    fn public_execution(&mut self, inputs: I) -> PuO;
}

/// Trait for private execution type
pub trait IPrivateExecutor<'a, I: IInputParameters<'a>, PrO: IPrivateOutput> {
    fn private_execution(&mut self, inputs: I) -> PrO;
}

/// Trait for shielded execution type
pub trait IShieldedExecutor<'a, I: IInputParameters<'a>, PuO: IPublicOutput, PrO: IPrivateOutput> {
    fn shielded_execution(&mut self, inputs: I) -> (PuO, PrO);
}

/// Trait for deshielded execution type
pub trait IDeshieldedExecutor<'a, I: IInputParameters<'a>, PuO: IPublicOutput, PrO: IPrivateOutput>
{
    fn deshielded_execution(&mut self, inputs: I) -> (PuO, PrO);
}
