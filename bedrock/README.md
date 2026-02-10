# Bedrock Configuration Files for All-in-One run and Integration Tests

## How to update

- `docker-compose.yml` file.

    Compare with `https://github.com/logos-blockchain/logos-blockchain/blob/master/compose.static.yml` and update the file accordingly, don't bring unneeded things like grafana and etc.
    Replace `sha` hash with the latest `testnet` tag hash.

- `scripts` folder.

    ```bash
    curl https://raw.githubusercontent.com/logos-blockchain/logos-blockchain/master/testnet/scripts/run_cfgsync.sh >> scripts/run_cfgsync.sh
    curl https://raw.githubusercontent.com/logos-blockchain/logos-blockchain/master/testnet/scripts/run_logos_blockchain_node.sh >> scripts/run_logos_blockchain_node.sh
    chmod +x scripts/*
    ```

    Then in `scripts/run_logos_blockchain_node.sh` update `cfgsync-client` to `logos-blockchain-cfgsync-client` and in `scripts/run_cfgsync.sh` update `cfgsync-server` to `logos-blockchain-cfgsync-server` if it hasn't been fixed already, see <https://github.com/logos-blockchain/logos-blockchain/pull/2092>.

- `cfgsync.yaml` file.

    ```bash
    curl -O https://raw.githubusercontent.com/logos-blockchain/logos-blockchain/master/testnet/cfgsync.yaml
    ```

    Set `logger`, `tracing` and `metrics` to `None`

- `kzgrs_test_params` file.

    ```bash
    curl -O https://raw.githubusercontent.com/logos-blockchain/logos-blockchain/master/tests/kzgrs/kzgrs_test_params
    ```
