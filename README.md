# nescience-testnet
This repo serves for Nescience Node testnet

For more details you can read [blogpost](https://vac.dev/rlog/Nescience-state-separation-architecture/)

For more details on node functionality [here](https://www.notion.so/5-Testnet-initial-results-analysis-18e8f96fb65c808a835cc43b7a84cddf)

# How to run
Node and sequecer require Rust installation to build. Preferable latest stable version.

Rust can be installed as 

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Node needs RISC0 toolchain to run.

It can be installed as

```sh
curl -L https://risczero.com/install | bash
```

After that, before next step, you may need to restart your console, as script updates PATH variable. Next:

```sh
rzup install
```

After cloning this repository the following actions need to be done:

Entrypoints to node and sequencer are `node_runner` and `sequencer_runner`. Both of them have a configuration of similar manner. Path to configs need to be given into runner binaries as first arguent. No other arguments have to be given. We search given directory for files "node_config.json" for node and "sequencer_config.json" for sequencer. 

With repository debug configs at `node_runner/configs/debug` and `sequencer_runner/configs/debug` are provided, you can use them, or modify as you wish.

For sequencer:

```yaml
{
    "home": ".",
    "override_rust_log": null,
    "genesis_id": 1,
    "is_genesis_random": true,
    "max_num_tx_in_block": 20,
    "block_create_timeout_millis": 10000,
    "port": 3040
}
```

* "home" shows relative path to directory with datebase.
* "override_rust_log" sets env var "RUST_LOG" to achieve different log levels(if null, using present "RUST_LOG" value).
* "genesis_id" is id of genesis block.
* "is_genesis_random" - flag to randomise forst block.
* "max_num_tx_in_block" - transaction mempool limit.
* "block_create_timeout_millis" - block timeout.
* "port" - port, which sequencer will listen.

For node:

```yaml
{
    "home": ".",
    "override_rust_log": null,
    "sequencer_addr": "http://127.0.0.1:3040",
    "seq_poll_timeout_secs": 10,
    "port": 3041
}
```

* "home" shows relative path to directory with datebase.
* "override_rust_log" sets env var "RUST_LOG" to achieve different log levels(if null, using present "RUST_LOG" value).
* "sequencer_addr" - address of sequencer.
* "seq_poll_timeout_secs" - polling interval on sequencer, in seconds.
* "port" - port, which sequencer will listen.

To run:

_FIRSTLY_ in sequencer_runner directory:

```sh
RUST_LOG=info cargo run <path-to-configs>
```

_SECONDLY_ in node_runner directory

```sh
RUST_LOG=info cargo run <path-to-configs>
```

# Node Public API

Node exposes public API with mutable and immutable methods to create and send transactions.

## Standards

Node supports JSON RPC 2.0 standard, details can be seen [there](https://www.jsonrpc.org/specification).

## API Structure

Right now API has only one endpoint for every request('/'), and JSON RPC 2.0 standard request structure is fairly simple

```yaml
{
    "jsonrpc": "2.0",
    "id": $number_or_dontcare, 
    "method": $string, 
    "params": $object
}
```

Response strucuture will look as follows:

Success:

```yaml
{
    "jsonrpc": "2.0",
    "result": $object,
    "id": "dontcare"
}
```

There $number - integer or string "dontcare", $string - string and $object - is some JSON object.

## Methods

* get_block

Get block data for specific block number.

Request:

```yaml
{
    "jsonrpc": "2.0",
    "id": $number_or_dontcare, 
    "method": "get_block", 
    "params": {
        "block_id": $number
    }
}
```

Responce:

```yaml
{
    "jsonrpc": "2.0",
    "result": {
        "block": $block
    },
    "id": $number_or_dontcare
}
```

There "block" field returns block for requested block id

* get_last_block

Get last block number.

Request:

```yaml
{
    "jsonrpc": "2.0",
    "id": $number_or_dontcare, 
    "method": "get_last_block", 
    "params": {}
}
```

Responce:

```yaml
{
    "jsonrpc": "2.0",
    "result": {
        "last_block": $number
    },
    "id": $number_or_dontcare
}
```

There "last_block" field returns number of last block

* write_register_account

Create new acccount with 0 public balance and no private UTXO.

Request:

```yaml
{
    "jsonrpc": "2.0",
    "id": $number_or_dontcare, 
    "method": "write_register_account", 
    "params": {}
}
```

Responce:

```yaml
{
    "jsonrpc": "2.0",
    "result": {
        "status": $string
    },
    "id": $number_or_dontcare
}
```

There "status" field shows address of generated account 

* show_account_public_balance

Show account public balance, field "account_addr" can be taken from response in "write_register_account" request.

Request:

```yaml
{
    "jsonrpc": "2.0",
    "id": $number_or_dontcare, 
    "method": "show_account_public_balance", 
    "params": {
        "account_addr": $string
    }
}
```

Responce:

```yaml
{
    "jsonrpc": "2.0",
    "result": {
        "addr": $string,
        "balance": $number
    },
    "id": $number_or_dontcare
}
```

Fields in response is self-explanatory.

* write_deposit_public_balance

Deposit public balance into account. Any amount under u64::MAX can be deposited, can overflow.
Due to hashing process(transactions currently does not have randomization factor), we can not send two deposits with same amount to one account.

Request:

```yaml
{
    "jsonrpc": "2.0",
    "id": $number_or_dontcare, 
    "method": "write_deposit_public_balance", 
    "params": {
        "account_addr": $string,
        "amount": $number
    }
}
```

Responce:

```yaml
{
    "jsonrpc": "2.0",
    "result": {
        "status": "success"
    },
    "id": $number_or_dontcare
}
```

Fields in response is self-explanatory.

* write_mint_utxo

Mint private UTXO for account.
Due to hashing process(transactions currently does not have randomization factor), we can not send two mints with same amount to one account.

Request:

```yaml
{
    "jsonrpc": "2.0",
    "id": $number_or_dontcare, 
    "method": "write_mint_utxo", 
    "params": {
        "account_addr": $string,
        "amount": $number
    }
}
```

Responce:

```yaml
{
    "jsonrpc": "2.0",
    "result": {
        "status": "success",
        "utxo": {
            "asset": [$number],
            "commitment_hash": $string,
            "hash": $string
        }
    },
    "id": $number_or_dontcare
}
```

There in "utxo" field "hash" is used for viewing purposes, field "commitment_hash" is used for sending purposes.

* show_account_utxo

Show UTXO data for account. "utxo_hash" there can be taken from "hash" field in response for "write_mint_utxo" request

Request:

```yaml
{
    "jsonrpc": "2.0",
    "id": $number_or_dontcare, 
    "method": "show_account_utxo", 
    "params": {
        "account_addr": $string,
        "utxo_hash": $string
    }
}
```

Responce:

```yaml
{
    "jsonrpc": "2.0",
    "result": {
        "amount": $number,
        "asset": [$number],
        "hash": $string
    },
    "id": $number_or_dontcare
}
```

Fields in response is self-explanatory.

* write_send_utxo_private

Send utxo from one account private balance into another(need to be different) private balance. 

Both parties are is hidden.

Request:

```yaml
{
    "jsonrpc": "2.0",
    "id": $number_or_dontcare, 
    "method": "write_send_utxo_private", 
    "params": {
        "account_addr_sender": $string,
        "account_addr_receiver": $string,
        "utxo_hash": $string,
        "utxo_commitment": $string
    }
}
```

Responce:

```yaml
{
    "jsonrpc": "2.0",
    "result": {
        "status": "success",
        "utxo_result": {
            "asset": [$number],
            "commitment_hash": $string,
            "hash": $string
        }
    },
    "id": $number_or_dontcare
}
```

Be aware, that during this action old UTXO is nullified, hence can not be used anymore, even if present in owner private state.

* write_send_utxo_deshielded

Send utxo from one account private balance into another(not neccesary different account) public balance. 

Sender is hidden.

Request:

```yaml
{
    "jsonrpc": "2.0",
    "id": $number_or_dontcare, 
    "method": "write_send_utxo_deshielded", 
    "params": {
        "account_addr_sender": $string,
        "account_addr_receiver": $string,
        "utxo_hash": $string,
        "utxo_commitment": $string
    }
}
```

Responce:

```yaml
{
    "jsonrpc": "2.0",
    "result": {
        "status": "success"
    },
    "id": $number_or_dontcare
}
```

Fields in response is self-explanatory.

* write_send_utxo_shielded

Send amount from one account public balance into another(not neccesary different account) private balance. 

Receiver is hidden.

Request:

```yaml
{
    "jsonrpc": "2.0",
    "id": $number_or_dontcare, 
    "method": "write_send_utxo_shielded", 
    "params": {
        "account_addr_sender": $string,
        "account_addr_receiver": $string,
        "amount": $number
    }
}
```

Responce:

```yaml
{
    "jsonrpc": "2.0",
    "result": {
        "status": "success",
        "utxo_result": {
            "asset": [$number],
            "commitment_hash": $string,
            "hash": $string
        }
    },
    "id": $number_or_dontcare
}
```

Fields in response is self-explanatory.
