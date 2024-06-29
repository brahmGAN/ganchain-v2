// SPDX-License-Identifier: Apache-2.0
// This file is part of Frontier.
//
// Copyright (c) 2020-2022 Parity Technologies (UK) Ltd.
//
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

//! Consensus extension module tests for BABE consensus.

use super::*;
use evm::{ExitReason, ExitRevert, ExitSucceed};
use fp_ethereum::ValidatedTransaction;
use frame_support::{
	dispatch::{DispatchClass, GetDispatchInfo},
	weights::Weight,
};
use pallet_evm::AddressMapping;

fn legacy_erc20_creation_unsigned_transaction() -> LegacyUnsignedTransaction {
	LegacyUnsignedTransaction {
		nonce: U256::zero(),
		gas_price: U256::from(1),
		gas_limit: U256::from(0x100000),
		action: ethereum::TransactionAction::Create,
		value: U256::zero(),
		input: hex::decode(ERC20_CONTRACT_BYTECODE.trim_end()).unwrap(),
	}
}

fn legacy_erc20_creation_transaction(account: &AccountInfo) -> Transaction {
	legacy_erc20_creation_unsigned_transaction().sign(&account.private_key)
}

#[test]
fn transaction_should_increment_nonce() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	ext.execute_with(|| {
		let t = legacy_erc20_creation_transaction(alice);
		assert_ok!(Ethereum::execute(alice.address, &t, None,));
		assert_eq!(
			pallet_evm::Pallet::<Test>::account_basic(&alice.address)
				.0
				.nonce,
			U256::from(1)
		);
	});
}

#[test]
fn transaction_without_enough_gas_should_not_work() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	ext.execute_with(|| {
		let mut transaction = legacy_erc20_creation_transaction(alice);
		match &mut transaction {
			Transaction::Legacy(t) => t.gas_price = U256::from(11_000_000),
			_ => {}
		}

		let call = crate::Call::<Test>::transact { transaction };
		let source = call.check_self_contained().unwrap().unwrap();
		let extrinsic = CheckedExtrinsic::<u64, _, SignedExtra, _> {
			signed: fp_self_contained::CheckedSignature::SelfContained(source),
			function: RuntimeCall::Ethereum(call.clone()),
		};
		let dispatch_info = extrinsic.get_dispatch_info();

		assert_err!(
			call.validate_self_contained(&source, &dispatch_info, 0)
				.unwrap(),
			InvalidTransaction::Payment
		);
	});
}

#[test]
fn transaction_with_to_low_nonce_should_not_work() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	ext.execute_with(|| {
		// nonce is 0
		let mut transaction = legacy_erc20_creation_unsigned_transaction();
		transaction.nonce = U256::from(1);

		let signed = transaction.sign(&alice.private_key);
		let call = crate::Call::<Test>::transact {
			transaction: signed,
		};
		let source = call.check_self_contained().unwrap().unwrap();
		let extrinsic = CheckedExtrinsic::<u64, _, SignedExtra, H160> {
			signed: fp_self_contained::CheckedSignature::SelfContained(source),
			function: RuntimeCall::Ethereum(call.clone()),
		};
		let dispatch_info = extrinsic.get_dispatch_info();

		assert_eq!(
			call.validate_self_contained(&source, &dispatch_info, 0)
				.unwrap(),
			ValidTransactionBuilder::default()
				.and_provides((alice.address, U256::from(1)))
				.priority(0u64)
				.and_requires((alice.address, U256::from(0)))
				.build()
		);

		let t = legacy_erc20_creation_transaction(alice);

		// nonce is 1
		assert_ok!(Ethereum::execute(alice.address, &t, None,));

		transaction.nonce = U256::from(0);

		let signed2 = transaction.sign(&alice.private_key);
		let call2 = crate::Call::<Test>::transact {
			transaction: signed2,
		};
		let source2 = call2.check_self_contained().unwrap().unwrap();
		let extrinsic2 = CheckedExtrinsic::<u64, _, SignedExtra, _> {
			signed: fp_self_contained::CheckedSignature::SelfContained(source),
			function: RuntimeCall::Ethereum(call2.clone()),
		};

		assert_err!(
			call2
				.validate_self_contained(&source2, &extrinsic2.get_dispatch_info(), 0)
				.unwrap(),
			InvalidTransaction::Stale
		);
	});
}

#[test]
fn transaction_with_to_hight_nonce_should_fail_in_block() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	ext.execute_with(|| {
		let mut transaction = legacy_erc20_creation_unsigned_transaction();
		transaction.nonce = U256::one();

		let signed = transaction.sign(&alice.private_key);
		let call = crate::Call::<Test>::transact {
			transaction: signed,
		};
		let source = call.check_self_contained().unwrap().unwrap();
		let extrinsic = CheckedExtrinsic::<_, _, SignedExtra, _> {
			signed: fp_self_contained::CheckedSignature::SelfContained(source),
			function: RuntimeCall::Ethereum(call),
		};
		let dispatch_info = extrinsic.get_dispatch_info();
		assert_err!(
			extrinsic.apply::<Test>(&dispatch_info, 0),
			TransactionValidityError::Invalid(InvalidTransaction::Future)
		);
	});
}

#[test]
fn transaction_with_invalid_chain_id_should_fail_in_block() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	ext.execute_with(|| {
		let transaction =
			legacy_erc20_creation_unsigned_transaction().sign_with_chain_id(&alice.private_key, 1);

		let call = crate::Call::<Test>::transact { transaction };
		let source = call.check_self_contained().unwrap().unwrap();
		let extrinsic = CheckedExtrinsic::<_, _, SignedExtra, _> {
			signed: fp_self_contained::CheckedSignature::SelfContained(source),
			function: RuntimeCall::Ethereum(call),
		};
		let dispatch_info = extrinsic.get_dispatch_info();
		assert_err!(
			extrinsic.apply::<Test>(&dispatch_info, 0),
			TransactionValidityError::Invalid(InvalidTransaction::Custom(
				fp_ethereum::TransactionValidationError::InvalidChainId as u8,
			))
		);
	});
}

#[test]
fn contract_constructor_should_get_executed() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];
	let erc20_address = contract_address(alice.address, 0);
	let alice_storage_address = storage_address(alice.address, H256::zero());

	ext.execute_with(|| {
		let t = legacy_erc20_creation_transaction(alice);

		assert_ok!(Ethereum::execute(alice.address, &t, None,));
		assert_eq!(
			pallet_evm::AccountStorages::<Test>::get(erc20_address, alice_storage_address),
			H256::from_str("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff")
				.unwrap()
		)
	});
}

#[test]
fn source_should_be_derived_from_signature() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	let erc20_address = contract_address(alice.address, 0);
	let alice_storage_address = storage_address(alice.address, H256::zero());

	ext.execute_with(|| {
		Ethereum::transact(
			RawOrigin::EthereumTransaction(alice.address).into(),
			legacy_erc20_creation_transaction(alice),
		)
		.expect("Failed to execute transaction");

		// We verify the transaction happened with alice account.
		assert_eq!(
			pallet_evm::AccountStorages::<Test>::get(erc20_address, alice_storage_address),
			H256::from_str("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff")
				.unwrap()
		)
	});
}

#[test]
fn contract_should_be_created_at_given_address() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	let erc20_address = contract_address(alice.address, 0);

	ext.execute_with(|| {
		let t = legacy_erc20_creation_transaction(alice);
		assert_ok!(Ethereum::execute(alice.address, &t, None,));
		assert_ne!(
			pallet_evm::AccountCodes::<Test>::get(erc20_address).len(),
			0
		);
	});
}

#[test]
fn transaction_should_generate_correct_gas_used() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	let expected_gas = U256::from(894198);

	ext.execute_with(|| {
		let t = legacy_erc20_creation_transaction(alice);
		let (_, _, info) = Ethereum::execute(alice.address, &t, None).unwrap();

		match info {
			CallOrCreateInfo::Create(info) => {
				assert_eq!(info.used_gas.standard, expected_gas);
			}
			CallOrCreateInfo::Call(_) => panic!("expected create info"),
		}
	});
}

#[test]
fn call_should_handle_errors() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	ext.execute_with(|| {
		let t = LegacyUnsignedTransaction {
			nonce: U256::zero(),
			gas_price: U256::from(1),
			gas_limit: U256::from(0x100000),
			action: ethereum::TransactionAction::Create,
			value: U256::zero(),
			input: hex::decode(TEST_CONTRACT_CODE).unwrap(),
		}
		.sign(&alice.private_key);
		assert_ok!(Ethereum::execute(alice.address, &t, None,));

		let contract_address = hex::decode("32dcab0ef3fb2de2fce1d2e0799d36239671f04a").unwrap();
		let foo = hex::decode("c2985578").unwrap();
		let bar = hex::decode("febb0f7e").unwrap();

		let t2 = LegacyUnsignedTransaction {
			nonce: U256::from(1),
			gas_price: U256::from(1),
			gas_limit: U256::from(0x100000),
			action: TransactionAction::Call(H160::from_slice(&contract_address)),
			value: U256::zero(),
			input: foo,
		}
		.sign(&alice.private_key);

		// calling foo will succeed
		let (_, _, info) = Ethereum::execute(alice.address, &t2, None).unwrap();

		match info {
			CallOrCreateInfo::Call(info) => {
				assert_eq!(
					hex::encode(info.value),
					"0000000000000000000000000000000000000000000000000000000000000001"
				);
			}
			CallOrCreateInfo::Create(_) => panic!("expected call info"),
		}

		let t3 = LegacyUnsignedTransaction {
			nonce: U256::from(2),
			gas_price: U256::from(1),
			gas_limit: U256::from(0x100000),
			action: TransactionAction::Call(H160::from_slice(&contract_address)),
			value: U256::zero(),
			input: bar,
		}
		.sign(&alice.private_key);

		// calling should always succeed even if the inner EVM execution fails.
		Ethereum::execute(alice.address, &t3, None).ok().unwrap();
	});
}

#[test]
fn event_extra_data_should_be_handle_properly() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	ext.execute_with(|| {
		System::set_block_number(1);

		let t = LegacyUnsignedTransaction {
			nonce: U256::zero(),
			gas_price: U256::from(1),
			gas_limit: U256::from(0x100000),
			action: ethereum::TransactionAction::Create,
			value: U256::zero(),
			input: hex::decode(TEST_CONTRACT_CODE).unwrap(),
		}
		.sign(&alice.private_key);
		assert_ok!(Ethereum::execute(alice.address, &t, None,));

		let contract_address = hex::decode("32dcab0ef3fb2de2fce1d2e0799d36239671f04a").unwrap();
		let foo = hex::decode("c2985578").unwrap();
		let bar = hex::decode("febb0f7e").unwrap();

		let t2 = LegacyUnsignedTransaction {
			nonce: U256::from(1),
			gas_price: U256::from(1),
			gas_limit: U256::from(0x100000),
			action: TransactionAction::Call(H160::from_slice(&contract_address)),
			value: U256::zero(),
			input: foo,
		}
		.sign(&alice.private_key);

		// calling foo
		assert_ok!(Ethereum::apply_validated_transaction(alice.address, t2,));
		System::assert_last_event(RuntimeEvent::Ethereum(Event::Executed {
			from: alice.address,
			to: H160::from_slice(&contract_address),
			transaction_hash: H256::from_str(
				"0xc256587e4b2718d2798e9e895821a72e6aa751b8fcc03ce754246cc0d8a541c0",
			)
			.unwrap(),
			exit_reason: ExitReason::Succeed(ExitSucceed::Returned),
			extra_data: vec![],
		}));

		let t3 = LegacyUnsignedTransaction {
			nonce: U256::from(2),
			gas_price: U256::from(1),
			gas_limit: U256::from(0x100000),
			action: TransactionAction::Call(H160::from_slice(&contract_address)),
			value: U256::zero(),
			input: bar,
		}
		.sign(&alice.private_key);

		// calling bar revert
		assert_ok!(Ethereum::apply_validated_transaction(alice.address, t3,));
		System::assert_last_event(RuntimeEvent::Ethereum(Event::Executed {
			from: alice.address,
			to: H160::from_slice(&contract_address),
			transaction_hash: H256::from_str(
				"0x27a75747783eb8959f1fe7b23e8b1152a9ec945d9b90354582cb7c3ea1481287",
			)
			.unwrap(),
			exit_reason: ExitReason::Revert(ExitRevert::Reverted),
			extra_data: b"very_long_error_msg_that_we_ex".to_vec(),
		}));
	});
}

#[test]
fn self_contained_transaction_with_extra_gas_should_adjust_weight_with_post_dispatch() {
	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];
	let base_extrinsic_weight = frame_system::limits::BlockWeights::with_sensible_defaults(
		Weight::from_parts(2000000000000, u64::MAX),
		sp_runtime::Perbill::from_percent(75),
	)
	.per_class
	.get(DispatchClass::Normal)
	.base_extrinsic;

	ext.execute_with(|| {
		let mut transaction = legacy_erc20_creation_unsigned_transaction();
		transaction.gas_limit = 9_000_000.into();
		let signed = transaction.sign(&alice.private_key);
		let call = crate::Call::<Test>::transact {
			transaction: signed,
		};
		let source = call.check_self_contained().unwrap().unwrap();
		let extrinsic = CheckedExtrinsic::<_, _, frame_system::CheckWeight<Test>, _> {
			signed: fp_self_contained::CheckedSignature::SelfContained(source),
			function: RuntimeCall::Ethereum(call),
		};
		let dispatch_info = extrinsic.get_dispatch_info();
		let post_dispatch_weight = extrinsic
			.apply::<Test>(&dispatch_info, 0)
			.unwrap()
			.unwrap()
			.actual_weight
			.unwrap();

		let expected_weight = base_extrinsic_weight.saturating_add(post_dispatch_weight);
		let actual_weight =
			*frame_system::Pallet::<Test>::block_weight().get(DispatchClass::Normal);
		assert_eq!(
			expected_weight,
			actual_weight,
			"the block weight was unexpected, excess '{}'",
			actual_weight - expected_weight
		);
	});
}

#[test]
fn validated_transaction_apply_zero_gas_price_works() {
	let (pairs, mut ext) = new_test_ext_with_initial_balance(2, 1_000);
	let alice = &pairs[0];
	let bob = &pairs[1];
	let substrate_alice =
		<Test as pallet_evm::Config>::AddressMapping::into_account_id(alice.address);
	let substrate_bob = <Test as pallet_evm::Config>::AddressMapping::into_account_id(bob.address);

	ext.execute_with(|| {
		let transaction = LegacyUnsignedTransaction {
			nonce: U256::zero(),
			gas_price: U256::zero(),
			gas_limit: U256::from(21_000),
			action: ethereum::TransactionAction::Call(bob.address),
			value: U256::from(100),
			input: Default::default(),
		}
		.sign(&alice.private_key);

		assert_ok!(crate::ValidatedTransaction::<Test>::apply(
			alice.address,
			transaction
		));
		// Alice didn't pay fees, transfer 100 to Bob.
		assert_eq!(Balances::free_balance(&substrate_alice), 900);
		// Bob received 100 from Alice.
		assert_eq!(Balances::free_balance(&substrate_bob), 1_100);
	});
}

#[test]
fn proof_size_weight_limit_validation_works() {
	use pallet_evm::GasWeightMapping;

	let (pairs, mut ext) = new_test_ext(1);
	let alice = &pairs[0];

	ext.execute_with(|| {
		let mut tx = LegacyUnsignedTransaction {
			nonce: U256::from(2),
			gas_price: U256::from(1),
			gas_limit: U256::from(0x100000),
			action: ethereum::TransactionAction::Call(alice.address),
			value: U256::from(1),
			input: Vec::new(),
		};

		let gas_limit: u64 = 1_000_000;
		tx.gas_limit = U256::from(gas_limit);

		let weight_limit =
			<Test as pallet_evm::Config>::GasWeightMapping::gas_to_weight(gas_limit, true);

		// Gas limit cannot afford the extra byte and thus is expected to exhaust.
		tx.input = vec![0u8; (weight_limit.proof_size() + 1) as usize];
		let tx = tx.sign(&alice.private_key);

		// Execute
		assert!(
			Ethereum::transact(RawOrigin::EthereumTransaction(alice.address).into(), tx,).is_err()
		);
	});
}
