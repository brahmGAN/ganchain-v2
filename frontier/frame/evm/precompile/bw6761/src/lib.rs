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

// Arkworks
use ark_bw6_761::{Fq, Fr, G1Affine, G1Projective, G2Affine, G2Projective, BW6_761};
use ark_ec::{pairing::Pairing, AffineRepr, CurveGroup, VariableBaseMSM};
use ark_ff::{BigInteger768, PrimeField, Zero};
use ark_std::ops::Mul;

// Frontier
use fp_evm::{
	ExitError, ExitSucceed, Precompile, PrecompileFailure, PrecompileHandle, PrecompileOutput,
	PrecompileResult,
};

/// Gas discount table for BW6-761 G1 and G2 multi exponentiation operations.
// TODO::to be estimated
const BW6761_MULTIEXP_DISCOUNT_TABLE: [u16; 128] = [0u16; 128];

/// Encode Fq as `96` bytes by performing Big-Endian encoding of the corresponding (unsigned) integer.
fn encode_fq(field: Fq) -> [u8; 96] {
	let mut result = [0u8; 96];
	let rep = field.into_bigint().0;

	result[0..8].copy_from_slice(&rep[11].to_be_bytes());
	result[8..16].copy_from_slice(&rep[10].to_be_bytes());
	result[16..24].copy_from_slice(&rep[9].to_be_bytes());
	result[24..32].copy_from_slice(&rep[8].to_be_bytes());
	result[32..40].copy_from_slice(&rep[7].to_be_bytes());
	result[40..48].copy_from_slice(&rep[6].to_be_bytes());
	result[48..56].copy_from_slice(&rep[5].to_be_bytes());
	result[56..64].copy_from_slice(&rep[4].to_be_bytes());
	result[64..72].copy_from_slice(&rep[3].to_be_bytes());
	result[72..80].copy_from_slice(&rep[2].to_be_bytes());
	result[80..88].copy_from_slice(&rep[1].to_be_bytes());
	result[88..96].copy_from_slice(&rep[0].to_be_bytes());

	result
}

/// Encode point G1 as byte concatenation of encodings of the `x` and `y` affine coordinates.
fn encode_g1(g1: G1Affine) -> [u8; 192] {
	let mut result = [0u8; 192];
	if !g1.is_zero() {
		result[0..96].copy_from_slice(&encode_fq(g1.x));
		result[96..192].copy_from_slice(&encode_fq(g1.y));
	}
	result
}

/// Encode point G2 as byte concatenation of encodings of the `x` and `y` affine coordinates.
fn encode_g2(g2: G2Affine) -> [u8; 192] {
	let mut result = [0u8; 192];
	if !g2.is_zero() {
		result[0..96].copy_from_slice(&encode_fq(g2.x));
		result[96..192].copy_from_slice(&encode_fq(g2.y));
	}
	result
}

/// Copy bytes from source.offset to target.
fn read_input(source: &[u8], target: &mut [u8], offset: usize) {
	let len = target.len();
	target[..len].copy_from_slice(&source[offset..][..len]);
}

/// Decode Fr expects 64 byte input, returns fr in scalar field.
fn decode_fr(input: &[u8], offset: usize) -> Fr {
	let mut bytes = [0u8; 64];
	read_input(input, &mut bytes, offset);
	Fr::from_be_bytes_mod_order(&bytes)
}

/// Decode Fq expects 96 byte input,
/// returns Fq in base field.
fn decode_fq(bytes: [u8; 96]) -> Option<Fq> {
	let mut tmp = BigInteger768::new([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
	// Note: The following unwraps are if the compiler cannot convert
	// the byte slice into [u8;8], we know this is infallible since we
	// are providing the indices at compile time and bytes has a fixed size
	tmp.0[11] = u64::from_be_bytes(<[u8; 8]>::try_from(&bytes[0..8]).unwrap());
	tmp.0[10] = u64::from_be_bytes(<[u8; 8]>::try_from(&bytes[8..16]).unwrap());
	tmp.0[9] = u64::from_be_bytes(<[u8; 8]>::try_from(&bytes[16..24]).unwrap());
	tmp.0[8] = u64::from_be_bytes(<[u8; 8]>::try_from(&bytes[24..32]).unwrap());
	tmp.0[7] = u64::from_be_bytes(<[u8; 8]>::try_from(&bytes[32..40]).unwrap());
	tmp.0[6] = u64::from_be_bytes(<[u8; 8]>::try_from(&bytes[40..48]).unwrap());
	tmp.0[5] = u64::from_be_bytes(<[u8; 8]>::try_from(&bytes[48..56]).unwrap());
	tmp.0[4] = u64::from_be_bytes(<[u8; 8]>::try_from(&bytes[56..64]).unwrap());
	tmp.0[3] = u64::from_be_bytes(<[u8; 8]>::try_from(&bytes[64..72]).unwrap());
	tmp.0[2] = u64::from_be_bytes(<[u8; 8]>::try_from(&bytes[72..80]).unwrap());
	tmp.0[1] = u64::from_be_bytes(<[u8; 8]>::try_from(&bytes[80..88]).unwrap());
	tmp.0[0] = u64::from_be_bytes(<[u8; 8]>::try_from(&bytes[88..96]).unwrap());

	Fq::from_bigint(tmp)
}

fn extract_fq(bytes: [u8; 96]) -> Result<Fq, PrecompileFailure> {
	let fq = decode_fq(bytes);
	match fq {
		None => Err(PrecompileFailure::Error {
			exit_status: ExitError::Other("invalid Fq".into()),
		}),
		Some(c) => Ok(c),
	}
}

/// Decode G1 given encoded (x, y) coordinates in 192 bytes returns a valid G1 Point.
fn decode_g1(input: &[u8], offset: usize) -> Result<G1Projective, PrecompileFailure> {
	let mut px_buf = [0u8; 96];
	let mut py_buf = [0u8; 96];
	read_input(input, &mut px_buf, offset);
	read_input(input, &mut py_buf, offset + 96);

	// Decode x
	let px = extract_fq(px_buf)?;
	// Decode y
	let py = extract_fq(py_buf)?;

	// Check if given input points to infinity
	if px.is_zero() && py.is_zero() {
		Ok(G1Projective::zero())
	} else {
		let g1 = G1Affine::new_unchecked(px, py);
		if !g1.is_on_curve() {
			Err(PrecompileFailure::Error {
				exit_status: ExitError::Other("point is not on curve".into()),
			})
		} else {
			Ok(g1.into())
		}
	}
}

// Decode G2 given encoded (x, y) coordinates in 192 bytes returns a valid G2 Point.
fn decode_g2(input: &[u8], offset: usize) -> Result<G2Projective, PrecompileFailure> {
	let mut px_buf = [0u8; 96];
	let mut py_buf = [0u8; 96];
	read_input(input, &mut px_buf, offset);
	read_input(input, &mut py_buf, offset + 96);

	// Decode x
	let px = extract_fq(px_buf)?;
	// Decode y
	let py = extract_fq(py_buf)?;

	// Check if given input points to infinity
	if px.is_zero() && py.is_zero() {
		Ok(G2Projective::zero())
	} else {
		let g2 = G2Affine::new_unchecked(px, py);
		if !g2.is_on_curve() {
			Err(PrecompileFailure::Error {
				exit_status: ExitError::Other("point is not on curve".into()),
			})
		} else {
			Ok(g2.into())
		}
	}
}

/// Bw6761G1Add implements EIP-3026 G1Add precompile.
pub struct Bw6761G1Add;

impl Bw6761G1Add {
	// TODO::to be estimated
	const GAS_COST: u64 = 0;
}

impl Precompile for Bw6761G1Add {
	/// Implements EIP-3026 G1Add precompile.
	/// > G1 addition call expects `384` bytes as an input that is interpreted as byte concatenation of two G1 points (`192` bytes each).
	/// > Output is an encoding of addition operation result - single G1 point (`192` bytes).
	fn execute(handle: &mut impl PrecompileHandle) -> PrecompileResult {
		handle.record_cost(Bw6761G1Add::GAS_COST)?;

		let input = handle.input();
		if input.len() != 384 {
			return Err(PrecompileFailure::Error {
				exit_status: ExitError::Other("invalid input length".into()),
			});
		}

		// Decode G1 point p_0
		let p0 = decode_g1(input, 0)?;
		// Decode G1 point p_1
		let p1 = decode_g1(input, 192)?;
		// Compute r = p_0 + p_1
		let r = p0 + p1;
		// Encode the G1 point into 192 bytes output
		let output = encode_g1(r.into_affine());

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			output: output.to_vec(),
		})
	}
}

/// Bw6761G1Mul implements EIP-3026 G1Mul precompile.
pub struct Bw6761G1Mul;

impl Bw6761G1Mul {
	// TODO::to be estimated
	const GAS_COST: u64 = 0;
}

impl Precompile for Bw6761G1Mul {
	/// Implements EIP-3026 G1Mul precompile.
	/// > G1 multiplication call expects `256` bytes as an input that is interpreted as byte concatenation of encoding of G1 point (`192` bytes) and encoding of a scalar value (`64` bytes).
	/// > Output is an encoding of multiplication operation result - single G1 point (`192` bytes).
	fn execute(handle: &mut impl PrecompileHandle) -> PrecompileResult {
		handle.record_cost(Bw6761G1Mul::GAS_COST)?;

		let input = handle.input();
		if input.len() != 256 {
			return Err(PrecompileFailure::Error {
				exit_status: ExitError::Other("invalid input length".into()),
			});
		}

		// Decode G1 point
		let p = decode_g1(input, 0)?;
		// Decode scalar value
		let e = decode_fr(input, 192);
		// Compute r = e * p
		let r = p.mul(e);
		// Encode the G1 point into 192 bytes output
		let output = encode_g1(r.into_affine());

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			output: output.to_vec(),
		})
	}
}

/// Bw6761G1MultiExp implements EIP-3026 G1MultiExp precompile.
pub struct Bw6761G1MultiExp;

impl Bw6761G1MultiExp {
	// TODO::to be estimated
	const MULTIPLIER: u64 = 1_000;

	/// Returns the gas required to execute the pre-compiled contract.
	fn calculate_gas_cost(input_len: usize) -> u64 {
		// Calculate G1 point, scalar value pair length
		let k = input_len / 256;
		if k == 0 {
			return 0;
		}
		// Lookup discount value for G1 point, scalar value pair length
		let d_len = BW6761_MULTIEXP_DISCOUNT_TABLE.len();
		let discount = if k <= d_len {
			BW6761_MULTIEXP_DISCOUNT_TABLE[k - 1]
		} else {
			BW6761_MULTIEXP_DISCOUNT_TABLE[d_len - 1]
		};
		// Calculate gas and return the result
		k as u64 * Bw6761G1Mul::GAS_COST * discount as u64 / Bw6761G1MultiExp::MULTIPLIER
	}
}

impl Precompile for Bw6761G1MultiExp {
	/// Implements EIP-3026 G1MultiExp precompile.
	/// G1 multiplication call expects `256*k` bytes as an input that is interpreted as byte concatenation of `k` slices each of them being a byte concatenation of encoding of G1 point (`192` bytes) and encoding of a scalar value (`64` bytes).
	/// Output is an encoding of multiexponentiation operation result - single G1 point (`192` bytes).
	fn execute(handle: &mut impl PrecompileHandle) -> PrecompileResult {
		let gas_cost = Bw6761G1MultiExp::calculate_gas_cost(handle.input().len());
		handle.record_cost(gas_cost)?;

		let k = handle.input().len() / 256;
		if handle.input().is_empty() || handle.input().len() % 256 != 0 {
			return Err(PrecompileFailure::Error {
				exit_status: ExitError::Other("invalid input length".into()),
			});
		}

		let input = handle.input();

		let mut points = Vec::new();
		let mut scalars = Vec::new();
		// Decode point scalar pairs
		for idx in 0..k {
			let offset = idx * 256;
			// Decode G1 point
			let p = decode_g1(input, offset)?;
			// Decode scalar value
			let scalar = decode_fr(input, offset + 192);
			points.push(p.into_affine());
			scalars.push(scalar);
		}

		// Compute r = e_0 * p_0 + e_1 * p_1 + ... + e_(k-1) * p_(k-1)
		let r = G1Projective::msm(&points.to_vec(), &scalars.to_vec()).map_err(|_| {
			PrecompileFailure::Error {
				exit_status: ExitError::Other("MSM failed".into()),
			}
		})?;

		// Encode the G1 point into 128 bytes output
		let output = encode_g1(r.into_affine());
		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			output: output.to_vec(),
		})
	}
}

/// Bw6761G2Add implements EIP-3026 G2Add precompile.
pub struct Bw6761G2Add;

impl Bw6761G2Add {
	// TODO::to be estimated
	const GAS_COST: u64 = 0;
}

impl Precompile for Bw6761G2Add {
	/// Implements EIP-3026 G2Add precompile.
	/// > G2 addition call expects `384` bytes as an input that is interpreted as byte concatenation of two G2 points (`192` bytes each).
	/// > Output is an encoding of addition operation result - single G2 point (`192` bytes).
	fn execute(handle: &mut impl PrecompileHandle) -> PrecompileResult {
		handle.record_cost(Bw6761G2Add::GAS_COST)?;

		let input = handle.input();
		if input.len() != 384 {
			return Err(PrecompileFailure::Error {
				exit_status: ExitError::Other("invalid input length".into()),
			});
		}

		// Decode G2 point p_0
		let p0 = decode_g2(input, 0)?;
		// Decode G2 point p_1
		let p1 = decode_g2(input, 192)?;
		// Compute r = p_0 + p_1
		let r = p0 + p1;
		// Encode the G2 point into 256 bytes output
		let output = encode_g2(r.into_affine());

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			output: output.to_vec(),
		})
	}
}

/// Bw6761G2Mul implements EIP-3026 G2Mul precompile.
pub struct Bw6761G2Mul;

impl Bw6761G2Mul {
	// TODO::to be estimated
	const GAS_COST: u64 = 0;
}

impl Precompile for Bw6761G2Mul {
	/// Implements EIP-3026 G2MUL precompile logic.
	/// > G2 multiplication call expects `256` bytes as an input that is interpreted as byte concatenation of encoding of G2 point (`192` bytes) and encoding of a scalar value (`64` bytes).
	/// > Output is an encoding of multiplication operation result - single G2 point (`192` bytes).
	fn execute(handle: &mut impl PrecompileHandle) -> PrecompileResult {
		handle.record_cost(Bw6761G2Mul::GAS_COST)?;

		let input = handle.input();
		if input.len() != 256 {
			return Err(PrecompileFailure::Error {
				exit_status: ExitError::Other("invalid input length".into()),
			});
		}

		// Decode G2 point
		let p = decode_g2(input, 0)?;
		// Decode scalar value
		let e = decode_fr(input, 192);
		// Compute r = e * p
		let r = p.mul(e);
		// Encode the G2 point into 256 bytes output
		let output = encode_g2(r.into_affine());

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			output: output.to_vec(),
		})
	}
}

// Bw6761G2MultiExp implements EIP-3026 G2MultiExp precompile.
pub struct Bw6761G2MultiExp;

impl Bw6761G2MultiExp {
	// TODO::to be estimated
	const MULTIPLIER: u64 = 1_000;

	/// Returns the gas required to execute the pre-compiled contract.
	fn calculate_gas_cost(input_len: usize) -> u64 {
		// Calculate G2 point, scalar value pair length
		let k = input_len / 256;
		if k == 0 {
			return 0;
		}
		// Lookup discount value for G2 point, scalar value pair length
		let d_len = BW6761_MULTIEXP_DISCOUNT_TABLE.len();
		let discount = if k <= d_len {
			BW6761_MULTIEXP_DISCOUNT_TABLE[k - 1]
		} else {
			BW6761_MULTIEXP_DISCOUNT_TABLE[d_len - 1]
		};
		// Calculate gas and return the result
		k as u64 * Bw6761G2Mul::GAS_COST * discount as u64 / Bw6761G2MultiExp::MULTIPLIER
	}
}

impl Precompile for Bw6761G2MultiExp {
	/// Implements EIP-3026 G2MultiExp precompile logic
	/// > G2 multiplication call expects `256*k` bytes as an input that is interpreted as byte concatenation of `k` slices each of them being a byte concatenation of encoding of G2 point (`256` bytes) and encoding of a scalar value (`64` bytes).
	/// > Output is an encoding of multiexponentiation operation result - single G2 point (`192` bytes).
	fn execute(handle: &mut impl PrecompileHandle) -> PrecompileResult {
		let gas_cost = Bw6761G2MultiExp::calculate_gas_cost(handle.input().len());
		handle.record_cost(gas_cost)?;

		let k = handle.input().len() / 256;
		if handle.input().is_empty() || handle.input().len() % 256 != 0 {
			return Err(PrecompileFailure::Error {
				exit_status: ExitError::Other("invalid input length".into()),
			});
		}

		let input = handle.input();

		let mut points = Vec::new();
		let mut scalars = Vec::new();
		// Decode point scalar pairs
		for idx in 0..k {
			let offset = idx * 256;
			// Decode G2 point
			let p = decode_g2(input, offset)?;
			// Decode scalar value
			let scalar = decode_fr(input, offset + 192);
			points.push(p.into_affine());
			scalars.push(scalar);
		}

		// Compute r = e_0 * p_0 + e_1 * p_1 + ... + e_(k-1) * p_(k-1)
		let r = G2Projective::msm(&points.to_vec(), &scalars.to_vec()).map_err(|_| {
			PrecompileFailure::Error {
				exit_status: ExitError::Other("MSM failed".into()),
			}
		})?;

		// Encode the G2 point to 256 bytes output
		let output = encode_g2(r.into_affine());
		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			output: output.to_vec(),
		})
	}
}

/// Bw6761Pairing implements EIP-3026 Pairing precompile.
pub struct Bw6761Pairing;

impl Bw6761Pairing {
	// TODO::to be estimated
	const BASE_GAS: u64 = 0;
	const PER_PAIR_GAS: u64 = 0;
}

impl Precompile for Bw6761Pairing {
	/// Implements EIP-3026 Pairing precompile logic.
	/// > Pairing call expects `384*k` bytes as an inputs that is interpreted as byte concatenation of `k` slices. Each slice has the following structure:
	/// > - `192` bytes of G1 point encoding
	/// > - `192` bytes of G2 point encoding
	/// > Output is a `32` bytes where last single byte is `0x01` if pairing result is equal to multiplicative identity in a pairing target field and `0x00` otherwise
	/// > (which is equivalent of Big Endian encoding of Solidity values `uint256(1)` and `uin256(0)` respectively).
	fn execute(handle: &mut impl PrecompileHandle) -> PrecompileResult {
		if handle.input().is_empty() || handle.input().len() % 384 != 0 {
			return Err(PrecompileFailure::Error {
				exit_status: ExitError::Other("invalid input length".into()),
			});
		}

		let k = handle.input().len() / 384;
		let gas_cost: u64 = Bw6761Pairing::BASE_GAS + (k as u64 * Bw6761Pairing::PER_PAIR_GAS);

		handle.record_cost(gas_cost)?;

		let input = handle.input();

		let mut a = Vec::new();
		let mut b = Vec::new();
		// Decode G1 G2 pairs
		for idx in 0..k {
			let offset = idx * 384;
			// Decode G1 point
			let g1 = decode_g1(input, offset)?;
			// Decode G2 point
			let g2 = decode_g2(input, offset + 192)?;

			// 'point is on curve' check already done,
			// Here we need to apply subgroup checks.
			if !g1.into_affine().is_in_correct_subgroup_assuming_on_curve() {
				return Err(PrecompileFailure::Error {
					exit_status: ExitError::Other("g1 point is not on correct subgroup".into()),
				});
			}
			if !g2.into_affine().is_in_correct_subgroup_assuming_on_curve() {
				return Err(PrecompileFailure::Error {
					exit_status: ExitError::Other("g2 point is not on correct subgroup".into()),
				});
			}

			a.push(g1);
			b.push(g2);
		}

		let mut output = [0u8; 32];
		// Compute pairing and set the output
		if BW6_761::multi_pairing(a, b).is_zero() {
			output[31] = 1;
		}

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			output: output.to_vec(),
		})
	}
}

#[cfg(test)]
mod tests;
