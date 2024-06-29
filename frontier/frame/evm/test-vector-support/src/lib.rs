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

#![deny(unused_crate_dependencies)]

use std::fs;

use evm::{Context, ExitError, ExitReason, ExitSucceed, Transfer};
use fp_evm::{Precompile, PrecompileFailure, PrecompileHandle};
use sp_core::{H160, H256};

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct EthConsensusTest {
	input: String,
	expected: String,
	name: String,
	gas: Option<u64>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct EthConsensusFailureTest {
	input: String,
	expected_error: String,
	name: String,
}

pub struct MockHandle {
	pub input: Vec<u8>,
	pub gas_limit: Option<u64>,
	pub context: Context,
	pub is_static: bool,
	pub gas_used: u64,
}

impl MockHandle {
	pub fn new(input: Vec<u8>, gas_limit: Option<u64>, context: Context) -> Self {
		Self {
			input,
			gas_limit,
			context,
			is_static: false,
			gas_used: 0,
		}
	}
}

impl PrecompileHandle for MockHandle {
	/// Perform subcall in provided context.
	/// Precompile specifies in which context the subcall is executed.
	fn call(
		&mut self,
		_: H160,
		_: Option<Transfer>,
		_: Vec<u8>,
		_: Option<u64>,
		_: bool,
		_: &Context,
	) -> (ExitReason, Vec<u8>) {
		unimplemented!()
	}

	fn record_cost(&mut self, cost: u64) -> Result<(), ExitError> {
		self.gas_used += cost;
		Ok(())
	}

	fn record_external_cost(&mut self, _: Option<u64>, _: Option<u64>) -> Result<(), ExitError> {
		Ok(())
	}

	fn refund_external_cost(&mut self, _: Option<u64>, _: Option<u64>) {}

	fn log(&mut self, _: H160, _: Vec<H256>, _: Vec<u8>) -> Result<(), ExitError> {
		unimplemented!()
	}

	fn remaining_gas(&self) -> u64 {
		unimplemented!()
	}

	fn code_address(&self) -> H160 {
		unimplemented!()
	}

	fn input(&self) -> &[u8] {
		&self.input
	}

	fn context(&self) -> &Context {
		&self.context
	}

	fn is_static(&self) -> bool {
		self.is_static
	}

	fn gas_limit(&self) -> Option<u64> {
		self.gas_limit
	}
}

/// Tests a precompile against the ethereum consensus tests defined in the given file at filepath.
/// The file is expected to be in JSON format and contain an array of test vectors, where each
/// vector can be deserialized into an "EthConsensusTest".
pub fn test_precompile_test_vectors<P: Precompile>(filepath: &str) -> Result<(), String> {
	let data = fs::read_to_string(filepath).expect("Failed to read blake2F.json");

	let tests: Vec<EthConsensusTest> = serde_json::from_str(&data).expect("expected json array");

	for test in tests {
		let input: Vec<u8> = hex::decode(test.input).expect("Could not hex-decode test input data");

		let cost: u64 = 10000000;

		let context: Context = Context {
			address: Default::default(),
			caller: Default::default(),
			apparent_value: From::from(0),
		};

		let mut handle = MockHandle::new(input, Some(cost), context);

		match P::execute(&mut handle) {
			Ok(result) => {
				let as_hex: String = hex::encode(result.output);
				assert_eq!(
					result.exit_status,
					ExitSucceed::Returned,
					"test '{}' returned {:?} (expected 'Returned')",
					test.name,
					result.exit_status
				);
				assert_eq!(
					as_hex, test.expected,
					"test '{}' failed (different output)",
					test.name
				);
				if let Some(expected_gas) = test.gas {
					assert_eq!(
						handle.gas_used, expected_gas,
						"test '{}' failed (different gas cost)",
						test.name
					);
				}
			}
			Err(err) => {
				return Err(format!("Test '{}' returned error: {:?}", test.name, err));
			}
		}
	}

	Ok(())
}

pub fn test_precompile_failure_test_vectors<P: Precompile>(filepath: &str) -> Result<(), String> {
	let data = fs::read_to_string(filepath).expect("Failed to read json file");

	let tests: Vec<EthConsensusFailureTest> =
		serde_json::from_str(&data).expect("expected json array");

	for test in tests {
		let input: Vec<u8> = hex::decode(test.input).expect("Could not hex-decode test input data");

		let cost: u64 = 10000000;

		let context: Context = Context {
			address: Default::default(),
			caller: Default::default(),
			apparent_value: From::from(0),
		};

		let mut handle = MockHandle::new(input, Some(cost), context);

		match P::execute(&mut handle) {
			Ok(..) => {
				unreachable!("Test should be failed");
			}
			Err(err) => {
				let expected_err = PrecompileFailure::Error {
					exit_status: ExitError::Other(test.expected_error.into()),
				};
				assert_eq!(
					expected_err, err,
					"Test '{}' failed (different error)",
					test.name
				);
			}
		}
	}

	Ok(())
}
