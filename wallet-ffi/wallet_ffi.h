/**
 * NSSA Wallet FFI Bindings
 *
 * Thread Safety: All functions are thread-safe. The wallet handle can be
 * shared across threads, but operations are serialized internally.
 *
 * Memory Management:
 * - Functions returning pointers allocate memory that must be freed
 * - Use the corresponding wallet_ffi_free_* function to free memory
 * - Never free memory returned by FFI using standard C free()
 *
 * Error Handling:
 * - Functions return WalletFfiError codes
 * - On error, call wallet_ffi_get_last_error() for detailed message
 * - The error string must be freed with wallet_ffi_free_error_string()
 *
 * Initialization:
 * 1. Call wallet_ffi_init_runtime() before any other function
 * 2. Create wallet with wallet_ffi_create_new() or wallet_ffi_open()
 * 3. Destroy wallet with wallet_ffi_destroy() when done
 */


#ifndef WALLET_FFI_H
#define WALLET_FFI_H

/* Generated with cbindgen:0.26.0 */

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

/**
 * Error codes returned by FFI functions.
 */
typedef enum WalletFfiError {
  /**
   * Operation completed successfully
   */
  SUCCESS = 0,
  /**
   * A null pointer was passed where a valid pointer was expected
   */
  NULL_POINTER = 1,
  /**
   * Invalid UTF-8 string
   */
  INVALID_UTF8 = 2,
  /**
   * Wallet handle is not initialized
   */
  WALLET_NOT_INITIALIZED = 3,
  /**
   * Configuration error
   */
  CONFIG_ERROR = 4,
  /**
   * Storage/persistence error
   */
  STORAGE_ERROR = 5,
  /**
   * Network/RPC error
   */
  NETWORK_ERROR = 6,
  /**
   * Account not found
   */
  ACCOUNT_NOT_FOUND = 7,
  /**
   * Key not found for account
   */
  KEY_NOT_FOUND = 8,
  /**
   * Insufficient funds for operation
   */
  INSUFFICIENT_FUNDS = 9,
  /**
   * Invalid account ID format
   */
  INVALID_ACCOUNT_ID = 10,
  /**
   * Tokio runtime error
   */
  RUNTIME_ERROR = 11,
  /**
   * Password required but not provided
   */
  PASSWORD_REQUIRED = 12,
  /**
   * Block synchronization error
   */
  SYNC_ERROR = 13,
  /**
   * Serialization/deserialization error
   */
  SERIALIZATION_ERROR = 14,
  /**
   * Internal error (catch-all)
   */
  INTERNAL_ERROR = 99,
} WalletFfiError;

/**
 * Opaque pointer to the Wallet instance.
 *
 * This type is never instantiated directly - it's used as an opaque handle
 * to hide the internal wallet structure from C code.
 */
typedef struct WalletHandle {
  uint8_t _private[0];
} WalletHandle;

/**
 * 32-byte array type for AccountId, keys, hashes, etc.
 */
typedef struct FfiBytes32 {
  uint8_t data[32];
} FfiBytes32;

/**
 * Single entry in the account list.
 */
typedef struct FfiAccountListEntry {
  struct FfiBytes32 account_id;
  bool is_public;
} FfiAccountListEntry;

/**
 * List of accounts returned by wallet_ffi_list_accounts.
 */
typedef struct FfiAccountList {
  struct FfiAccountListEntry *entries;
  uintptr_t count;
} FfiAccountList;

/**
 * Program ID - 8 u32 values (32 bytes total).
 */
typedef struct FfiProgramId {
  uint32_t data[8];
} FfiProgramId;

/**
 * Account data structure - C-compatible version of nssa Account.
 *
 * Note: `balance` and `nonce` are u128 values represented as little-endian
 * byte arrays since C doesn't have native u128 support.
 */
typedef struct FfiAccount {
  struct FfiProgramId program_owner;
  /**
   * Balance as little-endian [u8; 16]
   */
  uint8_t balance[16];
  /**
   * Pointer to account data bytes
   */
  const uint8_t *data;
  /**
   * Length of account data
   */
  uintptr_t data_len;
  /**
   * Nonce as little-endian [u8; 16]
   */
  uint8_t nonce[16];
} FfiAccount;

/**
 * Public key info for a public account.
 */
typedef struct FfiPublicAccountKey {
  struct FfiBytes32 public_key;
} FfiPublicAccountKey;

/**
 * Public keys for a private account (safe to expose).
 */
typedef struct FfiPrivateAccountKeys {
  /**
   * Nullifier public key (32 bytes)
   */
  struct FfiBytes32 nullifier_public_key;
  /**
   * Incoming viewing public key (compressed secp256k1 point)
   */
  const uint8_t *incoming_viewing_public_key;
  /**
   * Length of incoming viewing public key (typically 33 bytes)
   */
  uintptr_t incoming_viewing_public_key_len;
} FfiPrivateAccountKeys;

/**
 * Result of a transfer operation.
 */
typedef struct FfiTransferResult {
  /**
   * Transaction hash (null-terminated string, or null on failure)
   */
  char *tx_hash;
  /**
   * Whether the transfer succeeded
   */
  bool success;
} FfiTransferResult;

/**
 * Initialize the global Tokio runtime.
 *
 * This must be called before any async operations (like network calls).
 * Safe to call multiple times - subsequent calls are no-ops.
 *
 * # Returns
 * - `Success` if the runtime was initialized or already exists
 * - `RuntimeError` if runtime creation failed
 */
enum WalletFfiError wallet_ffi_init_runtime(void);

/**
 * Check if the runtime is initialized.
 *
 * # Returns
 * - `true` if the runtime is ready
 * - `false` if `wallet_ffi_init_runtime()` hasn't been called yet
 */
bool wallet_ffi_runtime_initialized(void);

/**
 * Create a new public account.
 *
 * Public accounts use standard transaction signing and are suitable for
 * non-private operations.
 *
 * # Parameters
 * - `handle`: Valid wallet handle
 * - `out_account_id`: Output pointer for the new account ID (32 bytes)
 *
 * # Returns
 * - `Success` on successful creation
 * - Error code on failure
 */
enum WalletFfiError wallet_ffi_create_account_public(struct WalletHandle *handle,
                                                     struct FfiBytes32 *out_account_id);

/**
 * Create a new private account.
 *
 * Private accounts use privacy-preserving transactions with nullifiers
 * and commitments.
 *
 * # Parameters
 * - `handle`: Valid wallet handle
 * - `out_account_id`: Output pointer for the new account ID (32 bytes)
 *
 * # Returns
 * - `Success` on successful creation
 * - Error code on failure
 */
enum WalletFfiError wallet_ffi_create_account_private(struct WalletHandle *handle,
                                                      struct FfiBytes32 *out_account_id);

/**
 * List all accounts in the wallet.
 *
 * Returns both public and private accounts managed by this wallet.
 *
 * # Parameters
 * - `handle`: Valid wallet handle
 * - `out_list`: Output pointer for the account list
 *
 * # Returns
 * - `Success` on successful listing
 * - Error code on failure
 *
 * # Memory
 * The returned list must be freed with `wallet_ffi_free_account_list()`.
 */
enum WalletFfiError wallet_ffi_list_accounts(struct WalletHandle *handle,
                                             struct FfiAccountList *out_list);

/**
 * Free an account list returned by `wallet_ffi_list_accounts`.
 *
 * # Safety
 * The list must be either null or a valid list returned by `wallet_ffi_list_accounts`.
 */
void wallet_ffi_free_account_list(struct FfiAccountList *list);

/**
 * Get account balance.
 *
 * For public accounts, this fetches the balance from the network.
 * For private accounts, this returns the locally cached balance.
 *
 * # Parameters
 * - `handle`: Valid wallet handle
 * - `account_id`: The account ID (32 bytes)
 * - `is_public`: Whether this is a public account
 * - `out_balance`: Output for balance as little-endian [u8; 16]
 *
 * # Returns
 * - `Success` on successful query
 * - Error code on failure
 */
enum WalletFfiError wallet_ffi_get_balance(struct WalletHandle *handle,
                                           const struct FfiBytes32 *account_id,
                                           bool is_public,
                                           uint8_t (*out_balance)[16]);

/**
 * Get full public account data from the network.
 *
 * # Parameters
 * - `handle`: Valid wallet handle
 * - `account_id`: The account ID (32 bytes)
 * - `out_account`: Output pointer for account data
 *
 * # Returns
 * - `Success` on successful query
 * - Error code on failure
 *
 * # Memory
 * The account data must be freed with `wallet_ffi_free_account_data()`.
 */
enum WalletFfiError wallet_ffi_get_account_public(struct WalletHandle *handle,
                                                  const struct FfiBytes32 *account_id,
                                                  struct FfiAccount *out_account);

/**
 * Free account data returned by `wallet_ffi_get_account_public`.
 *
 * # Safety
 * The account must be either null or a valid account returned by
 * `wallet_ffi_get_account_public`.
 */
void wallet_ffi_free_account_data(struct FfiAccount *account);

/**
 * Get the last error message.
 *
 * Returns a pointer to a null-terminated string, or null if no error is set.
 * The caller owns the returned string and must free it with
 * `wallet_ffi_free_error_string`.
 */
char *wallet_ffi_get_last_error(void);

/**
 * Free an error string returned by `wallet_ffi_get_last_error`.
 *
 * # Safety
 * The pointer must be either null or a valid pointer returned by
 * `wallet_ffi_get_last_error`.
 */
void wallet_ffi_free_error_string(char *ptr);

/**
 * Get the public key for a public account.
 *
 * This returns the public key derived from the account's signing key.
 *
 * # Parameters
 * - `handle`: Valid wallet handle
 * - `account_id`: The account ID (32 bytes)
 * - `out_public_key`: Output pointer for the public key
 *
 * # Returns
 * - `Success` on successful retrieval
 * - `KeyNotFound` if the account's key is not in this wallet
 * - Error code on other failures
 */
enum WalletFfiError wallet_ffi_get_public_account_key(struct WalletHandle *handle,
                                                      const struct FfiBytes32 *account_id,
                                                      struct FfiPublicAccountKey *out_public_key);

/**
 * Get keys for a private account.
 *
 * Returns the nullifier public key (NPK) and incoming viewing public key (IPK)
 * for the specified private account. These keys are safe to share publicly.
 *
 * # Parameters
 * - `handle`: Valid wallet handle
 * - `account_id`: The account ID (32 bytes)
 * - `out_keys`: Output pointer for the key data
 *
 * # Returns
 * - `Success` on successful retrieval
 * - `AccountNotFound` if the private account is not in this wallet
 * - Error code on other failures
 *
 * # Memory
 * The keys structure must be freed with `wallet_ffi_free_private_account_keys()`.
 */
enum WalletFfiError wallet_ffi_get_private_account_keys(struct WalletHandle *handle,
                                                        const struct FfiBytes32 *account_id,
                                                        struct FfiPrivateAccountKeys *out_keys);

/**
 * Free private account keys returned by `wallet_ffi_get_private_account_keys`.
 *
 * # Safety
 * The keys must be either null or valid keys returned by
 * `wallet_ffi_get_private_account_keys`.
 */
void wallet_ffi_free_private_account_keys(struct FfiPrivateAccountKeys *keys);

/**
 * Convert an account ID to a Base58 string.
 *
 * # Parameters
 * - `account_id`: The account ID (32 bytes)
 *
 * # Returns
 * - Pointer to null-terminated Base58 string on success
 * - Null pointer on error
 *
 * # Memory
 * The returned string must be freed with `wallet_ffi_free_string()`.
 */
char *wallet_ffi_account_id_to_base58(const struct FfiBytes32 *account_id);

/**
 * Parse a Base58 string into an account ID.
 *
 * # Parameters
 * - `base58_str`: Null-terminated Base58 string
 * - `out_account_id`: Output pointer for the account ID (32 bytes)
 *
 * # Returns
 * - `Success` on successful parsing
 * - `InvalidAccountId` if the string is not valid Base58
 * - Error code on other failures
 */
enum WalletFfiError wallet_ffi_account_id_from_base58(const char *base58_str,
                                                      struct FfiBytes32 *out_account_id);

/**
 * Synchronize private accounts to a specific block.
 *
 * This scans the blockchain from the last synced block to the specified block,
 * updating private account balances based on any relevant transactions.
 *
 * # Parameters
 * - `handle`: Valid wallet handle
 * - `block_id`: Target block number to sync to
 *
 * # Returns
 * - `Success` if synchronization completed
 * - `SyncError` if synchronization failed
 * - Error code on other failures
 *
 * # Note
 * This operation can take a while for large block ranges. The wallet
 * internally uses a progress bar which may output to stdout.
 */
enum WalletFfiError wallet_ffi_sync_to_block(struct WalletHandle *handle, uint64_t block_id);

/**
 * Get the last synced block number.
 *
 * # Parameters
 * - `handle`: Valid wallet handle
 * - `out_block_id`: Output pointer for the block number
 *
 * # Returns
 * - `Success` on success
 * - Error code on failure
 */
enum WalletFfiError wallet_ffi_get_last_synced_block(struct WalletHandle *handle,
                                                     uint64_t *out_block_id);

/**
 * Get the current block height from the sequencer.
 *
 * # Parameters
 * - `handle`: Valid wallet handle
 * - `out_block_height`: Output pointer for the current block height
 *
 * # Returns
 * - `Success` on success
 * - `NetworkError` if the sequencer is unreachable
 * - Error code on other failures
 */
enum WalletFfiError wallet_ffi_get_current_block_height(struct WalletHandle *handle,
                                                        uint64_t *out_block_height);

/**
 * Send a public token transfer.
 *
 * Transfers tokens from one public account to another on the network.
 *
 * # Parameters
 * - `handle`: Valid wallet handle
 * - `from`: Source account ID (must be owned by this wallet)
 * - `to`: Destination account ID
 * - `amount`: Amount to transfer as little-endian [u8; 16]
 * - `out_result`: Output pointer for transfer result
 *
 * # Returns
 * - `Success` if the transfer was submitted successfully
 * - `InsufficientFunds` if the source account doesn't have enough balance
 * - `KeyNotFound` if the source account's signing key is not in this wallet
 * - Error code on other failures
 *
 * # Memory
 * The result must be freed with `wallet_ffi_free_transfer_result()`.
 */
enum WalletFfiError wallet_ffi_transfer_public(struct WalletHandle *handle,
                                               const struct FfiBytes32 *from,
                                               const struct FfiBytes32 *to,
                                               const uint8_t (*amount)[16],
                                               struct FfiTransferResult *out_result);

/**
 * Register a public account on the network.
 *
 * This initializes a public account on the blockchain. The account must be
 * owned by this wallet.
 *
 * # Parameters
 * - `handle`: Valid wallet handle
 * - `account_id`: Account ID to register
 * - `out_result`: Output pointer for registration result
 *
 * # Returns
 * - `Success` if the registration was submitted successfully
 * - Error code on failure
 *
 * # Memory
 * The result must be freed with `wallet_ffi_free_transfer_result()`.
 */
enum WalletFfiError wallet_ffi_register_public_account(struct WalletHandle *handle,
                                                       const struct FfiBytes32 *account_id,
                                                       struct FfiTransferResult *out_result);

/**
 * Free a transfer result returned by `wallet_ffi_transfer_public` or
 * `wallet_ffi_register_public_account`.
 *
 * # Safety
 * The result must be either null or a valid result from a transfer function.
 */
void wallet_ffi_free_transfer_result(struct FfiTransferResult *result);

/**
 * Create a new wallet with fresh storage.
 *
 * This initializes a new wallet with a new seed derived from the password.
 * Use this for first-time wallet creation.
 *
 * # Parameters
 * - `config_path`: Path to the wallet configuration file (JSON)
 * - `storage_path`: Path where wallet data will be stored
 * - `password`: Password for encrypting the wallet seed
 *
 * # Returns
 * - Opaque wallet handle on success
 * - Null pointer on error (call `wallet_ffi_get_last_error()` for details)
 *
 * # Safety
 * All string parameters must be valid null-terminated UTF-8 strings.
 */
struct WalletHandle *wallet_ffi_create_new(const char *config_path,
                                           const char *storage_path,
                                           const char *password);

/**
 * Open an existing wallet from storage.
 *
 * This loads a wallet that was previously created with `wallet_ffi_create_new()`.
 *
 * # Parameters
 * - `config_path`: Path to the wallet configuration file (JSON)
 * - `storage_path`: Path where wallet data is stored
 *
 * # Returns
 * - Opaque wallet handle on success
 * - Null pointer on error (call `wallet_ffi_get_last_error()` for details)
 *
 * # Safety
 * All string parameters must be valid null-terminated UTF-8 strings.
 */
struct WalletHandle *wallet_ffi_open(const char *config_path, const char *storage_path);

/**
 * Destroy a wallet handle and free its resources.
 *
 * After calling this function, the handle is invalid and must not be used.
 *
 * # Safety
 * - The handle must be either null or a valid handle from `wallet_ffi_create_new()`
 *   or `wallet_ffi_open()`.
 * - The handle must not be used after this call.
 */
void wallet_ffi_destroy(struct WalletHandle *handle);

/**
 * Save wallet state to persistent storage.
 *
 * This should be called periodically or after important operations to ensure
 * wallet data is persisted to disk.
 *
 * # Parameters
 * - `handle`: Valid wallet handle
 *
 * # Returns
 * - `Success` on successful save
 * - Error code on failure
 */
enum WalletFfiError wallet_ffi_save(struct WalletHandle *handle);

/**
 * Get the sequencer address from the wallet configuration.
 *
 * # Parameters
 * - `handle`: Valid wallet handle
 *
 * # Returns
 * - Pointer to null-terminated string on success (caller must free with `wallet_ffi_free_string()`)
 * - Null pointer on error
 */
char *wallet_ffi_get_sequencer_addr(struct WalletHandle *handle);

/**
 * Free a string returned by wallet FFI functions.
 *
 * # Safety
 * The pointer must be either null or a valid string returned by an FFI function.
 */
void wallet_ffi_free_string(char *ptr);

#endif /* WALLET_FFI_H */
