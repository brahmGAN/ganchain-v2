// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
// This file is part of Frontier.
//
// Copyright (c) 2017-2022 Parity Technologies (UK) Ltd.
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

use std::{collections::BTreeMap, marker::PhantomData, sync::Arc};

use ethereum::BlockV2 as EthereumBlock;
use ethereum_types::{H160, H256, U256};
// Substrate
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_io::hashing::{blake2_128, twox_128};
use sp_runtime::{traits::Block as BlockT, Permill};
// Frontier
use fp_rpc::{EthereumRuntimeRPCApi, TransactionStatus};
use fp_storage::EthereumStorageSchema;

mod schema_v1_override;
mod schema_v2_override;
mod schema_v3_override;

pub use self::{
	schema_v1_override::SchemaV1Override, schema_v2_override::SchemaV2Override,
	schema_v3_override::SchemaV3Override,
};

pub struct OverrideHandle<Block: BlockT> {
	pub schemas: BTreeMap<EthereumStorageSchema, Box<dyn StorageOverride<Block>>>,
	pub fallback: Box<dyn StorageOverride<Block>>,
}

/// Something that can fetch Ethereum-related data. This trait is quite similar to the runtime API,
/// and indeed oe implementation of it uses the runtime API.
/// Having this trait is useful because it allows optimized implementations that fetch data from a
/// State Backend with some assumptions about pallet-ethereum's storage schema. Using such an
/// optimized implementation avoids spawning a runtime and the overhead associated with it.
pub trait StorageOverride<Block: BlockT>: Send + Sync {
	/// For a given account address, returns pallet_evm::AccountCodes.
	fn account_code_at(&self, block_hash: Block::Hash, address: H160) -> Option<Vec<u8>>;
	/// For a given account address and index, returns pallet_evm::AccountStorages.
	fn storage_at(&self, block_hash: Block::Hash, address: H160, index: U256) -> Option<H256>;
	/// Return the current block.
	fn current_block(&self, block_hash: Block::Hash) -> Option<EthereumBlock>;
	/// Return the current receipt.
	fn current_receipts(&self, block_hash: Block::Hash) -> Option<Vec<ethereum::ReceiptV3>>;
	/// Return the current transaction status.
	fn current_transaction_statuses(
		&self,
		block_hash: Block::Hash,
	) -> Option<Vec<TransactionStatus>>;
	/// Return the base fee at the given height.
	fn elasticity(&self, block_hash: Block::Hash) -> Option<Permill>;
	/// Return `true` if the request BlockId is post-eip1559.
	fn is_eip1559(&self, block_hash: Block::Hash) -> bool;
}

fn storage_prefix_build(module: &[u8], storage: &[u8]) -> Vec<u8> {
	[twox_128(module), twox_128(storage)].concat().to_vec()
}

fn blake2_128_extend(bytes: &[u8]) -> Vec<u8> {
	let mut ext: Vec<u8> = blake2_128(bytes).to_vec();
	ext.extend_from_slice(bytes);
	ext
}

/// A wrapper type for the Runtime API. This type implements `StorageOverride`, so it can be used
/// when calling the runtime API is desired but a `dyn StorageOverride` is required.
pub struct RuntimeApiStorageOverride<B: BlockT, C> {
	client: Arc<C>,
	_marker: PhantomData<B>,
}

impl<B: BlockT, C> RuntimeApiStorageOverride<B, C> {
	pub fn new(client: Arc<C>) -> Self {
		Self {
			client,
			_marker: PhantomData,
		}
	}
}

impl<Block, C> StorageOverride<Block> for RuntimeApiStorageOverride<Block, C>
where
	Block: BlockT,
	C: ProvideRuntimeApi<Block> + Send + Sync,
	C::Api: EthereumRuntimeRPCApi<Block>,
{
	/// For a given account address, returns pallet_evm::AccountCodes.
	fn account_code_at(&self, block_hash: Block::Hash, address: H160) -> Option<Vec<u8>> {
		self.client
			.runtime_api()
			.account_code_at(block_hash, address)
			.ok()
	}

	/// For a given account address and index, returns pallet_evm::AccountStorages.
	fn storage_at(&self, block_hash: Block::Hash, address: H160, index: U256) -> Option<H256> {
		self.client
			.runtime_api()
			.storage_at(block_hash, address, index)
			.ok()
	}

	/// Return the current block.
	fn current_block(&self, block_hash: Block::Hash) -> Option<ethereum::BlockV2> {
		let api = self.client.runtime_api();

		let api_version = if let Ok(Some(api_version)) =
			api.api_version::<dyn EthereumRuntimeRPCApi<Block>>(block_hash)
		{
			api_version
		} else {
			return None;
		};
		if api_version == 1 {
			#[allow(deprecated)]
			let old_block = api.current_block_before_version_2(block_hash).ok()?;
			old_block.map(|block| block.into())
		} else {
			api.current_block(block_hash).ok()?
		}
	}

	/// Return the current receipt.
	fn current_receipts(&self, block_hash: Block::Hash) -> Option<Vec<ethereum::ReceiptV3>> {
		let api = self.client.runtime_api();

		let api_version = if let Ok(Some(api_version)) =
			api.api_version::<dyn EthereumRuntimeRPCApi<Block>>(block_hash)
		{
			api_version
		} else {
			return None;
		};
		if api_version < 4 {
			#[allow(deprecated)]
			let old_receipts = api.current_receipts_before_version_4(block_hash).ok()?;
			old_receipts.map(|receipts| {
				receipts
					.into_iter()
					.map(|r| {
						ethereum::ReceiptV3::Legacy(ethereum::EIP658ReceiptData {
							status_code: r.state_root.to_low_u64_be() as u8,
							used_gas: r.used_gas,
							logs_bloom: r.logs_bloom,
							logs: r.logs,
						})
					})
					.collect()
			})
		} else {
			self.client
				.runtime_api()
				.current_receipts(block_hash)
				.ok()?
		}
	}

	/// Return the current transaction status.
	fn current_transaction_statuses(
		&self,
		block_hash: Block::Hash,
	) -> Option<Vec<TransactionStatus>> {
		self.client
			.runtime_api()
			.current_transaction_statuses(block_hash)
			.ok()?
	}

	/// Return the elasticity multiplier at the give post-eip1559 height.
	fn elasticity(&self, block_hash: Block::Hash) -> Option<Permill> {
		if self.is_eip1559(block_hash) {
			self.client.runtime_api().elasticity(block_hash).ok()?
		} else {
			None
		}
	}

	fn is_eip1559(&self, block_hash: Block::Hash) -> bool {
		if let Ok(Some(api_version)) = self
			.client
			.runtime_api()
			.api_version::<dyn EthereumRuntimeRPCApi<Block>>(block_hash)
		{
			return api_version >= 2;
		}
		false
	}
}
