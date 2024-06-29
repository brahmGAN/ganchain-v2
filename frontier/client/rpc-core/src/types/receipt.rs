// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
// This file is part of Frontier.
//
// Copyright (c) 2015-2022 Parity Technologies (UK) Ltd.
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use ethereum_types::{Bloom as H2048, H160, H256, U256, U64};
use serde::Serialize;

use crate::types::Log;

/// Receipt
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
	/// Transaction Hash
	pub transaction_hash: Option<H256>,
	/// Transaction index
	pub transaction_index: Option<U256>,
	/// Block hash
	pub block_hash: Option<H256>,
	/// Sender
	pub from: Option<H160>,
	/// Recipient
	pub to: Option<H160>,
	/// Block number
	pub block_number: Option<U256>,
	/// Cumulative gas used
	pub cumulative_gas_used: U256,
	/// Gas used
	pub gas_used: Option<U256>,
	/// Contract address
	pub contract_address: Option<H160>,
	/// Logs
	pub logs: Vec<Log>,
	/// State Root
	// NOTE(niklasad1): EIP98 makes this optional field, if it's missing then skip serializing it
	#[serde(skip_serializing_if = "Option::is_none", rename = "root")]
	pub state_root: Option<H256>,
	/// Logs bloom
	pub logs_bloom: H2048,
	/// Status code
	// NOTE(niklasad1): Unknown after EIP98 rules, if it's missing then skip serializing it
	#[serde(skip_serializing_if = "Option::is_none", rename = "status")]
	pub status_code: Option<U64>,
	/// Effective gas price. Pre-eip1559 this is just the gasprice. Post-eip1559 this is base fee + priority fee.
	pub effective_gas_price: U256,
	/// EIP-2718 type
	#[serde(rename = "type")]
	pub transaction_type: U256,
}
