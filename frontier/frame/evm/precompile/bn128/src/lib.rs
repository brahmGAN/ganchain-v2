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
use fp_evm::{
	ExitError, ExitSucceed, Precompile, PrecompileFailure, PrecompileHandle, PrecompileOutput,
	PrecompileResult,
};
use sp_core::U256;

/// Copy bytes from input to target.
fn read_input(source: &[u8], target: &mut [u8], offset: usize) {
	// Out of bounds, nothing to copy.
	if source.len() <= offset {
		return;
	}

	// Find len to copy up to target len, but not out of bounds.
	let len = core::cmp::min(target.len(), source.len() - offset);
	target[..len].copy_from_slice(&source[offset..][..len]);
}

fn read_fr(input: &[u8], start_inx: usize) -> Result<bn::Fr, PrecompileFailure> {
	let mut buf = [0u8; 32];
	read_input(input, &mut buf, start_inx);

	bn::Fr::from_slice(&buf).map_err(|_| PrecompileFailure::Error {
		exit_status: ExitError::Other("Invalid field element".into()),
	})
}

fn read_point(input: &[u8], start_inx: usize) -> Result<bn::G1, PrecompileFailure> {
	use bn::{AffineG1, Fq, Group, G1};

	let mut px_buf = [0u8; 32];
	let mut py_buf = [0u8; 32];
	read_input(input, &mut px_buf, start_inx);
	read_input(input, &mut py_buf, start_inx + 32);

	let px = Fq::from_slice(&px_buf).map_err(|_| PrecompileFailure::Error {
		exit_status: ExitError::Other("Invalid point x coordinate".into()),
	})?;

	let py = Fq::from_slice(&py_buf).map_err(|_| PrecompileFailure::Error {
		exit_status: ExitError::Other("Invalid point y coordinate".into()),
	})?;

	Ok(if px == Fq::zero() && py == Fq::zero() {
		G1::zero()
	} else {
		AffineG1::new(px, py)
			.map_err(|_| PrecompileFailure::Error {
				exit_status: ExitError::Other("Invalid curve point".into()),
			})?
			.into()
	})
}

/// The Bn128Add builtin
pub struct Bn128Add;

impl Bn128Add {
	const GAS_COST: u64 = 150; // https://eips.ethereum.org/EIPS/eip-1108
}

impl Precompile for Bn128Add {
	fn execute(handle: &mut impl PrecompileHandle) -> PrecompileResult {
		use bn::AffineG1;

		handle.record_cost(Bn128Add::GAS_COST)?;

		let input = handle.input();

		let p1 = read_point(input, 0)?;
		let p2 = read_point(input, 64)?;

		let mut buf = [0u8; 64];
		if let Some(sum) = AffineG1::from_jacobian(p1 + p2) {
			// point not at infinity
			sum.x()
				.to_big_endian(&mut buf[0..32])
				.map_err(|_| PrecompileFailure::Error {
					exit_status: ExitError::Other(
						"Cannot fail since 0..32 is 32-byte length".into(),
					),
				})?;
			sum.y()
				.to_big_endian(&mut buf[32..64])
				.map_err(|_| PrecompileFailure::Error {
					exit_status: ExitError::Other(
						"Cannot fail since 32..64 is 32-byte length".into(),
					),
				})?;
		}

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			output: buf.to_vec(),
		})
	}
}

/// The Bn128Mul builtin
pub struct Bn128Mul;

impl Bn128Mul {
	const GAS_COST: u64 = 6_000; // https://eips.ethereum.org/EIPS/eip-1108
}

impl Precompile for Bn128Mul {
	fn execute(handle: &mut impl PrecompileHandle) -> PrecompileResult {
		use bn::AffineG1;

		handle.record_cost(Bn128Mul::GAS_COST)?;

		let input = handle.input();

		let p = read_point(input, 0)?;
		let fr = read_fr(input, 64)?;

		let mut buf = [0u8; 64];
		if let Some(sum) = AffineG1::from_jacobian(p * fr) {
			// point not at infinity
			sum.x()
				.to_big_endian(&mut buf[0..32])
				.map_err(|_| PrecompileFailure::Error {
					exit_status: ExitError::Other(
						"Cannot fail since 0..32 is 32-byte length".into(),
					),
				})?;
			sum.y()
				.to_big_endian(&mut buf[32..64])
				.map_err(|_| PrecompileFailure::Error {
					exit_status: ExitError::Other(
						"Cannot fail since 32..64 is 32-byte length".into(),
					),
				})?;
		}

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			output: buf.to_vec(),
		})
	}
}

/// The Bn128Pairing builtin
pub struct Bn128Pairing;

impl Bn128Pairing {
	// https://eips.ethereum.org/EIPS/eip-1108
	const BASE_GAS_COST: u64 = 45_000;
	const GAS_COST_PER_PAIRING: u64 = 34_000;
}

impl Precompile for Bn128Pairing {
	fn execute(handle: &mut impl PrecompileHandle) -> PrecompileResult {
		use bn::{pairing_batch, AffineG1, AffineG2, Fq, Fq2, Group, Gt, G1, G2};

		let ret_val = if handle.input().is_empty() {
			handle.record_cost(Bn128Pairing::BASE_GAS_COST)?;
			U256::one()
		} else {
			if handle.input().len() % 192 > 0 {
				return Err(PrecompileFailure::Error {
					exit_status: ExitError::Other("bad elliptic curve pairing size".into()),
				});
			}

			// (a, b_a, b_b - each 64-byte affine coordinates)
			let elements = handle.input().len() / 192;

			let gas_cost: u64 = Bn128Pairing::BASE_GAS_COST
				+ (elements as u64 * Bn128Pairing::GAS_COST_PER_PAIRING);

			handle.record_cost(gas_cost)?;

			let input = handle.input();

			let mut vals = Vec::new();
			for idx in 0..elements {
				let a_x = Fq::from_slice(&input[idx * 192..idx * 192 + 32]).map_err(|_| {
					PrecompileFailure::Error {
						exit_status: ExitError::Other("Invalid a argument x coordinate".into()),
					}
				})?;

				let a_y = Fq::from_slice(&input[idx * 192 + 32..idx * 192 + 64]).map_err(|_| {
					PrecompileFailure::Error {
						exit_status: ExitError::Other("Invalid a argument y coordinate".into()),
					}
				})?;

				let b_a_y =
					Fq::from_slice(&input[idx * 192 + 64..idx * 192 + 96]).map_err(|_| {
						PrecompileFailure::Error {
							exit_status: ExitError::Other(
								"Invalid b argument imaginary coeff x coordinate".into(),
							),
						}
					})?;

				let b_a_x =
					Fq::from_slice(&input[idx * 192 + 96..idx * 192 + 128]).map_err(|_| {
						PrecompileFailure::Error {
							exit_status: ExitError::Other(
								"Invalid b argument imaginary coeff y coordinate".into(),
							),
						}
					})?;

				let b_b_y =
					Fq::from_slice(&input[idx * 192 + 128..idx * 192 + 160]).map_err(|_| {
						PrecompileFailure::Error {
							exit_status: ExitError::Other(
								"Invalid b argument real coeff x coordinate".into(),
							),
						}
					})?;

				let b_b_x =
					Fq::from_slice(&input[idx * 192 + 160..idx * 192 + 192]).map_err(|_| {
						PrecompileFailure::Error {
							exit_status: ExitError::Other(
								"Invalid b argument real coeff y coordinate".into(),
							),
						}
					})?;

				let b_a = Fq2::new(b_a_x, b_a_y);
				let b_b = Fq2::new(b_b_x, b_b_y);
				let b = if b_a.is_zero() && b_b.is_zero() {
					G2::zero()
				} else {
					G2::from(
						AffineG2::new(b_a, b_b).map_err(|_| PrecompileFailure::Error {
							exit_status: ExitError::Other(
								"Invalid b argument - not on curve".into(),
							),
						})?,
					)
				};
				let a = if a_x.is_zero() && a_y.is_zero() {
					G1::zero()
				} else {
					G1::from(
						AffineG1::new(a_x, a_y).map_err(|_| PrecompileFailure::Error {
							exit_status: ExitError::Other(
								"Invalid a argument - not on curve".into(),
							),
						})?,
					)
				};
				vals.push((a, b));
			}

			let mul = pairing_batch(&vals);

			if mul == Gt::one() {
				U256::one()
			} else {
				U256::zero()
			}
		};

		let mut buf = [0u8; 32];
		ret_val.to_big_endian(&mut buf);

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			output: buf.to_vec(),
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use pallet_evm_test_vector_support::test_precompile_test_vectors;

	#[test]
	fn process_consensus_tests_for_add() -> Result<(), String> {
		test_precompile_test_vectors::<Bn128Add>("../testdata/common_bnadd.json")?;
		Ok(())
	}

	#[test]
	fn process_consensus_tests_for_mul() -> Result<(), String> {
		test_precompile_test_vectors::<Bn128Mul>("../testdata/common_bnmul.json")?;
		Ok(())
	}

	#[test]
	fn process_consensus_tests_for_pair() -> Result<(), String> {
		test_precompile_test_vectors::<Bn128Pairing>("../testdata/common_bnpair.json")?;
		Ok(())
	}
}
