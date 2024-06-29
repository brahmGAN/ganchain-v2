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

use ethereum_types::{H160, U256, U64};
use jsonrpsee::core::RpcResult;
// Substrate
use sc_client_api::backend::{Backend, StorageProvider};
use sc_transaction_pool::ChainApi;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_consensus::SyncOracle;
use sp_runtime::traits::{Block as BlockT, UniqueSaturatedInto};
// Frontier
use fc_rpc_core::types::*;
use fp_rpc::EthereumRuntimeRPCApi;

use crate::{
	eth::{Eth, EthConfig},
	internal_err,
};

impl<B, C, P, CT, BE, A: ChainApi, EC: EthConfig<B, C>> Eth<B, C, P, CT, BE, A, EC>
where
	B: BlockT,
	C: ProvideRuntimeApi<B>,
	C::Api: EthereumRuntimeRPCApi<B>,
	C: HeaderBackend<B> + StorageProvider<B, BE> + 'static,
	BE: Backend<B>,
{
	pub fn protocol_version(&self) -> RpcResult<u64> {
		Ok(1)
	}

	pub fn syncing(&self) -> RpcResult<SyncStatus> {
		if self.sync.is_major_syncing() {
			let block_number = U256::from(UniqueSaturatedInto::<u128>::unique_saturated_into(
				self.client.info().best_number,
			));
			Ok(SyncStatus::Info(SyncInfo {
				starting_block: U256::zero(),
				current_block: block_number,
				// TODO `highest_block` is not correct, should load `best_seen_block` from NetworkWorker,
				// but afaik that is not currently possible in Substrate:
				// https://github.com/paritytech/substrate/issues/7311
				highest_block: block_number,
				warp_chunks_amount: None,
				warp_chunks_processed: None,
			}))
		} else {
			Ok(SyncStatus::None)
		}
	}

	pub fn author(&self) -> RpcResult<H160> {
		let hash = self.client.info().best_hash;
		let schema = fc_storage::onchain_storage_schema(self.client.as_ref(), hash);

		Ok(self
			.overrides
			.schemas
			.get(&schema)
			.unwrap_or(&self.overrides.fallback)
			.current_block(hash)
			.ok_or_else(|| internal_err("fetching author through override failed"))?
			.header
			.beneficiary)
	}

	pub fn accounts(&self) -> RpcResult<Vec<H160>> {
		let mut accounts = Vec::new();
		for signer in &*self.signers {
			accounts.append(&mut signer.accounts());
		}
		Ok(accounts)
	}

	pub fn block_number(&self) -> RpcResult<U256> {
		Ok(U256::from(
			UniqueSaturatedInto::<u128>::unique_saturated_into(self.client.info().best_number),
		))
	}

	pub fn chain_id(&self) -> RpcResult<Option<U64>> {
		let hash = self.client.info().best_hash;
		Ok(Some(
			self.client
				.runtime_api()
				.chain_id(hash)
				.map_err(|err| internal_err(format!("fetch runtime chain id failed: {:?}", err)))?
				.into(),
		))
	}
}
