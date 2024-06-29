// This file is part of Astar.

// Copyright (C) 2019-2023 Stake Technologies Pte.Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later

// Astar is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Astar is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Astar. If not, see <http://www.gnu.org/licenses/>.

// Copyright 2019-2022 PureStake Inc.
// Copyright 2022 Stake Technologies
// This file is part of pallet-evm-precompile-batch package, originally developed by Purestake Inc.
// pallet-evm-precompile-batch package used in Astar Network in terms of GPLv3.
//
// pallet-evm-precompile-batch is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// pallet-evm-precompile-batch is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with pallet-evm-precompile-batch.  If not, see <http://www.gnu.org/licenses/>.
#![cfg_attr(not(feature = "std"), no_std)]

use ::evm::{ExitError, ExitReason};
use fp_evm::{Context, Log, PrecompileFailure, PrecompileHandle, Transfer};
use frame_support::{
	dispatch::{Dispatchable, GetDispatchInfo, PostDispatchInfo},
	traits::ConstU32,
};
use pallet_evm::{Precompile, PrecompileOutput};
use precompile_utils::{bytes::BoundedBytes, data::BoundedVec, *};
use sp_core::{H160, U256};
use sp_std::{iter::repeat, marker::PhantomData, vec, vec::Vec};

// #[cfg(test)]
// mod mock;
// #[cfg(test)]
// mod tests;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Mode {
	BatchSome,             // = "batchSome(address[],uint256[],bytes[],uint64[])",
	BatchSomeUntilFailure, // = "batchSomeUntilFailure(address[],uint256[],bytes[],uint64[])",
	BatchAll,              // = "batchAll(address[],uint256[],bytes[],uint64[])",
}

pub const LOG_SUBCALL_SUCCEEDED: [u8; 32] = keccak256!("SubcallSucceeded(uint256)");
pub const LOG_SUBCALL_FAILED: [u8; 32] = keccak256!("SubcallFailed(uint256)");
pub const CALL_DATA_LIMIT: u32 = 2u32.pow(16);
pub const ARRAY_LIMIT: u32 = 2u32.pow(9);

type GetCallDataLimit = ConstU32<CALL_DATA_LIMIT>;
type GetArrayLimit = ConstU32<ARRAY_LIMIT>;

fn log_subcall_succeeded(address: impl Into<H160>, index: usize) -> Log {
	LogsBuilder::new(address.into())
		.log1(LOG_SUBCALL_SUCCEEDED, data::encode_event_data(U256::from(index)))
}

fn log_subcall_failed(address: impl Into<H160>, index: usize) -> Log {
	LogsBuilder::new(address.into())
		.log1(LOG_SUBCALL_FAILED, data::encode_event_data(U256::from(index)))
}

#[precompile_utils::generate_function_selector]
#[derive(Debug, PartialEq)]
pub enum Action {
	BatchSome = "batchSome(address[],uint256[],bytes[],uint64[])",
	BatchSomeUntilFailure = "batchSomeUntilFailure(address[],uint256[],bytes[],uint64[])",
	BatchAll = "batchAll(address[],uint256[],bytes[],uint64[])",
}

/// Batch precompile.
#[derive(Debug, Clone)]
pub struct BatchPrecompile<Runtime>(PhantomData<Runtime>);

impl<Runtime> Precompile for BatchPrecompile<Runtime>
where
	Runtime: pallet_evm::Config,
	Runtime::RuntimeCall: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
{
	fn execute(handle: &mut impl PrecompileHandle) -> EvmResult<PrecompileOutput> {
		let selector = handle.read_selector()?;

		handle.check_function_modifier(FunctionModifier::NonPayable)?;
		match selector {
			Action::BatchSome => Self::batch_some(handle),
			Action::BatchAll => Self::batch_all(handle),
			Action::BatchSomeUntilFailure => Self::batch_some_until_failure(handle),
		}
	}
}
// No funds are transfered to the precompile address.
// Transfers will directly be made on the behalf of the user by the precompile.
// #[precompile_utils::precompile]
impl<Runtime> BatchPrecompile<Runtime>
where
	Runtime: pallet_evm::Config,
	Runtime::RuntimeCall: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
{
	fn batch_some(handle: &mut impl PrecompileHandle) -> EvmResult<PrecompileOutput> {
		let mut input = handle.read_input()?;
		input.expect_arguments(4)?;
		let to = input.read::<BoundedVec<Address, GetArrayLimit>>()?;
		let value = input.read::<BoundedVec<U256, GetArrayLimit>>()?;
		let call_data =
			input.read::<BoundedVec<BoundedBytes<GetCallDataLimit>, GetArrayLimit>>()?;
		let gas_limit = input.read::<BoundedVec<u64, GetArrayLimit>>()?;
		log::trace!(target: "batch-precompile", "batch_some\n to address(s) {:?}, value(s) {:?} call_data(s) {:?}, gas_limit(s) {:?}", to, value,call_data, gas_limit);
		Self::inner_batch(Mode::BatchSome, handle, to, value, call_data, gas_limit)
	}

	fn batch_some_until_failure(handle: &mut impl PrecompileHandle) -> EvmResult<PrecompileOutput> {
		let mut input = handle.read_input()?;
		input.expect_arguments(4)?;
		let to = input.read::<BoundedVec<Address, GetArrayLimit>>()?;
		let value = input.read::<BoundedVec<U256, GetArrayLimit>>()?;
		let call_data =
			input.read::<BoundedVec<BoundedBytes<GetCallDataLimit>, GetArrayLimit>>()?;
		let gas_limit = input.read::<BoundedVec<u64, GetArrayLimit>>()?;
		log::trace!(target: "batch-precompile", "batch_some_until_failure\n to address(s) {:?}, value(s) {:?} call_data(s) {:?}, gas_limit(s) {:?}", to, value,call_data, gas_limit);
		Self::inner_batch(Mode::BatchSomeUntilFailure, handle, to, value, call_data, gas_limit)
	}

	fn batch_all(handle: &mut impl PrecompileHandle) -> EvmResult<PrecompileOutput> {
		let mut input = handle.read_input()?;
		input.expect_arguments(4)?;
		let to = input.read::<BoundedVec<Address, GetArrayLimit>>()?;
		let value = input.read::<BoundedVec<U256, GetArrayLimit>>()?;
		let call_data =
			input.read::<BoundedVec<BoundedBytes<GetCallDataLimit>, GetArrayLimit>>()?;
		let gas_limit = input.read::<BoundedVec<u64, GetArrayLimit>>()?;
		log::trace!(target: "batch-precompile", "batch_all\n to address(s) {:?}, value(s) {:?} call_data(s) {:?}, gas_limit(s) {:?}", to, value,call_data, gas_limit);
		Self::inner_batch(Mode::BatchAll, handle, to, value, call_data, gas_limit)
	}

	fn inner_batch(
		mode: Mode,
		handle: &mut impl PrecompileHandle,
		to: BoundedVec<Address, GetArrayLimit>,
		value: BoundedVec<U256, GetArrayLimit>,
		call_data: BoundedVec<BoundedBytes<GetCallDataLimit>, GetArrayLimit>,
		gas_limit: BoundedVec<u64, GetArrayLimit>,
	) -> EvmResult<PrecompileOutput> {
		let addresses = Vec::from(to).into_iter().enumerate();
		let values = Vec::from(value).into_iter().map(|x| Some(x)).chain(repeat(None));
		let calls_data =
			Vec::from(call_data).into_iter().map(|x| Some(x.into())).chain(repeat(None));
		let gas_limits = Vec::from(gas_limit).into_iter().map(|x|
			// x = 0 => forward all remaining gas
			if x == 0 {
				None
			} else {
				Some(x)
			}
		).chain(repeat(None));

		// Cost of batch log. (doesn't change when index changes)
		let log_cost = log_subcall_failed(handle.code_address(), 0)
			.compute_cost()
			.map_err(|_| revert("Failed to compute log cost"))?;

		for ((i, address), (value, (call_data, gas_limit))) in
			addresses.zip(values.zip(calls_data.zip(gas_limits)))
		{
			let address = address.0;
			let value = value.unwrap_or(U256::zero());
			let call_data = call_data.unwrap_or(vec![]);

			let sub_context = Context {
				caller: handle.context().caller,
				address: address.clone(),
				apparent_value: value,
			};

			let transfer = if value.is_zero() {
				None
			} else {
				Some(Transfer { source: handle.context().caller, target: address.clone(), value })
			};

			// We reserve enough gas to emit a final log and perform the subcall itself.
			// If not enough gas we stop there according to Mode strategy.
			let remaining_gas = handle.remaining_gas();

			let forwarded_gas = match (remaining_gas.checked_sub(log_cost), mode) {
				(Some(remaining), _) => remaining,
				(None, Mode::BatchAll) =>
					return Err(PrecompileFailure::Error { exit_status: ExitError::OutOfGas }),
				(None, _) => return Ok(succeed(EvmDataWriter::new().write(true).build())),
			};

			// Cost of the call itself that the batch precompile must pay.
			let call_cost = call_cost(value, <Runtime as pallet_evm::Config>::config());

			let forwarded_gas = match forwarded_gas.checked_sub(call_cost) {
				Some(remaining) => remaining,
				None => {
					let log = log_subcall_failed(handle.code_address(), i);
					handle.record_log_costs(&[&log])?;
					log.record(handle)?;

					match mode {
						Mode::BatchAll =>
							return Err(PrecompileFailure::Error {
								exit_status: ExitError::OutOfGas,
							}),
						Mode::BatchSomeUntilFailure =>
							return Ok(succeed(EvmDataWriter::new().write(true).build())),
						Mode::BatchSome => continue,
					}
				},
			};

			// If there is a provided gas limit we ensure there is enough gas remaining.
			let forwarded_gas = match gas_limit {
				None => forwarded_gas, // provide all gas if no gas limit,
				Some(limit) => {
					if limit > forwarded_gas {
						let log = log_subcall_failed(handle.code_address(), i);
						handle.record_log_costs(&[&log])?;
						log.record(handle)?;

						match mode {
							Mode::BatchAll =>
								return Err(PrecompileFailure::Error {
									exit_status: ExitError::OutOfGas,
								}),
							Mode::BatchSomeUntilFailure =>
								return Ok(succeed(EvmDataWriter::new().write(true).build())),
							Mode::BatchSome => continue,
						}
					}
					limit
				},
			};

			let (reason, output) =
				handle.call(address, transfer, call_data, Some(forwarded_gas), false, &sub_context);

			// Logs
			// We reserved enough gas so this should not OOG.
			match reason {
				ExitReason::Revert(_) | ExitReason::Error(_) => {
					let log = log_subcall_failed(handle.code_address(), i);
					handle.record_log_costs(&[&log])?;
					log.record(handle)?
				},
				ExitReason::Succeed(_) => {
					let log = log_subcall_succeeded(handle.code_address(), i);
					handle.record_log_costs(&[&log])?;
					log.record(handle)?
				},
				_ => (),
			}

			// How to proceed
			match (mode, reason) {
				// _: Fatal is always fatal
				(_, ExitReason::Fatal(exit_status)) =>
					return Err(PrecompileFailure::Fatal { exit_status }),

				// BatchAll : Reverts and errors are immediatly forwarded.
				(Mode::BatchAll, ExitReason::Revert(exit_status)) =>
					return Err(PrecompileFailure::Revert { exit_status, output }),
				(Mode::BatchAll, ExitReason::Error(exit_status)) =>
					return Err(PrecompileFailure::Error { exit_status }),

				// BatchSomeUntilFailure : Reverts and errors prevent subsequent subcalls to
				// be executed but the precompile still succeed.
				(Mode::BatchSomeUntilFailure, ExitReason::Revert(_) | ExitReason::Error(_)) =>
					return Ok(succeed(EvmDataWriter::new().write(true).build())),

				// Success or ignored revert/error.
				(_, _) => (),
			}
		}

		Ok(succeed(EvmDataWriter::new().write(true).build()))
	}
}
