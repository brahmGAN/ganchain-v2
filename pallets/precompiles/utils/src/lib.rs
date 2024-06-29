// This file is part of Astar.

// Copyright 2019-2022 PureStake Inc.
// Copyright (C) 2022-2023 Stake Technologies Pte.Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Utils package, originally developed by Purestake Inc.
// Utils package used in Astar Network in terms of GPLv3.
//
// Utils is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Utils is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Utils.  If not, see <http://www.gnu.org/licenses/>.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use crate::alloc::borrow::ToOwned;
pub use alloc::string::String;
use fp_evm::{
	Context, ExitError, ExitRevert, ExitSucceed, PrecompileFailure, PrecompileHandle,
	PrecompileOutput,
};
use frame_support::{
	dispatch::{Dispatchable, GetDispatchInfo, PostDispatchInfo},
	pallet_prelude::Weight,
	traits::Get,
};
use pallet_evm::{GasWeightMapping, Log};
use sp_core::{H160, H256, U256};
use sp_std::{marker::PhantomData, vec, vec::Vec};

pub mod bytes;
pub mod data;

pub use data::{Address, Bytes, EvmData, EvmDataReader, EvmDataWriter};
pub use precompile_utils_macro::{generate_function_selector, keccak256};

#[cfg(feature = "testing")]
pub mod testing;
#[cfg(test)]
mod tests;

/// Alias for Result returning an EVM precompile error.
pub type EvmResult<T = ()> = Result<T, PrecompileFailure>;

/// Return an error with provided (static) text.
/// Using the `revert` function of `Gasometer` is preferred as erroring
/// consumed all the gas limit and the error message is not easily
/// retrievable.
pub fn error<T: Into<alloc::borrow::Cow<'static, str>>>(text: T) -> PrecompileFailure {
	PrecompileFailure::Error { exit_status: ExitError::Other(text.into()) }
}

/// Builder for PrecompileOutput.
#[derive(Clone, Debug)]
pub struct LogsBuilder {
	address: H160,
}

impl LogsBuilder {
	/// Create a new builder with no logs.
	/// Takes the address of the precompile (usually `context.address`).
	pub fn new(address: H160) -> Self {
		Self { address }
	}

	/// Create a 0-topic log.
	#[must_use]
	pub fn log0(&self, data: impl Into<Vec<u8>>) -> Log {
		Log { address: self.address, topics: vec![], data: data.into() }
	}

	/// Create a 1-topic log.
	#[must_use]
	pub fn log1(&self, topic0: impl Into<H256>, data: impl Into<Vec<u8>>) -> Log {
		Log { address: self.address, topics: vec![topic0.into()], data: data.into() }
	}

	/// Create a 2-topics log.
	#[must_use]
	pub fn log2(
		&self,
		topic0: impl Into<H256>,
		topic1: impl Into<H256>,
		data: impl Into<Vec<u8>>,
	) -> Log {
		Log { address: self.address, topics: vec![topic0.into(), topic1.into()], data: data.into() }
	}

	/// Create a 3-topics log.
	#[must_use]
	pub fn log3(
		&self,
		topic0: impl Into<H256>,
		topic1: impl Into<H256>,
		topic2: impl Into<H256>,
		data: impl Into<Vec<u8>>,
	) -> Log {
		Log {
			address: self.address,
			topics: vec![topic0.into(), topic1.into(), topic2.into()],
			data: data.into(),
		}
	}

	/// Create a 4-topics log.
	#[must_use]
	pub fn log4(
		&self,
		topic0: impl Into<H256>,
		topic1: impl Into<H256>,
		topic2: impl Into<H256>,
		topic3: impl Into<H256>,
		data: impl Into<Vec<u8>>,
	) -> Log {
		Log {
			address: self.address,
			topics: vec![topic0.into(), topic1.into(), topic2.into(), topic3.into()],
			data: data.into(),
		}
	}
}

/// Extension trait allowing to record logs into a PrecompileHandle.
pub trait LogExt {
	fn record(self, handle: &mut impl PrecompileHandle) -> EvmResult;

	fn compute_cost(&self) -> EvmResult<u64>;
}

impl LogExt for Log {
	fn record(self, handle: &mut impl PrecompileHandle) -> EvmResult {
		handle.log(self.address, self.topics, self.data)?;
		Ok(())
	}

	fn compute_cost(&self) -> EvmResult<u64> {
		log_costs(self.topics.len(), self.data.len())
	}
}

/// Helper functions requiring a Runtime.
/// This runtime must of course implement `pallet_evm::Config`.
#[derive(Clone, Copy, Debug)]
pub struct RuntimeHelper<Runtime>(PhantomData<Runtime>);

impl<Runtime> RuntimeHelper<Runtime>
where
	Runtime: pallet_evm::Config,
	Runtime::RuntimeCall: Dispatchable<PostInfo = PostDispatchInfo> + GetDispatchInfo,
{
	#[inline(always)]
	pub fn record_weight_v2_cost(
		handle: &mut impl PrecompileHandle,
		weight: Weight,
	) -> Result<(), ExitError> {
		// Make sure there is enough gas.
		let remaining_gas = handle.remaining_gas();
		let required_gas = Runtime::GasWeightMapping::weight_to_gas(weight);
		if required_gas > remaining_gas {
			return Err(ExitError::OutOfGas)
		}

		// Make sure there is enough remaining weight
		handle.record_external_cost(None, Some(weight.proof_size()))
	}

	#[inline(always)]
	pub fn refund_weight_v2_cost(
		handle: &mut impl PrecompileHandle,
		weight: Weight,
		maybe_actual_weight: Option<Weight>,
	) -> Result<u64, ExitError> {
		// Refund weights and compute used weight them record used gas
		let used_weight = if let Some(actual_weight) = maybe_actual_weight {
			let refund_weight = weight - actual_weight;
			handle.refund_external_cost(None, Some(refund_weight.proof_size()));
			actual_weight
		} else {
			weight
		};
		let used_gas = Runtime::GasWeightMapping::weight_to_gas(used_weight);
		handle.record_cost(used_gas)?;
		Ok(used_gas)
	}

	/// Try to dispatch a Substrate call.
	/// Return an error if there are not enough gas, or if the call fails.
	/// If successful returns the used gas using the Runtime GasWeightMapping.
	pub fn try_dispatch<Call>(
		handle: &mut impl PrecompileHandleExt,
		origin: <Runtime::RuntimeCall as Dispatchable>::RuntimeOrigin,
		call: Call,
	) -> EvmResult<()>
	where
		Runtime::RuntimeCall: From<Call>,
	{
		let call = Runtime::RuntimeCall::from(call);
		let dispatch_info = call.get_dispatch_info();

		// Make sure there is enough gas.
		Self::record_weight_v2_cost(handle, dispatch_info.weight)?;

		// Dispatch call.
		// It may be possible to not record gas cost if the call returns Pays::No.
		// However while Substrate handle checking weight while not making the sender pay for it,
		// the EVM doesn't. It seems this safer to always record the costs to avoid unmetered
		// computations.
		let post_dispatch_info = call
			.dispatch(origin)
			.map_err(|e| revert(alloc::format!("Dispatched call failed with error: {:?}", e)))?;

		Self::refund_weight_v2_cost(
			handle,
			dispatch_info.weight,
			post_dispatch_info.actual_weight,
		)?;

		Ok(())
	}
}

impl<Runtime> RuntimeHelper<Runtime>
where
	Runtime: pallet_evm::Config,
{
	/// Cost of a Substrate DB write in gas.
	pub fn db_write_gas_cost() -> u64 {
		<Runtime as pallet_evm::Config>::GasWeightMapping::weight_to_gas(
			<Runtime as frame_system::Config>::DbWeight::get().writes(1),
		)
	}

	/// Cost of a Substrate DB read in gas.
	pub fn db_read_gas_cost() -> u64 {
		<Runtime as pallet_evm::Config>::GasWeightMapping::weight_to_gas(
			<Runtime as frame_system::Config>::DbWeight::get().reads(1),
		)
	}
}

/// Represents modifiers a Solidity function can be annotated with.
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum FunctionModifier {
	/// Function that doesn't modify the state.
	View,
	/// Function that modifies the state but refuse receiving funds.
	/// Correspond to a Solidity function with no modifiers.
	NonPayable,
	/// Function that modifies the state and accept funds.
	Payable,
}

pub trait PrecompileHandleExt: PrecompileHandle {
	#[must_use]
	/// Record cost of a log manually.
	/// This can be useful to record log costs early when their content have static size.
	fn record_log_costs_manual(&mut self, topics: usize, data_len: usize) -> EvmResult;

	#[must_use]
	/// Record cost of logs.
	fn record_log_costs(&mut self, logs: &[&Log]) -> EvmResult;

	#[must_use]
	/// Check that a function call is compatible with the context it is
	/// called into.
	fn check_function_modifier(&self, modifier: FunctionModifier) -> EvmResult;

	#[must_use]
	/// Read the selector from the input data.
	fn read_selector<T>(&self) -> EvmResult<T>
	where
		T: num_enum::TryFromPrimitive<Primitive = u32>;

	#[must_use]
	/// Returns a reader of the input, skipping the selector.
	fn read_input(&self) -> EvmResult<EvmDataReader>;

	/// Record cost of one DB read manually.
	/// The expected key & value data length should be provided.
	#[must_use]
	fn record_db_read<Runtime: pallet_evm::Config>(
		&mut self,
		data_length: usize,
	) -> Result<(), ExitError>;
}

pub fn log_costs(topics: usize, data_len: usize) -> EvmResult<u64> {
	// Cost calculation is copied from EVM code that is not publicly exposed by the crates.
	// https://github.com/rust-blockchain/evm/blob/master/gasometer/src/costs.rs#L148

	const G_LOG: u64 = 375;
	const G_LOGDATA: u64 = 8;
	const G_LOGTOPIC: u64 = 375;

	let topic_cost = G_LOGTOPIC
		.checked_mul(topics as u64)
		.ok_or(PrecompileFailure::Error { exit_status: ExitError::OutOfGas })?;

	let data_cost = G_LOGDATA
		.checked_mul(data_len as u64)
		.ok_or(PrecompileFailure::Error { exit_status: ExitError::OutOfGas })?;

	G_LOG
		.checked_add(topic_cost)
		.ok_or(PrecompileFailure::Error { exit_status: ExitError::OutOfGas })?
		.checked_add(data_cost)
		.ok_or(PrecompileFailure::Error { exit_status: ExitError::OutOfGas })
}

// Compute the cost of doing a subcall.
// Some parameters cannot be known in advance, so we estimate the worst possible cost.
pub fn call_cost(value: U256, config: &evm::Config) -> u64 {
	// Copied from EVM code since not public.
	pub const G_CALLVALUE: u64 = 9000;
	pub const G_NEWACCOUNT: u64 = 25000;

	fn address_access_cost(is_cold: bool, regular_value: u64, config: &evm::Config) -> u64 {
		if config.increase_state_access_gas {
			if is_cold {
				config.gas_account_access_cold
			} else {
				config.gas_storage_read_warm
			}
		} else {
			regular_value
		}
	}

	fn xfer_cost(is_call_or_callcode: bool, transfers_value: bool) -> u64 {
		if is_call_or_callcode && transfers_value {
			G_CALLVALUE
		} else {
			0
		}
	}

	fn new_cost(
		is_call_or_staticcall: bool,
		new_account: bool,
		transfers_value: bool,
		config: &evm::Config,
	) -> u64 {
		let eip161 = !config.empty_considered_exists;
		if is_call_or_staticcall {
			if eip161 {
				if transfers_value && new_account {
					G_NEWACCOUNT
				} else {
					0
				}
			} else if new_account {
				G_NEWACCOUNT
			} else {
				0
			}
		} else {
			0
		}
	}

	let transfers_value = value != U256::default();
	let is_cold = true;
	let is_call_or_callcode = true;
	let is_call_or_staticcall = true;
	let new_account = true;

	address_access_cost(is_cold, config.gas_call, config) +
		xfer_cost(is_call_or_callcode, transfers_value) +
		new_cost(is_call_or_staticcall, new_account, transfers_value, config)
}

impl<T: PrecompileHandle> PrecompileHandleExt for T {
	#[must_use]
	/// Record cost of a log manualy.
	/// This can be useful to record log costs early when their content have static size.
	fn record_log_costs_manual(&mut self, topics: usize, data_len: usize) -> EvmResult {
		self.record_cost(log_costs(topics, data_len)?)?;

		Ok(())
	}

	#[must_use]
	/// Record cost of logs.
	fn record_log_costs(&mut self, logs: &[&Log]) -> EvmResult {
		for log in logs {
			self.record_log_costs_manual(log.topics.len(), log.data.len())?;
		}

		Ok(())
	}

	#[must_use]
	/// Check that a function call is compatible with the context it is
	/// called into.
	fn check_function_modifier(&self, modifier: FunctionModifier) -> EvmResult {
		check_function_modifier(self.context(), self.is_static(), modifier)
	}

	#[must_use]
	/// Read the selector from the input data.
	fn read_selector<S>(&self) -> EvmResult<S>
	where
		S: num_enum::TryFromPrimitive<Primitive = u32>,
	{
		EvmDataReader::read_selector(self.input())
	}

	#[must_use]
	/// Returns a reader of the input, skipping the selector.
	fn read_input(&self) -> EvmResult<EvmDataReader> {
		EvmDataReader::new_skip_selector(self.input())
	}

	#[must_use]
	fn record_db_read<Runtime: pallet_evm::Config>(
		&mut self,
		data_length: usize,
	) -> Result<(), ExitError> {
		self.record_cost(RuntimeHelper::<Runtime>::db_read_gas_cost())?;
		self.record_external_cost(None, Some(data_length as u64))
	}
}

#[must_use]
pub fn revert(output: impl AsRef<[u8]>) -> PrecompileFailure {
	PrecompileFailure::Revert {
		exit_status: ExitRevert::Reverted,
		output: output.as_ref().to_owned(),
	}
}

#[must_use]
pub fn succeed(output: impl AsRef<[u8]>) -> PrecompileOutput {
	PrecompileOutput { exit_status: ExitSucceed::Returned, output: output.as_ref().to_owned() }
}

#[must_use]
/// Check that a function call is compatible with the context it is
/// called into.
pub fn check_function_modifier(
	context: &Context,
	is_static: bool,
	modifier: FunctionModifier,
) -> EvmResult {
	if is_static && modifier != FunctionModifier::View {
		return Err(revert("can't call non-static function in static context"))
	}

	if modifier != FunctionModifier::Payable && context.apparent_value > U256::zero() {
		return Err(revert("function is not payable"))
	}

	Ok(())
}
