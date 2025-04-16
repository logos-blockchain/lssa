use anyhow::Result;
use common::nullifier::UTXONullifier;
use utxo::utxo_core::{UTXOPayload, UTXO};

pub fn utxo_change_owner(
    utxo: &mut UTXO,
    nullifier: UTXONullifier,
    new_owner: [u8; 32],
) -> Result<UTXO> {
    let new_payload = UTXOPayload {
        owner: new_owner,
        asset: utxo.asset.clone(),
        amount: utxo.amount,
        privacy_flag: utxo.privacy_flag,
    };

    utxo.consume_utxo(nullifier)?;

    Ok(UTXO::create_utxo_from_payload(new_payload)?)
}

pub fn utxo_substact_part_another_owner(
    utxo: &mut UTXO,
    nullifier: UTXONullifier,
    amount: u128,
    new_owner: [u8; 32],
) -> Result<(UTXO, UTXO)> {
    if amount > utxo.amount {
        anyhow::bail!("Amount too big");
    }

    let diff = utxo.amount - amount;

    let new_payload1 = UTXOPayload {
        owner: utxo.owner,
        asset: utxo.asset.clone(),
        amount: diff,
        privacy_flag: utxo.privacy_flag,
    };

    let new_payload2 = UTXOPayload {
        owner: new_owner,
        asset: utxo.asset.clone(),
        amount,
        privacy_flag: utxo.privacy_flag,
    };

    utxo.consume_utxo(nullifier)?;

    Ok((
        UTXO::create_utxo_from_payload(new_payload1)?,
        UTXO::create_utxo_from_payload(new_payload2)?,
    ))
}

pub fn utxo_substract_part(
    utxo: &mut UTXO,
    nullifier: UTXONullifier,
    amount: u128,
) -> Result<(UTXO, UTXO)> {
    let new_owner = utxo.owner;

    utxo_substact_part_another_owner(utxo, nullifier, amount, new_owner)
}

pub fn utxo_split_n_users(
    utxo: &mut UTXO,
    nullifier: UTXONullifier,
    users_amounts: Vec<([u8; 32], u128)>,
) -> Result<Vec<UTXO>> {
    let cumulative_diff = users_amounts
        .iter()
        .fold(0, |acc, (_, amount)| acc + *amount);

    if cumulative_diff > utxo.amount {
        anyhow::bail!("Amount too big");
    }

    let mut utxo_res = vec![];

    for (new_owner, amount) in users_amounts {
        let new_payload = UTXOPayload {
            owner: new_owner,
            asset: utxo.asset.clone(),
            amount,
            privacy_flag: utxo.privacy_flag,
        };

        let new_utxo = UTXO::create_utxo_from_payload(new_payload)?;

        utxo_res.push(new_utxo);
    }

    if cumulative_diff != utxo.amount {
        let new_payload = UTXOPayload {
            owner: utxo.owner,
            asset: utxo.asset.clone(),
            amount: utxo.amount - cumulative_diff,
            privacy_flag: utxo.privacy_flag,
        };

        let new_utxo = UTXO::create_utxo_from_payload(new_payload)?;

        utxo_res.push(new_utxo);
    }

    utxo.consume_utxo(nullifier)?;

    Ok(utxo_res)
}
