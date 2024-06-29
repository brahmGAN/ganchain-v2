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

use crate::testing::PrettyLog;
use evm::{ExitRevert, ExitSucceed};
use fp_evm::{Context, ExitError, ExitReason, Log, PrecompileHandle, Transfer};
use sp_core::{H160, H256};
use sp_std::boxed::Box;

#[derive(Debug, Clone)]
pub struct Subcall {
	pub address: H160,
	pub transfer: Option<Transfer>,
	pub input: Vec<u8>,
	pub target_gas: Option<u64>,
	pub is_static: bool,
	pub context: Context,
}

#[derive(Debug, Clone)]
pub struct SubcallOutput {
	pub reason: ExitReason,
	pub output: Vec<u8>,
	pub cost: u64,
	pub logs: Vec<Log>,
}

impl SubcallOutput {
	pub fn revert() -> Self {
		Self {
			reason: ExitReason::Revert(ExitRevert::Reverted),
			output: Vec::new(),
			cost: 0,
			logs: Vec::new(),
		}
	}

	pub fn succeed() -> Self {
		Self {
			reason: ExitReason::Succeed(ExitSucceed::Returned),
			output: Vec::new(),
			cost: 0,
			logs: Vec::new(),
		}
	}

	pub fn out_of_gas() -> Self {
		Self {
			reason: ExitReason::Error(ExitError::OutOfGas),
			output: Vec::new(),
			cost: 0,
			logs: Vec::new(),
		}
	}
}

pub trait SubcallTrait: FnMut(Subcall) -> SubcallOutput + 'static {}

impl<T: FnMut(Subcall) -> SubcallOutput + 'static> SubcallTrait for T {}

pub type SubcallHandle = Box<dyn SubcallTrait>;

/// Mock handle to write tests for precompiles.
pub struct MockHandle {
	pub gas_limit: u64,
	pub gas_used: u64,
	pub logs: Vec<PrettyLog>,
	pub subcall_handle: Option<SubcallHandle>,
	pub code_address: H160,
	pub input: Vec<u8>,
	pub context: Context,
	pub is_static: bool,
}

impl MockHandle {
	pub fn new(code_address: H160, context: Context) -> Self {
		Self {
			gas_limit: u64::MAX,
			gas_used: 0,
			logs: vec![],
			subcall_handle: None,
			code_address,
			input: Vec::new(),
			context,
			is_static: false,
		}
	}
}

impl PrecompileHandle for MockHandle {
	/// Perform subcall in provided context.
	/// Precompile specifies in which context the subcall is executed.
	fn call(
		&mut self,
		address: H160,
		transfer: Option<Transfer>,
		input: Vec<u8>,
		target_gas: Option<u64>,
		is_static: bool,
		context: &Context,
	) -> (ExitReason, Vec<u8>) {
		if self
			.record_cost(crate::call_cost(context.apparent_value, &evm::Config::london()))
			.is_err()
		{
			return (ExitReason::Error(ExitError::OutOfGas), vec![])
		}

		match &mut self.subcall_handle {
			Some(handle) => {
				let SubcallOutput { reason, output, cost, logs } = handle(Subcall {
					address,
					transfer,
					input,
					target_gas,
					is_static,
					context: context.clone(),
				});

				if self.record_cost(cost).is_err() {
					return (ExitReason::Error(ExitError::OutOfGas), vec![])
				}

				for log in logs {
					self.log(log.address, log.topics, log.data).expect("cannot fail");
				}

				(reason, output)
			},
			None => panic!("no subcall handle registered"),
		}
	}

	fn record_cost(&mut self, cost: u64) -> Result<(), ExitError> {
		self.gas_used += cost;

		if self.gas_used > self.gas_limit {
			Err(ExitError::OutOfGas)
		} else {
			Ok(())
		}
	}

	fn record_external_cost(
		&mut self,
		_ref_time: Option<u64>,
		_proof_size: Option<u64>,
	) -> Result<(), ExitError> {
		Ok(())
	}
	fn refund_external_cost(&mut self, _ref_time: Option<u64>, _proof_size: Option<u64>) {}

	fn remaining_gas(&self) -> u64 {
		self.gas_limit - self.gas_used
	}

	fn log(&mut self, address: H160, topics: Vec<H256>, data: Vec<u8>) -> Result<(), ExitError> {
		self.logs.push(PrettyLog(Log { address, topics, data }));
		Ok(())
	}

	/// Retreive the code address (what is the address of the precompile being called).
	fn code_address(&self) -> H160 {
		self.code_address
	}

	/// Retreive the input data the precompile is called with.
	fn input(&self) -> &[u8] {
		&self.input
	}

	/// Retreive the context in which the precompile is executed.
	fn context(&self) -> &Context {
		&self.context
	}

	/// Is the precompile call is done statically.
	fn is_static(&self) -> bool {
		self.is_static
	}

	/// Retreive the gas limit of this call.
	fn gas_limit(&self) -> Option<u64> {
		Some(self.gas_limit)
	}
}
