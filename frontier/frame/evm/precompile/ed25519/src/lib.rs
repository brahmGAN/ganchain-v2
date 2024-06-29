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
use core::convert::TryFrom;
use ed25519_dalek::{PublicKey, Signature, Verifier};
use fp_evm::{ExitError, ExitSucceed, LinearCostPrecompile, PrecompileFailure};

pub struct Ed25519Verify;

impl LinearCostPrecompile for Ed25519Verify {
	const BASE: u64 = 15;
	const WORD: u64 = 3;

	fn execute(input: &[u8], _: u64) -> Result<(ExitSucceed, Vec<u8>), PrecompileFailure> {
		if input.len() < 128 {
			return Err(PrecompileFailure::Error {
				exit_status: ExitError::Other("input must contain 128 bytes".into()),
			});
		};

		let mut i = [0u8; 128];
		i[..128].copy_from_slice(&input[..128]);

		let mut buf = [0u8; 4];

		let msg = &i[0..32];
		let pk = PublicKey::from_bytes(&i[32..64]).map_err(|_| PrecompileFailure::Error {
			exit_status: ExitError::Other("Public key recover failed".into()),
		})?;
		let sig = Signature::try_from(&i[64..128]).map_err(|_| PrecompileFailure::Error {
			exit_status: ExitError::Other("Signature recover failed".into()),
		})?;

		// https://docs.rs/rust-crypto/0.2.36/crypto/ed25519/fn.verify.html
		if pk.verify(msg, &sig).is_ok() {
			buf[3] = 0u8;
		} else {
			buf[3] = 1u8;
		};

		Ok((ExitSucceed::Returned, buf.to_vec()))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use ed25519_dalek::{Keypair, SecretKey, Signer};

	#[test]
	fn test_empty_input() -> Result<(), PrecompileFailure> {
		let input: [u8; 0] = [];
		let cost: u64 = 1;

		match Ed25519Verify::execute(&input, cost) {
			Ok((_, _)) => {
				panic!("Test not expected to pass");
			}
			Err(e) => {
				assert_eq!(
					e,
					PrecompileFailure::Error {
						exit_status: ExitError::Other("input must contain 128 bytes".into())
					}
				);
				Ok(())
			}
		}
	}

	#[test]
	fn test_verify() -> Result<(), PrecompileFailure> {
		let secret_key_bytes: [u8; ed25519_dalek::SECRET_KEY_LENGTH] = [
			157, 097, 177, 157, 239, 253, 090, 096, 186, 132, 074, 244, 146, 236, 044, 196, 068,
			073, 197, 105, 123, 050, 105, 025, 112, 059, 172, 003, 028, 174, 127, 096,
		];

		let secret_key =
			SecretKey::from_bytes(&secret_key_bytes).expect("Failed to generate secretkey");
		let public_key = (&secret_key).into();

		let keypair = Keypair {
			secret: secret_key,
			public: public_key,
		};

		let msg: &[u8] = b"abcdefghijklmnopqrstuvwxyz123456";
		assert_eq!(msg.len(), 32);
		let signature = keypair.sign(msg);

		// input is:
		// 1) message (32 bytes)
		// 2) pubkey (32 bytes)
		// 3) signature (64 bytes)
		let mut input: Vec<u8> = Vec::with_capacity(128);
		input.extend_from_slice(msg);
		input.extend_from_slice(&public_key.to_bytes());
		input.extend_from_slice(&signature.to_bytes());
		assert_eq!(input.len(), 128);

		let cost: u64 = 1;

		match Ed25519Verify::execute(&input, cost) {
			Ok((_, output)) => {
				assert_eq!(output.len(), 4);
				assert_eq!(output[0], 0u8);
				assert_eq!(output[1], 0u8);
				assert_eq!(output[2], 0u8);
				assert_eq!(output[3], 0u8);
			}
			Err(e) => {
				return Err(e);
			}
		};

		// try again with a different message
		let msg: &[u8] = b"BAD_MESSAGE_mnopqrstuvwxyz123456";

		let mut input: Vec<u8> = Vec::with_capacity(128);
		input.extend_from_slice(msg);
		input.extend_from_slice(&public_key.to_bytes());
		input.extend_from_slice(&signature.to_bytes());
		assert_eq!(input.len(), 128);

		match Ed25519Verify::execute(&input, cost) {
			Ok((_, output)) => {
				assert_eq!(output.len(), 4);
				assert_eq!(output[0], 0u8);
				assert_eq!(output[1], 0u8);
				assert_eq!(output[2], 0u8);
				assert_eq!(output[3], 1u8); // non-zero indicates error (in our case, 1)
			}
			Err(e) => {
				return Err(e);
			}
		};

		Ok(())
	}
}
