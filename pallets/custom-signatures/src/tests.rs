// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate as custom_signatures;
use assert_matches::assert_matches;
use custom_signatures::*;
use frame_support::{
    traits::{ConstU32, Contains},
    {assert_err, assert_ok, parameter_types},
};
use hex_literal::hex;
use parity_scale_codec::Encode;
use sp_core::{ecdsa, Pair};
use sp_io::{hashing::keccak_256, TestExternalities};
use sp_keyring::AccountKeyring as Keyring;
use sp_runtime::{
    testing::{H256, Header},
    traits::{BlakeTwo256, IdentifyAccount, IdentityLookup, Verify},
    transaction_validity::TransactionPriority,
    MultiSignature, MultiSigner,
};

pub const ECDSA_SEED: [u8; 32] =
    hex_literal::hex!["7e9c7ad85df5cdc88659f53e06fb2eb9bab3ebc59083a3190eaf2c730332529c"];

type Balance = u128;
type BlockNumber = u64;
type Signature = MultiSignature;
type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;
type Block = frame_system::mocking::MockBlock<Runtime>;
type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Runtime>;

frame_support::construct_runtime! {
	pub enum Runtime
	{
        Balances: pallet_balances,
        System: frame_system,
        CustomSignatures: custom_signatures,
    }
}

parameter_types! {
    pub const BlockHashCount: u64 = 250;
}

pub struct NoRemarkFilter;
impl Contains<RuntimeCall> for NoRemarkFilter {
    fn contains(call: &RuntimeCall) -> bool {
        match call {
            RuntimeCall::System(method) => match method {
                frame_system::Call::remark { .. } => false,
                _ => true,
            },
            _ => true,
        }
    }
}

impl frame_system::Config for Runtime {
    type RuntimeOrigin = RuntimeOrigin;
    type BaseCallFilter = NoRemarkFilter;
    type Nonce = u32;
    type Block = BlockNumber;
    type RuntimeCall = RuntimeCall;
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type AccountId = AccountId;
    type Lookup = IdentityLookup<Self::AccountId>;
    type RuntimeEvent = RuntimeEvent;
    type BlockHashCount = BlockHashCount;
    type Version = ();
    type PalletInfo = PalletInfo;
    type AccountData = pallet_balances::AccountData<Balance>;
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type DbWeight = ();
    type SystemWeightInfo = ();
    type BlockWeights = ();
    type BlockLength = ();
    type SS58Prefix = ();
    type OnSetCode = ();
    type MaxConsumers = frame_support::traits::ConstU32<16>;
}

parameter_types! {
    pub const ExistentialDeposit: Balance = 1;
}

impl pallet_balances::Config for Runtime {
    type Balance = Balance;
    type RuntimeEvent = RuntimeEvent;
    type DustRemoval = ();
    type ExistentialDeposit = ExistentialDeposit;
    type AccountStore = frame_system::Pallet<Runtime>;
    type WeightInfo = ();
    type MaxLocks = ();
    type MaxReserves = ();
    type ReserveIdentifier = ();
    type FreezeIdentifier = ();
    type MaxHolds = ConstU32<0>;
    type MaxFreezes = ConstU32<0>;
}

const MAGIC_NUMBER: u16 = 0xff50;
parameter_types! {
    pub const Priority: TransactionPriority = TransactionPriority::MAX;
    pub const CallFee: Balance = 42;
    pub const CallMagicNumber: u16 = MAGIC_NUMBER;
}

impl Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type Signature = ethereum::EthereumSignature;
    type Signer = <Signature as Verify>::Signer;
    type CallMagicNumber = CallMagicNumber;
    type Currency = Balances;
    type CallFee = CallFee;
    type OnChargeTransaction = ();
    type UnsignedPriority = Priority;
}

fn new_test_ext() -> TestExternalities {
    let mut storage = frame_system::GenesisConfig::default()
        .build_storage::<Runtime>()
        .unwrap();

    let pair = ecdsa::Pair::from_seed(&ECDSA_SEED);
    let account = MultiSigner::from(pair.public()).into_account();
    let _ = pallet_balances::GenesisConfig::<Runtime> {
        balances: vec![(account, 1_000_000_000)],
    }
    .assimilate_storage(&mut storage);

    let mut ext = TestExternalities::from(storage);
    ext.execute_with(|| System::set_block_number(1));
    ext
}

/// Simple `eth_sign` implementation, should be equal to exported by RPC
fn eth_sign(seed: &[u8; 32], data: &[u8]) -> Vec<u8> {
    let call_msg = ethereum::signable_message(data);
    let ecdsa_msg = libsecp256k1::Message::parse(&keccak_256(&call_msg));
    let secret = libsecp256k1::SecretKey::parse(&seed).expect("valid seed");
    let (signature, recovery_id) = libsecp256k1::sign(&ecdsa_msg, &secret);
    let mut out = Vec::new();
    out.extend_from_slice(&signature.serialize()[..]);
    // Fix recovery ID: Ethereum uses 27/28 notation
    out.push(recovery_id.serialize() + 27);
    out
}

#[test]
fn eth_sign_works() {
    let seed = hex!["ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"];
    let text = b"Hello Astar";
    let signature = hex!["0cc6d5de6db06727fe43a260e7c9a417be3daab9b0e4e65e276f543e5c2f3de67e9e26d903d5301181e13033f61692db2dca67c1f8992b62476eaf8cb3a597101c"];
    assert_eq!(eth_sign(&seed, &text[..]), signature);
}

#[test]
fn invalid_signature() {
    let bob: <Runtime as frame_system::Config>::AccountId = Keyring::Bob.into();
    let alice: <Runtime as frame_system::Config>::AccountId = Keyring::Alice.into();
    let call = pallet_balances::Call::<Runtime>::transfer {
        dest: alice.clone(),
        value: 1_000,
    }
    .into();
    let signature = Vec::from(&hex!["dd0992d40e5cdf99db76bed162808508ac65acd7ae2fdc8573594f03ed9c939773e813181788fc02c3c68f3fdc592759b35f6354484343e18cb5317d34dab6c61b"][..]);
    new_test_ext().execute_with(|| {
        assert_err!(
            CustomSignatures::call(RuntimeOrigin::none(), Box::new(call), bob, signature, 0),
            Error::<Runtime>::InvalidSignature,
        );
    });
}

#[test]
fn balance_transfer() {
    new_test_ext().execute_with(|| {
        let pair = ecdsa::Pair::from_seed(&ECDSA_SEED);
        let account = MultiSigner::from(pair.public()).into_account();

        let alice: <Runtime as frame_system::Config>::AccountId = Keyring::Alice.into();
        assert_eq!(System::account(alice.clone()).data.free, 0);

        let call: RuntimeCall = pallet_balances::Call::<Runtime>::transfer {
            dest: alice.clone(),
            value: 1_000,
        }
        .into();
        let payload = (MAGIC_NUMBER, 0u32, call.clone());
        let signature = eth_sign(&ECDSA_SEED, payload.encode().as_ref()).into();

        assert_eq!(System::account(account.clone()).nonce, 0);
        assert_ok!(CustomSignatures::call(
            RuntimeOrigin::none(),
            Box::new(call.clone()),
            account.clone(),
            signature,
            0,
        ));
        assert_eq!(System::account(alice.clone()).data.free, 1_000);
        assert_eq!(System::account(account.clone()).nonce, 1);
        assert_eq!(System::account(account.clone()).data.free, 999_998_958);
        assert_matches!(
            System::events()
                .last()
                .expect("events expected")
                .event
                .clone(),
            RuntimeEvent::CustomSignatures(Event::Executed(used_account, Ok(..),))
            if used_account == account
        );

        let signature = eth_sign(&ECDSA_SEED, payload.encode().as_ref()).into();
        assert_err!(
            CustomSignatures::call(
                RuntimeOrigin::none(),
                Box::new(call.clone()),
                account.clone(),
                signature,
                0,
            ),
            Error::<Runtime>::BadNonce,
        );

        let payload = (MAGIC_NUMBER, 1u32, call.clone());
        let signature = eth_sign(&ECDSA_SEED, payload.encode().as_ref()).into();
        assert_eq!(System::account(account.clone()).nonce, 1);
        assert_ok!(CustomSignatures::call(
            RuntimeOrigin::none(),
            Box::new(call.clone()),
            account.clone(),
            signature,
            1,
        ));
        assert_eq!(System::account(alice).data.free, 2_000);
        assert_eq!(System::account(account.clone()).nonce, 2);
        assert_eq!(System::account(account.clone()).data.free, 999_997_916);
    })
}

#[test]
fn call_fixtures() {
    use sp_core::crypto::Ss58Codec;

    let seed = hex!["ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"];
    let pair = ecdsa::Pair::from_seed(&seed);
    assert_eq!(
        MultiSigner::from(pair.public())
            .into_account()
            .to_ss58check(),
        "5EGynCAEvv8NLeHx8vDMvb8hTcEcMYUMWCDQEEncNEfNWB2W",
    );

    let dest =
        AccountId::from_ss58check("5GVwcV6EzxxYbXBm7H6dtxc9TCgL4oepMXtgqWYEc3VXJoaf").unwrap();
    let call: RuntimeCall =
        pallet_balances::Call::<Runtime>::transfer_allow_death { dest, value: 1000 }.into();
    assert_eq!(
        call.encode(),
        hex!["0000c4305fb88b6ccb43d6552dc11d18e7b0ee3185247adcc6e885eb284adf6c563da10f"],
    );

    let payload = (MAGIC_NUMBER, 0u32, call.clone());
    assert_eq!(
        payload.encode(),
        hex![
            "50ff000000000000c4305fb88b6ccb43d6552dc11d18e7b0ee3185247adcc6e885eb284adf6c563da10f"
        ],
    );

    let signature = hex!["6ecb474240df46ee5cde8f51cf5ccf4c75d15ac3c1772aea6c8189604263c98b16350883438c4eaa447ebcb6889d516f70351fd704bb3521072cd2fccc7c99dc1c"];
    assert_eq!(eth_sign(&seed, payload.encode().as_ref()), signature)
}

#[test]
fn not_allowed_call_filtered() {
    new_test_ext().execute_with(|| {
        let pair = ecdsa::Pair::from_seed(&ECDSA_SEED);
        let account = MultiSigner::from(pair.public()).into_account();

        let alice: <Runtime as frame_system::Config>::AccountId = Keyring::Alice.into();
        assert_eq!(System::account(alice.clone()).data.free, 0);

        let call: RuntimeCall = frame_system::Call::<Runtime>::remark {
            remark: Vec::<_>::new(),
        }
        .into();
        // sanity check, call should be filtered out
        assert!(!<Runtime as frame_system::Config>::BaseCallFilter::contains(&call));

        let payload = (MAGIC_NUMBER, 0u32, call.clone());
        let signature = eth_sign(&ECDSA_SEED, payload.encode().as_ref()).into();

        assert_eq!(System::account(account.clone()).nonce, 0);
        assert_ok!(CustomSignatures::call(
            RuntimeOrigin::none(),
            Box::new(call.clone()),
            account.clone(),
            signature,
            0,
        ));
        assert_eq!(System::account(account.clone()).nonce, 1);

        assert_matches!(
            System::events()
                .last()
                .expect("events expected")
                .event
                .clone(),
            RuntimeEvent::CustomSignatures(Event::Executed(used_account, Err(..),))
            if used_account == account
        );
    })
}
