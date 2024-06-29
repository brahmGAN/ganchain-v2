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

#![cfg(test)]

use super::*;
use crate::mock::*;

use fp_evm::Context;
use frame_support::{assert_err, assert_ok};
use scale_codec::Encode;
use sp_core::{H160, U256};
use sp_runtime::BuildStorage;

pub fn new_test_ext() -> sp_io::TestExternalities {
	frame_system::GenesisConfig::<Test>::default()
		.build_storage()
		.unwrap()
		.into()
}

#[test]
fn decode_limit_too_high() {
	new_test_ext().execute_with(|| {
		let mut nested_call =
			RuntimeCall::System(frame_system::Call::remark { remark: Vec::new() });

		// More than 8 depth
		for _ in 0..9 {
			nested_call = RuntimeCall::Utility(pallet_utility::Call::as_derivative {
				index: 0,
				call: Box::new(nested_call),
			});
		}

		let mut handle = MockHandle {
			input: nested_call.encode(),
			context: Context {
				address: H160::default(),
				caller: H160::default(),
				apparent_value: U256::default(),
			},
		};

		assert_eq!(
			Dispatch::<Test>::execute(&mut handle),
			Err(PrecompileFailure::Error {
				exit_status: ExitError::Other("decode failed".into())
			})
		);
	});
}

#[test]
fn decode_limit_ok() {
	new_test_ext().execute_with(|| {
		let mut nested_call =
			RuntimeCall::System(frame_system::Call::remark { remark: Vec::new() });

		for _ in 0..8 {
			nested_call = RuntimeCall::Utility(pallet_utility::Call::as_derivative {
				index: 0,
				call: Box::new(nested_call),
			});
		}

		let mut handle = MockHandle {
			input: nested_call.encode(),
			context: Context {
				address: H160::default(),
				caller: H160::default(),
				apparent_value: U256::default(),
			},
		};

		assert_ok!(Dispatch::<Test>::execute(&mut handle));
	});
}

#[test]
fn dispatch_validator_works_well() {
	new_test_ext().execute_with(|| {
		let call = RuntimeCall::System(frame_system::Call::remark { remark: Vec::new() });
		let mut handle = MockHandle {
			input: call.encode(),
			context: Context {
				address: H160::default(),
				caller: H160::default(),
				apparent_value: U256::default(),
			},
		};
		assert_ok!(Dispatch::<Test>::execute(&mut handle));

		pub struct MockValidator;
		impl DispatchValidateT<H160, RuntimeCall> for MockValidator {
			fn validate_before_dispatch(
				_origin: &H160,
				call: &RuntimeCall,
			) -> Option<PrecompileFailure> {
				match call {
					RuntimeCall::System(frame_system::Call::remark { remark: _ }) => {
						return Some(PrecompileFailure::Error {
							exit_status: ExitError::Other("This call is not allowed".into()),
						})
					}
					_ => None,
				}
			}
		}
		assert_err!(
			Dispatch::<Test, MockValidator>::execute(&mut handle),
			PrecompileFailure::Error {
				exit_status: ExitError::Other("This call is not allowed".into()),
			}
		);
	});
}
