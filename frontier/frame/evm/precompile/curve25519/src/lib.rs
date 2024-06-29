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
use curve25519_dalek::{
	ristretto::{CompressedRistretto, RistrettoPoint},
	scalar::Scalar,
	traits::Identity,
};
use fp_evm::{ExitError, ExitSucceed, LinearCostPrecompile, PrecompileFailure};

// Adds at most 10 curve25519 points and returns the CompressedRistretto bytes representation
pub struct Curve25519Add;

impl LinearCostPrecompile for Curve25519Add {
	const BASE: u64 = 60;
	const WORD: u64 = 12;

	fn execute(input: &[u8], _: u64) -> Result<(ExitSucceed, Vec<u8>), PrecompileFailure> {
		if input.len() % 32 != 0 {
			return Err(PrecompileFailure::Error {
				exit_status: ExitError::Other("input must contain multiple of 32 bytes".into()),
			});
		};

		if input.len() > 320 {
			return Err(PrecompileFailure::Error {
				exit_status: ExitError::Other(
					"input cannot be greater than 320 bytes (10 compressed points)".into(),
				),
			});
		};

		let mut points = Vec::new();
		let mut temp_buf = <&[u8]>::clone(&input);
		while !temp_buf.is_empty() {
			let mut buf = [0; 32];
			buf.copy_from_slice(&temp_buf[0..32]);
			let point = CompressedRistretto(buf);
			points.push(point);
			temp_buf = &temp_buf[32..];
		}

		let sum = points
			.iter()
			.fold(RistrettoPoint::identity(), |acc, point| {
				let pt = point.decompress().unwrap_or_else(RistrettoPoint::identity);
				acc + pt
			});

		Ok((ExitSucceed::Returned, sum.compress().to_bytes().to_vec()))
	}
}

// Multiplies a scalar field element with an elliptic curve point
pub struct Curve25519ScalarMul;

impl LinearCostPrecompile for Curve25519ScalarMul {
	const BASE: u64 = 60;
	const WORD: u64 = 12;

	fn execute(input: &[u8], _: u64) -> Result<(ExitSucceed, Vec<u8>), PrecompileFailure> {
		if input.len() != 64 {
			return Err(PrecompileFailure::Error {
				exit_status: ExitError::Other(
					"input must contain 64 bytes (scalar - 32 bytes, point - 32 bytes)".into(),
				),
			});
		};

		// first 32 bytes is for the scalar value
		let mut scalar_buf = [0; 32];
		scalar_buf.copy_from_slice(&input[0..32]);
		let scalar = Scalar::from_bytes_mod_order(scalar_buf);

		// second 32 bytes is for the compressed ristretto point bytes
		let mut pt_buf = [0; 32];
		pt_buf.copy_from_slice(&input[32..64]);
		let point = CompressedRistretto(pt_buf)
			.decompress()
			.unwrap_or_else(RistrettoPoint::identity);

		let scalar_mul = scalar * point;
		Ok((
			ExitSucceed::Returned,
			scalar_mul.compress().to_bytes().to_vec(),
		))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use curve25519_dalek::constants;

	#[test]
	fn test_sum() -> Result<(), PrecompileFailure> {
		let s1 = Scalar::from(999u64);
		let p1 = &constants::RISTRETTO_BASEPOINT_POINT * &s1;

		let s2 = Scalar::from(333u64);
		let p2 = &constants::RISTRETTO_BASEPOINT_POINT * &s2;

		let vec = vec![p1.clone(), p2.clone()];
		let mut input = vec![];
		input.extend_from_slice(&p1.compress().to_bytes());
		input.extend_from_slice(&p2.compress().to_bytes());

		let sum: RistrettoPoint = vec.iter().sum();
		let cost: u64 = 1;

		match Curve25519Add::execute(&input, cost) {
			Ok((_, out)) => {
				assert_eq!(out, sum.compress().to_bytes());
				Ok(())
			}
			Err(e) => {
				panic!("Test not expected to fail: {:?}", e);
			}
		}
	}

	#[test]
	fn test_empty() -> Result<(), PrecompileFailure> {
		// Test that sum works for the empty iterator
		let input = vec![];

		let cost: u64 = 1;

		match Curve25519Add::execute(&input, cost) {
			Ok((_, out)) => {
				assert_eq!(out, RistrettoPoint::identity().compress().to_bytes());
				Ok(())
			}
			Err(e) => {
				panic!("Test not expected to fail: {:?}", e);
			}
		}
	}

	#[test]
	fn test_scalar_mul() -> Result<(), PrecompileFailure> {
		let s1 = Scalar::from(999u64);
		let s2 = Scalar::from(333u64);
		let p1 = &constants::RISTRETTO_BASEPOINT_POINT * &s1;
		let p2 = &constants::RISTRETTO_BASEPOINT_POINT * &s2;

		let mut input = vec![];
		input.extend_from_slice(&s1.to_bytes());
		input.extend_from_slice(&constants::RISTRETTO_BASEPOINT_POINT.compress().to_bytes());

		let cost: u64 = 1;

		match Curve25519ScalarMul::execute(&input, cost) {
			Ok((_, out)) => {
				assert_eq!(out, p1.compress().to_bytes());
				assert_ne!(out, p2.compress().to_bytes());
				Ok(())
			}
			Err(e) => {
				panic!("Test not expected to fail: {:?}", e);
			}
		}
	}

	#[test]
	fn test_scalar_mul_empty_error() -> Result<(), PrecompileFailure> {
		let input = vec![];

		let cost: u64 = 1;

		match Curve25519ScalarMul::execute(&input, cost) {
			Ok((_, _out)) => {
				panic!("Test not expected to work");
			}
			Err(e) => {
				assert_eq!(
					e,
					PrecompileFailure::Error {
						exit_status: ExitError::Other(
							"input must contain 64 bytes (scalar - 32 bytes, point - 32 bytes)"
								.into()
						)
					}
				);
				Ok(())
			}
		}
	}

	#[test]
	fn test_point_addition_bad_length() -> Result<(), PrecompileFailure> {
		let input: Vec<u8> = [0u8; 33].to_vec();

		let cost: u64 = 1;

		match Curve25519Add::execute(&input, cost) {
			Ok((_, _out)) => {
				panic!("Test not expected to work");
			}
			Err(e) => {
				assert_eq!(
					e,
					PrecompileFailure::Error {
						exit_status: ExitError::Other(
							"input must contain multiple of 32 bytes".into()
						)
					}
				);
				Ok(())
			}
		}
	}

	#[test]
	fn test_point_addition_too_many_points() -> Result<(), PrecompileFailure> {
		let mut input = vec![];
		input.extend_from_slice(&constants::RISTRETTO_BASEPOINT_POINT.compress().to_bytes()); // 1
		input.extend_from_slice(&constants::RISTRETTO_BASEPOINT_POINT.compress().to_bytes()); // 2
		input.extend_from_slice(&constants::RISTRETTO_BASEPOINT_POINT.compress().to_bytes()); // 3
		input.extend_from_slice(&constants::RISTRETTO_BASEPOINT_POINT.compress().to_bytes()); // 4
		input.extend_from_slice(&constants::RISTRETTO_BASEPOINT_POINT.compress().to_bytes()); // 5
		input.extend_from_slice(&constants::RISTRETTO_BASEPOINT_POINT.compress().to_bytes()); // 6
		input.extend_from_slice(&constants::RISTRETTO_BASEPOINT_POINT.compress().to_bytes()); // 7
		input.extend_from_slice(&constants::RISTRETTO_BASEPOINT_POINT.compress().to_bytes()); // 8
		input.extend_from_slice(&constants::RISTRETTO_BASEPOINT_POINT.compress().to_bytes()); // 9
		input.extend_from_slice(&constants::RISTRETTO_BASEPOINT_POINT.compress().to_bytes()); // 10
		input.extend_from_slice(&constants::RISTRETTO_BASEPOINT_POINT.compress().to_bytes()); // 11

		let cost: u64 = 1;

		match Curve25519Add::execute(&input, cost) {
			Ok((_, _out)) => {
				panic!("Test not expected to work");
			}
			Err(e) => {
				assert_eq!(
					e,
					PrecompileFailure::Error {
						exit_status: ExitError::Other(
							"input cannot be greater than 320 bytes (10 compressed points)".into()
						)
					}
				);
				Ok(())
			}
		}
	}
}
