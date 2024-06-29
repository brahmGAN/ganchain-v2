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

use ethereum_types::H256;
use futures::future::TryFutureExt;
use jsonrpsee::core::RpcResult;
// Substrate
use sc_client_api::backend::{Backend, StorageProvider};
use sc_transaction_pool::ChainApi;
use sc_transaction_pool_api::TransactionPool;
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_block_builder::BlockBuilder as BlockBuilderApi;
use sp_blockchain::HeaderBackend;
use sp_runtime::{
	generic::BlockId, traits::Block as BlockT, transaction_validity::TransactionSource,
};
// Frontier
use fc_rpc_core::types::*;
use fp_rpc::{ConvertTransaction, ConvertTransactionRuntimeApi, EthereumRuntimeRPCApi};

use crate::{
	eth::{format, Eth, EthConfig},
	internal_err,
};

impl<B, C, P, CT, BE, A: ChainApi, EC: EthConfig<B, C>> Eth<B, C, P, CT, BE, A, EC>
where
	B: BlockT,
	C: ProvideRuntimeApi<B>,
	C::Api: BlockBuilderApi<B> + ConvertTransactionRuntimeApi<B> + EthereumRuntimeRPCApi<B>,
	C: HeaderBackend<B> + StorageProvider<B, BE> + 'static,
	BE: Backend<B> + 'static,
	P: TransactionPool<Block = B> + 'static,
	CT: ConvertTransaction<<B as BlockT>::Extrinsic> + 'static,
	A: ChainApi<Block = B> + 'static,
{
	pub async fn send_transaction(&self, request: TransactionRequest) -> RpcResult<H256> {
		let from = match request.from {
			Some(from) => from,
			None => {
				let accounts = match self.accounts() {
					Ok(accounts) => accounts,
					Err(e) => return Err(e),
				};

				match accounts.get(0) {
					Some(account) => *account,
					None => return Err(internal_err("no signer available")),
				}
			}
		};

		let nonce = match request.nonce {
			Some(nonce) => nonce,
			None => match self.transaction_count(from, None).await {
				Ok(nonce) => nonce,
				Err(e) => return Err(e),
			},
		};

		let chain_id = match self.chain_id() {
			Ok(Some(chain_id)) => chain_id.as_u64(),
			Ok(None) => return Err(internal_err("chain id not available")),
			Err(e) => return Err(e),
		};

		let hash = self.client.info().best_hash;

		let gas_price = request.gas_price;
		let gas_limit = match request.gas {
			Some(gas_limit) => gas_limit,
			None => {
				let block = self.client.runtime_api().current_block(hash);
				if let Ok(Some(block)) = block {
					block.header.gas_limit
				} else {
					return Err(internal_err("block unavailable, cannot query gas limit"));
				}
			}
		};
		let max_fee_per_gas = request.max_fee_per_gas;
		let message: Option<TransactionMessage> = request.into();
		let message = match message {
			Some(TransactionMessage::Legacy(mut m)) => {
				m.nonce = nonce;
				m.chain_id = Some(chain_id);
				m.gas_limit = gas_limit;
				if gas_price.is_none() {
					m.gas_price = self.gas_price().unwrap_or_default();
				}
				TransactionMessage::Legacy(m)
			}
			Some(TransactionMessage::EIP2930(mut m)) => {
				m.nonce = nonce;
				m.chain_id = chain_id;
				m.gas_limit = gas_limit;
				if gas_price.is_none() {
					m.gas_price = self.gas_price().unwrap_or_default();
				}
				TransactionMessage::EIP2930(m)
			}
			Some(TransactionMessage::EIP1559(mut m)) => {
				m.nonce = nonce;
				m.chain_id = chain_id;
				m.gas_limit = gas_limit;
				if max_fee_per_gas.is_none() {
					m.max_fee_per_gas = self.gas_price().unwrap_or_default();
				}
				TransactionMessage::EIP1559(m)
			}
			_ => return Err(internal_err("invalid transaction parameters")),
		};

		let mut transaction = None;

		for signer in &self.signers {
			if signer.accounts().contains(&from) {
				match signer.sign(message, &from) {
					Ok(t) => transaction = Some(t),
					Err(e) => return Err(e),
				}
				break;
			}
		}

		let transaction = match transaction {
			Some(transaction) => transaction,
			None => return Err(internal_err("no signer available")),
		};
		let transaction_hash = transaction.hash();

		let block_hash = self.client.info().best_hash;
		let api_version = match self
			.client
			.runtime_api()
			.api_version::<dyn ConvertTransactionRuntimeApi<B>>(block_hash)
		{
			Ok(api_version) => api_version,
			_ => return Err(internal_err("cannot access runtime api")),
		};

		let extrinsic = match api_version {
			Some(2) => match self
				.client
				.runtime_api()
				.convert_transaction(block_hash, transaction)
			{
				Ok(extrinsic) => extrinsic,
				Err(_) => return Err(internal_err("cannot access runtime api")),
			},
			Some(1) => {
				if let ethereum::TransactionV2::Legacy(legacy_transaction) = transaction {
					// To be compatible with runtimes that do not support transactions v2
					#[allow(deprecated)]
					match self
						.client
						.runtime_api()
						.convert_transaction_before_version_2(block_hash, legacy_transaction)
					{
						Ok(extrinsic) => extrinsic,
						Err(_) => return Err(internal_err("cannot access runtime api")),
					}
				} else {
					return Err(internal_err("This runtime not support eth transactions v2"));
				}
			}
			None => {
				if let Some(ref convert_transaction) = self.convert_transaction {
					convert_transaction.convert_transaction(transaction.clone())
				} else {
					return Err(internal_err(
						"No TransactionConverter is provided and the runtime api ConvertTransactionRuntimeApi is not found"
					));
				}
			}
			_ => {
				return Err(internal_err(
					"ConvertTransactionRuntimeApi version not supported",
				))
			}
		};

		self.pool
			.submit_one(
				&BlockId::Hash(block_hash),
				TransactionSource::Local,
				extrinsic,
			)
			.map_ok(move |_| transaction_hash)
			.map_err(|err| internal_err(format::Geth::pool_error(err)))
			.await
	}

	pub async fn send_raw_transaction(&self, bytes: Bytes) -> RpcResult<H256> {
		let slice = &bytes.0[..];
		if slice.is_empty() {
			return Err(internal_err("transaction data is empty"));
		}
		let transaction: ethereum::TransactionV2 = match ethereum::EnvelopedDecodable::decode(slice)
		{
			Ok(transaction) => transaction,
			Err(_) => return Err(internal_err("decode transaction failed")),
		};

		let transaction_hash = transaction.hash();

		let block_hash = self.client.info().best_hash;
		let api_version = match self
			.client
			.runtime_api()
			.api_version::<dyn ConvertTransactionRuntimeApi<B>>(block_hash)
		{
			Ok(api_version) => api_version,
			_ => return Err(internal_err("cannot access runtime api")),
		};

		let extrinsic = match api_version {
			Some(2) => match self
				.client
				.runtime_api()
				.convert_transaction(block_hash, transaction)
			{
				Ok(extrinsic) => extrinsic,
				Err(_) => return Err(internal_err("cannot access runtime api")),
			},
			Some(1) => {
				if let ethereum::TransactionV2::Legacy(legacy_transaction) = transaction {
					// To be compatible with runtimes that do not support transactions v2
					#[allow(deprecated)]
					match self
						.client
						.runtime_api()
						.convert_transaction_before_version_2(block_hash, legacy_transaction)
					{
						Ok(extrinsic) => extrinsic,
						Err(_) => {
							return Err(internal_err("cannot access runtime api"));
						}
					}
				} else {
					return Err(internal_err("This runtime not support eth transactions v2"));
				}
			}
			None => {
				if let Some(ref convert_transaction) = self.convert_transaction {
					convert_transaction.convert_transaction(transaction.clone())
				} else {
					return Err(internal_err(
						"No TransactionConverter is provided and the runtime api ConvertTransactionRuntimeApi is not found"
					));
				}
			}
			_ => {
				return Err(internal_err(
					"ConvertTransactionRuntimeApi version not supported",
				))
			}
		};

		self.pool
			.submit_one(
				&BlockId::Hash(block_hash),
				TransactionSource::Local,
				extrinsic,
			)
			.map_ok(move |_| transaction_hash)
			.map_err(|err| internal_err(format::Geth::pool_error(err)))
			.await
	}
}
