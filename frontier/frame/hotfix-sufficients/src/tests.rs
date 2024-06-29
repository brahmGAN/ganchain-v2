// SPDX-License-Identifier: Apache-2.0
// This file is part of Frontier.
//
// Copyright (c) 2021-2022 Parity Technologies (UK) Ltd.
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

use sp_core::H160;

use super::*;
use crate::{
	mock::{new_test_ext, RuntimeOrigin, Test},
	pallet::Pallet,
};

#[test]
fn test_hotfix_inc_account_sufficients_returns_error_if_max_addresses_exceeded() {
	new_test_ext().execute_with(|| {
		let max_address_count = 1000;
		let addresses = (0..max_address_count + 1 as u64)
			.map(H160::from_low_u64_le)
			.collect::<Vec<H160>>();

		let result = <Pallet<Test>>::hotfix_inc_account_sufficients(
			RuntimeOrigin::signed(H160::default()),
			addresses,
		);

		assert!(result.is_err(), "expected error");
	});
}

#[test]
fn test_hotfix_inc_account_sufficients_requires_signed_origin() {
	new_test_ext().execute_with(|| {
		let addr = "1230000000000000000000000000000000000001"
			.parse::<H160>()
			.unwrap();
		let unsigned_origin = RuntimeOrigin::root();
		let result = <Pallet<Test>>::hotfix_inc_account_sufficients(unsigned_origin, vec![addr]);

		assert!(result.is_err(), "expected error");
	});
}

#[test]
fn test_hotfix_inc_account_sufficients_increments_if_nonce_nonzero() {
	new_test_ext().execute_with(|| {
		let addr_1 = "1230000000000000000000000000000000000001"
			.parse::<H160>()
			.unwrap();
		let addr_2 = "1234000000000000000000000000000000000001"
			.parse::<H160>()
			.unwrap();
		let substrate_addr_1: <Test as frame_system::Config>::AccountId =
			<Test as Config>::AddressMapping::into_account_id(addr_1);
		let substrate_addr_2: <Test as frame_system::Config>::AccountId =
			<Test as Config>::AddressMapping::into_account_id(addr_2);

		frame_system::Pallet::<Test>::inc_account_nonce(&substrate_addr_1);

		let account_1 = frame_system::Account::<Test>::get(substrate_addr_1);
		let account_2 = frame_system::Account::<Test>::get(substrate_addr_2);
		assert_eq!(account_1.nonce, 1);
		assert_eq!(account_1.sufficients, 0);
		assert_eq!(account_2.nonce, 0);
		assert_eq!(account_2.sufficients, 0);

		<Pallet<Test>>::hotfix_inc_account_sufficients(
			RuntimeOrigin::signed(H160::default()),
			vec![addr_1, addr_2],
		)
		.unwrap();

		let account_1 = frame_system::Account::<Test>::get(substrate_addr_1);
		let account_2 = frame_system::Account::<Test>::get(substrate_addr_2);
		assert_eq!(account_1.nonce, 1);
		assert_eq!(account_1.sufficients, 1);
		assert_eq!(account_2.nonce, 0);
		assert_eq!(account_2.sufficients, 0);
	});
}

#[test]
fn test_hotfix_inc_account_sufficients_increments_with_saturation_if_nonce_nonzero() {
	new_test_ext().execute_with(|| {
		let addr = "1230000000000000000000000000000000000001"
			.parse::<H160>()
			.unwrap();
		let substrate_addr: <Test as frame_system::Config>::AccountId =
			<Test as Config>::AddressMapping::into_account_id(addr);

		frame_system::Account::<Test>::mutate(substrate_addr, |x| {
			x.nonce = 1;
			x.sufficients = u32::MAX;
		});

		let account = frame_system::Account::<Test>::get(substrate_addr);

		assert_eq!(account.sufficients, u32::MAX);
		assert_eq!(account.nonce, 1);

		<Pallet<Test>>::hotfix_inc_account_sufficients(
			RuntimeOrigin::signed(H160::default()),
			vec![addr],
		)
		.unwrap();

		let account = frame_system::Account::<Test>::get(substrate_addr);
		assert_eq!(account.sufficients, u32::MAX);
		assert_eq!(account.nonce, 1);
	});
}

#[test]
fn test_hotfix_inc_account_sufficients_does_not_increment_if_both_nonce_and_refs_nonzero() {
	new_test_ext().execute_with(|| {
		let addr = "1230000000000000000000000000000000000001"
			.parse::<H160>()
			.unwrap();
		let substrate_addr: <Test as frame_system::Config>::AccountId =
			<Test as Config>::AddressMapping::into_account_id(addr);

		frame_system::Account::<Test>::mutate(substrate_addr, |x| {
			x.nonce = 1;
			x.consumers = 1;
		});

		let account = frame_system::Account::<Test>::get(substrate_addr);

		assert_eq!(account.sufficients, 0);
		assert_eq!(account.nonce, 1);
		assert_eq!(account.consumers, 1);

		<Pallet<Test>>::hotfix_inc_account_sufficients(
			RuntimeOrigin::signed(H160::default()),
			vec![addr],
		)
		.unwrap();

		let account = frame_system::Account::<Test>::get(substrate_addr);
		assert_eq!(account.sufficients, 0);
		assert_eq!(account.nonce, 1);
		assert_eq!(account.consumers, 1);
	});
}
