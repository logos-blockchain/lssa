use key_protocol::key_management::{
    KeyChain,
    key_tree::{
        chain_index::ChainIndex, keys_private::ChildKeysPrivate, keys_public::ChildKeysPublic,
    },
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitialAccountDataPublic {
    pub account_id: String,
    pub pub_sign_key: nssa::PrivateKey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistentAccountDataPublic {
    pub account_id: nssa::AccountId,
    pub chain_index: ChainIndex,
    pub data: ChildKeysPublic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitialAccountDataPrivate {
    pub account_id: String,
    pub account: nssa_core::account::Account,
    pub key_chain: KeyChain,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistentAccountDataPrivate {
    pub account_id: nssa::AccountId,
    pub chain_index: ChainIndex,
    pub data: ChildKeysPrivate,
}

// Big difference in enum variants sizes
// however it is improbable, that we will have that much accounts, that it will substantialy affect
// memory
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InitialAccountData {
    Public(InitialAccountDataPublic),
    Private(InitialAccountDataPrivate),
}

// Big difference in enum variants sizes
// however it is improbable, that we will have that much accounts, that it will substantialy affect
// memory
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PersistentAccountData {
    Public(PersistentAccountDataPublic),
    Private(PersistentAccountDataPrivate),
    Preconfigured(InitialAccountData),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistentStorage {
    pub accounts: Vec<PersistentAccountData>,
    pub last_synced_block: u64,
}

impl InitialAccountData {
    pub fn account_id(&self) -> nssa::AccountId {
        match &self {
            Self::Public(acc) => acc.account_id.parse().unwrap(),
            Self::Private(acc) => acc.account_id.parse().unwrap(),
        }
    }
}

impl PersistentAccountData {
    pub fn account_id(&self) -> nssa::AccountId {
        match &self {
            Self::Public(acc) => acc.account_id,
            Self::Private(acc) => acc.account_id,
            Self::Preconfigured(acc) => acc.account_id(),
        }
    }
}

impl From<InitialAccountDataPublic> for InitialAccountData {
    fn from(value: InitialAccountDataPublic) -> Self {
        Self::Public(value)
    }
}

impl From<InitialAccountDataPrivate> for InitialAccountData {
    fn from(value: InitialAccountDataPrivate) -> Self {
        Self::Private(value)
    }
}

impl From<PersistentAccountDataPublic> for PersistentAccountData {
    fn from(value: PersistentAccountDataPublic) -> Self {
        Self::Public(value)
    }
}

impl From<PersistentAccountDataPrivate> for PersistentAccountData {
    fn from(value: PersistentAccountDataPrivate) -> Self {
        Self::Private(value)
    }
}

impl From<InitialAccountData> for PersistentAccountData {
    fn from(value: InitialAccountData) -> Self {
        Self::Preconfigured(value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasConfig {
    /// Gas spent per deploying one byte of data
    pub gas_fee_per_byte_deploy: u64,
    /// Gas spent per reading one byte of data in VM
    pub gas_fee_per_input_buffer_runtime: u64,
    /// Gas spent per one byte of contract data in runtime
    pub gas_fee_per_byte_runtime: u64,
    /// Cost of one gas of runtime in public balance
    pub gas_cost_runtime: u64,
    /// Cost of one gas of deployment in public balance
    pub gas_cost_deploy: u64,
    /// Gas limit for deployment
    pub gas_limit_deploy: u64,
    /// Gas limit for runtime
    pub gas_limit_runtime: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletConfig {
    /// Override rust log (env var logging level)
    pub override_rust_log: Option<String>,
    /// Sequencer URL
    pub sequencer_addr: String,
    /// Sequencer polling duration for new blocks in milliseconds
    pub seq_poll_timeout_millis: u64,
    /// Sequencer polling max number of blocks to find transaction
    pub seq_tx_poll_max_blocks: usize,
    /// Sequencer polling max number error retries
    pub seq_poll_max_retries: u64,
    /// Max amount of blocks to poll in one request
    pub seq_block_poll_max_amount: u64,
    /// Initial accounts for wallet
    pub initial_accounts: Vec<InitialAccountData>,
}

impl Default for WalletConfig {
    fn default() -> Self {
        Self {
            override_rust_log: None,
            sequencer_addr: "http://127.0.0.1:3040".to_string(),
            seq_poll_timeout_millis: 12000,
            seq_tx_poll_max_blocks: 5,
            seq_poll_max_retries: 5,
            seq_block_poll_max_amount: 100,
            initial_accounts: {
                let init_acc_json = r#"
                [
        {
            "Public": {
                "account_id": "BLgCRDXYdQPMMWVHYRFGQZbgeHx9frkipa8GtpG2Syqy",
                "pub_sign_key": [
                    16,
                    162,
                    106,
                    154,
                    236,
                    125,
                    52,
                    184,
                    35,
                    100,
                    238,
                    174,
                    69,
                    197,
                    41,
                    77,
                    187,
                    10,
                    118,
                    75,
                    0,
                    11,
                    148,
                    238,
                    185,
                    181,
                    133,
                    17,
                    220,
                    72,
                    124,
                    77
                ]
            }
        },
        {
            "Public": {
                "account_id": "Gj1mJy5W7J5pfmLRujmQaLfLMWidNxQ6uwnhb666ZwHw",
                "pub_sign_key": [
                    113,
                    121,
                    64,
                    177,
                    204,
                    85,
                    229,
                    214,
                    178,
                    6,
                    109,
                    191,
                    29,
                    154,
                    63,
                    38,
                    242,
                    18,
                    244,
                    219,
                    8,
                    208,
                    35,
                    136,
                    23,
                    127,
                    207,
                    237,
                    216,
                    169,
                    190,
                    27
                ]
            }
        },
        {
            "Private": {
                "account_id": "3oCG8gqdKLMegw4rRfyaMQvuPHpcASt7xwttsmnZLSkw",
                "account": {
                    "program_owner": [
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0
                    ],
                    "balance": 10000,
                    "data": [],
                    "nonce": 0
                },
                "key_chain": {
                    "secret_spending_key": [
                        251,
                        82,
                        235,
                        1,
                        146,
                        96,
                        30,
                        81,
                        162,
                        234,
                        33,
                        15,
                        123,
                        129,
                        116,
                        0,
                        84,
                        136,
                        176,
                        70,
                        190,
                        224,
                        161,
                        54,
                        134,
                        142,
                        154,
                        1,
                        18,
                        251,
                        242,
                        189
                    ],
                    "private_key_holder": {
                        "nullifier_secret_key": [
                            29,
                            250,
                            10,
                            187,
                            35,
                            123,
                            180,
                            250,
                            246,
                            97,
                            216,
                            153,
                            44,
                            156,
                            16,
                            93,
                            241,
                            26,
                            174,
                            219,
                            72,
                            84,
                            34,
                            247,
                            112,
                            101,
                            217,
                            243,
                            189,
                            173,
                            75,
                            20
                        ],
                        "incoming_viewing_secret_key": [
                            251,
                            201,
                            22,
                            154,
                            100,
                            165,
                            218,
                            108,
                            163,
                            190,
                            135,
                            91,
                            145,
                            84,
                            69,
                            241,
                            46,
                            117,
                            217,
                            110,
                            197,
                            248,
                            91,
                            193,
                            14,
                            104,
                            88,
                            103,
                            67,
                            153,
                            182,
                            158
                        ],
                        "outgoing_viewing_secret_key": [
                            25,
                            67,
                            121,
                            76,
                            175,
                            100,
                            30,
                            198,
                            105,
                            123,
                            49,
                            169,
                            75,
                            178,
                            75,
                            210,
                            100,
                            143,
                            210,
                            243,
                            228,
                            243,
                            21,
                            18,
                            36,
                            84,
                            164,
                            186,
                            139,
                            113,
                            214,
                            12
                        ]
                    },
                    "nullifer_public_key": [
                        63,
                        202,
                        178,
                        231,
                        183,
                        82,
                        237,
                        212,
                        216,
                        221,
                        215,
                        255,
                        153,
                        101,
                        177,
                        161,
                        254,
                        210,
                        128,
                        122,
                        54,
                        190,
                        230,
                        151,
                        183,
                        64,
                        225,
                        229,
                        113,
                        1,
                        228,
                        97
                    ],
                    "incoming_viewing_public_key": [
                        3,
                        235,
                        139,
                        131,
                        237,
                        177,
                        122,
                        189,
                        6,
                        177,
                        167,
                        178,
                        202,
                        117,
                        246,
                        58,
                        28,
                        65,
                        132,
                        79,
                        220,
                        139,
                        119,
                        243,
                        187,
                        160,
                        212,
                        121,
                        61,
                        247,
                        116,
                        72,
                        205
                    ]
                }
            }
        },
        {
            "Private": {
                "account_id": "AKTcXgJ1xoynta1Ec7y6Jso1z1JQtHqd7aPQ1h9er6xX",
                "account": {
                    "program_owner": [
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0
                    ],
                    "balance": 20000,
                    "data": [],
                    "nonce": 0
                },
                "key_chain": {
                    "secret_spending_key": [
                        238,
                        171,
                        241,
                        69,
                        111,
                        217,
                        85,
                        64,
                        19,
                        82,
                        18,
                        189,
                        32,
                        91,
                        78,
                        175,
                        107,
                        7,
                        109,
                        60,
                        52,
                        44,
                        243,
                        230,
                        72,
                        244,
                        192,
                        92,
                        137,
                        33,
                        118,
                        254
                    ],
                    "private_key_holder": {
                        "nullifier_secret_key": [
                            25,
                            211,
                            215,
                            119,
                            57,
                            223,
                            247,
                            37,
                            245,
                            144,
                            122,
                            29,
                            118,
                            245,
                            83,
                            228,
                            23,
                            9,
                            101,
                            120,
                            88,
                            33,
                            238,
                            207,
                            128,
                            61,
                            110,
                            2,
                            89,
                            62,
                            164,
                            13
                        ],
                        "incoming_viewing_secret_key": [
                            193,
                            181,
                            14,
                            196,
                            142,
                            84,
                            15,
                            65,
                            128,
                            101,
                            70,
                            196,
                            241,
                            47,
                            130,
                            221,
                            23,
                            146,
                            161,
                            237,
                            221,
                            40,
                            19,
                            126,
                            59,
                            15,
                            169,
                            236,
                            25,
                            105,
                            104,
                            231
                        ],
                        "outgoing_viewing_secret_key": [
                            20,
                            170,
                            220,
                            108,
                            41,
                            23,
                            155,
                            217,
                            247,
                            190,
                            175,
                            168,
                            247,
                            34,
                            105,
                            134,
                            114,
                            74,
                            104,
                            91,
                            211,
                            62,
                            126,
                            13,
                            130,
                            100,
                            241,
                            214,
                            250,
                            236,
                            38,
                            150
                        ]
                    },
                    "nullifer_public_key": [
                        192,
                        251,
                        166,
                        243,
                        167,
                        236,
                        84,
                        249,
                        35,
                        136,
                        130,
                        172,
                        219,
                        225,
                        161,
                        139,
                        229,
                        89,
                        243,
                        125,
                        194,
                        213,
                        209,
                        30,
                        23,
                        174,
                        100,
                        244,
                        124,
                        74,
                        140,
                        47
                    ],
                    "incoming_viewing_public_key": [
                        2,
                        181,
                        98,
                        93,
                        216,
                        241,
                        241,
                        110,
                        58,
                        198,
                        119,
                        174,
                        250,
                        184,
                        1,
                        204,
                        200,
                        173,
                        44,
                        238,
                        37,
                        247,
                        170,
                        156,
                        100,
                        254,
                        116,
                        242,
                        28,
                        183,
                        187,
                        77,
                        255
                    ]
                }
            }
        }
    ]
                "#;
                serde_json::from_str(init_acc_json).unwrap()
            },
        }
    }
}
