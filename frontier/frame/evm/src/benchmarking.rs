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

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::{benchmarks, impl_benchmark_test_suite};

type CurrencyOf<T> = <T as Config>::Currency;

benchmarks! {

	runner_execute {
	// This benchmark tests the relationship between gas and weight. It deploys a contract which
	// has an infinite loop in a public function. We then call this function with varying amounts
	// of gas, expecting it to OOG. The benchmarking framework measures the amount of time (aka
	// weight) it takes before OOGing and relates that to the amount of gas provided, leaving us
	// with an estimate for gas-to-weight mapping.

		let x in 1..10000000;

		use frame_benchmarking::vec;
		use rlp::RlpStream;
		use sp_core::{H160, U256};

		// contract bytecode below is for:
		//
		// pragma solidity >=0.8.0;
		//
		// contract InfiniteContractVar {
		//     uint public count;

		//     constructor() public {
		//         count = 0;
		//     }

		//     function infinite() public {
		//         while (true) {
		//             count=count+1;
		//         }
		//     }
		// }

		let contract_bytecode = hex::decode(concat!(
			"608060405234801561001057600080fd5b506000808190555061017c806100276000396000f3fe60",
			"8060405234801561001057600080fd5b50600436106100365760003560e01c806306661abd146100",
			"3b5780635bec9e6714610059575b600080fd5b610043610063565b604051610050919061009c565b",
			"60405180910390f35b610061610069565b005b60005481565b5b60011561008b5760016000546100",
			"8091906100b7565b60008190555061006a565b565b6100968161010d565b82525050565b60006020",
			"820190506100b1600083018461008d565b92915050565b60006100c28261010d565b91506100cd83",
			"61010d565b9250827fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
			"ff0382111561010257610101610117565b5b828201905092915050565b6000819050919050565b7f",
			"4e487b71000000000000000000000000000000000000000000000000000000006000526011600452",
			"60246000fdfea2646970667358221220bcab0385167dbed28dee34f1c5b30ecfcd67915495f0a32d",
			"2eeada8e094193a364736f6c63430008030033"))
			.expect("Bad hex string");

		let caller = "1000000000000000000000000000000000000001".parse::<H160>().unwrap();

		let mut nonce: u64 = 1;
		let nonce_as_u256: U256 = nonce.into();

		let value = U256::default();
		let gas_limit_create: u64 = 1000000;
		let is_transactional = true;
		let validate = true;
		let create_runner_results = T::Runner::create(
			caller,
			contract_bytecode,
			value,
			gas_limit_create,
			Some(U256::from(1_000_000_000)),
			Some(U256::from(1_000_000_000)),
			Some(nonce_as_u256),
			Vec::new(),
			is_transactional,
			validate,
			None,
			None,
			T::config(),
		);
		assert!(create_runner_results.is_ok(), "create() failed");

		// derive the resulting contract address from our create
		let mut rlp = RlpStream::new_list(2);
		rlp.append(&caller);
		rlp.append(&0u8);
		let contract_address = H160::from_slice(&sp_io::hashing::keccak_256(&rlp.out())[12..]);

		// derive encoded contract call -- in this case, just the function selector
		let mut encoded_call = vec![0u8; 4];
		encoded_call[0..4].copy_from_slice(&sp_io::hashing::keccak_256(b"infinite()")[0..4]);

		let gas_limit_call = gas_limit_create;

	}: {

		nonce += 1;
		let nonce_as_u256: U256 = nonce.into();

		let is_transactional = true;
		let validate = true;
		let call_runner_results = T::Runner::call(
			caller,
			contract_address,
			encoded_call,
			value,
			gas_limit_call,
			Some(U256::from(1_000_000_000)),
			Some(U256::from(1_000_000_000)),
			Some(nonce_as_u256),
			Vec::new(),
			is_transactional,
			validate,
			None,
			None,
			T::config(),
		);
		assert!(call_runner_results.is_ok(), "call() failed");
	}

	withdraw {
		let caller = frame_benchmarking::whitelisted_caller::<T::AccountId>();
		let from = H160::from_low_u64_le(0);
		let from_account_id = T::AddressMapping::into_account_id(from);
		CurrencyOf::<T>::make_free_balance_be(&from_account_id, 100_000u32.into());
	}: {
		// Withdraw should always fail with `EnsureAddressNever` WithdrawOrigin.
		let result = Pallet::<T>::withdraw(RawOrigin::Signed(caller.clone()).into(), from, 100_000u32.into());
		assert!(result.is_err());
		assert_eq!(result.unwrap_err(), sp_runtime::DispatchError::BadOrigin);
	}
}

impl_benchmark_test_suite!(Pallet, crate::tests::new_test_ext(), crate::mock::Test);
