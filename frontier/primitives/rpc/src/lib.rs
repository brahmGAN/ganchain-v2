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
#![allow(clippy::too_many_arguments)]
#![deny(unused_crate_dependencies)]

use ethereum::Log;
use ethereum_types::Bloom;
use scale_codec::{Decode, Encode};
use scale_info::TypeInfo;
// Substrate
use sp_core::{H160, H256, U256};
use sp_runtime::{traits::Block as BlockT, Permill, RuntimeDebug};
use sp_state_machine::OverlayedChanges;
use sp_std::vec::Vec;

#[derive(Clone, Eq, PartialEq, Default, RuntimeDebug, Encode, Decode, TypeInfo)]
pub struct TransactionStatus {
	pub transaction_hash: H256,
	pub transaction_index: u32,
	pub from: H160,
	pub to: Option<H160>,
	pub contract_address: Option<H160>,
	pub logs: Vec<Log>,
	pub logs_bloom: Bloom,
}

#[derive(Eq, PartialEq, Clone, Encode, Decode, sp_runtime::RuntimeDebug)]
pub struct TxPoolResponse {
	pub ready: Vec<ethereum::TransactionV2>,
	pub future: Vec<ethereum::TransactionV2>,
}

pub trait RuntimeStorageOverride<B: BlockT, C>: Send + Sync {
	fn is_enabled() -> bool;

	fn set_overlayed_changes(
		client: &C,
		overlayed_changes: &mut OverlayedChanges,
		block: B::Hash,
		version: u32,
		address: H160,
		balance: Option<U256>,
		nonce: Option<U256>,
	);

	fn into_account_id_bytes(address: H160) -> Vec<u8>;
}

impl<B: BlockT, C> RuntimeStorageOverride<B, C> for () {
	fn is_enabled() -> bool {
		false
	}

	fn set_overlayed_changes(
		_client: &C,
		_overlayed_changes: &mut OverlayedChanges,
		_block: B::Hash,
		_version: u32,
		_address: H160,
		_balance: Option<U256>,
		_nonce: Option<U256>,
	) {
	}

	fn into_account_id_bytes(_address: H160) -> Vec<u8> {
		Vec::default()
	}
}

sp_api::decl_runtime_apis! {
	/// API necessary for Ethereum-compatibility layer.
	#[api_version(5)]
	pub trait EthereumRuntimeRPCApi {
		/// Returns runtime defined pallet_evm::ChainId.
		fn chain_id() -> u64;
		/// Returns pallet_evm::Accounts by address.
		fn account_basic(address: H160) -> fp_evm::Account;
		/// Returns FixedGasPrice::min_gas_price
		fn gas_price() -> U256;
		/// For a given account address, returns pallet_evm::AccountCodes.
		fn account_code_at(address: H160) -> Vec<u8>;
		/// Returns the converted FindAuthor::find_author authority id.
		fn author() -> H160;
		/// For a given account address and index, returns pallet_evm::AccountStorages.
		fn storage_at(address: H160, index: U256) -> H256;
		/// Returns a frame_ethereum::call response. If `estimate` is true,
		#[changed_in(2)]
		fn call(
			from: H160,
			to: H160,
			data: Vec<u8>,
			value: U256,
			gas_limit: U256,
			gas_price: Option<U256>,
			nonce: Option<U256>,
			estimate: bool,
		) -> Result<fp_evm::ExecutionInfo::<Vec<u8>>, sp_runtime::DispatchError>;
		#[changed_in(4)]
		fn call(
			from: H160,
			to: H160,
			data: Vec<u8>,
			value: U256,
			gas_limit: U256,
			max_fee_per_gas: Option<U256>,
			max_priority_fee_per_gas: Option<U256>,
			nonce: Option<U256>,
			estimate: bool,
		) -> Result<fp_evm::ExecutionInfo::<Vec<u8>>, sp_runtime::DispatchError>;
		#[changed_in(5)]
		fn call(
			from: H160,
			to: H160,
			data: Vec<u8>,
			value: U256,
			gas_limit: U256,
			max_fee_per_gas: Option<U256>,
			max_priority_fee_per_gas: Option<U256>,
			nonce: Option<U256>,
			estimate: bool,
			access_list: Option<Vec<(H160, Vec<H256>)>>,
		) -> Result<fp_evm::ExecutionInfo::<Vec<u8>>, sp_runtime::DispatchError>;
		fn call(
			from: H160,
			to: H160,
			data: Vec<u8>,
			value: U256,
			gas_limit: U256,
			max_fee_per_gas: Option<U256>,
			max_priority_fee_per_gas: Option<U256>,
			nonce: Option<U256>,
			estimate: bool,
			access_list: Option<Vec<(H160, Vec<H256>)>>,
		) -> Result<fp_evm::ExecutionInfoV2::<Vec<u8>>, sp_runtime::DispatchError>;
		/// Returns a frame_ethereum::create response.
		#[changed_in(2)]
		fn create(
			from: H160,
			data: Vec<u8>,
			value: U256,
			gas_limit: U256,
			gas_price: Option<U256>,
			nonce: Option<U256>,
			estimate: bool,
		) -> Result<fp_evm::ExecutionInfo::<H160>, sp_runtime::DispatchError>;
		#[changed_in(4)]
		fn create(
			from: H160,
			data: Vec<u8>,
			value: U256,
			gas_limit: U256,
			max_fee_per_gas: Option<U256>,
			max_priority_fee_per_gas: Option<U256>,
			nonce: Option<U256>,
			estimate: bool,
		) -> Result<fp_evm::ExecutionInfo::<H160>, sp_runtime::DispatchError>;
		#[changed_in(5)]
		fn create(
			from: H160,
			data: Vec<u8>,
			value: U256,
			gas_limit: U256,
			max_fee_per_gas: Option<U256>,
			max_priority_fee_per_gas: Option<U256>,
			nonce: Option<U256>,
			estimate: bool,
			access_list: Option<Vec<(H160, Vec<H256>)>>,
		) -> Result<fp_evm::ExecutionInfo::<H160>, sp_runtime::DispatchError>;
		fn create(
			from: H160,
			data: Vec<u8>,
			value: U256,
			gas_limit: U256,
			max_fee_per_gas: Option<U256>,
			max_priority_fee_per_gas: Option<U256>,
			nonce: Option<U256>,
			estimate: bool,
			access_list: Option<Vec<(H160, Vec<H256>)>>,
		) -> Result<fp_evm::ExecutionInfoV2::<H160>, sp_runtime::DispatchError>;
		/// Return the current block. Legacy.
		#[changed_in(2)]
		fn current_block() -> Option<ethereum::BlockV0>;
		/// Return the current block.
		fn current_block() -> Option<ethereum::BlockV2>;
		/// Return the current receipt.
		#[changed_in(4)]
		fn current_receipts() -> Option<Vec<ethereum::ReceiptV0>>;
		/// Return the current receipt.
		fn current_receipts() -> Option<Vec<ethereum::ReceiptV3>>;
		/// Return the current transaction status.
		fn current_transaction_statuses() -> Option<Vec<TransactionStatus>>;
		/// Return all the current data for a block in a single runtime call. Legacy.
		#[changed_in(2)]
		fn current_all() -> (
			Option<ethereum::BlockV0>,
			Option<Vec<ethereum::ReceiptV0>>,
			Option<Vec<TransactionStatus>>
		);
		/// Return all the current data for a block in a single runtime call.
		#[changed_in(4)]
		fn current_all() -> (
			Option<ethereum::BlockV2>,
			Option<Vec<ethereum::ReceiptV0>>,
			Option<Vec<TransactionStatus>>
		);
		fn current_all() -> (
			Option<ethereum::BlockV2>,
			Option<Vec<ethereum::ReceiptV3>>,
			Option<Vec<TransactionStatus>>
		);
		/// Receives a `Vec<OpaqueExtrinsic>` and filters all the ethereum transactions. Legacy.
		#[changed_in(2)]
		fn extrinsic_filter(
			xts: Vec<<Block as BlockT>::Extrinsic>,
		) -> Vec<ethereum::TransactionV0>;
		/// Receives a `Vec<OpaqueExtrinsic>` and filters all the ethereum transactions.
		fn extrinsic_filter(
			xts: Vec<<Block as BlockT>::Extrinsic>,
		) -> Vec<ethereum::TransactionV2>;
		/// Return the elasticity multiplier.
		fn elasticity() -> Option<Permill>;
		/// Used to determine if gas limit multiplier for non-transactional calls (eth_call/estimateGas)
		/// is supported.
		fn gas_limit_multiplier_support();
		/// Return the pending block.
		fn pending_block(
			xts: Vec<<Block as BlockT>::Extrinsic>,
		) -> (Option<ethereum::BlockV2>, Option<Vec<TransactionStatus>>);
	}

	#[api_version(2)]
	pub trait ConvertTransactionRuntimeApi {
		fn convert_transaction(transaction: ethereum::TransactionV2) -> <Block as BlockT>::Extrinsic;
		#[changed_in(2)]
		fn convert_transaction(transaction: ethereum::TransactionV0) -> <Block as BlockT>::Extrinsic;
	}
}

pub trait ConvertTransaction<E> {
	fn convert_transaction(&self, transaction: ethereum::TransactionV2) -> E;
}

// `NoTransactionConverter` is a non-instantiable type (an enum with no variants),
// so we are guaranteed at compile time that `NoTransactionConverter` can never be instantiated.
pub enum NoTransactionConverter {}
impl<E> ConvertTransaction<E> for NoTransactionConverter {
	// `convert_transaction` is a method taking `&self` as a parameter, so it can only be called via an instance of type Self,
	// so we are guaranteed at compile time that this method can never be called.
	fn convert_transaction(&self, _transaction: ethereum::TransactionV2) -> E {
		unreachable!()
	}
}
