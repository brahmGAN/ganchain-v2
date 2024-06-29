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

pub use evm::{
	executor::stack::{
		IsPrecompileResult, PrecompileFailure, PrecompileHandle, PrecompileOutput, PrecompileSet,
	},
	Context, ExitError, ExitRevert, ExitSucceed, Transfer,
};
use sp_std::vec::Vec;

pub type PrecompileResult = Result<PrecompileOutput, PrecompileFailure>;

/// One single precompile used by EVM engine.
pub trait Precompile {
	/// Try to execute the precompile with given `handle` which provides all call data
	/// and allow to register costs and logs.
	fn execute(handle: &mut impl PrecompileHandle) -> PrecompileResult;
}

pub trait LinearCostPrecompile {
	const BASE: u64;
	const WORD: u64;

	fn execute(
		input: &[u8],
		cost: u64,
	) -> core::result::Result<(ExitSucceed, Vec<u8>), PrecompileFailure>;
}

impl<T: LinearCostPrecompile> Precompile for T {
	fn execute(handle: &mut impl PrecompileHandle) -> PrecompileResult {
		let target_gas = handle.gas_limit();
		let cost = ensure_linear_cost(target_gas, handle.input().len() as u64, T::BASE, T::WORD)?;

		handle.record_cost(cost)?;
		let (exit_status, output) = T::execute(handle.input(), cost)?;
		Ok(PrecompileOutput {
			exit_status,
			output,
		})
	}
}

/// Linear gas cost
fn ensure_linear_cost(
	target_gas: Option<u64>,
	len: u64,
	base: u64,
	word: u64,
) -> Result<u64, PrecompileFailure> {
	let cost = base
		.checked_add(word.checked_mul(len.saturating_add(31) / 32).ok_or(
			PrecompileFailure::Error {
				exit_status: ExitError::OutOfGas,
			},
		)?)
		.ok_or(PrecompileFailure::Error {
			exit_status: ExitError::OutOfGas,
		})?;

	if let Some(target_gas) = target_gas {
		if cost > target_gas {
			return Err(PrecompileFailure::Error {
				exit_status: ExitError::OutOfGas,
			});
		}
	}

	Ok(cost)
}
