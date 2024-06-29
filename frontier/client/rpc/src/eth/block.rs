// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
// This file is part of Frontier.
//
// Copyright (c) 2022 Parity Technologies (UK) Ltd.
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

use std::sync::Arc;

use ethereum_types::{H256, U256};
use jsonrpsee::core::RpcResult;
// Substrate
use sc_client_api::backend::{Backend, StorageProvider};
use sc_transaction_pool::ChainApi;
use sc_transaction_pool_api::InPoolTransaction;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_core::hashing::keccak_256;
use sp_runtime::traits::Block as BlockT;
// Frontier
use fc_rpc_core::types::*;
use fp_rpc::EthereumRuntimeRPCApi;

use crate::{
	eth::{rich_block_build, Eth, EthConfig},
	frontier_backend_client, internal_err,
};

impl<B, C, P, CT, BE, A: ChainApi, EC: EthConfig<B, C>> Eth<B, C, P, CT, BE, A, EC>
where
	B: BlockT,
	C: ProvideRuntimeApi<B>,
	C::Api: EthereumRuntimeRPCApi<B>,
	C: HeaderBackend<B> + StorageProvider<B, BE> + 'static,
	BE: Backend<B>,
	A: ChainApi<Block = B> + 'static,
{
	pub async fn block_by_hash(&self, hash: H256, full: bool) -> RpcResult<Option<RichBlock>> {
		let client = Arc::clone(&self.client);
		let block_data_cache = Arc::clone(&self.block_data_cache);
		let backend = Arc::clone(&self.backend);

		let substrate_hash = match frontier_backend_client::load_hash::<B, C>(
			client.as_ref(),
			backend.as_ref(),
			hash,
		)
		.await
		.map_err(|err| internal_err(format!("{:?}", err)))?
		{
			Some(hash) => hash,
			_ => return Ok(None),
		};

		let schema = fc_storage::onchain_storage_schema(client.as_ref(), substrate_hash);

		let block = block_data_cache.current_block(schema, substrate_hash).await;
		let statuses = block_data_cache
			.current_transaction_statuses(schema, substrate_hash)
			.await;

		let base_fee = client.runtime_api().gas_price(substrate_hash).ok();

		match (block, statuses) {
			(Some(block), Some(statuses)) => {
				let mut rich_block = rich_block_build(
					block,
					statuses.into_iter().map(Option::Some).collect(),
					Some(hash),
					full,
					base_fee,
					false,
				);

				let substrate_hash = H256::from_slice(substrate_hash.as_ref());
				if let Some(parent_hash) = self
					.forced_parent_hashes
					.as_ref()
					.and_then(|parent_hashes| parent_hashes.get(&substrate_hash).cloned())
				{
					rich_block.inner.header.parent_hash = parent_hash
				}

				Ok(Some(rich_block))
			}
			_ => Ok(None),
		}
	}

	pub async fn block_by_number(
		&self,
		number: BlockNumber,
		full: bool,
	) -> RpcResult<Option<RichBlock>> {
		let client = Arc::clone(&self.client);
		let block_data_cache = Arc::clone(&self.block_data_cache);
		let backend = Arc::clone(&self.backend);
		let graph = Arc::clone(&self.graph);

		match frontier_backend_client::native_block_id::<B, C>(
			client.as_ref(),
			backend.as_ref(),
			Some(number),
		)
		.await?
		{
			Some(id) => {
				let substrate_hash = client
					.expect_block_hash_from_id(&id)
					.map_err(|_| internal_err(format!("Expect block number from id: {}", id)))?;

				let schema = fc_storage::onchain_storage_schema(client.as_ref(), substrate_hash);

				let block = block_data_cache.current_block(schema, substrate_hash).await;
				let statuses = block_data_cache
					.current_transaction_statuses(schema, substrate_hash)
					.await;

				let base_fee = client.runtime_api().gas_price(substrate_hash).ok();

				match (block, statuses) {
					(Some(block), Some(statuses)) => {
						let hash = H256::from(keccak_256(&rlp::encode(&block.header)));
						let mut rich_block = rich_block_build(
							block,
							statuses.into_iter().map(Option::Some).collect(),
							Some(hash),
							full,
							base_fee,
							false,
						);

						let substrate_hash = H256::from_slice(substrate_hash.as_ref());
						if let Some(parent_hash) = self
							.forced_parent_hashes
							.as_ref()
							.and_then(|parent_hashes| parent_hashes.get(&substrate_hash).cloned())
						{
							rich_block.inner.header.parent_hash = parent_hash
						}

						Ok(Some(rich_block))
					}
					_ => Ok(None),
				}
			}
			None if number == BlockNumber::Pending => {
				let api = client.runtime_api();
				let best_hash = client.info().best_hash;

				// Get current in-pool transactions
				let mut xts: Vec<<B as BlockT>::Extrinsic> = Vec::new();
				// ready validated pool
				xts.extend(
					graph
						.validated_pool()
						.ready()
						.map(|in_pool_tx| in_pool_tx.data().clone())
						.collect::<Vec<<B as BlockT>::Extrinsic>>(),
				);

				// future validated pool
				xts.extend(
					graph
						.validated_pool()
						.futures()
						.iter()
						.map(|(_hash, extrinsic)| extrinsic.clone())
						.collect::<Vec<<B as BlockT>::Extrinsic>>(),
				);

				let (block, statuses) = api
					.pending_block(best_hash, xts)
					.map_err(|_| internal_err(format!("Runtime access error at {}", best_hash)))?;

				let base_fee = api.gas_price(best_hash).ok();

				match (block, statuses) {
					(Some(block), Some(statuses)) => Ok(Some(rich_block_build(
						block,
						statuses.into_iter().map(Option::Some).collect(),
						None,
						full,
						base_fee,
						true,
					))),
					_ => Ok(None),
				}
			}
			None => Ok(None),
		}
	}

	pub async fn block_transaction_count_by_hash(&self, hash: H256) -> RpcResult<Option<U256>> {
		let substrate_hash = match frontier_backend_client::load_hash::<B, C>(
			self.client.as_ref(),
			self.backend.as_ref(),
			hash,
		)
		.await
		.map_err(|err| internal_err(format!("{:?}", err)))?
		{
			Some(hash) => hash,
			_ => return Ok(None),
		};
		let schema = fc_storage::onchain_storage_schema(self.client.as_ref(), substrate_hash);
		let block = self
			.overrides
			.schemas
			.get(&schema)
			.unwrap_or(&self.overrides.fallback)
			.current_block(substrate_hash);

		match block {
			Some(block) => Ok(Some(U256::from(block.transactions.len()))),
			None => Ok(None),
		}
	}

	pub async fn block_transaction_count_by_number(
		&self,
		number: BlockNumber,
	) -> RpcResult<Option<U256>> {
		if let BlockNumber::Pending = number {
			// get the pending transactions count
			return Ok(Some(U256::from(
				self.graph.validated_pool().ready().count(),
			)));
		}

		let id = match frontier_backend_client::native_block_id::<B, C>(
			self.client.as_ref(),
			self.backend.as_ref(),
			Some(number),
		)
		.await?
		{
			Some(id) => id,
			None => return Ok(None),
		};
		let substrate_hash = self
			.client
			.expect_block_hash_from_id(&id)
			.map_err(|_| internal_err(format!("Expect block number from id: {}", id)))?;
		let schema = fc_storage::onchain_storage_schema(self.client.as_ref(), substrate_hash);
		let block = self
			.overrides
			.schemas
			.get(&schema)
			.unwrap_or(&self.overrides.fallback)
			.current_block(substrate_hash);

		match block {
			Some(block) => Ok(Some(U256::from(block.transactions.len()))),
			None => Ok(None),
		}
	}

	pub fn block_uncles_count_by_hash(&self, _: H256) -> RpcResult<U256> {
		Ok(U256::zero())
	}

	pub fn block_uncles_count_by_number(&self, _: BlockNumber) -> RpcResult<U256> {
		Ok(U256::zero())
	}

	pub fn uncle_by_block_hash_and_index(&self, _: H256, _: Index) -> RpcResult<Option<RichBlock>> {
		Ok(None)
	}

	pub fn uncle_by_block_number_and_index(
		&self,
		_: BlockNumber,
		_: Index,
	) -> RpcResult<Option<RichBlock>> {
		Ok(None)
	}
}
