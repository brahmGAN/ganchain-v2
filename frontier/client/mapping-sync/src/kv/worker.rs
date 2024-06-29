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

use std::{pin::Pin, sync::Arc, time::Duration};

use futures::{
	prelude::*,
	task::{Context, Poll},
};
use futures_timer::Delay;
use log::debug;
// Substrate
use sc_client_api::{
	backend::{Backend, StorageProvider},
	client::ImportNotifications,
};
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_consensus::SyncOracle;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};
// Frontier
use fc_storage::OverrideHandle;
use fp_rpc::EthereumRuntimeRPCApi;

use crate::SyncStrategy;

pub struct MappingSyncWorker<Block: BlockT, C, BE> {
	import_notifications: ImportNotifications<Block>,
	timeout: Duration,
	inner_delay: Option<Delay>,

	client: Arc<C>,
	substrate_backend: Arc<BE>,
	overrides: Arc<OverrideHandle<Block>>,
	frontier_backend: Arc<fc_db::kv::Backend<Block>>,

	have_next: bool,
	retry_times: usize,
	sync_from: <Block::Header as HeaderT>::Number,
	strategy: SyncStrategy,

	sync_oracle: Arc<dyn SyncOracle + Send + Sync + 'static>,
	pubsub_notification_sinks:
		Arc<crate::EthereumBlockNotificationSinks<crate::EthereumBlockNotification<Block>>>,
}

impl<Block: BlockT, C, BE> Unpin for MappingSyncWorker<Block, C, BE> {}

impl<Block: BlockT, C, BE> MappingSyncWorker<Block, C, BE> {
	pub fn new(
		import_notifications: ImportNotifications<Block>,
		timeout: Duration,
		client: Arc<C>,
		substrate_backend: Arc<BE>,
		overrides: Arc<OverrideHandle<Block>>,
		frontier_backend: Arc<fc_db::kv::Backend<Block>>,
		retry_times: usize,
		sync_from: <Block::Header as HeaderT>::Number,
		strategy: SyncStrategy,
		sync_oracle: Arc<dyn SyncOracle + Send + Sync + 'static>,
		pubsub_notification_sinks: Arc<
			crate::EthereumBlockNotificationSinks<crate::EthereumBlockNotification<Block>>,
		>,
	) -> Self {
		Self {
			import_notifications,
			timeout,
			inner_delay: None,

			client,
			substrate_backend,
			overrides,
			frontier_backend,

			have_next: true,
			retry_times,
			sync_from,
			strategy,

			sync_oracle,
			pubsub_notification_sinks,
		}
	}
}

impl<Block: BlockT, C, BE> Stream for MappingSyncWorker<Block, C, BE>
where
	C: ProvideRuntimeApi<Block>,
	C::Api: EthereumRuntimeRPCApi<Block>,
	C: HeaderBackend<Block> + StorageProvider<Block, BE>,
	BE: Backend<Block>,
{
	type Item = ();

	fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<()>> {
		let mut fire = false;

		loop {
			match Stream::poll_next(Pin::new(&mut self.import_notifications), cx) {
				Poll::Pending => break,
				Poll::Ready(Some(_)) => {
					fire = true;
				}
				Poll::Ready(None) => return Poll::Ready(None),
			}
		}

		let timeout = self.timeout;
		let inner_delay = self.inner_delay.get_or_insert_with(|| Delay::new(timeout));

		match Future::poll(Pin::new(inner_delay), cx) {
			Poll::Pending => (),
			Poll::Ready(()) => {
				fire = true;
			}
		}

		if self.have_next {
			fire = true;
		}

		if fire {
			self.inner_delay = None;

			match crate::kv::sync_blocks(
				self.client.as_ref(),
				self.substrate_backend.as_ref(),
				self.overrides.clone(),
				self.frontier_backend.as_ref(),
				self.retry_times,
				self.sync_from,
				self.strategy,
				self.sync_oracle.clone(),
				self.pubsub_notification_sinks.clone(),
			) {
				Ok(have_next) => {
					self.have_next = have_next;
					Poll::Ready(Some(()))
				}
				Err(e) => {
					self.have_next = false;
					debug!(target: "mapping-sync", "Syncing failed with error {:?}, retrying.", e);
					Poll::Ready(Some(()))
				}
			}
		} else {
			Poll::Pending
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{EthereumBlockNotification, EthereumBlockNotificationSinks};
	use fc_storage::{OverrideHandle, SchemaV3Override, StorageOverride};
	use fp_storage::{EthereumStorageSchema, PALLET_ETHEREUM_SCHEMA};
	use sc_block_builder::BlockBuilderProvider;
	use sc_client_api::BlockchainEvents;
	use sp_api::Encode;
	use sp_consensus::BlockOrigin;
	use sp_core::{H160, H256, U256};
	use sp_runtime::{generic::Header, traits::BlakeTwo256, Digest};
	use std::collections::BTreeMap;
	use substrate_test_runtime_client::{
		ClientBlockImportExt, DefaultTestClientBuilderExt, TestClientBuilder, TestClientBuilderExt,
	};
	use tempfile::tempdir;

	type OpaqueBlock = sp_runtime::generic::Block<
		Header<u64, BlakeTwo256>,
		substrate_test_runtime_client::runtime::Extrinsic,
	>;

	fn ethereum_digest() -> Digest {
		let partial_header = ethereum::PartialHeader {
			parent_hash: H256::random(),
			beneficiary: H160::default(),
			state_root: H256::default(),
			receipts_root: H256::default(),
			logs_bloom: ethereum_types::Bloom::default(),
			difficulty: U256::zero(),
			number: U256::zero(),
			gas_limit: U256::zero(),
			gas_used: U256::zero(),
			timestamp: 0u64,
			extra_data: Vec::new(),
			mix_hash: H256::default(),
			nonce: ethereum_types::H64::default(),
		};
		let ethereum_block = ethereum::Block::new(partial_header, vec![], vec![]);
		Digest {
			logs: vec![sp_runtime::generic::DigestItem::Consensus(
				fp_consensus::FRONTIER_ENGINE_ID,
				fp_consensus::PostLog::Hashes(fp_consensus::Hashes::from_block(ethereum_block))
					.encode(),
			)],
		}
	}

	struct TestSyncOracleNotSyncing;
	impl sp_consensus::SyncOracle for TestSyncOracleNotSyncing {
		fn is_major_syncing(&self) -> bool {
			false
		}
		fn is_offline(&self) -> bool {
			false
		}
	}

	struct TestSyncOracleSyncing;
	impl sp_consensus::SyncOracle for TestSyncOracleSyncing {
		fn is_major_syncing(&self) -> bool {
			true
		}
		fn is_offline(&self) -> bool {
			false
		}
	}

	#[tokio::test]
	async fn block_import_notification_works() {
		let tmp = tempdir().expect("create a temporary directory");
		let builder = TestClientBuilder::new().add_extra_storage(
			PALLET_ETHEREUM_SCHEMA.to_vec(),
			Encode::encode(&EthereumStorageSchema::V3),
		);
		let test_sync_oracle = TestSyncOracleNotSyncing {};
		// Backend
		let backend = builder.backend();
		// Client
		let (client, _) =
			builder.build_with_native_executor::<frontier_template_runtime::RuntimeApi, _>(None);
		let mut client = Arc::new(client);
		// Overrides
		let mut overrides_map = BTreeMap::new();
		overrides_map.insert(
			EthereumStorageSchema::V3,
			Box::new(SchemaV3Override::new(client.clone())) as Box<dyn StorageOverride<_>>,
		);
		let overrides = Arc::new(OverrideHandle {
			schemas: overrides_map,
			fallback: Box::new(SchemaV3Override::new(client.clone())),
		});

		let frontier_backend = Arc::new(
			fc_db::kv::Backend::<OpaqueBlock>::new(
				client.clone(),
				&fc_db::kv::DatabaseSettings {
					source: sc_client_db::DatabaseSource::RocksDb {
						path: tmp.path().to_path_buf(),
						cache_size: 0,
					},
				},
			)
			.expect("frontier backend"),
		);

		let notification_stream = client.clone().import_notification_stream();
		let client_inner = client.clone();

		let pubsub_notification_sinks: EthereumBlockNotificationSinks<
			EthereumBlockNotification<OpaqueBlock>,
		> = Default::default();
		let pubsub_notification_sinks = Arc::new(pubsub_notification_sinks);

		let pubsub_notification_sinks_inner = pubsub_notification_sinks.clone();

		tokio::task::spawn(async move {
			MappingSyncWorker::new(
				notification_stream,
				Duration::new(6, 0),
				client_inner,
				backend,
				overrides.clone(),
				frontier_backend,
				3,
				0,
				SyncStrategy::Normal,
				Arc::new(test_sync_oracle),
				pubsub_notification_sinks_inner,
			)
			.for_each(|()| future::ready(()))
			.await
		});

		{
			// A new mpsc channel
			let (inner_sink, mut block_notification_stream) =
				sc_utils::mpsc::tracing_unbounded("pubsub_notification_stream", 100_000);

			{
				// This scope represents a call to eth_subscribe, where it briefly locks the pool
				// to push the new sink.
				let sinks = &mut pubsub_notification_sinks.lock();
				// Push to sink pool
				sinks.push(inner_sink);
			}

			// Let's produce a block, which we expect to trigger a channel message
			let builder = client.new_block(ethereum_digest()).unwrap();
			let block = builder.build().unwrap().block;
			let block_hash = block.header.hash();
			let _res = client.import(BlockOrigin::Own, block).await;

			// Receive
			assert_eq!(
				block_notification_stream
					.next()
					.await
					.expect("a message")
					.hash,
				block_hash
			);
		}

		{
			// Assert we still hold a sink in the pool after switching scopes
			let sinks = pubsub_notification_sinks.lock();
			assert_eq!(sinks.len(), 1);
		}

		{
			// Create yet another mpsc channel
			let (inner_sink, mut block_notification_stream) =
				sc_utils::mpsc::tracing_unbounded("pubsub_notification_stream", 100_000);

			{
				let sinks = &mut pubsub_notification_sinks.lock();
				// Push it
				sinks.push(inner_sink);
				// Now we expect two sinks in the pool
				assert_eq!(sinks.len(), 2);
			}

			// Let's produce another block, this not only triggers a message in the new channel
			// but also removes the closed channels from the pool.
			let builder = client.new_block(ethereum_digest()).unwrap();
			let block = builder.build().unwrap().block;
			let block_hash = block.header.hash();
			let _res = client.import(BlockOrigin::Own, block).await;

			// Receive
			assert_eq!(
				block_notification_stream
					.next()
					.await
					.expect("a message")
					.hash,
				block_hash
			);

			// So we expect the pool to hold one sink only after cleanup
			let sinks = &mut pubsub_notification_sinks.lock();
			assert_eq!(sinks.len(), 1);
		}
	}

	#[tokio::test]
	async fn sink_removal_when_syncing_works() {
		let tmp = tempdir().expect("create a temporary directory");
		let builder = TestClientBuilder::new().add_extra_storage(
			PALLET_ETHEREUM_SCHEMA.to_vec(),
			Encode::encode(&EthereumStorageSchema::V3),
		);
		let test_sync_oracle = TestSyncOracleSyncing {};
		// Backend
		let backend = builder.backend();
		// Client
		let (client, _) =
			builder.build_with_native_executor::<frontier_template_runtime::RuntimeApi, _>(None);
		let mut client = Arc::new(client);
		// Overrides
		let mut overrides_map = BTreeMap::new();
		overrides_map.insert(
			EthereumStorageSchema::V3,
			Box::new(SchemaV3Override::new(client.clone())) as Box<dyn StorageOverride<_>>,
		);
		let overrides = Arc::new(OverrideHandle {
			schemas: overrides_map,
			fallback: Box::new(SchemaV3Override::new(client.clone())),
		});

		let frontier_backend = Arc::new(
			fc_db::kv::Backend::<OpaqueBlock>::new(
				client.clone(),
				&fc_db::kv::DatabaseSettings {
					source: sc_client_db::DatabaseSource::RocksDb {
						path: tmp.path().to_path_buf(),
						cache_size: 0,
					},
				},
			)
			.expect("frontier backend"),
		);

		let notification_stream = client.clone().import_notification_stream();
		let client_inner = client.clone();

		let pubsub_notification_sinks: EthereumBlockNotificationSinks<
			EthereumBlockNotification<OpaqueBlock>,
		> = Default::default();
		let pubsub_notification_sinks = Arc::new(pubsub_notification_sinks);

		let pubsub_notification_sinks_inner = pubsub_notification_sinks.clone();

		tokio::task::spawn(async move {
			MappingSyncWorker::new(
				notification_stream,
				Duration::new(6, 0),
				client_inner,
				backend,
				overrides.clone(),
				frontier_backend,
				3,
				0,
				SyncStrategy::Normal,
				Arc::new(test_sync_oracle),
				pubsub_notification_sinks_inner,
			)
			.for_each(|()| future::ready(()))
			.await
		});

		{
			// A new mpsc channel
			let (inner_sink, mut block_notification_stream) =
				sc_utils::mpsc::tracing_unbounded("pubsub_notification_stream", 100_000);

			{
				// This scope represents a call to eth_subscribe, where it briefly locks the pool
				// to push the new sink.
				let sinks = &mut pubsub_notification_sinks.lock();
				// Push to sink pool
				sinks.push(inner_sink);
			}

			// Let's produce a block, which we expect to trigger a channel message
			let builder = client.new_block(ethereum_digest()).unwrap();
			let block = builder.build().unwrap().block;
			let _res = client.import(BlockOrigin::Own, block).await;

			// Not received, channel closed because major syncing
			assert!(block_notification_stream.next().await.is_none());
		}

		{
			// Assert sink was removed from pool on major syncing
			let sinks = pubsub_notification_sinks.lock();
			assert_eq!(sinks.len(), 0);
		}
	}
}
