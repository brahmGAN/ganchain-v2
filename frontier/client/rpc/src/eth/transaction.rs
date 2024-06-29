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

use ethereum::TransactionV2 as EthereumTransaction;
use ethereum_types::{H256, U256, U64};
use jsonrpsee::core::RpcResult;
// Substrate
use sc_client_api::backend::{Backend, StorageProvider};
use sc_transaction_pool::ChainApi;
use sc_transaction_pool_api::InPoolTransaction;
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_core::hashing::keccak_256;
use sp_runtime::traits::Block as BlockT;
// Frontier
use fc_rpc_core::types::*;
use fp_rpc::EthereumRuntimeRPCApi;

use crate::{
	eth::{transaction_build, Eth, EthConfig},
	frontier_backend_client, internal_err,
};

impl<B, C, P, CT, BE, A: ChainApi, EC: EthConfig<B, C>> Eth<B, C, P, CT, BE, A, EC>
where
	B: BlockT,
	C: ProvideRuntimeApi<B>,
	C::Api: EthereumRuntimeRPCApi<B>,
	C: HeaderBackend<B> + StorageProvider<B, BE> + 'static,
	BE: Backend<B> + 'static,
	A: ChainApi<Block = B> + 'static,
{
	pub async fn transaction_by_hash(&self, hash: H256) -> RpcResult<Option<Transaction>> {
		let client = Arc::clone(&self.client);
		let block_data_cache = Arc::clone(&self.block_data_cache);
		let backend = Arc::clone(&self.backend);
		let graph = Arc::clone(&self.graph);

		let (hash, index) = match frontier_backend_client::load_transactions::<B, C>(
			client.as_ref(),
			backend.as_ref(),
			hash,
			true,
		)
		.await
		.map_err(|err| internal_err(format!("{:?}", err)))?
		{
			Some((hash, index)) => (hash, index as usize),
			None => {
				let api = client.runtime_api();
				let best_block = client.info().best_hash;

				let api_version = if let Ok(Some(api_version)) =
					api.api_version::<dyn EthereumRuntimeRPCApi<B>>(best_block)
				{
					api_version
				} else {
					return Err(internal_err("failed to retrieve Runtime Api version"));
				};
				// If the transaction is not yet mapped in the frontier db,
				// check for it in the transaction pool.
				let mut xts: Vec<<B as BlockT>::Extrinsic> = Vec::new();
				// Collect transactions in the ready validated pool.
				xts.extend(
					graph
						.validated_pool()
						.ready()
						.map(|in_pool_tx| in_pool_tx.data().clone())
						.collect::<Vec<<B as BlockT>::Extrinsic>>(),
				);

				// Collect transactions in the future validated pool.
				xts.extend(
					graph
						.validated_pool()
						.futures()
						.iter()
						.map(|(_hash, extrinsic)| extrinsic.clone())
						.collect::<Vec<<B as BlockT>::Extrinsic>>(),
				);

				let ethereum_transactions: Vec<EthereumTransaction> = if api_version > 1 {
					api.extrinsic_filter(best_block, xts).map_err(|err| {
						internal_err(format!("fetch runtime extrinsic filter failed: {:?}", err))
					})?
				} else {
					#[allow(deprecated)]
					let legacy = api.extrinsic_filter_before_version_2(best_block, xts)
						.map_err(|err| {
							internal_err(format!(
								"fetch runtime extrinsic filter failed: {:?}",
								err
							))
						})?;
					legacy.into_iter().map(|tx| tx.into()).collect()
				};

				for txn in ethereum_transactions {
					let inner_hash = txn.hash();
					if hash == inner_hash {
						return Ok(Some(transaction_build(txn, None, None, None)));
					}
				}
				// Unknown transaction.
				return Ok(None);
			}
		};

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

		let base_fee = client
			.runtime_api()
			.gas_price(substrate_hash)
			.unwrap_or_default();

		match (block, statuses) {
			(Some(block), Some(statuses)) => Ok(Some(transaction_build(
				block.transactions[index].clone(),
				Some(block),
				Some(statuses[index].clone()),
				Some(base_fee),
			))),
			_ => Ok(None),
		}
	}

	pub async fn transaction_by_block_hash_and_index(
		&self,
		hash: H256,
		index: Index,
	) -> RpcResult<Option<Transaction>> {
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

		let index = index.value();

		let schema = fc_storage::onchain_storage_schema(client.as_ref(), substrate_hash);

		let block = block_data_cache.current_block(schema, substrate_hash).await;
		let statuses = block_data_cache
			.current_transaction_statuses(schema, substrate_hash)
			.await;

		let base_fee = client
			.runtime_api()
			.gas_price(substrate_hash)
			.unwrap_or_default();

		match (block, statuses) {
			(Some(block), Some(statuses)) => {
				if let (Some(transaction), Some(status)) =
					(block.transactions.get(index), statuses.get(index))
				{
					Ok(Some(transaction_build(
						transaction.clone(),
						Some(block),
						Some(status.clone()),
						Some(base_fee),
					)))
				} else {
					Err(internal_err(format!("{:?} is out of bounds", index)))
				}
			}
			_ => Ok(None),
		}
	}

	pub async fn transaction_by_block_number_and_index(
		&self,
		number: BlockNumber,
		index: Index,
	) -> RpcResult<Option<Transaction>> {
		let client = Arc::clone(&self.client);
		let block_data_cache = Arc::clone(&self.block_data_cache);
		let backend = Arc::clone(&self.backend);

		let id = match frontier_backend_client::native_block_id::<B, C>(
			client.as_ref(),
			backend.as_ref(),
			Some(number),
		)
		.await?
		{
			Some(id) => id,
			None => return Ok(None),
		};
		let substrate_hash = client
			.expect_block_hash_from_id(&id)
			.map_err(|_| internal_err(format!("Expect block number from id: {}", id)))?;

		let index = index.value();
		let schema = fc_storage::onchain_storage_schema(client.as_ref(), substrate_hash);

		let block = block_data_cache.current_block(schema, substrate_hash).await;
		let statuses = block_data_cache
			.current_transaction_statuses(schema, substrate_hash)
			.await;

		let base_fee = client
			.runtime_api()
			.gas_price(substrate_hash)
			.unwrap_or_default();

		match (block, statuses) {
			(Some(block), Some(statuses)) => {
				if let (Some(transaction), Some(status)) =
					(block.transactions.get(index), statuses.get(index))
				{
					Ok(Some(transaction_build(
						transaction.clone(),
						Some(block),
						Some(status.clone()),
						Some(base_fee),
					)))
				} else {
					Err(internal_err(format!("{:?} is out of bounds", index)))
				}
			}
			_ => Ok(None),
		}
	}

	pub async fn transaction_receipt(&self, hash: H256) -> RpcResult<Option<Receipt>> {
		let client = Arc::clone(&self.client);
		let overrides = Arc::clone(&self.overrides);
		let block_data_cache = Arc::clone(&self.block_data_cache);
		let backend = Arc::clone(&self.backend);

		let (hash, index) = match frontier_backend_client::load_transactions::<B, C>(
			client.as_ref(),
			backend.as_ref(),
			hash,
			true,
		)
		.await
		.map_err(|err| internal_err(format!("{:?}", err)))?
		{
			Some((hash, index)) => (hash, index as usize),
			None => return Ok(None),
		};

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
		let handler = overrides
			.schemas
			.get(&schema)
			.unwrap_or(&overrides.fallback);

		let block = block_data_cache.current_block(schema, substrate_hash).await;
		let statuses = block_data_cache
			.current_transaction_statuses(schema, substrate_hash)
			.await;

		let receipts = handler.current_receipts(substrate_hash);
		let is_eip1559 = handler.is_eip1559(substrate_hash);

		match (block, statuses, receipts) {
			(Some(block), Some(statuses), Some(receipts)) => {
				let block_hash = H256::from(keccak_256(&rlp::encode(&block.header)));
				let receipt = receipts[index].clone();

				let (logs, logs_bloom, status_code, cumulative_gas_used, gas_used) = if !is_eip1559
				{
					// Pre-london frontier update stored receipts require cumulative gas calculation.
					match receipt {
						ethereum::ReceiptV3::Legacy(ref d) => {
							let index = core::cmp::min(receipts.len(), index + 1);
							let cumulative_gas: u32 = receipts[..index]
								.iter()
								.map(|r| match r {
									ethereum::ReceiptV3::Legacy(d) => Ok(d.used_gas.as_u32()),
									_ => Err(internal_err(format!(
										"Unknown receipt for request {}",
										hash
									))),
								})
								.sum::<RpcResult<u32>>()?;
							(
								d.logs.clone(),
								d.logs_bloom,
								d.status_code,
								U256::from(cumulative_gas),
								d.used_gas,
							)
						}
						_ => {
							return Err(internal_err(format!(
								"Unknown receipt for request {}",
								hash
							)))
						}
					}
				} else {
					match receipt {
						ethereum::ReceiptV3::Legacy(ref d)
						| ethereum::ReceiptV3::EIP2930(ref d)
						| ethereum::ReceiptV3::EIP1559(ref d) => {
							let cumulative_gas = d.used_gas;
							let gas_used = if index > 0 {
								let previous_receipt = receipts[index - 1].clone();
								let previous_gas_used = match previous_receipt {
									ethereum::ReceiptV3::Legacy(d)
									| ethereum::ReceiptV3::EIP2930(d)
									| ethereum::ReceiptV3::EIP1559(d) => d.used_gas,
								};
								cumulative_gas.saturating_sub(previous_gas_used)
							} else {
								cumulative_gas
							};
							(
								d.logs.clone(),
								d.logs_bloom,
								d.status_code,
								cumulative_gas,
								gas_used,
							)
						}
					}
				};

				let status = statuses[index].clone();
				let mut cumulative_receipts = receipts;
				cumulative_receipts.truncate((status.transaction_index + 1) as usize);
				let transaction = block.transactions[index].clone();
				let effective_gas_price = match transaction {
					EthereumTransaction::Legacy(t) => t.gas_price,
					EthereumTransaction::EIP2930(t) => t.gas_price,
					EthereumTransaction::EIP1559(t) => client
						.runtime_api()
						.gas_price(substrate_hash)
						.unwrap_or_default()
						.checked_add(t.max_priority_fee_per_gas)
						.unwrap_or_else(U256::max_value)
						.min(t.max_fee_per_gas),
				};

				return Ok(Some(Receipt {
					transaction_hash: Some(status.transaction_hash),
					transaction_index: Some(status.transaction_index.into()),
					block_hash: Some(block_hash),
					from: Some(status.from),
					to: status.to,
					block_number: Some(block.header.number),
					cumulative_gas_used,
					gas_used: Some(gas_used),
					contract_address: status.contract_address,
					logs: {
						let mut pre_receipts_log_index = None;
						if cumulative_receipts.len() > 0 {
							cumulative_receipts.truncate(cumulative_receipts.len() - 1);
							pre_receipts_log_index = Some(
								cumulative_receipts
									.iter()
									.map(|r| match r {
										ethereum::ReceiptV3::Legacy(d)
										| ethereum::ReceiptV3::EIP2930(d)
										| ethereum::ReceiptV3::EIP1559(d) => d.logs.len() as u32,
									})
									.sum::<u32>(),
							);
						}
						logs.iter()
							.enumerate()
							.map(|(i, log)| Log {
								address: log.address,
								topics: log.topics.clone(),
								data: Bytes(log.data.clone()),
								block_hash: Some(block_hash),
								block_number: Some(block.header.number),
								transaction_hash: Some(status.transaction_hash),
								transaction_index: Some(status.transaction_index.into()),
								log_index: Some(U256::from(
									(pre_receipts_log_index.unwrap_or(0)) + i as u32,
								)),
								transaction_log_index: Some(U256::from(i)),
								removed: false,
							})
							.collect()
					},
					status_code: Some(U64::from(status_code)),
					logs_bloom,
					state_root: None,
					effective_gas_price,
					transaction_type: match receipt {
						ethereum::ReceiptV3::Legacy(_) => U256::from(0),
						ethereum::ReceiptV3::EIP2930(_) => U256::from(1),
						ethereum::ReceiptV3::EIP1559(_) => U256::from(2),
					},
				}));
			}
			_ => Ok(None),
		}
	}
}
