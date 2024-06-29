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

use frame_benchmarking::{benchmarks, impl_benchmark_test_suite};

use super::*;

benchmarks! {
	hotfix_inc_account_sufficients {
		// This benchmark tests the resource utilization by hotfixing N number of accounts
		// by incrementing their `sufficients` if `nonce` is > 0.

		let n in 0 .. 1000;

		use frame_benchmarking::{whitelisted_caller};
		use sp_core::H160;
		use frame_system::RawOrigin;

		// The caller account is whitelisted for DB reads/write by the benchmarking macro.
		let caller: T::AccountId = whitelisted_caller();
		let addresses = (0..n as u64)
							.map(H160::from_low_u64_le)
							.collect::<Vec<H160>>();
		let accounts = addresses
			.iter()
			.cloned()
			.map(|addr| {
				let account_id = T::AddressMapping::into_account_id(addr);
				frame_system::Pallet::<T>::inc_account_nonce(&account_id);
				assert_eq!(frame_system::Pallet::<T>::sufficients(&account_id), 0);

				account_id
			})
			.collect::<Vec<_>>();

	}: _(RawOrigin::Signed(caller), addresses)
	verify {
		accounts
			.iter()
			.for_each(|id| {
				assert_eq!(frame_system::Pallet::<T>::sufficients(id), 1);
			});
	}
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
