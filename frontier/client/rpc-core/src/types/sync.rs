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

use std::collections::BTreeMap;

use ethereum_types::{H512, U256};
use serde::{Serialize, Serializer};

/// Sync info
#[derive(Debug, Eq, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncInfo {
	/// Starting block
	pub starting_block: U256,
	/// Current block
	pub current_block: U256,
	/// Highest block seen so far
	pub highest_block: U256,
	/// Warp sync snapshot chunks total.
	pub warp_chunks_amount: Option<U256>,
	/// Warp sync snpashot chunks processed.
	pub warp_chunks_processed: Option<U256>,
}

/// Peers info
#[derive(Debug, Default, Serialize)]
pub struct Peers {
	/// Number of active peers
	pub active: usize,
	/// Number of connected peers
	pub connected: usize,
	/// Max number of peers
	pub max: u32,
	/// Detailed information on peers
	pub peers: Vec<PeerInfo>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum PeerCount {
	U32(u32),
	String(String),
}

/// Peer connection information
#[derive(Debug, Default, Serialize)]
pub struct PeerInfo {
	/// Public node id
	pub id: Option<String>,
	/// Node client ID
	pub name: String,
	/// Capabilities
	pub caps: Vec<String>,
	/// Network information
	pub network: PeerNetworkInfo,
	/// Protocols information
	pub protocols: PeerProtocolsInfo,
}

/// Peer network information
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerNetworkInfo {
	/// Remote endpoint address
	pub remote_address: String,
	/// Local endpoint address
	pub local_address: String,
}

/// Peer protocols information
#[derive(Debug, Default, Serialize)]
pub struct PeerProtocolsInfo {
	/// Ethereum protocol information
	pub eth: Option<EthProtocolInfo>,
	/// PIP protocol information.
	pub pip: Option<PipProtocolInfo>,
}

/// Peer Ethereum protocol information
#[derive(Debug, Default, Serialize)]
pub struct EthProtocolInfo {
	/// Negotiated ethereum protocol version
	pub version: u32,
	/// Peer total difficulty if known
	pub difficulty: Option<U256>,
	/// SHA3 of peer best block hash
	pub head: String,
}

/// Peer PIP protocol information
#[derive(Debug, Default, Serialize)]
pub struct PipProtocolInfo {
	/// Negotiated PIP protocol version
	pub version: u32,
	/// Peer total difficulty
	pub difficulty: U256,
	/// SHA3 of peer best block hash
	pub head: String,
}

/// Sync status
#[derive(Debug, Eq, PartialEq)]
pub enum SyncStatus {
	/// Info when syncing
	Info(SyncInfo),
	/// Not syncing
	None,
}

impl Serialize for SyncStatus {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		match *self {
			SyncStatus::Info(ref info) => info.serialize(serializer),
			SyncStatus::None => false.serialize(serializer),
		}
	}
}

/// Propagation statistics for pending transaction.
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionStats {
	/// Block no this transaction was first seen.
	pub first_seen: u64,
	/// Peers this transaction was propagated to with count.
	pub propagated_to: BTreeMap<H512, usize>,
}

/// Chain status.
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChainStatus {
	/// Describes the gap in the blockchain, if there is one: (first, last)
	pub block_gap: Option<(U256, U256)>,
}
