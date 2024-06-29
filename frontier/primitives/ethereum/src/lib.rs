// SPDX-License-Identifier: Apache-2.0
// This file is part of Frontier.
//
// Copyright (c) 2020-2022 Parity Technologies (UK) Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![cfg_attr(not(feature = "std"), no_std)]
#![deny(unused_crate_dependencies)]

pub use ethereum::{
	AccessListItem, BlockV2 as Block, LegacyTransactionMessage, Log, ReceiptV3 as Receipt,
	TransactionAction, TransactionV2 as Transaction,
};
use ethereum_types::{H160, H256, U256};
use fp_evm::CheckEvmTransactionInput;
use scale_codec::{Decode, Encode};
use sp_std::vec::Vec;

#[repr(u8)]
#[derive(num_enum::FromPrimitive, num_enum::IntoPrimitive)]
pub enum TransactionValidationError {
	#[allow(dead_code)]
	#[num_enum(default)]
	UnknownError,
	InvalidChainId,
	InvalidSignature,
	GasLimitTooLow,
	GasLimitTooHigh,
	MaxFeePerGasTooLow,
}

pub trait ValidatedTransaction {
	fn apply(
		source: H160,
		transaction: Transaction,
	) -> frame_support::dispatch::DispatchResultWithPostInfo;
}

#[derive(Clone, Debug, Eq, PartialEq, Encode, Decode)]
pub struct TransactionData {
	pub action: TransactionAction,
	pub input: Vec<u8>,
	pub nonce: U256,
	pub gas_limit: U256,
	pub gas_price: Option<U256>,
	pub max_fee_per_gas: Option<U256>,
	pub max_priority_fee_per_gas: Option<U256>,
	pub value: U256,
	pub chain_id: Option<u64>,
	pub access_list: Vec<(H160, Vec<H256>)>,
}

impl From<TransactionData> for CheckEvmTransactionInput {
	fn from(t: TransactionData) -> Self {
		CheckEvmTransactionInput {
			to: if let TransactionAction::Call(to) = t.action {
				Some(to)
			} else {
				None
			},
			chain_id: t.chain_id,
			input: t.input,
			nonce: t.nonce,
			gas_limit: t.gas_limit,
			gas_price: t.gas_price,
			max_fee_per_gas: t.max_fee_per_gas,
			max_priority_fee_per_gas: t.max_priority_fee_per_gas,
			value: t.value,
			access_list: t.access_list,
		}
	}
}

impl From<&Transaction> for TransactionData {
	fn from(t: &Transaction) -> Self {
		match t {
			Transaction::Legacy(t) => TransactionData {
				action: t.action,
				input: t.input.clone(),
				nonce: t.nonce,
				gas_limit: t.gas_limit,
				gas_price: Some(t.gas_price),
				max_fee_per_gas: None,
				max_priority_fee_per_gas: None,
				value: t.value,
				chain_id: t.signature.chain_id(),
				access_list: Vec::new(),
			},
			Transaction::EIP2930(t) => TransactionData {
				action: t.action,
				input: t.input.clone(),
				nonce: t.nonce,
				gas_limit: t.gas_limit,
				gas_price: Some(t.gas_price),
				max_fee_per_gas: None,
				max_priority_fee_per_gas: None,
				value: t.value,
				chain_id: Some(t.chain_id),
				access_list: t
					.access_list
					.iter()
					.map(|d| (d.address, d.storage_keys.clone()))
					.collect(),
			},
			Transaction::EIP1559(t) => TransactionData {
				action: t.action,
				input: t.input.clone(),
				nonce: t.nonce,
				gas_limit: t.gas_limit,
				gas_price: None,
				max_fee_per_gas: Some(t.max_fee_per_gas),
				max_priority_fee_per_gas: Some(t.max_priority_fee_per_gas),
				value: t.value,
				chain_id: Some(t.chain_id),
				access_list: t
					.access_list
					.iter()
					.map(|d| (d.address, d.storage_keys.clone()))
					.collect(),
			},
		}
	}
}
