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

use std::{
	collections::BTreeMap,
	sync::{Arc, Mutex},
};

use ethereum_types::U256;
use serde::Serialize;

/// `eth_feeHistory` response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeHistory {
	/// Lowest number block of the returned range.
	pub oldest_block: U256,
	/// An array of block base fees per gas.
	/// This includes the next block after the newest of the returned range,
	/// because this value can be derived from the newest block. Zeroes are
	/// returned for pre-EIP-1559 blocks.
	pub base_fee_per_gas: Vec<U256>,
	/// An array of block gas used ratios. These are calculated as the ratio
	/// of gasUsed and gasLimit.
	pub gas_used_ratio: Vec<f64>,
	/// An array of effective priority fee per gas data points from a single
	/// block. All zeroes are returned if the block is empty.
	pub reward: Option<Vec<Vec<U256>>>,
}

pub type FeeHistoryCache = Arc<Mutex<BTreeMap<u64, FeeHistoryCacheItem>>>;
/// Maximum fee history cache size.
pub type FeeHistoryCacheLimit = u64;

pub struct FeeHistoryCacheItem {
	pub base_fee: u64,
	pub gas_used_ratio: f64,
	pub rewards: Vec<u64>,
}
