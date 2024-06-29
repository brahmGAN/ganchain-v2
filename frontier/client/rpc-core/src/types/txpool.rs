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

use std::collections::HashMap;

use ethereum::{TransactionAction, TransactionV2 as EthereumTransaction};
use ethereum_types::{H160, H256, U256};
use serde::{Serialize, Serializer};

use crate::types::Bytes;

pub type TransactionMap<T> = HashMap<H160, HashMap<U256, T>>;

pub trait Get {
	fn get(hash: H256, from_address: H160, txn: &EthereumTransaction) -> Self;
}

#[derive(Debug, Serialize)]
pub struct TxPoolResult<T: Serialize> {
	pub pending: T,
	pub queued: T,
}

#[derive(Clone, Debug)]
pub struct Summary {
	pub to: Option<H160>,
	pub value: U256,
	pub gas: U256,
	pub gas_price: U256,
}

impl Serialize for Summary {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		let res = format!(
			"0x{:x}: {} wei + {} gas x {} wei",
			self.to.unwrap_or_default(),
			self.value,
			self.gas,
			self.gas_price
		);
		serializer.serialize_str(&res)
	}
}

impl Get for Summary {
	fn get(_hash: H256, _from_address: H160, txn: &EthereumTransaction) -> Self {
		let (action, value, gas_price, gas_limit) = match txn {
			EthereumTransaction::Legacy(t) => (t.action, t.value, t.gas_price, t.gas_limit),
			EthereumTransaction::EIP2930(t) => (t.action, t.value, t.gas_price, t.gas_limit),
			EthereumTransaction::EIP1559(t) => (t.action, t.value, t.max_fee_per_gas, t.gas_limit),
		};
		Self {
			to: match action {
				TransactionAction::Call(to) => Some(to),
				_ => None,
			},
			value,
			gas_price,
			gas: gas_limit,
		}
	}
}

#[derive(Debug, Default, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TxPoolTransaction {
	/// Hash
	pub hash: H256,
	/// Nonce
	pub nonce: U256,
	/// Block hash
	#[serde(serialize_with = "block_hash_serialize")]
	pub block_hash: Option<H256>,
	/// Block number
	pub block_number: Option<U256>,
	/// Sender
	pub from: H160,
	/// Recipient
	#[serde(serialize_with = "to_serialize")]
	pub to: Option<H160>,
	/// Transfered value
	pub value: U256,
	/// Gas Price
	pub gas_price: U256,
	/// Gas
	pub gas: U256,
	/// Data
	pub input: Bytes,
	/// Transaction Index
	pub transaction_index: Option<U256>,
}

fn block_hash_serialize<S>(hash: &Option<H256>, serializer: S) -> Result<S::Ok, S::Error>
where
	S: Serializer,
{
	serializer.serialize_str(&format!("0x{:x}", hash.unwrap_or_default()))
}

fn to_serialize<S>(hash: &Option<H160>, serializer: S) -> Result<S::Ok, S::Error>
where
	S: Serializer,
{
	serializer.serialize_str(&format!("0x{:x}", hash.unwrap_or_default()))
}

impl Get for TxPoolTransaction {
	fn get(hash: H256, from_address: H160, txn: &EthereumTransaction) -> Self {
		let (nonce, action, value, gas_price, gas_limit, input) = match txn {
			EthereumTransaction::Legacy(t) => (
				t.nonce,
				t.action,
				t.value,
				t.gas_price,
				t.gas_limit,
				t.input.clone(),
			),
			EthereumTransaction::EIP2930(t) => (
				t.nonce,
				t.action,
				t.value,
				t.gas_price,
				t.gas_limit,
				t.input.clone(),
			),
			EthereumTransaction::EIP1559(t) => (
				t.nonce,
				t.action,
				t.value,
				t.max_fee_per_gas,
				t.gas_limit,
				t.input.clone(),
			),
		};
		Self {
			hash,
			nonce,
			block_hash: None,
			block_number: None,
			from: from_address,
			to: match action {
				TransactionAction::Call(to) => Some(to),
				_ => None,
			},
			value,
			gas_price,
			gas: gas_limit,
			input: Bytes(input),
			transaction_index: None,
		}
	}
}
