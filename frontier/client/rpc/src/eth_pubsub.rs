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

use std::{collections::BTreeMap, marker::PhantomData, sync::Arc};

use ethereum::{BlockV2 as EthereumBlock, TransactionV2 as EthereumTransaction};
use ethereum_types::{H256, U256};
use futures::{FutureExt as _, StreamExt as _};
use jsonrpsee::{types::SubscriptionResult, SubscriptionSink};
// Substrate
use sc_client_api::{
	backend::{Backend, StorageProvider},
	client::BlockchainEvents,
};
use sc_network_sync::SyncingService;
use sc_rpc::SubscriptionTaskExecutor;
use sc_transaction_pool_api::TransactionPool;
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_consensus::SyncOracle;
use sp_core::hashing::keccak_256;
use sp_runtime::traits::{Block as BlockT, UniqueSaturatedInto};
// Frontier
use fc_mapping_sync::{EthereumBlockNotification, EthereumBlockNotificationSinks};
use fc_rpc_core::{
	types::{
		pubsub::{Kind, Params, PubSubSyncStatus, Result as PubSubResult, SyncStatusMetadata},
		Bytes, FilteredParams, Header, Log, Rich,
	},
	EthPubSubApiServer,
};
use fc_storage::OverrideHandle;
use fp_rpc::EthereumRuntimeRPCApi;

#[derive(Debug)]
pub struct EthereumSubIdProvider;

impl jsonrpsee::core::traits::IdProvider for EthereumSubIdProvider {
	fn next_id(&self) -> jsonrpsee::types::SubscriptionId<'static> {
		format!("0x{}", hex::encode(rand::random::<u128>().to_le_bytes())).into()
	}
}

/// Eth pub-sub API implementation.
pub struct EthPubSub<B: BlockT, P, C, BE> {
	pool: Arc<P>,
	client: Arc<C>,
	sync: Arc<SyncingService<B>>,
	subscriptions: SubscriptionTaskExecutor,
	overrides: Arc<OverrideHandle<B>>,
	starting_block: u64,
	pubsub_notification_sinks: Arc<EthereumBlockNotificationSinks<EthereumBlockNotification<B>>>,
	_marker: PhantomData<BE>,
}

impl<B: BlockT, P, C, BE> EthPubSub<B, P, C, BE>
where
	C: HeaderBackend<B>,
{
	pub fn new(
		pool: Arc<P>,
		client: Arc<C>,
		sync: Arc<SyncingService<B>>,
		subscriptions: SubscriptionTaskExecutor,
		overrides: Arc<OverrideHandle<B>>,
		pubsub_notification_sinks: Arc<
			EthereumBlockNotificationSinks<EthereumBlockNotification<B>>,
		>,
	) -> Self {
		// Capture the best block as seen on initialization. Used for syncing subscriptions.
		let starting_block =
			UniqueSaturatedInto::<u64>::unique_saturated_into(client.info().best_number);
		Self {
			pool,
			client,
			sync,
			subscriptions,
			overrides,
			starting_block,
			pubsub_notification_sinks,
			_marker: PhantomData,
		}
	}
}

struct EthSubscriptionResult;
impl EthSubscriptionResult {
	pub fn new_heads(block: EthereumBlock) -> PubSubResult {
		PubSubResult::Header(Box::new(Rich {
			inner: Header {
				hash: Some(H256::from(keccak_256(&rlp::encode(&block.header)))),
				parent_hash: block.header.parent_hash,
				uncles_hash: block.header.ommers_hash,
				author: block.header.beneficiary,
				miner: Some(block.header.beneficiary),
				state_root: block.header.state_root,
				transactions_root: block.header.transactions_root,
				receipts_root: block.header.receipts_root,
				number: Some(block.header.number),
				gas_used: block.header.gas_used,
				gas_limit: block.header.gas_limit,
				extra_data: Bytes(block.header.extra_data.clone()),
				logs_bloom: block.header.logs_bloom,
				timestamp: U256::from(block.header.timestamp),
				difficulty: block.header.difficulty,
				nonce: Some(block.header.nonce),
				size: Some(U256::from(rlp::encode(&block.header).len() as u32)),
			},
			extra_info: BTreeMap::new(),
		}))
	}
	pub fn logs(
		block: EthereumBlock,
		receipts: Vec<ethereum::ReceiptV3>,
		params: &FilteredParams,
	) -> Vec<Log> {
		let block_hash = Some(H256::from(keccak_256(&rlp::encode(&block.header))));
		let mut logs: Vec<Log> = vec![];
		let mut log_index: u32 = 0;
		for (receipt_index, receipt) in receipts.into_iter().enumerate() {
			let receipt_logs = match receipt {
				ethereum::ReceiptV3::Legacy(d)
				| ethereum::ReceiptV3::EIP2930(d)
				| ethereum::ReceiptV3::EIP1559(d) => d.logs,
			};
			let mut transaction_log_index: u32 = 0;
			let transaction_hash: Option<H256> = if receipt_logs.len() > 0 {
				Some(block.transactions[receipt_index].hash())
			} else {
				None
			};
			for log in receipt_logs {
				if Self::add_log(block_hash.unwrap(), &log, &block, params) {
					logs.push(Log {
						address: log.address,
						topics: log.topics,
						data: Bytes(log.data),
						block_hash,
						block_number: Some(block.header.number),
						transaction_hash,
						transaction_index: Some(U256::from(receipt_index)),
						log_index: Some(U256::from(log_index)),
						transaction_log_index: Some(U256::from(transaction_log_index)),
						removed: false,
					});
				}
				log_index += 1;
				transaction_log_index += 1;
			}
		}
		logs
	}
	fn add_log(
		block_hash: H256,
		ethereum_log: &ethereum::Log,
		block: &EthereumBlock,
		params: &FilteredParams,
	) -> bool {
		let log = Log {
			address: ethereum_log.address,
			topics: ethereum_log.topics.clone(),
			data: Bytes(ethereum_log.data.clone()),
			block_hash: None,
			block_number: None,
			transaction_hash: None,
			transaction_index: None,
			log_index: None,
			transaction_log_index: None,
			removed: false,
		};
		if params.filter.is_some() {
			let block_number =
				UniqueSaturatedInto::<u64>::unique_saturated_into(block.header.number);
			if !params.filter_block_range(block_number)
				|| !params.filter_block_hash(block_hash)
				|| !params.filter_address(&log)
				|| !params.filter_topics(&log)
			{
				return false;
			}
		}
		true
	}
}

impl<B: BlockT, P, C, BE> EthPubSubApiServer for EthPubSub<B, P, C, BE>
where
	B: BlockT,
	P: TransactionPool<Block = B> + 'static,
	C: ProvideRuntimeApi<B>,
	C::Api: EthereumRuntimeRPCApi<B>,
	C: BlockchainEvents<B> + 'static,
	C: HeaderBackend<B> + StorageProvider<B, BE>,
	BE: Backend<B> + 'static,
{
	fn subscribe(
		&self,
		mut sink: SubscriptionSink,
		kind: Kind,
		params: Option<Params>,
	) -> SubscriptionResult {
		sink.accept()?;

		let filtered_params = match params {
			Some(Params::Logs(filter)) => FilteredParams::new(Some(filter)),
			_ => FilteredParams::default(),
		};

		let client = self.client.clone();
		// Everytime a new subscription is created, a new mpsc channel is added to the sink pool.
		let (inner_sink, block_notification_stream) =
			sc_utils::mpsc::tracing_unbounded("pubsub_notification_stream", 100_000);
		self.pubsub_notification_sinks.lock().push(inner_sink);
		let pool = self.pool.clone();
		let sync = self.sync.clone();
		let overrides = self.overrides.clone();
		let starting_block = self.starting_block;
		let fut = async move {
			match kind {
				Kind::Logs => {
					let stream = block_notification_stream
						.filter_map(move |notification| {
							if notification.is_new_best {
								let substrate_hash = notification.hash;

								let schema = fc_storage::onchain_storage_schema(
									client.as_ref(),
									substrate_hash,
								);
								let handler = overrides
									.schemas
									.get(&schema)
									.unwrap_or(&overrides.fallback);

								let block = handler.current_block(substrate_hash);
								let receipts = handler.current_receipts(substrate_hash);

								match (receipts, block) {
									(Some(receipts), Some(block)) => {
										futures::future::ready(Some((block, receipts)))
									}
									_ => futures::future::ready(None),
								}
							} else {
								futures::future::ready(None)
							}
						})
						.flat_map(move |(block, receipts)| {
							futures::stream::iter(EthSubscriptionResult::logs(
								block,
								receipts,
								&filtered_params,
							))
						})
						.map(|x| PubSubResult::Log(Box::new(x)));
					sink.pipe_from_stream(stream).await;
				}
				Kind::NewHeads => {
					let stream = block_notification_stream
						.filter_map(move |notification| {
							if notification.is_new_best {
								let schema = fc_storage::onchain_storage_schema(
									client.as_ref(),
									notification.hash,
								);
								let handler = overrides
									.schemas
									.get(&schema)
									.unwrap_or(&overrides.fallback);

								let block = handler.current_block(notification.hash);
								futures::future::ready(block)
							} else {
								futures::future::ready(None)
							}
						})
						.map(EthSubscriptionResult::new_heads);
					sink.pipe_from_stream(stream).await;
				}
				Kind::NewPendingTransactions => {
					use sc_transaction_pool_api::InPoolTransaction;

					let stream = pool
						.import_notification_stream()
						.filter_map(move |txhash| {
							if let Some(xt) = pool.ready_transaction(&txhash) {
								let best_block = client.info().best_hash;

								let api = client.runtime_api();

								let api_version = if let Ok(Some(api_version)) =
									api.api_version::<dyn EthereumRuntimeRPCApi<B>>(best_block)
								{
									api_version
								} else {
									return futures::future::ready(None);
								};

								let xts = vec![xt.data().clone()];

								let txs: Option<Vec<EthereumTransaction>> = if api_version > 1 {
									api.extrinsic_filter(best_block, xts).ok()
								} else {
									#[allow(deprecated)]
									if let Ok(legacy) =
										api.extrinsic_filter_before_version_2(best_block, xts)
									{
										Some(legacy.into_iter().map(|tx| tx.into()).collect())
									} else {
										None
									}
								};

								let res = match txs {
									Some(txs) => {
										if txs.len() == 1 {
											Some(txs[0].clone())
										} else {
											None
										}
									}
									_ => None,
								};
								futures::future::ready(res)
							} else {
								futures::future::ready(None)
							}
						})
						.map(|transaction| PubSubResult::TransactionHash(transaction.hash()));
					sink.pipe_from_stream(stream).await;
				}
				Kind::Syncing => {
					let client = Arc::clone(&client);
					let sync = Arc::clone(&sync);
					// Gets the node syncing status.
					// The response is expected to be serialized either as a plain boolean
					// if the node is not syncing, or a structure containing syncing metadata
					// in case it is.
					async fn status<C: HeaderBackend<B>, B: BlockT>(
						client: Arc<C>,
						sync: Arc<SyncingService<B>>,
						starting_block: u64,
					) -> PubSubSyncStatus {
						if sync.is_major_syncing() {
							// Get the target block to sync.
							// This value is only exposed through substrate async Api
							// in the `NetworkService`.
							let highest_block = sync
								.status()
								.await
								.ok()
								.and_then(|res| res.best_seen_block)
								.map(UniqueSaturatedInto::<u64>::unique_saturated_into);
							// Best imported block.
							let current_block = UniqueSaturatedInto::<u64>::unique_saturated_into(
								client.info().best_number,
							);

							PubSubSyncStatus::Detailed(SyncStatusMetadata {
								syncing: true,
								starting_block,
								current_block,
								highest_block,
							})
						} else {
							PubSubSyncStatus::Simple(false)
						}
					}
					// On connection subscriber expects a value.
					// Because import notifications are only emitted when the node is synced or
					// in case of reorg, the first event is emitted right away.
					let _ = sink.send(&PubSubResult::SyncState(
						status(Arc::clone(&client), Arc::clone(&sync), starting_block).await,
					));

					// When the node is not under a major syncing (i.e. from genesis), react
					// normally to import notifications.
					//
					// Only send new notifications down the pipe when the syncing status changed.
					let mut stream = client.clone().import_notification_stream();
					let mut last_syncing_status = sync.is_major_syncing();
					while (stream.next().await).is_some() {
						let syncing_status = sync.is_major_syncing();
						if syncing_status != last_syncing_status {
							let _ = sink.send(&PubSubResult::SyncState(
								status(client.clone(), sync.clone(), starting_block).await,
							));
						}
						last_syncing_status = syncing_status;
					}
				}
			}
		}
		.boxed();
		self.subscriptions.spawn(
			"frontier-rpc-subscription",
			Some("rpc"),
			fut.map(drop).boxed(),
		);
		Ok(())
	}
}
