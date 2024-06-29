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

//! Pub-Sub types.

use ethereum_types::H256;
use serde::{de::Error, Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{from_value, Value};

use crate::types::{Filter, Log, RichHeader};

/// Subscription result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Result {
	/// New block header.
	Header(Box<RichHeader>),
	/// Log
	Log(Box<Log>),
	/// Transaction hash
	TransactionHash(H256),
	/// SyncStatus
	SyncState(PubSubSyncStatus),
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum PubSubSyncStatus {
	Simple(bool),
	Detailed(SyncStatusMetadata),
}

/// PubSbub sync status
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatusMetadata {
	pub syncing: bool,
	pub starting_block: u64,
	pub current_block: u64,
	#[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
	pub highest_block: Option<u64>,
}

impl Serialize for Result {
	fn serialize<S>(&self, serializer: S) -> ::std::result::Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		match *self {
			Result::Header(ref header) => header.serialize(serializer),
			Result::Log(ref log) => log.serialize(serializer),
			Result::TransactionHash(ref hash) => hash.serialize(serializer),
			Result::SyncState(ref sync) => sync.serialize(serializer),
		}
	}
}

/// Subscription kind.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
	/// New block headers subscription.
	NewHeads,
	/// Logs subscription.
	Logs,
	/// New Pending Transactions subscription.
	NewPendingTransactions,
	/// Node syncing status subscription.
	Syncing,
}

/// Subscription kind.
#[derive(Clone, Debug, Eq, PartialEq, Default, Hash)]
pub enum Params {
	/// No parameters passed.
	#[default]
	None,
	/// Log parameters.
	Logs(Filter),
}

impl<'a> Deserialize<'a> for Params {
	fn deserialize<D>(deserializer: D) -> ::std::result::Result<Params, D::Error>
	where
		D: Deserializer<'a>,
	{
		let v: Value = Deserialize::deserialize(deserializer)?;

		if v.is_null() {
			return Ok(Params::None);
		}

		from_value(v)
			.map(Params::Logs)
			.map_err(|e| D::Error::custom(format!("Invalid Pub-Sub parameters: {}", e)))
	}
}
