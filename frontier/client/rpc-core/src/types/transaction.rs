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

use ethereum::{AccessListItem, TransactionV2};
use ethereum_types::{H160, H256, H512, U256, U64};
use serde::{ser::SerializeStruct, Serialize, Serializer};

use crate::types::Bytes;

/// Transaction
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Transaction {
	/// Hash
	pub hash: H256,
	/// Nonce
	pub nonce: U256,
	/// Block hash
	pub block_hash: Option<H256>,
	/// Block number
	pub block_number: Option<U256>,
	/// Transaction Index
	pub transaction_index: Option<U256>,
	/// Sender
	pub from: H160,
	/// Recipient
	pub to: Option<H160>,
	/// Transfered value
	pub value: U256,
	/// Gas Price
	#[serde(skip_serializing_if = "Option::is_none")]
	pub gas_price: Option<U256>,
	/// Max BaseFeePerGas the user is willing to pay.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub max_fee_per_gas: Option<U256>,
	/// The miner's tip.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub max_priority_fee_per_gas: Option<U256>,
	/// Gas
	pub gas: U256,
	/// Data
	pub input: Bytes,
	/// Creates contract
	pub creates: Option<H160>,
	/// Raw transaction data
	pub raw: Bytes,
	/// Public key of the signer.
	pub public_key: Option<H512>,
	/// The network id of the transaction, if any.
	pub chain_id: Option<U64>,
	/// The standardised V field of the signature (0 or 1).
	pub standard_v: U256,
	/// The standardised V field of the signature.
	pub v: U256,
	/// The R field of the signature.
	pub r: U256,
	/// The S field of the signature.
	pub s: U256,
	/// Pre-pay to warm storage access.
	#[cfg_attr(feature = "std", serde(skip_serializing_if = "Option::is_none"))]
	pub access_list: Option<Vec<AccessListItem>>,
	/// EIP-2718 type
	#[serde(rename = "type", skip_serializing_if = "Option::is_none")]
	pub transaction_type: Option<U256>,
}

impl From<TransactionV2> for Transaction {
	fn from(transaction: TransactionV2) -> Self {
		let serialized = ethereum::EnvelopedEncodable::encode(&transaction);
		let hash = transaction.hash();
		let raw = Bytes(serialized.to_vec());
		match transaction {
			TransactionV2::Legacy(t) => Transaction {
				hash,
				nonce: t.nonce,
				block_hash: None,
				block_number: None,
				transaction_index: None,
				from: H160::default(),
				to: None,
				value: t.value,
				gas_price: Some(t.gas_price),
				max_fee_per_gas: None,
				max_priority_fee_per_gas: None,
				gas: t.gas_limit,
				input: Bytes(t.clone().input),
				creates: None,
				raw,
				public_key: None,
				chain_id: t.signature.chain_id().map(U64::from),
				standard_v: U256::from(t.signature.standard_v()),
				v: U256::from(t.signature.v()),
				r: U256::from(t.signature.r().as_bytes()),
				s: U256::from(t.signature.s().as_bytes()),
				access_list: None,
				transaction_type: Some(U256::from(0)),
			},
			TransactionV2::EIP2930(t) => Transaction {
				hash,
				nonce: t.nonce,
				block_hash: None,
				block_number: None,
				transaction_index: None,
				from: H160::default(),
				to: None,
				value: t.value,
				gas_price: Some(t.gas_price),
				max_fee_per_gas: None,
				max_priority_fee_per_gas: None,
				gas: t.gas_limit,
				input: Bytes(t.clone().input),
				creates: None,
				raw,
				public_key: None,
				chain_id: Some(U64::from(t.chain_id)),
				standard_v: U256::from(t.odd_y_parity as u8),
				v: U256::from(t.odd_y_parity as u8),
				r: U256::from(t.r.as_bytes()),
				s: U256::from(t.s.as_bytes()),
				access_list: Some(t.access_list),
				transaction_type: Some(U256::from(1)),
			},
			TransactionV2::EIP1559(t) => Transaction {
				hash,
				nonce: t.nonce,
				block_hash: None,
				block_number: None,
				transaction_index: None,
				from: H160::default(),
				to: None,
				value: t.value,
				gas_price: None,
				max_fee_per_gas: Some(t.max_fee_per_gas),
				max_priority_fee_per_gas: Some(t.max_priority_fee_per_gas),
				gas: t.gas_limit,
				input: Bytes(t.clone().input),
				creates: None,
				raw,
				public_key: None,
				chain_id: Some(U64::from(t.chain_id)),
				standard_v: U256::from(t.odd_y_parity as u8),
				v: U256::from(t.odd_y_parity as u8),
				r: U256::from(t.r.as_bytes()),
				s: U256::from(t.s.as_bytes()),
				access_list: Some(t.access_list),
				transaction_type: Some(U256::from(2)),
			},
		}
	}
}

/// Local Transaction Status
#[derive(Debug)]
pub enum LocalTransactionStatus {
	/// Transaction is pending
	Pending,
	/// Transaction is in future part of the queue
	Future,
	/// Transaction was mined.
	Mined(Transaction),
	/// Transaction was removed from the queue, but not mined.
	Culled(Transaction),
	/// Transaction was dropped because of limit.
	Dropped(Transaction),
	/// Transaction was replaced by transaction with higher gas price.
	Replaced(Transaction, U256, H256),
	/// Transaction never got into the queue.
	Rejected(Transaction, String),
	/// Transaction is invalid.
	Invalid(Transaction),
	/// Transaction was canceled.
	Canceled(Transaction),
}

impl Serialize for LocalTransactionStatus {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		use self::LocalTransactionStatus::*;

		let elems = match *self {
			Pending | Future => 1,
			Mined(..) | Culled(..) | Dropped(..) | Invalid(..) | Canceled(..) => 2,
			Rejected(..) => 3,
			Replaced(..) => 4,
		};

		let status = "status";
		let transaction = "transaction";

		let mut struc = serializer.serialize_struct("LocalTransactionStatus", elems)?;
		match *self {
			Pending => struc.serialize_field(status, "pending")?,
			Future => struc.serialize_field(status, "future")?,
			Mined(ref tx) => {
				struc.serialize_field(status, "mined")?;
				struc.serialize_field(transaction, tx)?;
			}
			Culled(ref tx) => {
				struc.serialize_field(status, "culled")?;
				struc.serialize_field(transaction, tx)?;
			}
			Dropped(ref tx) => {
				struc.serialize_field(status, "dropped")?;
				struc.serialize_field(transaction, tx)?;
			}
			Canceled(ref tx) => {
				struc.serialize_field(status, "canceled")?;
				struc.serialize_field(transaction, tx)?;
			}
			Invalid(ref tx) => {
				struc.serialize_field(status, "invalid")?;
				struc.serialize_field(transaction, tx)?;
			}
			Rejected(ref tx, ref reason) => {
				struc.serialize_field(status, "rejected")?;
				struc.serialize_field(transaction, tx)?;
				struc.serialize_field("error", reason)?;
			}
			Replaced(ref tx, ref gas_price, ref hash) => {
				struc.serialize_field(status, "replaced")?;
				struc.serialize_field(transaction, tx)?;
				struc.serialize_field("hash", hash)?;
				struc.serialize_field("gasPrice", gas_price)?;
			}
		}

		struc.end()
	}
}

/// Geth-compatible output for eth_signTransaction method
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct RichRawTransaction {
	/// Raw transaction RLP
	pub raw: Bytes,
	/// Transaction details
	#[serde(rename = "tx")]
	pub transaction: Transaction,
}
