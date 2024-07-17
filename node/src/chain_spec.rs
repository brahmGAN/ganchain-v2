// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Substrate chain configurations.
use std::{collections::BTreeMap, str::FromStr};
use grandpa_primitives::AuthorityId as GrandpaId;
use gpu_runtime::{
	wasm_binary_unwrap, BabeConfig, BalancesConfig, BaseFeeConfig, Block,
	EVMConfig, ImOnlineConfig, IndicesConfig, MaxNominations, SessionConfig, SessionKeys,
	StakerStatus, StakingConfig, SudoConfig, SystemConfig,NftmapConfig,
};
use gpu_runtime_constants::currency::*;

use pallet_im_online::sr25519::AuthorityId as ImOnlineId;
use sc_chain_spec::ChainSpecExtension;
use sc_service::ChainType;
use sc_telemetry::TelemetryEndpoints;
use serde::{Deserialize, Serialize};
use sp_authority_discovery::AuthorityId as AuthorityDiscoveryId;
use sp_consensus_babe::AuthorityId as BabeId;
use sp_core::{crypto::UncheckedInto, sr25519, Pair, Public, U256, H160};
use sp_runtime::{
	traits::{IdentifyAccount, Verify},
	Perbill,
};

pub use gpu_primitives::{AccountId, Balance, Signature};
pub use gpu_runtime::RuntimeGenesisConfig;

type AccountPublic = <Signature as Verify>::Signer;

const STAGING_TELEMETRY_URL: &str = "wss://telemetry.polkadot.io/submit";

/// Node `ChainSpec` extensions.
///
/// Additional parameters for some Substrate core modules,
/// customizable from the chain spec.
#[derive(Default, Clone, Serialize, Deserialize, ChainSpecExtension)]
#[serde(rename_all = "camelCase")]
pub struct Extensions {
	/// Block numbers with known hashes.
	pub fork_blocks: sc_client_api::ForkBlocks<Block>,
	/// Known bad block hashes.
	pub bad_blocks: sc_client_api::BadBlocks<Block>,
	/// The light sync state extension used by the sync-state rpc.
	pub light_sync_state: sc_sync_state_rpc::LightSyncStateExtension,
}

/// Specialized `ChainSpec`.
pub type ChainSpec = sc_service::GenericChainSpec<RuntimeGenesisConfig, Extensions>;

pub fn gpu_config() -> Result<ChainSpec, String> {
	ChainSpec::from_json_bytes(&include_bytes!("../res/gpu.json")[..])
}

pub fn gpu_testnet_config() -> Result<ChainSpec, String> {
	ChainSpec::from_json_bytes(&include_bytes!("../res/Raw.json")[..])
}

/// Returns the properties for the [`gpuChainSpec`].
pub fn gpu_chain_spec_properties() -> serde_json::map::Map<String, serde_json::Value> {
	serde_json::json!({
		"tokenDecimals": 18,
		"tokenSymbol": "GP"
	})
	.as_object()
	.expect("Map given; qed")
	.clone()
}

fn session_keys(
	grandpa: GrandpaId,
	babe: BabeId,
	im_online: ImOnlineId,
	authority_discovery: AuthorityDiscoveryId,
) -> SessionKeys {
	SessionKeys { grandpa, babe, im_online, authority_discovery }
}

fn staging_testnet_config_genesis() -> RuntimeGenesisConfig {
	#[rustfmt::skip]
	// stash, controller, session-key
	// generated with secret:
	// for i in 1 2 3 4 ; do for j in stash controller; do subkey inspect "$secret"/fir/$j/$i; done; done
	//
	// and
	//
	// for i in 1 2 3 4 ; do for j in session; do subkey --ed25519 inspect "$secret"//fir//$j//$i; done; done

	let initial_authorities: Vec<(
		AccountId,
		AccountId,
		GrandpaId,
		BabeId,
		ImOnlineId,
		AuthorityDiscoveryId,
	)> = vec![

		// (
        //     // 5Hh3AQJ7EkqmwPir16GFjtAN2G3YzHt4hi1AhMRx8swdYVLN
        //     array_bytes::hex_n_into_unchecked("f8e2fda3d85ecd9b1b71bb56e5d5d13ede1c2cd07e5d01d2a08c0f1a8932ac3b"),
        //     // 5GbvpBFcBoWs4XUVsp3gZoSz8iQYU4pAF2gGGercofSHtepH
        //     array_bytes::hex_n_into_unchecked("c8c1289e0097de3684e00d8d26ae3e3c86b9f44aed4659c4caa8013d7f53fb14"),
        //     // 5FGiAFbbVBfxthkCz7UiudEjsAmU8jTNvsTaH3BCRYHqr1Gg
        //     array_bytes::hex2array_unchecked("8ddcaa6f901574617a0c893c0b737f188dd35909cc9fd3908b4b7f9f1558e6c9").unchecked_into(),
        //     // 5CwAFSYCQH32EEqLdxBHc8QmN4CjYmTzh2KDcPM8sm45Bzpo
        //     array_bytes::hex2array_unchecked("267b1d9fe00f9b79c9c839bab998fd751201a07488e2b631b250bae4ee6cb830").unchecked_into(),
        //     // 5Gdb61uhNZxSFkC3QLCiiDa4R7gPCt6txPY4NVfa79b1JNK7
        //     array_bytes::hex2array_unchecked("ca053e250f0e112fa76e037c23aad132a29c97e23652a4d4a6bff99fee1e6146").unchecked_into(),
        //     // 5Ey4cohtD411B4MinT8WQC8fbZ3Wz7Jirmi1VamdsCvaxyra
        //     array_bytes::hex2array_unchecked("80671d9b306887f3759444c27e6f6eb0bb4fe6487f843b4ede57863889f2e85f").unchecked_into()
        // ),
        // (
        //     // 5Co5EVwQ2ReNt911h3ujDHg8cft618H7DPzScJ1HJdXfP47F
        //     array_bytes::hex_n_into_unchecked("2050442f6493b06d57b1381885e175b87385ae6a21869ace012a1c3dd46e504d"),
        //     // 5D7eGpFFP4PBjGqDnCkf6HU1p3RmsTo4826zvZG81WKaUpqN
        //     array_bytes::hex_n_into_unchecked("2e79e7eaf26c83754d7e4755edb976dc801a86782c29c0cafcda7f3b6cb51270"),
        //     // 5Evpd7a7TxYR1nymh5xE5ejosYC5v75SAbPHtmTNDaLFi1Ng
        //     array_bytes::hex2array_unchecked("7eb183b76e88e4c760bb7ed303786190c3f6d2d4b6f08d72161bdb14c1383434").unchecked_into(),
        //     // 5GbQKXxq6rWZtaB3KE4MLyfu3QNFDHcmvkv9zVhhZhtbw3gx
        //     array_bytes::hex2array_unchecked("c85a81da0e70342692464b6d471c5bf196c491431aba382ad8eca3a459ccb32a").unchecked_into(),
        //     // 5FA7wdbZuNJNWrqMKNTePFNt7zqVRaHDdNHjUW1dPoH8yeKd
        //     array_bytes::hex2array_unchecked("88d601b2d6e6d7688ee40c0bbf44d2ee1ff6eae8e1266382cf2dd750efd0854b").unchecked_into(),
        //     // 5ELGRpgBDvdwiTgmG6NQN5my1MUGXUpqYt5hgNdPWgHDtgNz
        //     array_bytes::hex2array_unchecked("6456cb380918514b5be57f728a0b9effb8f06ef08e9997d332524c2bc6436f0a").unchecked_into()
        // ),
		
		(
				// 5Hj6sEvDiBhnZRi3K77N1pSYziUHUDK9r3LCAQidMtGgj8xb
			array_bytes::hex_n_into_unchecked("fa75f36332c30d971a9059b66a915b97b23ad901db097b442a30c3dd51e34f1c"),
			// 5EFTZAbF9tm9NhKcH7hnojy1sCScVXEFQAUjGXYoyPmLYsUt
			array_bytes::hex_n_into_unchecked("60ac05b60a8341ac5077e211c50f79b17eaa5028a8338e5f9f46383202348b74"),
			// 5DEsR78GfPYhitMjvddvaf79WWNtAPgoarFm52bdVPh4WF6t
			array_bytes::hex2array_unchecked("33fcddeb1c43a96449ff8aa6c6db4e3c3574b05425097e61927ff6f21d363dac").unchecked_into(),
			// 5CMd64D815azwYY53Dh53U64MvYqyiXKoSYSkvxLNLngdC8v
			array_bytes::hex2array_unchecked("0ce71e0c2f241ae49d4c8b1bf3371f9b853a969ed26c12f6e7aaacdf22928d5e").unchecked_into(),
			// 5CMPddVQo7PtZYPLFnyfTYaYaDzVqySxSjJsPfbihxkGEJ4a
			array_bytes::hex2array_unchecked("0cb9d25b47aec99c843ff06b764c8544b5716cfb13aff68e481e65e2e35bd47f").unchecked_into(),
			// 5H6zBpq8ddzQ3axFDkjafgW8FpXqbdkuq65wRJgmGPU1avMH
			array_bytes::hex2array_unchecked("deeaa177230a283968152a3dfbd37b48b4d64de7d69f0dcc699fb9abf56ee029").unchecked_into(),
		),
		(
			// 5DFpeoo5dyfe7hi4rjCAFWptNkqe8bAV6oAepmoFJNxfdHrJ
			array_bytes::hex_n_into_unchecked("34b6ceff734d5fc80ca8f0595b95cb2bd9fe9f25639a78e0da815af73ad42f23"),
			// 5Cqp2KksHAeH6nDrDwrhfTdk41zQggkbNv7YyKyxzj7f9sCh
			array_bytes::hex_n_into_unchecked("2266cd2845235585c8991bbaf13fd5869a8bc181278f3b755571db566ad67f4d"),
			// 5FQoZGjyHURomcaTEDSbcjPmnHGzBDMmEKncXfmhmawxPfza
			array_bytes::hex2array_unchecked("9408cbeb2f251b9ccdf22852daccb4e2a25a609888dc272fb69b5ac2e57fbed2").unchecked_into(),
			// 5HNVjn4sQM3h9NGgBzaxTwHU3G9Ytc8BG74RfziPhBfcfLp2
			array_bytes::hex2array_unchecked("eabec9fb9f77577afa24c26f34b0985c5ee2dc517cdd4d63bb79b8961247a312").unchecked_into(),
			// 5EX82fhrPrWB6yC5YRW4KvK5VnL9M38ztdbQ9f4PWJS7vQnQ
			array_bytes::hex2array_unchecked("6c9e380f3133d08c33be12b02bc4c0db946cfe3c71f7d1245a772fd27350776b").unchecked_into(),
			// 5Eef1HTNeqEzw1BKQjNfhiyPJeWB6k9oAz58fkXKzu9dNgqR
			array_bytes::hex2array_unchecked("725d3639312f1f716f31dade26495e97c0449a943d292119cf696e1d863df05b").unchecked_into()
		),
		
		
	];

	// generated with secret: subkey inspect "$secret"/fir
	let root_key: AccountId = array_bytes::hex_n_into_unchecked(
		// 5F9TWFHkXUGskaakup7XCUpMSUNSr3e3nK7Z82Aa8NYipH1M
		// "88549d29c1da75923b6829d42aaeab762d608153adc9c6ef5febb925e7d50323",
		"0xd993d45491765a71d2212d4242948a4e4a3293514ae30e5d0f6bc686c7bec4dd"
	);

	let endowed_accounts: Vec<AccountId> = vec![root_key.clone()];

	let mut val_accounts = Vec::new(); 



	val_accounts.push((array_bytes::hex_n_into_unchecked("fa75f36332c30d971a9059b66a915b97b23ad901db097b442a30c3dd51e34f1c"),5u32));
	val_accounts.push((array_bytes::hex_n_into_unchecked("34b6ceff734d5fc80ca8f0595b95cb2bd9fe9f25639a78e0da815af73ad42f23"),101u32));





	testnet_genesis(initial_authorities, vec![], root_key, Some(endowed_accounts),val_accounts)
}

/// Staging testnet config.
pub fn staging_testnet_config() -> ChainSpec {
	let boot_nodes = vec![];
	ChainSpec::from_genesis(
		"Staging Testnet",
		"staging_testnet",
		ChainType::Live,
		staging_testnet_config_genesis,
		boot_nodes,
		Some(
			TelemetryEndpoints::new(vec![(STAGING_TELEMETRY_URL.to_string(), 0)])
				.expect("Staging telemetry url is valid; qed"),
		),
		None,
		None,
		Some(gpu_chain_spec_properties()),
		Default::default(),
	)
}

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

/// Helper function to generate stash, controller and session key from seed
pub fn authority_keys_from_seed(
	seed: &str,
) -> (AccountId, AccountId, GrandpaId, BabeId, ImOnlineId, AuthorityDiscoveryId) {
	(
		get_account_id_from_seed::<sr25519::Public>(&format!("{}//stash", seed)),
		get_account_id_from_seed::<sr25519::Public>(seed),
		get_from_seed::<GrandpaId>(seed),
		get_from_seed::<BabeId>(seed),
		get_from_seed::<ImOnlineId>(seed),
		get_from_seed::<AuthorityDiscoveryId>(seed),
	)
}

/// Helper function to create RuntimeGenesisConfig for testing
pub fn testnet_genesis(
	initial_authorities: Vec<(
		AccountId,
		AccountId,
		GrandpaId,
		BabeId,
		ImOnlineId,
		AuthorityDiscoveryId,
	)>,
	initial_nominators: Vec<AccountId>,
	root_key: AccountId,
	endowed_accounts: Option<Vec<AccountId>>,
	val_accounts :Vec<(AccountId,u32)>,
) -> RuntimeGenesisConfig {
	let mut endowed_accounts: Vec<AccountId> = endowed_accounts.unwrap_or_else(|| {
		vec![
			get_account_id_from_seed::<sr25519::Public>("Alice"),
			get_account_id_from_seed::<sr25519::Public>("Bob"),
			get_account_id_from_seed::<sr25519::Public>("Charlie"),
			get_account_id_from_seed::<sr25519::Public>("Dave"),
			get_account_id_from_seed::<sr25519::Public>("Eve"),
			get_account_id_from_seed::<sr25519::Public>("Ferdie"),
			get_account_id_from_seed::<sr25519::Public>("Alice//stash"),
			get_account_id_from_seed::<sr25519::Public>("Bob//stash"),
			get_account_id_from_seed::<sr25519::Public>("Charlie//stash"),
			get_account_id_from_seed::<sr25519::Public>("Dave//stash"),
			get_account_id_from_seed::<sr25519::Public>("Eve//stash"),
			get_account_id_from_seed::<sr25519::Public>("Ferdie//stash"),
		]
	});
	// endow all authorities and nominators.
	initial_authorities
		.iter()
		.map(|x| &x.0)
		.chain(initial_nominators.iter())
		.for_each(|x| {
			if !endowed_accounts.contains(x) {
				endowed_accounts.push(x.clone())
			}
		});

	// stakers: all validators and nominators.
	let mut rng = rand::thread_rng();
	let stakers = initial_authorities
		.iter()
		.map(|x| (x.0.clone(), x.0.clone(), STASH, StakerStatus::Validator))
		.chain(initial_nominators.iter().map(|x| {
			use rand::{seq::SliceRandom, Rng};
			let limit = (MaxNominations::get() as usize).min(initial_authorities.len());
			let count = rng.gen::<usize>() % limit;
			let nominations = initial_authorities
				.as_slice()
				.choose_multiple(&mut rng, count)
				.into_iter()
				.map(|choice| choice.0.clone())
				.collect::<Vec<_>>();
			(x.clone(), x.clone(), STASH, StakerStatus::Nominator(nominations))
		}))
		.collect::<Vec<_>>();

	const ENDOWMENT: Balance = 1_000 * DOLLARS;
	const STASH: Balance = ENDOWMENT / 10;

	// let revert_bytecode = vec![0x60, 0x00, 0x60, 0x00, 0xFD];

	RuntimeGenesisConfig {
		system: SystemConfig { code: wasm_binary_unwrap().to_vec(), ..Default::default() },
		balances: BalancesConfig {
			balances: endowed_accounts.iter().cloned().map(|x| (x, ENDOWMENT)).collect(),
		},
		indices: IndicesConfig { indices: vec![] },
		session: SessionConfig {
			keys: initial_authorities
				.iter()
				.map(|x| {
					(
						x.0.clone(),
						x.0.clone(),
						session_keys(x.2.clone(), x.3.clone(), x.4.clone(), x.5.clone()),
					)
				})
				.collect::<Vec<_>>(),
		},

		nftmap: NftmapConfig{
			nft_mappers : val_accounts,
		},
		staking: StakingConfig {
			validator_count: 7500 * initial_authorities.len() as u32,
			minimum_validator_count: initial_authorities.len() as u32,
			invulnerables: initial_authorities.iter().map(|x| x.0.clone()).collect(),
			slash_reward_fraction: Perbill::from_percent(10),
			stakers,
			..Default::default()
		},
		sudo: SudoConfig { key: Some(root_key) },
		babe: BabeConfig {
			epoch_config: Some(gpu_runtime::BABE_GENESIS_EPOCH_CONFIG),
			..Default::default()
		},
		im_online: ImOnlineConfig { keys: vec![] },
		authority_discovery: Default::default(),
		grandpa: Default::default(),
		treasury: Default::default(),
		vesting: Default::default(),
		assets: Default::default(),
		transaction_payment: Default::default(),
		evm: EVMConfig {
			// We need _some_ code inserted at the precompile address so that
			// the evm will actually call the address.
			
			accounts: {
				let mut map = BTreeMap::new();
				map.insert(
					// H160 address of Alice dev account
					// Derived from SS58 (42 prefix) address
					// SS58: 5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY
					// hex: 0xd43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d
					// Using the full hex key, truncating to the first 20 bytes (the first 40 hex chars)
					H160::from_str("d43593c715fdd31c61141abd04a99fd6822")
						.expect("internal H160 is valid; qed"),
					fp_evm::GenesisAccount {
						balance: U256::from_str("0xffffffffffffffffffffffffffffffff")
							.expect("internal U256 is valid; qed"),
						code: Default::default(),
						nonce: Default::default(),
						storage: Default::default(),
					},
				);
				map
			},
			..Default::default()
		},
		base_fee: BaseFeeConfig::new(
			sp_core::U256::from(1_000),
			sp_runtime::Permill::zero(),
		),
		ethereum: Default::default(),
	}
}

fn development_config_genesis() -> RuntimeGenesisConfig {

	let mut val_accounts = Vec::new();
	val_accounts.push((get_account_id_from_seed::<sr25519::Public>("Alice"),4u32));
	testnet_genesis(
		vec![authority_keys_from_seed("Alice")],
		vec![],
		get_account_id_from_seed::<sr25519::Public>("Alice"),
		None,
		val_accounts,
	)
}

/// Development config (single validator Alice)
pub fn development_config() -> ChainSpec {
	ChainSpec::from_genesis(
		"Development",
		"dev",
		ChainType::Development,
		development_config_genesis,
		vec![],
		None,
		None,
		None,
		Some(gpu_chain_spec_properties()),
		Default::default(),
	)
}

fn local_testnet_genesis() -> RuntimeGenesisConfig {
	testnet_genesis(
		vec![authority_keys_from_seed("Alice"), authority_keys_from_seed("Bob")],
		vec![],
		get_account_id_from_seed::<sr25519::Public>("Alice"),
		None,
		vec![],
	)
}

/// Local testnet config (multivalidator Alice + Bob)
pub fn local_testnet_config() -> ChainSpec {
	ChainSpec::from_genesis(
		"Local Testnet",
		"local_testnet",
		ChainType::Local,
		local_testnet_genesis,
		vec![],
		None,
		None,
		None,
		Some(gpu_chain_spec_properties()),
		Default::default(),
	)
}