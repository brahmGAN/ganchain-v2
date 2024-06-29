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

mod eip_152;

use fp_evm::{
	ExitError, ExitSucceed, Precompile, PrecompileFailure, PrecompileHandle, PrecompileOutput,
	PrecompileResult,
};

pub struct Blake2F;

impl Blake2F {
	const GAS_COST_PER_ROUND: u64 = 1; // https://eips.ethereum.org/EIPS/eip-152#gas-costs-and-benchmarks
}

impl Precompile for Blake2F {
	/// Format of `input`:
	/// [4 bytes for rounds][64 bytes for h][128 bytes for m][8 bytes for t_0][8 bytes for t_1][1 byte for f]
	fn execute(handle: &mut impl PrecompileHandle) -> PrecompileResult {
		const BLAKE2_F_ARG_LEN: usize = 213;

		let input = handle.input();

		if input.len() != BLAKE2_F_ARG_LEN {
			return Err(PrecompileFailure::Error {
				exit_status: ExitError::Other(
					"input length for Blake2 F precompile should be exactly 213 bytes".into(),
				),
			});
		}

		let mut rounds_buf: [u8; 4] = [0; 4];
		rounds_buf.copy_from_slice(&input[0..4]);
		let rounds: u32 = u32::from_be_bytes(rounds_buf);

		let gas_cost: u64 = (rounds as u64) * Blake2F::GAS_COST_PER_ROUND;
		handle.record_cost(gas_cost)?;

		let input = handle.input();

		// we use from_le_bytes below to effectively swap byte order to LE if architecture is BE

		let mut h_buf: [u8; 64] = [0; 64];
		h_buf.copy_from_slice(&input[4..68]);
		let mut h = [0u64; 8];
		let mut ctr = 0;
		for state_word in &mut h {
			let mut temp: [u8; 8] = Default::default();
			temp.copy_from_slice(&h_buf[(ctr * 8)..(ctr + 1) * 8]);
			*state_word = u64::from_le_bytes(temp);
			ctr += 1;
		}

		let mut m_buf: [u8; 128] = [0; 128];
		m_buf.copy_from_slice(&input[68..196]);
		let mut m = [0u64; 16];
		ctr = 0;
		for msg_word in &mut m {
			let mut temp: [u8; 8] = Default::default();
			temp.copy_from_slice(&m_buf[(ctr * 8)..(ctr + 1) * 8]);
			*msg_word = u64::from_le_bytes(temp);
			ctr += 1;
		}

		let mut t_0_buf: [u8; 8] = [0; 8];
		t_0_buf.copy_from_slice(&input[196..204]);
		let t_0 = u64::from_le_bytes(t_0_buf);

		let mut t_1_buf: [u8; 8] = [0; 8];
		t_1_buf.copy_from_slice(&input[204..212]);
		let t_1 = u64::from_le_bytes(t_1_buf);

		let f = if input[212] == 1 {
			true
		} else if input[212] == 0 {
			false
		} else {
			return Err(PrecompileFailure::Error {
				exit_status: ExitError::Other("incorrect final block indicator flag".into()),
			});
		};

		crate::eip_152::compress(&mut h, m, [t_0, t_1], f, rounds as usize);

		let mut output_buf = [0u8; u64::BITS as usize];
		for (i, state_word) in h.iter().enumerate() {
			output_buf[i * 8..(i + 1) * 8].copy_from_slice(&state_word.to_le_bytes());
		}

		Ok(PrecompileOutput {
			exit_status: ExitSucceed::Returned,
			output: output_buf.to_vec(),
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use pallet_evm_test_vector_support::test_precompile_test_vectors;

	#[test]
	fn process_consensus_tests() -> Result<(), String> {
		test_precompile_test_vectors::<Blake2F>("../testdata/blake2F.json")?;
		Ok(())
	}
}
