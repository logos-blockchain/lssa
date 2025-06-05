use risc0_zkvm::{
    guest::env,
};

use {sc_name}::{SmartContract, InputParameters, PublicOutputs, PrivateOutputs};
use sc_core::traits::{IContract, IInputParameters, IPublicOutput, IPrivateOutput};
use sc_core::{{execution_type_trait}};

//Not sure, how path will look like, so
//ToDo: Make it available from sc_core
//Curently in node_core
use sc_core::PublicSCContext;

//Not sure, how path will look like, so
//ToDo: Make it available from sc_core
//Curently in storage
use sc_core::{produce_blob_list_from_sc_public_state, compare_blob_lists};

fn main() {
    let mut state: SmartContract = env::read();
    //Must fail if this step fails
    let old_state = produce_blob_list_from_sc_public_state(&state).unwrap();

    let public_context: PublicSCContext = env::read();
    let inputs: InputParameters = env::read();

    //In RISC0 all input parameters are private, so we need to commit to public ones
    env::commit(&(inputs.public_input_parameters_ser())) 

    //Next, push one of possible variants depending on execution type
    {
        public => let public_outputs = state.public_execution(inputs);,
        private => let private_outputs = state.private_execution(inputs);,
        shielded => let (public_outputs, private_outputs) = state.shielded_execution(inputs);,
        deshielded => let (public_outputs, private_outputs) = state.shielded_execution(inputs);,
    }

    //Next, push one of possible variants depending on execution type
    //ToDo [Debatable]: Rework and update circuits to work with new trait system system
    {
        public => ,
        private => private_circuit(public_context, private_outputs);,
        shielded => shielded_circuit(public_context, public_outputs, private_outputs);,
        deshielded => deshielded_circuit(public_context, public_outputs, private_outputs);,
    }

    //Must fail if this step fails
    let new_state = produce_blob_list_from_sc_public_state(&state).unwrap();

    //Commiting public state changes
    let state_changes = compare_blob_lists(old_state, new_state);
    env::commit(&state_changes);

    //Next, push one of possible variants depending on execution type
    //ToDo: Make UTXO encoding for their owners available from PublicSCContext
    {
        public => {
            env::commit(&public_outputs);
        },
        private => {
            env::commit(&(public_context.encode_utxo_for_owners(private_outputs.make_utxo_list())));
        },
        shielded | deshielded => {
            env::commit(&public_outputs);
            env::commit(&(public_context.encode_utxo_for_owners(private_outputs.make_utxo_list())));
        },
    }
}
