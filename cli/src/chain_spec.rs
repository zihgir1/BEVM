// Copyright 2019-2023 ChainX Project Authors. Licensed under GPL-3.0.

#![allow(unused)]
use std::collections::BTreeMap;
use std::convert::TryInto;

use hex_literal::hex;
use serde::{Deserialize, Serialize};
use serde_json::json;

use sc_chain_spec::ChainSpecExtension;
use sc_service::config::TelemetryEndpoints;
use sc_service::{ChainType, Properties};

use sp_authority_discovery::AuthorityId as AuthorityDiscoveryId;
use sp_consensus_babe::AuthorityId as BabeId;
use sp_core::{crypto::UncheckedInto, sr25519, Pair, Public};
use sp_finality_grandpa::AuthorityId as GrandpaId;
use sp_runtime::traits::{IdentifyAccount, Verify};

use pallet_im_online::sr25519::AuthorityId as ImOnlineId;
use sp_core::crypto::AccountId32;

use chainx_primitives::{AccountId, AssetId, Balance, ReferralId, Signature};
use chainx_runtime::constants::{currency::DOLLARS, time::DAYS};
use xp_assets_registrar::Chain;
use xp_protocol::{NetworkType, PCX, PCX_DECIMALS, X_BTC};
use xpallet_gateway_bitcoin::{BtcParams, BtcTxVerifier};
use xpallet_gateway_common::types::TrusteeInfoConfig;

use crate::genesis::assets::{genesis_assets, init_assets, pcx, AssetParams};
use crate::genesis::bitcoin::{btc_genesis_params, BtcGenesisParams, BtcTrusteeParams};

use chainx_runtime as chainx;
use dev_runtime as dev;
use malan_runtime as malan;

// Note this is the URL for the telemetry server
#[allow(unused)]
const POLKADOT_TELEMETRY_URL: &str = "wss://telemetry.polkadot.io/submit/";
#[allow(unused)]
const CHAINX_TELEMETRY_URL: &str = "wss://telemetry.chainx.org/submit/";

/// Node `ChainSpec` extensions.
///
/// Additional parameters for some Substrate core modules,
/// customizable from the chain spec.
#[derive(Default, Clone, Serialize, Deserialize, ChainSpecExtension)]
#[serde(rename_all = "camelCase")]
pub struct Extensions {
    /// Block numbers with known hashes.
    pub fork_blocks: sc_client_api::ForkBlocks<chainx_primitives::Block>,
    /// Known bad block hashes.
    pub bad_blocks: sc_client_api::BadBlocks<chainx_primitives::Block>,
    /// This value will be set by the `sync-state rpc` implementation.
    pub light_sync_state: sc_sync_state_rpc::LightSyncStateExtension,
}

/// The `ChainSpec` parameterised for the chainx mainnet runtime.
pub type ChainXChainSpec = sc_service::GenericChainSpec<chainx::GenesisConfig, Extensions>;
/// The `ChainSpec` parameterised for the chainx development runtime.
pub type DevChainSpec = sc_service::GenericChainSpec<dev::GenesisConfig, Extensions>;
/// The `ChainSpec` parameterised for the chainx testnet runtime.
pub type MalanChainSpec = sc_service::GenericChainSpec<malan::GenesisConfig, Extensions>;

type AccountPublic = <Signature as Verify>::Signer;

/// Helper function to generate a crypto pair from seed
pub fn get_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
    TPublic::Pair::from_string(&format!("//{}", seed), None)
        .expect("static values are valid; qed")
        .public()
}

/// Helper function to generate an account ID from seed
pub fn get_account_id_from_seed<TPublic: Public>(seed: &str) -> AccountId
where
    AccountPublic: From<<TPublic::Pair as Pair>::Public>,
{
    AccountPublic::from(get_from_seed::<TPublic>(seed)).into_account()
}

type AuthorityKeysTuple = (
    (AccountId, ReferralId), // (Staking ValidatorId, ReferralId)
    BabeId,
    GrandpaId,
    ImOnlineId,
    AuthorityDiscoveryId,
);

/// Helper function to generate an authority key for babe
pub fn authority_keys_from_seed(seed: &str) -> AuthorityKeysTuple {
    (
        (
            get_account_id_from_seed::<sr25519::Public>(seed),
            seed.as_bytes().to_vec(),
        ),
        get_from_seed::<BabeId>(seed),
        get_from_seed::<GrandpaId>(seed),
        get_from_seed::<ImOnlineId>(seed),
        get_from_seed::<AuthorityDiscoveryId>(seed),
    )
}

#[inline]
fn balance(input: Balance, decimals: u8) -> Balance {
    input * 10_u128.pow(decimals as u32)
}

/// A small macro for generating the info of PCX endowed accounts.
macro_rules! endowed_gen {
    ( $( ($seed:expr, $value:expr), )+ ) => {
        {
            let mut endowed = BTreeMap::new();
            let pcx_id = pcx().0;
            let endowed_info = vec![
                $((get_account_id_from_seed::<sr25519::Public>($seed), balance($value, PCX_DECIMALS)),)+
            ];
            endowed.insert(pcx_id, endowed_info);
            endowed
        }
    }
}

macro_rules! endowed {
    ( $( ($pubkey:expr, $value:expr), )+ ) => {
        {
            let mut endowed = BTreeMap::new();
            let pcx_id = pcx().0;
            let endowed_info = vec![
                $((($pubkey).into(), balance($value, PCX_DECIMALS)),)+
            ];
            endowed.insert(pcx_id, endowed_info);
            endowed
        }
    }
}

const ENDOWMENT: Balance = 10_000_000 * DOLLARS;
const STASH: Balance = 100 * DOLLARS;

/// Helper function to generate the network properties.
fn as_properties(network: NetworkType) -> Properties {
    json!({
        "ss58Format": network.ss58_addr_format_id(),
        "network": network,
        "tokenDecimals": PCX_DECIMALS,
        "tokenSymbol": "PCX"
    })
    .as_object()
    .expect("network properties generation can not fail; qed")
    .to_owned()
}

pub fn development_config() -> Result<DevChainSpec, String> {
    let wasm_binary =
        dev::WASM_BINARY.ok_or_else(|| "Development wasm binary not available".to_string())?;

    let endowed_balance = 50 * DOLLARS;
    let constructor = move || {
        build_dev_genesis(
            wasm_binary,
            vec![authority_keys_from_seed("Alice")],
            get_account_id_from_seed::<sr25519::Public>("Alice"),
            genesis_assets(),
            endowed_gen![
                ("Alice", endowed_balance),
                ("Bob", endowed_balance),
                ("Alice//stash", endowed_balance),
                ("Bob//stash", endowed_balance),
            ],
            btc_genesis_params(include_str!("res/btc_genesis_params_testnet.json")),
            crate::genesis::bitcoin::local_testnet_trustees(),
        )
    };
    Ok(DevChainSpec::from_genesis(
        "Development",
        "dev",
        ChainType::Development,
        constructor,
        vec![],
        None,
        Some("chainx-dev"),
        None,
        Some(as_properties(NetworkType::Testnet)),
        Default::default(),
    ))
}

#[cfg(feature = "runtime-benchmarks")]
pub fn benchmarks_config() -> Result<DevChainSpec, String> {
    let wasm_binary =
        dev::WASM_BINARY.ok_or_else(|| "Development wasm binary not available".to_string())?;

    let endowed_balance = 50 * DOLLARS;
    let constructor = move || {
        build_dev_genesis(
            wasm_binary,
            vec![authority_keys_from_seed("Alice")],
            get_account_id_from_seed::<sr25519::Public>("Alice"),
            genesis_assets(),
            endowed_gen![
                ("Alice", endowed_balance),
                ("Bob", endowed_balance),
                ("Alice//stash", endowed_balance),
                ("Bob//stash", endowed_balance),
            ],
            btc_genesis_params(include_str!("res/btc_genesis_params_benchmarks.json")),
            crate::genesis::bitcoin::benchmarks_trustees(),
        )
    };
    Ok(DevChainSpec::from_genesis(
        "Benchmarks",
        "dev",
        ChainType::Development,
        constructor,
        vec![],
        None,
        Some("chainx-dev"),
        None,
        Some(as_properties(NetworkType::Testnet)),
        Default::default(),
    ))
}

pub fn local_testnet_config() -> Result<DevChainSpec, String> {
    let wasm_binary =
        dev::WASM_BINARY.ok_or_else(|| "Development wasm binary not available".to_string())?;

    let endowed_balance = 50 * DOLLARS;
    let constructor = move || {
        build_dev_genesis(
            wasm_binary,
            vec![
                authority_keys_from_seed("Alice"),
                authority_keys_from_seed("Bob"),
            ],
            get_account_id_from_seed::<sr25519::Public>("Alice"),
            genesis_assets(),
            endowed_gen![
                ("Alice", endowed_balance),
                ("Bob", endowed_balance),
                ("Charlie", endowed_balance),
                ("Dave", endowed_balance),
                ("Eve", endowed_balance),
                ("Ferdie", endowed_balance),
                ("Alice//stash", endowed_balance),
                ("Bob//stash", endowed_balance),
                ("Charlie//stash", endowed_balance),
                ("Dave//stash", endowed_balance),
                ("Eve//stash", endowed_balance),
                ("Ferdie//stash", endowed_balance),
            ],
            btc_genesis_params(include_str!("res/btc_genesis_params_testnet.json")),
            crate::genesis::bitcoin::local_testnet_trustees(),
        )
    };
    Ok(DevChainSpec::from_genesis(
        "ChainX Local Testnet",
        "dev",
        ChainType::Local,
        constructor,
        vec![],
        None,
        Some("pcx"),
        None,
        Some(as_properties(NetworkType::Testnet)),
        Default::default(),
    ))
}

pub fn mainnet_config() -> Result<ChainXChainSpec, String> {
    ChainXChainSpec::from_json_bytes(&include_bytes!("./res/chainx_regenesis.json")[..])
}

pub fn new_mainnet_config() -> Result<ChainXChainSpec, String> {
    let wasm_binary =
        chainx::WASM_BINARY.ok_or_else(|| "ChainX wasm binary not available".to_string())?;

    let initial_authorities: Vec<AuthorityKeysTuple> = vec![
        (
            (
                // 5StNFoeSmLXr7SfDuwJqHR5CyKV2o4BD2yU36GGay3GVFhtt
                hex!["8fa51087d1a7327c90da45f8e369e31037606427f07ef77007a41036227a3a5b"].into(),
                b"Web3".to_vec(),
            ),
            // 5V8a6nmGmu7N9iCtds7Eb8EkdpBABx9BrcJvDewFNCLX3WKa
            hex!["f2f2d6e98256e93ed1ce9a089364193d08bb005276be3b312648585a12413c36"]
                .unchecked_into(),
            // 5R9SwUoWziEZyFr17AYB8Z1a4EYCXzyYGYWwWsGiP2pygtyX
            hex!["42ad0bf20a8a38084f62a8bc720cdf948994aa97c0afcc04f070ee85f7c3f4bf"]
                .unchecked_into(),
            // 5TDjip2pGZ8KUgW8iGFDGto4wt7gxXcmXFR98TcifiVYXnQA
            hex!["9e6afcacebf456bfc909d81d0bbdd0a337f1abca3677150bc83d388956cc1701"]
                .unchecked_into(),
            // 5RpGMngHcMKVHhY4f5CQ1LZDBbtuzofuVPKvFk9XxMjWaNLi
            hex!["6047ffbc896c22352433c0b3cf81b2e3264a3a0ab792709ef103b046bce86553"]
                .unchecked_into(),
        ),
        (
            (
                // 5RAZf8UHcbS5RBRpP9zptQJm84tpfnxcJ64ctSyxNJeLLxtq
                hex!["4386e83d66fbdf9ebe72af81d453f41fb8f877287f04823665fc81b58cab6e6b"].into(),
                b"XPool".to_vec(),
            ),
            // 5RuM4NsTGSTA3yVgWibxWNt8KEXYBLwmbq2p3rvz4TdJ2JVa
            hex!["64280c07db03b85ac6206e6558df9fd4abc8778ff9afe094e53aa7767ed05313"]
                .unchecked_into(),
            // 5RFAnoweNDFxvMsKNmeQazYRZwEeeYegGqHSCjTvgW8AxkbM
            hex!["470a27b38990f1a0101cb5f149c514bd0f5bc1ec24d24b3aa72ca4e92482ebff"]
                .unchecked_into(),
            // 5QcEM9mppDHKxKv5BnaYzFuHMmH35AtbS4JkCdPaWbVeCyyM
            hex!["2ade0d1735328c5753af812ce54df9db24d3979204d331cb1c08cc455dbb6f16"]
                .unchecked_into(),
            // 5UtnsNGRV867j5VHbBGsKxfiCbAJG4iFaocvW1dKg8T6dToi
            hex!["e87062ff7a629b90d2af59b1982442b2aa88fd470c961ab3f86de2f340e0fe55"]
                .unchecked_into(),
        ),
        (
            (
                // 5RaxFQc7E4ACr4FVoHj2SMA6MGMqT8Ck9mDV5byGZtPPUw8f
                hex!["5620d851190bda61acb08f1a06defcdd5a3c7da3c33819643e7d6ee232ee78bf"].into(),
                b"ChainY".to_vec(),
            ),
            // 5U3PkBpUqTxcqvdigWt21q2MnW3cyKVm44rsc4HLGPzf5Wn8
            hex!["c2c3852ff9feec91412b271a8a49eb41ca83b9d9fecf9906240e19d73eceed0d"]
                .unchecked_into(),
            // 5SVSrKBth8wwUAKS64Y2bUor7TUAhRgcH8HdqW3EPq3MBtdD
            hex!["7e29e36d138c6f789c0b6b4c98ca1162ee78a1828e4b1682f5e06756b6b1994f"]
                .unchecked_into(),
            // 5Ui2auEpdXnTybFMGFP183U7aUygrUcSGZzkvpas7k4wUHJT
            hex!["e03adc1ac1b442a5e0e2c6ef1b806a9874e478a7cfc8a3d4fce5731716fd951e"]
                .unchecked_into(),
            // 5USdrQDtyebV2d2exhGNjv1nG5vGCfUPhz5ZBMUSgLUQkFCe
            hex!["d47da74631ce8f5e0f29971538c4bae9834bbc28e82f250bbff060ba203ac035"]
                .unchecked_into(),
        ),
        (
            (
                // 5QpaUQudS4cxZEQTtviWm5pmv8NQWX5HkKmd9T1GFamqcu3h
                hex!["3448be503acf3f8c8831af55a4816e5382284dab213e1022edf368fb07aaeb25"].into(),
                b"PolkaWorld".to_vec(),
            ),
            // 5TTRAay4UF7C9tzua5K3TNTJ61gZ88XKZtqKqKeFvbAbrYLP
            hex!["a8d9f57c79d86a056b53b0a496359f0dc8099fb1f5a5ee46647b9e319a953a51"]
                .unchecked_into(),
            // 5ST6rf1SBgf7PSXVGypJMEsMFcujJ6wGhTFTJb5svuMfccKN
            hex!["7c60176664c7ba4c273771955509ed0a54ba45467356595b8961a3ec0ae71d55"]
                .unchecked_into(),
            // 5UMq5kxN8ExVj6USS8y7Umok1Dize44jteVR7eisLsc6xDds
            hex!["d0d33b1fead76b802c1aeebaeb86a25e133925b0cca6264da6feba6015b5bf4e"]
                .unchecked_into(),
            // 5QHcomC2mrLp51WbLW4j1uNjjSRMUdnq4H2FNADozp1jV5iY
            hex!["1cabfdd4b314033594b899a08fc285181fc928be962edef2e3323802df86283a"]
                .unchecked_into(),
        ),
        (
            (
                // 5SmuQ9LA8GexmSHgLsD3FSftBZBQqRySyJT2EQhvHWqYdMHn
                hex!["8ab72fa19af7cce7983af666d2945c238c10812d760b6b0181753cb9cbba127f"].into(),
                b"Polkadog".to_vec(),
            ),
            // 5UXwsgRGh2gNScAKyjKNC5TVDZLCGhSFp5UYcZcqfHKKZ2B8
            hex!["d88a8c4f49af34e68de2d71cb2ac390ad10c06f5bb7ddfa2df34098ec3ef3a10"]
                .unchecked_into(),
            // 5U2nLh22whssvbqCq6FQvmbqgvVthkZJuJdJhPy6EVgLuDUc
            hex!["c24c5605efdc47faedbb3c5fe6b7b47d73940597d1e3d898da53f8766c9678b4"]
                .unchecked_into(),
            // 5PniNjFJDBpjjc8GD6Xg8aJQhgS4ioWVAvcZUZsQenhndZXR
            hex!["06a09ea25228d2f53f0589a37b614282dcd94b118d2a029cb07b3cf568e90541"]
                .unchecked_into(),
            // 5TmtSzg3oAPs8Agv7x1WftQHNRQDJsMRH8P6Qu8ZABEKHXhR
            hex!["b6f037faa989b654b6869bbd931797078eb025dcb0cbd8ab17192461af634d32"]
                .unchecked_into(),
        ),
    ];
    let constructor = move || {
        mainnet_genesis(
            wasm_binary,
            initial_authorities.clone(),
            genesis_assets(),
            btc_genesis_params(include_str!("res/btc_genesis_params_mainnet.json")),
            crate::genesis::bitcoin::mainnet_trustees(),
        )
    };

    let bootnodes = Default::default();

    Ok(ChainXChainSpec::from_genesis(
        "ChainX",
        "chainx",
        ChainType::Live,
        constructor,
        bootnodes,
        Some(
            TelemetryEndpoints::new(vec![
                (CHAINX_TELEMETRY_URL.to_string(), 0),
                (POLKADOT_TELEMETRY_URL.to_string(), 0),
            ])
            .expect("ChainX telemetry url is valid; qed"),
        ),
        Some("pcx1"),
        None,
        Some(as_properties(NetworkType::Mainnet)),
        Default::default(),
    ))
}

fn mainnet_session_keys(
    babe: BabeId,
    grandpa: GrandpaId,
    im_online: ImOnlineId,
    authority_discovery: AuthorityDiscoveryId,
) -> chainx::SessionKeys {
    chainx::SessionKeys {
        grandpa,
        babe,
        im_online,
        authority_discovery,
    }
}

fn mainnet_genesis(
    wasm_binary: &[u8],
    initial_authorities: Vec<AuthorityKeysTuple>,
    assets: Vec<AssetParams>,
    bitcoin: BtcGenesisParams,
    trustees: Vec<(Chain, TrusteeInfoConfig, Vec<BtcTrusteeParams>)>,
) -> chainx::GenesisConfig {
    use malan_runtime::constants::time::DAYS;

    let (assets, assets_restrictions) = init_assets(assets);
    let tech_comm_members: Vec<AccountId> = vec![
        // 5TPu4DCQRSbNS9ESUcNGUn9HcF9AzrHiDP395bDxM9ZAqSD8
        hex!["a62add1af3bcf9256aa2def0fea1b9648cb72517ccee92a891dc2903a9093e52"].into(),
        // 5PgpWgUe5T5yw67hLAmbzge7viSwaKYQmpoMQosjpQsA9xvG
        hex!["0221ce7c4a0b771faaf0bbae23c3a1965348cb5257611313a73c3d4a53599509"].into(),
        // 5T1jHMHspov8UgD9ygXc7rL5oNZJdDB7WfRtAduDt4AXPUSq
        hex!["9542907d40eaab54d3a35a08be01ff82abe298ce210a7a3de3dd2cd0d6b0e9d3"].into(),
    ];

    let btc_genesis_trustees = trustees
        .iter()
        .find_map(|(chain, _, trustee_params)| {
            if *chain == Chain::Bitcoin {
                Some(
                    trustee_params
                        .iter()
                        .map(|i| (i.0).clone())
                        .collect::<Vec<_>>(),
                )
            } else {
                None
            }
        })
        .expect("bitcoin trustees generation can not fail; qed");

    chainx::GenesisConfig {
        system: chainx::SystemConfig {
            code: wasm_binary.to_vec(),
        },
        babe: chainx::BabeConfig {
            authorities: vec![],
            epoch_config: Some(chainx::BABE_GENESIS_EPOCH_CONFIG),
        },
        grandpa: chainx::GrandpaConfig {
            authorities: vec![],
        },
        council: chainx::CouncilConfig::default(),
        technical_committee: Default::default(),
        technical_membership: chainx::TechnicalMembershipConfig {
            members: tech_comm_members,
            phantom: Default::default(),
        },
        democracy: chainx::DemocracyConfig::default(),
        treasury: Default::default(),
        elections: Default::default(),
        im_online: chainx::ImOnlineConfig { keys: vec![] },
        authority_discovery: chainx::AuthorityDiscoveryConfig { keys: vec![] },
        session: chainx::SessionConfig {
            keys: initial_authorities
                .iter()
                .map(|x| {
                    (
                        (x.0).0.clone(),
                        (x.0).0.clone(),
                        chainx::SessionKeys {
                            grandpa: x.2.clone(),
                            babe: x.1.clone(),
                            im_online: x.3.clone(),
                            authority_discovery: x.4.clone(),
                        },
                    )
                })
                .collect::<Vec<_>>(),
        },
        balances: Default::default(),
        indices: chainx::IndicesConfig { indices: vec![] },
        x_system: chainx::XSystemConfig {
            network_props: NetworkType::Mainnet,
        },
        x_assets_registrar: chainx::XAssetsRegistrarConfig { assets },
        x_assets: chainx::XAssetsConfig {
            assets_restrictions,
            endowed: Default::default(),
        },
        x_gateway_common: chainx::XGatewayCommonConfig { trustees },
        x_gateway_bitcoin: chainx::XGatewayBitcoinConfig {
            genesis_trustees: btc_genesis_trustees,
            network_id: bitcoin.network,
            confirmation_number: bitcoin.confirmation_number,
            genesis_hash: bitcoin.hash(),
            genesis_info: (bitcoin.header(), bitcoin.height),
            params_info: BtcParams::new(
                // for bitcoin mainnet
                486604799,            // max_bits
                2 * 60 * 60,          // block_max_future
                2 * 7 * 24 * 60 * 60, // target_timespan_seconds
                10 * 60,              // target_spacing_seconds
                4,                    // retargeting_factor
            ), // retargeting_factor
            btc_withdrawal_fee: 500000,
            max_withdrawal_count: 100,
            verifier: BtcTxVerifier::Recover,
        },
        x_staking: chainx::XStakingConfig {
            validator_count: 40,
            sessions_per_era: 1,
            glob_dist_ratio: (12, 88), // (Treasury, X-type Asset and Staking) = (12, 88)
            mining_ratio: (10, 90),    // (Asset Mining, Staking) = (10, 90)
            minimum_penalty: 100 * DOLLARS,
            candidate_requirement: (100 * DOLLARS, 1_000 * DOLLARS), // Minimum value (self_bonded, total_bonded) to be a validator candidate
            ..Default::default()
        },
        x_mining_asset: chainx::XMiningAssetConfig {
            claim_restrictions: vec![(X_BTC, (10, DAYS * 7))],
            mining_power_map: vec![(X_BTC, 400)],
        },
        x_spot: chainx::XSpotConfig {
            trading_pairs: vec![(PCX, X_BTC, 9, 2, 100000, true)],
        },
        x_genesis_builder: chainx::XGenesisBuilderConfig {
            params: crate::genesis::genesis_builder_params(),
            initial_authorities: initial_authorities
                .iter()
                .map(|i| (i.0).1.clone())
                .collect(),
        },
        ethereum_chain_id: chainx::EthereumChainIdConfig { chain_id: 1501u64 },
        evm: Default::default(),
        ethereum: Default::default(),
        base_fee: chainx::BaseFeeConfig::new(
            chainx::DefaultBaseFeePerGas::get(),
            false,
            sp_runtime::Permill::from_parts(125_000),
        ),
        x_assets_bridge: chainx::XAssetsBridgeConfig { admin_key: None },
        x_btc_ledger: Default::default(),
    }
}

pub fn malan_config() -> Result<MalanChainSpec, String> {
    MalanChainSpec::from_json_bytes(&include_bytes!("./res/malan.json")[..])
}

pub fn new_malan_config() -> Result<MalanChainSpec, String> {
    let wasm_binary =
        malan::WASM_BINARY.ok_or_else(|| "ChainX wasm binary not available".to_string())?;

    let initial_authorities: Vec<AuthorityKeysTuple> = vec![
        (
            (
                // 5QkGjd5rsczm4qVgVpzRSdBe2SrhLvKrPrYFeAAtw4qbdRPh
                hex!["31000d19a3e9607d92b3697a661a6e7e9fbb65361846680d968cfc86c9561103"].into(),
                b"Hotbit".to_vec(),
            ),
            // 5DwEF6ek2uYzQEeW1Mx4YjFcrBVNvQyUHEUBWq7sXE9XbzEe
            hex!["52c4cb6299ef78711dd1025b7bfc91655abed0f028bbf04145c2e249b1454909"]
                .unchecked_into(),
            // 5EjGLje6XHExxPyuijBg8c8MGTbG6A3fLKGzEMFV8qHKZNjN
            hex!["75e1249435a447adc812cc418c01fb5719488025add677bcc931d36a2338848a"]
                .unchecked_into(),
            // 5GNFfpS8wy2bjHnR43AyRpYVyHsKHKwzDRPiJ83XvxeojrxX
            hex!["be533292b9da99f2d03eb1ef7c4c9dfe3dbe26bf2fca75562d2618fbc7870b24"]
                .unchecked_into(),
            // 5DSEUDH8scoC2XbeAuAxjki2zfJEHhg38HcseSWLJuJrzfj5
            hex!["3ca7705b612b2bd56a50a6284b8095bb23c71805e9ca047256f630589944f815"]
                .unchecked_into(),
        ),
        (
            (
                // 5StNFoeSmLXr7SfDuwJqHR5CyKV2o4BD2yU36GGay3GVFhtt
                hex!["8fa51087d1a7327c90da45f8e369e31037606427f07ef77007a41036227a3a5b"].into(),
                b"Web3".to_vec(),
            ),
            // 5GjmArSffr9wwZ1gJMfU7yAfguJzrpbrDdjMx9yTvi6zeQeK
            hex!["cebaa8ae0af251cbf2aa5e397a6186d440b1c9e4f930388b209d0b5f93dbbf70"]
                .unchecked_into(),
            // 5Hcq9FpiRhJywuVuvYWRSMakeB8dWwb1yKVkMzuCUxhMLxPG
            hex!["f5ad8c0b2806effb7a77234b2955860c95fad100ea706fa60ceb7274fd399e63"]
                .unchecked_into(),
            // 5C4rKSUr5p3Gc2bySXtqQcmsT1pRPUJiB33jgZ3YivXC9WqC
            hex!["001c7bf4abd047bc97a1fb3c201d6a785e1eb3c818c838b5f2f0be98121f586c"]
                .unchecked_into(),
            // 5Da114jPuaKkcFh8BTUKUmG9qD6oMYC5XXQAEFKo6gGudY3Z
            hex!["42941089ea8a4353e2dab6905d27260735526a1a408274fc6c0d233b1a9e311e"]
                .unchecked_into(),
        ),
        (
            (
                // 5RaxFQc7E4ACr4FVoHj2SMA6MGMqT8Ck9mDV5byGZtPPUw8f
                hex!["5620d851190bda61acb08f1a06defcdd5a3c7da3c33819643e7d6ee232ee78bf"].into(),
                b"ChainY".to_vec(),
            ),
            // 5FuwXy3d71LYWtgiECH2CCe6xqfzTQrjo8zv1T9EsxkBzVXx
            hex!["aa41c49785e1f4bc9079f3c2af7b9f43ff88545e9777b6bb291574982a5a9169"]
                .unchecked_into(),
            // 5Cq6tNkVFHZe1nrdtB58QXgAEwnX5q9a4QFzhPHs87noRBng
            hex!["21dc525f93a2afcb7abf0cb094c26ab807af5f89590269f0dd5fbaa2b91eb754"]
                .unchecked_into(),
            // 5DaTSiRZzAVJ4fqJc1eGA1yBz9TwFcWX4JwiJmqWgMSRBtaC
            hex!["42ed13bde38b21f479448b8ed9d155a9e7318acfafcb06f4e50d3098c1304c11"]
                .unchecked_into(),
            // 5H3DD5sSD2r6d79Kw3b78NGegEmqT1eVkE1e1waEvTmceHSv
            hex!["dc097bedcd2c06e87054f644a4cbe7f78470687a03fe019af6fffe775390d641"]
                .unchecked_into(),
        ),
    ];
    let constructor = move || {
        malan_genesis(
            wasm_binary,
            initial_authorities.clone(),
            genesis_assets(),
            btc_genesis_params(include_str!("res/btc_genesis_params_testnet.json")),
            crate::genesis::bitcoin::mainnet_trustees(),
        )
    };

    let bootnodes = Default::default();

    Ok(MalanChainSpec::from_genesis(
        "ChainX-Malan",
        "chainx-malan",
        ChainType::Live,
        constructor,
        bootnodes,
        Some(
            TelemetryEndpoints::new(vec![(CHAINX_TELEMETRY_URL.to_string(), 0)])
                .expect("ChainX telemetry url is valid; qed"),
        ),
        Some("pcx1"),
        None,
        Some(as_properties(NetworkType::Testnet)),
        Default::default(),
    ))
}

fn malan_session_keys(
    babe: BabeId,
    grandpa: GrandpaId,
    im_online: ImOnlineId,
    authority_discovery: AuthorityDiscoveryId,
) -> malan::SessionKeys {
    malan::SessionKeys {
        grandpa,
        babe,
        im_online,
        authority_discovery,
    }
}

fn malan_genesis(
    wasm_binary: &[u8],
    initial_authorities: Vec<AuthorityKeysTuple>,
    assets: Vec<AssetParams>,
    bitcoin: BtcGenesisParams,
    trustees: Vec<(Chain, TrusteeInfoConfig, Vec<BtcTrusteeParams>)>,
) -> malan::GenesisConfig {
    use malan_runtime::constants::time::DAYS;

    let (assets, assets_restrictions) = init_assets(assets);
    let tech_comm_members: Vec<AccountId> = vec![
        // 5QChfn7eDn96LDSy79WZHZYNWpjjNWuSUFxAwuZVmGmCpXfb
        hex!["2a077c909d0c5dcb3748cc11df2fb406ab8f35901b1a93010b78353e4a2bde0d"].into(),
        // 5GxS3YuwjhZZtmPmLEJuGPuz14gEJsunabqNLYTthXfThRwG
        hex!["d86477344ad5c27a45c4c178c7cca1b7b111380a4fbe7e23b3488a42ce56ca30"].into(),
        // 5DhacpyA2Ykpjx4AUJGbF7qa8tPqFELEVQYXQsxXQSauPb9r
        hex!["485bf22c979d4a61643f57a2006ff4fb7447a2a8ed905997c5f6b0230f39b860"].into(),
    ];

    let btc_genesis_trustees = trustees
        .iter()
        .find_map(|(chain, _, trustee_params)| {
            if *chain == Chain::Bitcoin {
                Some(
                    trustee_params
                        .iter()
                        .map(|i| (i.0).clone())
                        .collect::<Vec<_>>(),
                )
            } else {
                None
            }
        })
        .expect("bitcoin trustees generation can not fail; qed");

    malan::GenesisConfig {
        sudo: malan::SudoConfig {
            key: Some(
                hex!["b0ca18cce5c51f51655acf683453aa1ff319e3c3edd00b43b36a686a3ae34341"].into(),
            ),
        },
        system: malan::SystemConfig {
            code: wasm_binary.to_vec(),
        },
        babe: malan::BabeConfig {
            authorities: vec![],
            epoch_config: Some(malan::BABE_GENESIS_EPOCH_CONFIG),
        },
        grandpa: malan::GrandpaConfig {
            authorities: vec![],
        },
        council: malan::CouncilConfig::default(),
        technical_committee: Default::default(),
        technical_membership: malan::TechnicalMembershipConfig {
            members: tech_comm_members,
            phantom: Default::default(),
        },
        democracy: malan::DemocracyConfig::default(),
        treasury: Default::default(),
        elections: Default::default(),
        im_online: malan::ImOnlineConfig { keys: vec![] },
        authority_discovery: malan::AuthorityDiscoveryConfig { keys: vec![] },
        session: malan::SessionConfig {
            keys: initial_authorities
                .iter()
                .map(|x| {
                    (
                        (x.0).0.clone(),
                        (x.0).0.clone(),
                        malan::SessionKeys {
                            grandpa: x.2.clone(),
                            babe: x.1.clone(),
                            im_online: x.3.clone(),
                            authority_discovery: x.4.clone(),
                        },
                    )
                })
                .collect::<Vec<_>>(),
        },
        balances: Default::default(),
        indices: malan::IndicesConfig { indices: vec![] },
        x_system: malan::XSystemConfig {
            network_props: NetworkType::Testnet,
        },
        x_assets_registrar: malan::XAssetsRegistrarConfig { assets },
        x_assets: malan::XAssetsConfig {
            assets_restrictions,
            endowed: Default::default(),
        },
        x_gateway_common: malan::XGatewayCommonConfig { trustees },
        x_gateway_bitcoin: malan::XGatewayBitcoinConfig {
            genesis_trustees: btc_genesis_trustees,
            network_id: bitcoin.network,
            confirmation_number: bitcoin.confirmation_number,
            genesis_hash: bitcoin.hash(),
            genesis_info: (bitcoin.header(), bitcoin.height),
            params_info: BtcParams::new(
                // for signet and regtest
                545259519,            // max_bits
                2 * 60 * 60,          // block_max_future
                2 * 7 * 24 * 60 * 60, // target_timespan_seconds
                10 * 60,              // target_spacing_seconds
                4,                    // retargeting_factor
            ), // retargeting_factor
            btc_withdrawal_fee: 500000,
            max_withdrawal_count: 100,
            verifier: BtcTxVerifier::Recover,
        },
        x_staking: malan::XStakingConfig {
            validator_count: 40,
            sessions_per_era: 12,
            glob_dist_ratio: (12, 88), // (Treasury, X-type Asset and Staking) = (12, 88)
            mining_ratio: (10, 90),    // (Asset Mining, Staking) = (10, 90)
            minimum_penalty: 100 * DOLLARS,
            candidate_requirement: (100 * DOLLARS, 1_000 * DOLLARS), // Minimum value (self_bonded, total_bonded) to be a validator candidate
            minimum_validator_count: 2,
            ..Default::default()
        },
        x_mining_asset: malan::XMiningAssetConfig {
            claim_restrictions: vec![(X_BTC, (10, DAYS * 7))],
            mining_power_map: vec![(X_BTC, 400)],
        },
        x_spot: malan::XSpotConfig {
            trading_pairs: vec![(PCX, X_BTC, 9, 2, 100000, true)],
        },
        x_genesis_builder: malan::XGenesisBuilderConfig {
            params: crate::genesis::genesis_builder_params(),
            initial_authorities: initial_authorities
                .iter()
                .map(|i| (i.0).1.clone())
                .collect(),
        },
        ethereum_chain_id: malan::EthereumChainIdConfig { chain_id: 1502u64 },
        evm: Default::default(),
        ethereum: Default::default(),
        base_fee: malan::BaseFeeConfig::new(
            malan::DefaultBaseFeePerGas::get(),
            false,
            sp_runtime::Permill::from_parts(125_000),
        ),
        x_assets_bridge: malan::XAssetsBridgeConfig { admin_key: None },
        x_btc_ledger: Default::default(),
    }
}

fn build_dev_genesis(
    wasm_binary: &[u8],
    initial_authorities: Vec<AuthorityKeysTuple>,
    root_key: AccountId,
    assets: Vec<AssetParams>,
    endowed: BTreeMap<AssetId, Vec<(AccountId, Balance)>>,
    bitcoin: BtcGenesisParams,
    trustees: Vec<(Chain, TrusteeInfoConfig, Vec<BtcTrusteeParams>)>,
) -> dev::GenesisConfig {
    const ENDOWMENT: Balance = 10_000_000 * DOLLARS;
    const STASH: Balance = 100 * DOLLARS;
    let (assets, assets_restrictions) = init_assets(assets);

    let endowed_accounts = endowed
        .get(&PCX)
        .expect("PCX endowed; qed")
        .iter()
        .cloned()
        .map(|(k, _)| k)
        .collect::<Vec<_>>();

    let num_endowed_accounts = endowed_accounts.len();

    let mut total_endowed = Balance::default();
    let balances = endowed
        .get(&PCX)
        .expect("PCX endowed; qed")
        .iter()
        .cloned()
        .map(|(k, _)| {
            total_endowed += ENDOWMENT;
            (k, ENDOWMENT)
        })
        .collect::<Vec<_>>();

    // The value of STASH balance will be reserved per phragmen member.
    let phragmen_members = endowed_accounts
        .iter()
        .take((num_endowed_accounts + 1) / 2)
        .cloned()
        .map(|member| (member, STASH))
        .collect();

    let tech_comm_members = endowed_accounts
        .iter()
        .take((num_endowed_accounts + 1) / 2)
        .cloned()
        .collect::<Vec<_>>();

    // PCX only reserves the native asset id in assets module,
    // the actual native fund management is handled by pallet_balances.
    let mut assets_endowed = endowed;
    assets_endowed.remove(&PCX);

    let btc_genesis_trustees = trustees
        .iter()
        .find_map(|(chain, _, trustee_params)| {
            if *chain == Chain::Bitcoin {
                Some(
                    trustee_params
                        .iter()
                        .map(|i| (i.0).clone())
                        .collect::<Vec<_>>(),
                )
            } else {
                None
            }
        })
        .expect("bitcoin trustees generation can not fail; qed");
    dev::GenesisConfig {
        sudo: dev::SudoConfig {
            key: Some(root_key),
        },
        system: dev::SystemConfig {
            code: wasm_binary.to_vec(),
        },
        babe: dev::BabeConfig {
            authorities: vec![],
            epoch_config: Some(dev::BABE_GENESIS_EPOCH_CONFIG),
        },
        grandpa: dev::GrandpaConfig {
            authorities: vec![],
        },
        council: dev::CouncilConfig::default(),
        technical_committee: Default::default(),
        technical_membership: dev::TechnicalMembershipConfig {
            members: tech_comm_members,
            phantom: Default::default(),
        },
        democracy: dev::DemocracyConfig::default(),
        treasury: Default::default(),
        elections: dev::ElectionsConfig {
            members: phragmen_members,
        },
        im_online: dev::ImOnlineConfig { keys: vec![] },
        authority_discovery: dev::AuthorityDiscoveryConfig { keys: vec![] },
        session: dev::SessionConfig {
            keys: initial_authorities
                .iter()
                .map(|x| {
                    (
                        (x.0).0.clone(),
                        (x.0).0.clone(),
                        dev::SessionKeys {
                            grandpa: x.2.clone(),
                            babe: x.1.clone(),
                            im_online: x.3.clone(),
                            authority_discovery: x.4.clone(),
                        },
                    )
                })
                .collect::<Vec<_>>(),
        },
        balances: dev::BalancesConfig { balances },
        indices: dev::IndicesConfig { indices: vec![] },
        x_system: dev::XSystemConfig {
            network_props: NetworkType::Testnet,
        },
        x_assets_registrar: dev::XAssetsRegistrarConfig { assets },
        x_assets: dev::XAssetsConfig {
            assets_restrictions,
            endowed: assets_endowed,
        },
        x_gateway_common: dev::XGatewayCommonConfig { trustees },
        x_gateway_bitcoin: dev::XGatewayBitcoinConfig {
            genesis_trustees: btc_genesis_trustees,
            network_id: bitcoin.network,
            confirmation_number: bitcoin.confirmation_number,
            genesis_hash: bitcoin.hash(),
            genesis_info: (bitcoin.header(), bitcoin.height),
            params_info: BtcParams::new(
                // for signet and regtest
                545259519,            // max_bits
                2 * 60 * 60,          // block_max_future
                2 * 7 * 24 * 60 * 60, // target_timespan_seconds
                10 * 60,              // target_spacing_seconds
                4,                    // retargeting_factor
            ), // retargeting_factor
            btc_withdrawal_fee: 500000,
            max_withdrawal_count: 100,
            verifier: BtcTxVerifier::Recover,
        },
        x_staking: dev::XStakingConfig {
            validator_count: 40,
            sessions_per_era: 12,
            glob_dist_ratio: (12, 88), // (Treasury, X-type Asset and Staking) = (12, 88)
            mining_ratio: (10, 90),    // (Asset Mining, Staking) = (10, 90)
            minimum_penalty: 100 * DOLLARS,
            candidate_requirement: (100 * DOLLARS, 1_000 * DOLLARS), // Minimum value (self_bonded, total_bonded) to be a validator candidate
            ..Default::default()
        },
        x_mining_asset: dev::XMiningAssetConfig {
            claim_restrictions: vec![(X_BTC, (10, DAYS * 7))],
            mining_power_map: vec![(X_BTC, 400)],
        },
        x_spot: dev::XSpotConfig {
            trading_pairs: vec![(PCX, X_BTC, 9, 2, 100000, true)],
        },
        x_genesis_builder: dev::XGenesisBuilderConfig {
            params: crate::genesis::genesis_builder_params(),
            initial_authorities: initial_authorities
                .iter()
                .map(|i| (i.0).1.clone())
                .collect(),
        },
        ethereum_chain_id: dev::EthereumChainIdConfig { chain_id: 1503u64 },
        evm: Default::default(),
        ethereum: Default::default(),
        base_fee: dev::BaseFeeConfig::new(
            dev::DefaultBaseFeePerGas::get(),
            false,
            sp_runtime::Permill::from_parts(125_000),
        ),
        x_assets_bridge: dev::XAssetsBridgeConfig { admin_key: None },
        x_btc_ledger: Default::default(),
    }
}
