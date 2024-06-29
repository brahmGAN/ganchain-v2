// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
// This file is part of Frontier.
//
// Copyright (c) 2020-2022 Parity Technologies (UK) Ltd.
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

mod block;
mod cache;
mod client;
mod execute;
mod fee;
mod filter;
pub mod format;
mod mining;
mod state;
mod submit;
mod transaction;

use std::{collections::BTreeMap, marker::PhantomData, sync::Arc};

use ethereum::{BlockV2 as EthereumBlock, TransactionV2 as EthereumTransaction};
use ethereum_types::{H160, H256, H512, H64, U256, U64};
use jsonrpsee::core::{async_trait, RpcResult};
// Substrate
use sc_client_api::backend::{Backend, StorageProvider};
use sc_network_sync::SyncingService;
use sc_transaction_pool::{ChainApi, Pool};
use sc_transaction_pool_api::{InPoolTransaction, TransactionPool};
use sp_api::{ApiRef, CallApiAt, Core, HeaderT, ProvideRuntimeApi};
use sp_block_builder::BlockBuilder as BlockBuilderApi;
use sp_blockchain::HeaderBackend;
use sp_core::hashing::keccak_256;
use sp_runtime::traits::{Block as BlockT, UniqueSaturatedInto};
// Frontier
use fc_rpc_core::{types::*, EthApiServer};
use fc_storage::OverrideHandle;
use fp_rpc::{
	ConvertTransaction, ConvertTransactionRuntimeApi, EthereumRuntimeRPCApi,
	RuntimeStorageOverride, TransactionStatus,
};

use crate::{internal_err, public_key, signer::EthSigner};

pub use self::{
	cache::{EthBlockDataCacheTask, EthTask},
	execute::EstimateGasAdapter,
	filter::EthFilter,
};

// Configuration trait for RPC configuration.
pub trait EthConfig<B: BlockT, C>: Send + Sync + 'static {
	type EstimateGasAdapter: EstimateGasAdapter + Send + Sync;
	type RuntimeStorageOverride: RuntimeStorageOverride<B, C>;
}

impl<B: BlockT, C> EthConfig<B, C> for () {
	type EstimateGasAdapter = ();
	type RuntimeStorageOverride = ();
}

/// Eth API implementation.
pub struct Eth<B: BlockT, C, P, CT, BE, A: ChainApi, EC: EthConfig<B, C>> {
	pool: Arc<P>,
	graph: Arc<Pool<A>>,
	client: Arc<C>,
	convert_transaction: Option<CT>,
	sync: Arc<SyncingService<B>>,
	is_authority: bool,
	signers: Vec<Box<dyn EthSigner>>,
	overrides: Arc<OverrideHandle<B>>,
	backend: Arc<dyn fc_db::BackendReader<B> + Send + Sync>,
	block_data_cache: Arc<EthBlockDataCacheTask<B>>,
	fee_history_cache: FeeHistoryCache,
	fee_history_cache_limit: FeeHistoryCacheLimit,
	/// When using eth_call/eth_estimateGas, the maximum allowed gas limit will be
	/// block.gas_limit * execute_gas_limit_multiplier
	execute_gas_limit_multiplier: u64,
	forced_parent_hashes: Option<BTreeMap<H256, H256>>,
	_marker: PhantomData<(B, BE, EC)>,
}

impl<B: BlockT, C, P, CT, BE, A: ChainApi> Eth<B, C, P, CT, BE, A, ()> {
	pub fn new(
		client: Arc<C>,
		pool: Arc<P>,
		graph: Arc<Pool<A>>,
		convert_transaction: Option<CT>,
		sync: Arc<SyncingService<B>>,
		signers: Vec<Box<dyn EthSigner>>,
		overrides: Arc<OverrideHandle<B>>,
		backend: Arc<dyn fc_db::BackendReader<B> + Send + Sync>,
		is_authority: bool,
		block_data_cache: Arc<EthBlockDataCacheTask<B>>,
		fee_history_cache: FeeHistoryCache,
		fee_history_cache_limit: FeeHistoryCacheLimit,
		execute_gas_limit_multiplier: u64,
		forced_parent_hashes: Option<BTreeMap<H256, H256>>,
	) -> Self {
		Self {
			client,
			pool,
			graph,
			convert_transaction,
			sync,
			is_authority,
			signers,
			overrides,
			backend,
			block_data_cache,
			fee_history_cache,
			fee_history_cache_limit,
			execute_gas_limit_multiplier,
			forced_parent_hashes,
			_marker: PhantomData,
		}
	}
}

impl<B: BlockT, C, P, CT, BE, A: ChainApi, EC: EthConfig<B, C>> Eth<B, C, P, CT, BE, A, EC> {
	pub fn replace_config<EC2: EthConfig<B, C>>(self) -> Eth<B, C, P, CT, BE, A, EC2> {
		let Self {
			client,
			pool,
			graph,
			convert_transaction,
			sync,
			is_authority,
			signers,
			overrides,
			backend,
			block_data_cache,
			fee_history_cache,
			fee_history_cache_limit,
			execute_gas_limit_multiplier,
			forced_parent_hashes,
			_marker: _,
		} = self;

		Eth {
			client,
			pool,
			graph,
			convert_transaction,
			sync,
			is_authority,
			signers,
			overrides,
			backend,
			block_data_cache,
			fee_history_cache,
			fee_history_cache_limit,
			execute_gas_limit_multiplier,
			forced_parent_hashes,
			_marker: PhantomData,
		}
	}
}

#[async_trait]
impl<B, C, P, CT, BE, A, EC> EthApiServer for Eth<B, C, P, CT, BE, A, EC>
where
	B: BlockT,
	C: CallApiAt<B> + ProvideRuntimeApi<B>,
	C::Api: BlockBuilderApi<B> + ConvertTransactionRuntimeApi<B> + EthereumRuntimeRPCApi<B>,
	C: HeaderBackend<B> + StorageProvider<B, BE> + 'static,
	BE: Backend<B> + 'static,
	P: TransactionPool<Block = B> + 'static,
	CT: ConvertTransaction<<B as BlockT>::Extrinsic> + Send + Sync + 'static,
	A: ChainApi<Block = B> + 'static,
	EC: EthConfig<B, C>,
{
	// ########################################################################
	// Client
	// ########################################################################

	fn protocol_version(&self) -> RpcResult<u64> {
		self.protocol_version()
	}

	fn syncing(&self) -> RpcResult<SyncStatus> {
		self.syncing()
	}

	fn author(&self) -> RpcResult<H160> {
		self.author()
	}

	fn accounts(&self) -> RpcResult<Vec<H160>> {
		self.accounts()
	}

	fn block_number(&self) -> RpcResult<U256> {
		self.block_number()
	}

	fn chain_id(&self) -> RpcResult<Option<U64>> {
		self.chain_id()
	}

	// ########################################################################
	// Block
	// ########################################################################

	async fn block_by_hash(&self, hash: H256, full: bool) -> RpcResult<Option<RichBlock>> {
		self.block_by_hash(hash, full).await
	}

	async fn block_by_number(
		&self,
		number: BlockNumber,
		full: bool,
	) -> RpcResult<Option<RichBlock>> {
		self.block_by_number(number, full).await
	}

	async fn block_transaction_count_by_hash(&self, hash: H256) -> RpcResult<Option<U256>> {
		self.block_transaction_count_by_hash(hash).await
	}

	async fn block_transaction_count_by_number(
		&self,
		number: BlockNumber,
	) -> RpcResult<Option<U256>> {
		self.block_transaction_count_by_number(number).await
	}

	fn block_uncles_count_by_hash(&self, hash: H256) -> RpcResult<U256> {
		self.block_uncles_count_by_hash(hash)
	}

	fn block_uncles_count_by_number(&self, number: BlockNumber) -> RpcResult<U256> {
		self.block_uncles_count_by_number(number)
	}

	fn uncle_by_block_hash_and_index(
		&self,
		hash: H256,
		index: Index,
	) -> RpcResult<Option<RichBlock>> {
		self.uncle_by_block_hash_and_index(hash, index)
	}

	fn uncle_by_block_number_and_index(
		&self,
		number: BlockNumber,
		index: Index,
	) -> RpcResult<Option<RichBlock>> {
		self.uncle_by_block_number_and_index(number, index)
	}

	// ########################################################################
	// Transaction
	// ########################################################################

	async fn transaction_by_hash(&self, hash: H256) -> RpcResult<Option<Transaction>> {
		self.transaction_by_hash(hash).await
	}

	async fn transaction_by_block_hash_and_index(
		&self,
		hash: H256,
		index: Index,
	) -> RpcResult<Option<Transaction>> {
		self.transaction_by_block_hash_and_index(hash, index).await
	}

	async fn transaction_by_block_number_and_index(
		&self,
		number: BlockNumber,
		index: Index,
	) -> RpcResult<Option<Transaction>> {
		self.transaction_by_block_number_and_index(number, index)
			.await
	}

	async fn transaction_receipt(&self, hash: H256) -> RpcResult<Option<Receipt>> {
		self.transaction_receipt(hash).await
	}

	// ########################################################################
	// State
	// ########################################################################

	async fn balance(&self, address: H160, number: Option<BlockNumber>) -> RpcResult<U256> {
		self.balance(address, number).await
	}

	async fn storage_at(
		&self,
		address: H160,
		index: U256,
		number: Option<BlockNumber>,
	) -> RpcResult<H256> {
		self.storage_at(address, index, number).await
	}

	async fn transaction_count(
		&self,
		address: H160,
		number: Option<BlockNumber>,
	) -> RpcResult<U256> {
		self.transaction_count(address, number).await
	}

	async fn code_at(&self, address: H160, number: Option<BlockNumber>) -> RpcResult<Bytes> {
		self.code_at(address, number).await
	}

	// ########################################################################
	// Execute
	// ########################################################################

	async fn call(
		&self,
		request: CallRequest,
		number: Option<BlockNumber>,
		state_overrides: Option<BTreeMap<H160, CallStateOverride>>,
	) -> RpcResult<Bytes> {
		self.call(request, number, state_overrides).await
	}

	async fn estimate_gas(
		&self,
		request: CallRequest,
		number: Option<BlockNumber>,
	) -> RpcResult<U256> {
		self.estimate_gas(request, number).await
	}

	// ########################################################################
	// Fee
	// ########################################################################

	fn gas_price(&self) -> RpcResult<U256> {
		self.gas_price()
	}

	async fn fee_history(
		&self,
		block_count: U256,
		newest_block: BlockNumber,
		reward_percentiles: Option<Vec<f64>>,
	) -> RpcResult<FeeHistory> {
		self.fee_history(block_count, newest_block, reward_percentiles)
			.await
	}

	fn max_priority_fee_per_gas(&self) -> RpcResult<U256> {
		self.max_priority_fee_per_gas()
	}

	// ########################################################################
	// Mining
	// ########################################################################

	fn is_mining(&self) -> RpcResult<bool> {
		self.is_mining()
	}

	fn hashrate(&self) -> RpcResult<U256> {
		self.hashrate()
	}

	fn work(&self) -> RpcResult<Work> {
		self.work()
	}

	fn submit_hashrate(&self, hashrate: U256, id: H256) -> RpcResult<bool> {
		self.submit_hashrate(hashrate, id)
	}

	fn submit_work(&self, nonce: H64, pow_hash: H256, mix_digest: H256) -> RpcResult<bool> {
		self.submit_work(nonce, pow_hash, mix_digest)
	}

	// ########################################################################
	// Submit
	// ########################################################################

	async fn send_transaction(&self, request: TransactionRequest) -> RpcResult<H256> {
		self.send_transaction(request).await
	}

	async fn send_raw_transaction(&self, bytes: Bytes) -> RpcResult<H256> {
		self.send_raw_transaction(bytes).await
	}
}

fn rich_block_build(
	block: EthereumBlock,
	statuses: Vec<Option<TransactionStatus>>,
	hash: Option<H256>,
	full_transactions: bool,
	base_fee: Option<U256>,
	is_pending: bool,
) -> RichBlock {
	let (hash, miner, nonce, total_difficulty) = if !is_pending {
		(
			Some(hash.unwrap_or_else(|| H256::from(keccak_256(&rlp::encode(&block.header))))),
			Some(block.header.beneficiary),
			Some(block.header.nonce),
			Some(U256::zero()),
		)
	} else {
		(None, None, None, None)
	};
	Rich {
		inner: Block {
			header: Header {
				hash,
				parent_hash: block.header.parent_hash,
				uncles_hash: block.header.ommers_hash,
				author: block.header.beneficiary,
				miner,
				state_root: block.header.state_root,
				transactions_root: block.header.transactions_root,
				receipts_root: block.header.receipts_root,
				number: Some(block.header.number),
				gas_used: block.header.gas_used,
				gas_limit: block.header.gas_limit,
				extra_data: Bytes(block.header.extra_data.clone()),
				logs_bloom: block.header.logs_bloom,
				timestamp: U256::from(block.header.timestamp / 1000),
				difficulty: block.header.difficulty,
				nonce,
				size: Some(U256::from(rlp::encode(&block.header).len() as u32)),
			},
			total_difficulty,
			uncles: vec![],
			transactions: {
				if full_transactions {
					BlockTransactions::Full(
						block
							.transactions
							.iter()
							.enumerate()
							.map(|(index, transaction)| {
								transaction_build(
									transaction.clone(),
									Some(block.clone()),
									Some(statuses[index].clone().unwrap_or_default()),
									base_fee,
								)
							})
							.collect(),
					)
				} else {
					BlockTransactions::Hashes(
						block
							.transactions
							.iter()
							.map(|transaction| transaction.hash())
							.collect(),
					)
				}
			},
			size: Some(U256::from(rlp::encode(&block).len() as u32)),
			base_fee_per_gas: base_fee,
		},
		extra_info: BTreeMap::new(),
	}
}

fn transaction_build(
	ethereum_transaction: EthereumTransaction,
	block: Option<EthereumBlock>,
	status: Option<TransactionStatus>,
	base_fee: Option<U256>,
) -> Transaction {
	let mut transaction: Transaction = ethereum_transaction.clone().into();

	if let EthereumTransaction::EIP1559(_) = ethereum_transaction {
		if block.is_none() && status.is_none() {
			// If transaction is not mined yet, gas price is considered just max fee per gas.
			transaction.gas_price = transaction.max_fee_per_gas;
		} else {
			let base_fee = base_fee.unwrap_or_default();
			let max_priority_fee_per_gas = transaction.max_priority_fee_per_gas.unwrap_or_default();
			let max_fee_per_gas = transaction.max_fee_per_gas.unwrap_or_default();
			// If transaction is already mined, gas price is the effective gas price.
			transaction.gas_price = Some(
				base_fee
					.checked_add(max_priority_fee_per_gas)
					.unwrap_or_else(U256::max_value)
					.min(max_fee_per_gas),
			);
		}
	}

	let pubkey = match public_key(&ethereum_transaction) {
		Ok(p) => Some(p),
		Err(_e) => None,
	};

	// Block hash.
	transaction.block_hash = block
		.as_ref()
		.map(|block| H256::from(keccak_256(&rlp::encode(&block.header))));
	// Block number.
	transaction.block_number = block.as_ref().map(|block| block.header.number);
	// Transaction index.
	transaction.transaction_index = status.as_ref().map(|status| {
		U256::from(UniqueSaturatedInto::<u32>::unique_saturated_into(
			status.transaction_index,
		))
	});
	// From.
	transaction.from = status.as_ref().map_or(
		{
			match pubkey {
				Some(pk) => H160::from(H256::from(keccak_256(&pk))),
				_ => H160::default(),
			}
		},
		|status| status.from,
	);
	// To.
	transaction.to = status.as_ref().map_or(
		{
			let action = match ethereum_transaction {
				EthereumTransaction::Legacy(t) => t.action,
				EthereumTransaction::EIP2930(t) => t.action,
				EthereumTransaction::EIP1559(t) => t.action,
			};
			match action {
				ethereum::TransactionAction::Call(to) => Some(to),
				_ => None,
			}
		},
		|status| status.to,
	);
	// Creates.
	transaction.creates = status.as_ref().and_then(|status| status.contract_address);
	// Public key.
	transaction.public_key = pubkey.as_ref().map(H512::from);

	transaction
}

fn pending_runtime_api<'a, B: BlockT, C, BE, A: ChainApi>(
	client: &'a C,
	graph: &'a Pool<A>,
) -> RpcResult<ApiRef<'a, C::Api>>
where
	B: BlockT,
	C: ProvideRuntimeApi<B>,
	C::Api: BlockBuilderApi<B> + EthereumRuntimeRPCApi<B>,
	C: HeaderBackend<B> + StorageProvider<B, BE> + 'static,
	BE: Backend<B>,
	A: ChainApi<Block = B> + 'static,
{
	// In case of Pending, we need an overlayed state to query over.
	let api = client.runtime_api();
	let best_hash = client.info().best_hash;
	// Get all transactions in the ready queue.
	let xts: Vec<<B as BlockT>::Extrinsic> = graph
		.validated_pool()
		.ready()
		.map(|in_pool_tx| in_pool_tx.data().clone())
		.collect::<Vec<<B as BlockT>::Extrinsic>>();
	// Manually initialize the overlay.
	if let Ok(Some(header)) = client.header(best_hash) {
		let parent_hash = *header.parent_hash();
		api.initialize_block(parent_hash, &header)
			.map_err(|e| internal_err(format!("Runtime api access error: {:?}", e)))?;
		// Apply the ready queue to the best block's state.
		for xt in xts {
			let _ = api.apply_extrinsic(best_hash, xt);
		}
		Ok(api)
	} else {
		Err(internal_err(format!(
			"Cannot get header for block {:?}",
			best_hash
		)))
	}
}
