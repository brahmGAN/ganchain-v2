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

#![cfg_attr(not(feature = "std"), no_std)]
#![deny(unused_crate_dependencies)]

extern crate alloc;

use alloc::vec::Vec;

use fp_evm::{ExitSucceed, LinearCostPrecompile, PrecompileFailure};

pub struct Sha3FIPS256;

impl LinearCostPrecompile for Sha3FIPS256 {
	const BASE: u64 = 60;
	const WORD: u64 = 12;

	fn execute(input: &[u8], _: u64) -> Result<(ExitSucceed, Vec<u8>), PrecompileFailure> {
		use tiny_keccak::Hasher;
		let mut output = [0; 32];
		let mut sha3 = tiny_keccak::Sha3::v256();
		sha3.update(input);
		sha3.finalize(&mut output);
		Ok((ExitSucceed::Returned, output.to_vec()))
	}
}

pub struct Sha3FIPS512;

impl LinearCostPrecompile for Sha3FIPS512 {
	const BASE: u64 = 60;
	const WORD: u64 = 12;

	fn execute(input: &[u8], _: u64) -> Result<(ExitSucceed, Vec<u8>), PrecompileFailure> {
		use tiny_keccak::Hasher;
		let mut output = [0; 64];
		let mut sha3 = tiny_keccak::Sha3::v512();
		sha3.update(input);
		sha3.finalize(&mut output);
		Ok((ExitSucceed::Returned, output.to_vec()))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_empty_input() -> Result<(), PrecompileFailure> {
		let input: [u8; 0] = [];
		let expected = b"\
			\xa7\xff\xc6\xf8\xbf\x1e\xd7\x66\x51\xc1\x47\x56\xa0\x61\xd6\x62\
			\xf5\x80\xff\x4d\xe4\x3b\x49\xfa\x82\xd8\x0a\x4b\x80\xf8\x43\x4a\
		";

		let cost: u64 = 1;

		match Sha3FIPS256::execute(&input, cost) {
			Ok((_, out)) => {
				assert_eq!(out, expected);
				Ok(())
			}
			Err(e) => {
				panic!("Test not expected to fail: {:?}", e);
			}
		}
	}

	#[test]
	fn hello_sha3_256() -> Result<(), PrecompileFailure> {
		let input = b"hello";
		let expected = b"\
			\x33\x38\xbe\x69\x4f\x50\xc5\xf3\x38\x81\x49\x86\xcd\xf0\x68\x64\
			\x53\xa8\x88\xb8\x4f\x42\x4d\x79\x2a\xf4\xb9\x20\x23\x98\xf3\x92\
		";

		let cost: u64 = 1;

		match Sha3FIPS256::execute(input, cost) {
			Ok((_, out)) => {
				assert_eq!(out, expected);
				Ok(())
			}
			Err(e) => {
				panic!("Test not expected to fail: {:?}", e);
			}
		}
	}

	#[test]
	fn long_string_sha3_256() -> Result<(), PrecompileFailure> {
		let input = b"Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.";
		let expected = b"\
			\xbd\xe3\xf2\x69\x17\x5e\x1d\xcd\xa1\x38\x48\x27\x8a\xa6\x04\x6b\
			\xd6\x43\xce\xa8\x5b\x84\xc8\xb8\xbb\x80\x95\x2e\x70\xb6\xea\xe0\
		";

		let cost: u64 = 1;

		match Sha3FIPS256::execute(input, cost) {
			Ok((_, out)) => {
				assert_eq!(out, expected);
				Ok(())
			}
			Err(e) => {
				panic!("Test not expected to fail: {:?}", e);
			}
		}
	}

	#[test]
	fn long_string_sha3_512() -> Result<(), PrecompileFailure> {
		let input = b"Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.";
		let expected = b"\
			\xf3\x2a\x94\x23\x55\x13\x51\xdf\x0a\x07\xc0\xb8\xc2\x0e\xb9\x72\
			\x36\x7c\x39\x8d\x61\x06\x60\x38\xe1\x69\x86\x44\x8e\xbf\xbc\x3d\
			\x15\xed\xe0\xed\x36\x93\xe3\x90\x5e\x9a\x8c\x60\x1d\x9d\x00\x2a\
			\x06\x85\x3b\x97\x97\xef\x9a\xb1\x0c\xbd\xe1\x00\x9c\x7d\x0f\x09\
		";

		let cost: u64 = 1;

		match Sha3FIPS512::execute(input, cost) {
			Ok((_, out)) => {
				assert_eq!(out, expected);
				Ok(())
			}
			Err(e) => {
				panic!("Test not expected to fail: {:?}", e);
			}
		}
	}
}
