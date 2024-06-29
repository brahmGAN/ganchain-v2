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

use crate::testing::{decode_revert_message, MockHandle, PrettyLog, SubcallHandle, SubcallTrait};
use assert_matches::assert_matches;
use fp_evm::{
	Context, ExitError, ExitSucceed, Log, PrecompileFailure, PrecompileOutput, PrecompileResult,
	PrecompileSet,
};
use sp_core::{H160, U256};
use sp_std::boxed::Box;

#[must_use]
pub struct PrecompilesTester<'p, P> {
	precompiles: &'p P,
	handle: MockHandle,

	target_gas: Option<u64>,
	subcall_handle: Option<SubcallHandle>,

	expected_cost: Option<u64>,
	expected_logs: Option<Vec<PrettyLog>>,
}

impl<'p, P: PrecompileSet> PrecompilesTester<'p, P> {
	pub fn new(
		precompiles: &'p P,
		from: impl Into<H160>,
		to: impl Into<H160>,
		data: Vec<u8>,
	) -> Self {
		let to = to.into();
		let mut handle = MockHandle::new(
			to.clone(),
			Context { address: to, caller: from.into(), apparent_value: U256::zero() },
		);

		handle.input = data;

		Self {
			precompiles,
			handle,

			target_gas: None,
			subcall_handle: None,

			expected_cost: None,
			expected_logs: None,
		}
	}

	pub fn with_value(mut self, value: impl Into<U256>) -> Self {
		self.handle.context.apparent_value = value.into();
		self
	}

	pub fn with_subcall_handle(mut self, subcall_handle: impl SubcallTrait) -> Self {
		self.subcall_handle = Some(Box::new(subcall_handle));
		self
	}

	pub fn with_target_gas(mut self, target_gas: Option<u64>) -> Self {
		self.target_gas = target_gas;
		self
	}

	pub fn with_gas_limit(mut self, gas_limit: u64) -> Self {
		self.handle.gas_limit = gas_limit;
		self
	}

	pub fn expect_cost(mut self, cost: u64) -> Self {
		self.expected_cost = Some(cost);
		self
	}

	pub fn expect_no_logs(mut self) -> Self {
		self.expected_logs = Some(vec![]);
		self
	}

	pub fn expect_log(mut self, log: Log) -> Self {
		self.expected_logs = Some({
			let mut logs = self.expected_logs.unwrap_or_else(Vec::new);
			logs.push(PrettyLog(log));
			logs
		});
		self
	}

	fn assert_optionals(&self) {
		if let Some(cost) = &self.expected_cost {
			assert_eq!(&self.handle.gas_used, cost);
		}

		if let Some(logs) = &self.expected_logs {
			similar_asserts::assert_eq!(&self.handle.logs, logs);
		}
	}

	fn execute(&mut self) -> Option<PrecompileResult> {
		let handle = &mut self.handle;
		handle.subcall_handle = self.subcall_handle.take();

		if let Some(gas_limit) = self.target_gas {
			handle.gas_limit = gas_limit;
		}

		let res = self.precompiles.execute(handle);

		self.subcall_handle = handle.subcall_handle.take();

		res
	}

	/// Execute the precompile set and expect some precompile to have been executed, regardless of the
	/// result.
	pub fn execute_some(mut self) {
		let res = self.execute();
		assert!(res.is_some());
		self.assert_optionals();
	}

	/// Execute the precompile set and expect no precompile to have been executed.
	pub fn execute_none(mut self) {
		let res = self.execute();
		assert!(res.is_some());
		self.assert_optionals();
	}

	/// Execute the precompile set and check it returns provided output.
	pub fn execute_returns_raw(mut self, output: Vec<u8>) {
		let res = self.execute();

		match res {
			Some(Err(PrecompileFailure::Revert { output, .. })) => {
				let decoded = decode_revert_message(&output);
				eprintln!(
					"Revert message (bytes): {:?}",
					sp_core::hexdisplay::HexDisplay::from(&decoded)
				);
				eprintln!("Revert message (string): {:?}", core::str::from_utf8(decoded).ok());
				panic!("Shouldn't have reverted");
			},
			Some(Ok(PrecompileOutput {
				exit_status: ExitSucceed::Returned,
				output: execution_output,
			})) =>
				if execution_output != output {
					eprintln!(
						"Output (bytes): {:?}",
						sp_core::hexdisplay::HexDisplay::from(&execution_output)
					);
					eprintln!(
						"Output (string): {:?}",
						core::str::from_utf8(&execution_output).ok()
					);
					panic!("Output doesn't match");
				},
			other => panic!("Unexpected result: {:?}", other),
		}

		self.assert_optionals();
	}

	/// Execute the precompile set and check it returns provided Solidity encoded output.
	pub fn execute_returns(self, output: Vec<u8>) {
		self.execute_returns_raw(output)
	}

	/// Execute the precompile set and check if it reverts.
	/// Take a closure allowing to perform custom matching on the output.
	pub fn execute_reverts(mut self, check: impl Fn(&[u8]) -> bool) {
		let res = self.execute();
		assert_matches!(
			res,
			Some(Err(PrecompileFailure::Revert { output, ..}))
				if check(&output)
		);
		self.assert_optionals();
	}

	/// Execute the precompile set and check it returns provided output.
	pub fn execute_error(mut self, error: ExitError) {
		let res = self.execute();
		assert_eq!(res, Some(Err(PrecompileFailure::Error { exit_status: error })));
		self.assert_optionals();
	}
}

pub trait PrecompileTesterExt: PrecompileSet + Sized {
	fn prepare_test(
		&self,
		from: impl Into<H160>,
		to: impl Into<H160>,
		data: impl Into<Vec<u8>>,
	) -> PrecompilesTester<Self>;
}

impl<T: PrecompileSet> PrecompileTesterExt for T {
	fn prepare_test(
		&self,
		from: impl Into<H160>,
		to: impl Into<H160>,
		data: impl Into<Vec<u8>>,
	) -> PrecompilesTester<Self> {
		PrecompilesTester::new(self, from, to, data.into())
	}
}
