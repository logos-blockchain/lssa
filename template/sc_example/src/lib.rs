use sc_core::traits::{
    IContract, IDeshieldedExecutor, IInputParameters, IPrivateOutput, IPublicOutput,
};
use serde::{Deserialize, Serialize};
use utxo::utxo_core::UTXO;

#[derive(Debug, Serialize, Deserialize)]
struct SmartContract {}

impl<'a> IContract<'a> for SmartContract {}

#[derive(Debug, Serialize, Deserialize)]
struct InputParameters {
    pub a: u64,
    pub b: u64,
}

impl<'a> IInputParameters<'a> for InputParameters {
    fn public_input_parameters_ser(&self) -> Vec<Vec<u8>> {
        let param_vec = vec![self.a];

        param_vec
            .into_iter()
            .map(|item| serde_json::to_vec(&item).unwrap())
            .collect::<Vec<_>>()
    }
}

#[derive(Debug, Serialize)]
struct PublicOutputs {
    pub ab: u64,
}

impl IPublicOutput for PublicOutputs {}

struct PrivateOutputs {
    pub a_plus_b: u64,
}

impl IPrivateOutput for PrivateOutputs {
    fn make_utxo_list(&self) -> Vec<UTXO> {
        let mut utxo_list = vec![];

        let res_utxo = UTXO {
            hash: [0; 32],
            owner: [1; 32],
            asset: vec![1, 2, 3],
            amount: self.a_plus_b as u128,
            privacy_flag: true,
            randomness: [2; 32],
        };

        utxo_list.push(res_utxo);

        utxo_list
    }
}

impl<'a> IDeshieldedExecutor<'a, InputParameters, PublicOutputs, PrivateOutputs> for SmartContract {
    fn deshielded_execution(&self, inputs: InputParameters) -> (PublicOutputs, PrivateOutputs) {
        (
            PublicOutputs {
                ab: inputs.a * inputs.b,
            },
            PrivateOutputs {
                a_plus_b: inputs.a + inputs.b,
            },
        )
    }
}
