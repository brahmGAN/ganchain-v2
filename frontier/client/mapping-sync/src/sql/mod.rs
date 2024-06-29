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

use std::{ops::DerefMut, sync::Arc, time::Duration};

use futures::prelude::*;
// Substrate
use sc_client_api::backend::{Backend as BackendT, StateBackend, StorageProvider};
use sp_api::{HeaderT, ProvideRuntimeApi};
use sp_blockchain::{Backend, HeaderBackend};
use sp_consensus::SyncOracle;
use sp_core::H256;
use sp_runtime::traits::{BlakeTwo256, Block as BlockT, UniqueSaturatedInto};
// Frontier
use fp_rpc::EthereumRuntimeRPCApi;

use crate::{EthereumBlockNotification, EthereumBlockNotificationSinks, SyncStrategy};

/// Defines the commands for the sync worker.
#[derive(Debug)]
pub enum WorkerCommand {
	/// Resume indexing from the last indexed canon block.
	ResumeSync,
	/// Index leaves.
	IndexLeaves(Vec<H256>),
	/// Index the best block known so far via import notifications.
	IndexBestBlock(H256),
	/// Canonicalize the enacted and retracted blocks reported via import notifications.
	Canonicalize {
		common: H256,
		enacted: Vec<H256>,
		retracted: Vec<H256>,
	},
	/// Verify indexed blocks' consistency.
	/// Check for any canon blocks that haven't had their logs indexed.
	/// Check for any missing parent blocks from the latest canon block.
	CheckIndexedBlocks,
}

/// Config parameters for the SyncWorker.
pub struct SyncWorkerConfig {
	pub check_indexed_blocks_interval: Duration,
	pub read_notification_timeout: Duration,
}

/// Implements an indexer that imports blocks and their transactions.
pub struct SyncWorker<Block, Backend, Client> {
	_phantom: std::marker::PhantomData<(Block, Backend, Client)>,
}

impl<Block: BlockT, Backend, Client> SyncWorker<Block, Backend, Client>
where
	Block: BlockT<Hash = H256>,
	Client: ProvideRuntimeApi<Block>,
	Client::Api: EthereumRuntimeRPCApi<Block>,
	Client: HeaderBackend<Block> + StorageProvider<Block, Backend> + 'static,
	Backend: BackendT<Block> + 'static,
	Backend::State: StateBackend<BlakeTwo256>,
{
	/// Spawn the indexing worker. The worker can be given commands via the sender channel.
	/// Once the buffer is full, attempts to send new messages will wait until a message is read from the channel.
	pub async fn spawn_worker(
		client: Arc<Client>,
		substrate_backend: Arc<Backend>,
		indexer_backend: Arc<fc_db::sql::Backend<Block>>,
		pubsub_notification_sinks: Arc<
			EthereumBlockNotificationSinks<EthereumBlockNotification<Block>>,
		>,
	) -> tokio::sync::mpsc::Sender<WorkerCommand> {
		let (tx, mut rx) = tokio::sync::mpsc::channel(100);
		tokio::task::spawn(async move {
			while let Some(cmd) = rx.recv().await {
				log::debug!(target: "frontier-sql", "💬 Recv Worker Command {cmd:?}");
				match cmd {
					WorkerCommand::ResumeSync => {
						// Attempt to resume from last indexed block. If there is no data in the db, sync genesis.
						match indexer_backend.get_last_indexed_canon_block().await.ok() {
							Some(last_block_hash) => {
								log::debug!(target: "frontier-sql", "Resume from last block {last_block_hash:?}");
								if let Some(parent_hash) = client
									.header(last_block_hash)
									.ok()
									.flatten()
									.map(|header| *header.parent_hash())
								{
									index_canonical_block_and_ancestors(
										client.clone(),
										substrate_backend.clone(),
										indexer_backend.clone(),
										parent_hash,
									)
									.await;
								}
							}
							None => {
								index_genesis_block(client.clone(), indexer_backend.clone()).await;
							}
						};
					}
					WorkerCommand::IndexLeaves(leaves) => {
						for leaf in leaves {
							index_block_and_ancestors(
								client.clone(),
								substrate_backend.clone(),
								indexer_backend.clone(),
								leaf,
							)
							.await;
						}
					}
					WorkerCommand::IndexBestBlock(block_hash) => {
						index_canonical_block_and_ancestors(
							client.clone(),
							substrate_backend.clone(),
							indexer_backend.clone(),
							block_hash,
						)
						.await;
						let sinks = &mut pubsub_notification_sinks.lock();
						for sink in sinks.iter() {
							let _ = sink.unbounded_send(EthereumBlockNotification {
								is_new_best: true,
								hash: block_hash,
							});
						}
					}
					WorkerCommand::Canonicalize {
						common,
						enacted,
						retracted,
					} => {
						canonicalize_blocks(indexer_backend.clone(), common, enacted, retracted)
							.await;
					}
					WorkerCommand::CheckIndexedBlocks => {
						// Fix any indexed blocks that did not have their logs indexed
						if let Some(block_hash) =
							indexer_backend.get_first_pending_canon_block().await
						{
							log::debug!(target: "frontier-sql", "Indexing pending canonical block {block_hash:?}");
							indexer_backend
								.index_block_logs(client.clone(), block_hash)
								.await;
						}

						// Fix any missing blocks
						index_missing_blocks(
							client.clone(),
							substrate_backend.clone(),
							indexer_backend.clone(),
						)
						.await;
					}
				}
			}
		});

		tx
	}

	/// Start the worker.
	pub async fn run(
		client: Arc<Client>,
		substrate_backend: Arc<Backend>,
		indexer_backend: Arc<fc_db::sql::Backend<Block>>,
		import_notifications: sc_client_api::ImportNotifications<Block>,
		worker_config: SyncWorkerConfig,
		_sync_strategy: SyncStrategy,
		sync_oracle: Arc<dyn SyncOracle + Send + Sync + 'static>,
		pubsub_notification_sinks: Arc<
			EthereumBlockNotificationSinks<EthereumBlockNotification<Block>>,
		>,
	) {
		let tx = Self::spawn_worker(
			client.clone(),
			substrate_backend.clone(),
			indexer_backend.clone(),
			pubsub_notification_sinks.clone(),
		)
		.await;

		// Resume sync from the last indexed block until we reach an already indexed parent
		tx.send(WorkerCommand::ResumeSync).await.ok();
		// check missing blocks every interval
		let tx2 = tx.clone();
		tokio::task::spawn(async move {
			loop {
				futures_timer::Delay::new(worker_config.check_indexed_blocks_interval).await;
				tx2.send(WorkerCommand::CheckIndexedBlocks).await.ok();
			}
		});

		// check notifications
		let mut notifications = import_notifications.fuse();
		loop {
			let mut timeout =
				futures_timer::Delay::new(worker_config.read_notification_timeout).fuse();
			futures::select! {
				_ = timeout => {
					if let Ok(leaves) = substrate_backend.blockchain().leaves() {
						tx.send(WorkerCommand::IndexLeaves(leaves)).await.ok();
					}
					if sync_oracle.is_major_syncing() {
						let sinks = &mut pubsub_notification_sinks.lock();
						if !sinks.is_empty() {
							*sinks.deref_mut() = vec![];
						}
					}
				}
				notification = notifications.next() => if let Some(notification) = notification {
					log::debug!(
						target: "frontier-sql",
						"📣  New notification: #{} {:?} (parent {}), best = {}",
						notification.header.number(),
						notification.hash,
						notification.header.parent_hash(),
						notification.is_new_best,
					);
					if notification.is_new_best {
						if let Some(tree_route) = notification.tree_route {
							log::debug!(
								target: "frontier-sql",
								"🔀  Re-org happened at new best {}, proceeding to canonicalize db",
								notification.hash
							);
							let retracted = tree_route
								.retracted()
								.iter()
								.map(|hash_and_number| hash_and_number.hash)
								.collect::<Vec<_>>();
							let enacted = tree_route
								.enacted()
								.iter()
								.map(|hash_and_number| hash_and_number.hash)
								.collect::<Vec<_>>();

							let common = tree_route.common_block().hash;
							tx.send(WorkerCommand::Canonicalize {
								common,
								enacted,
								retracted,
							}).await.ok();
						}

						tx.send(WorkerCommand::IndexBestBlock(notification.hash)).await.ok();
					}
				}
			}
		}
	}
}

/// Index the provided blocks. The function loops over the ancestors of the provided nodes
/// until it encounters the genesis block, or a block that has already been imported, or
/// is already in the active set. The `hashes` parameter is populated with any parent blocks
/// that is scheduled to be indexed.
async fn index_block_and_ancestors<Block, Backend, Client>(
	client: Arc<Client>,
	substrate_backend: Arc<Backend>,
	indexer_backend: Arc<fc_db::sql::Backend<Block>>,
	hash: H256,
) where
	Block: BlockT<Hash = H256>,
	Client: ProvideRuntimeApi<Block>,
	Client::Api: EthereumRuntimeRPCApi<Block>,
	Client: HeaderBackend<Block> + StorageProvider<Block, Backend> + 'static,
	Backend: BackendT<Block> + 'static,
	Backend::State: StateBackend<BlakeTwo256>,
{
	let blockchain_backend = substrate_backend.blockchain();
	let mut hashes = vec![hash];
	while let Some(hash) = hashes.pop() {
		// exit if genesis block is reached
		if hash == H256::default() {
			break;
		}

		// exit if block is already imported
		if indexer_backend.is_block_indexed(hash).await {
			log::debug!(target: "frontier-sql", "🔴 Block {hash:?} already imported");
			break;
		}

		log::debug!(target: "frontier-sql", "🛠️  Importing {hash:?}");
		let _ = indexer_backend
			.insert_block_metadata(client.clone(), hash)
			.await
			.map_err(|e| {
				log::error!(target: "frontier-sql", "{e}");
			});
		log::debug!(target: "frontier-sql", "Inserted block metadata");
		indexer_backend.index_block_logs(client.clone(), hash).await;

		if let Ok(Some(header)) = blockchain_backend.header(hash) {
			let parent_hash = header.parent_hash();
			hashes.push(*parent_hash);
		}
	}
}

/// Index the provided known canonical blocks. The function loops over the ancestors of the provided nodes
/// until it encounters the genesis block, or a block that has already been imported, or
/// is already in the active set. The `hashes` parameter is populated with any parent blocks
/// that is scheduled to be indexed.
async fn index_canonical_block_and_ancestors<Block, Backend, Client>(
	client: Arc<Client>,
	substrate_backend: Arc<Backend>,
	indexer_backend: Arc<fc_db::sql::Backend<Block>>,
	hash: H256,
) where
	Block: BlockT<Hash = H256>,
	Client: ProvideRuntimeApi<Block>,
	Client::Api: EthereumRuntimeRPCApi<Block>,
	Client: HeaderBackend<Block> + StorageProvider<Block, Backend> + 'static,
	Backend: BackendT<Block> + 'static,
	Backend::State: StateBackend<BlakeTwo256>,
{
	let blockchain_backend = substrate_backend.blockchain();
	let mut hashes = vec![hash];
	while let Some(hash) = hashes.pop() {
		// exit if genesis block is reached
		if hash == H256::default() {
			break;
		}

		let status = indexer_backend.block_indexed_and_canon_status(hash).await;

		// exit if canonical block is already imported
		if status.indexed && status.canon {
			log::debug!(target: "frontier-sql", "🔴 Block {hash:?} already imported");
			break;
		}

		// If block was previously indexed as non-canon then mark it as canon
		if status.indexed && !status.canon {
			if let Err(err) = indexer_backend.set_block_as_canon(hash).await {
				log::error!(target: "frontier-sql", "Failed setting block {hash:?} as canon: {err:?}");
				continue;
			}

			log::debug!(target: "frontier-sql", "🛠️  Marked block as canon {hash:?}");

			// Check parent block
			if let Ok(Some(header)) = blockchain_backend.header(hash) {
				let parent_hash = header.parent_hash();
				hashes.push(*parent_hash);
			}
			continue;
		}

		// Else, import the new block
		log::debug!(target: "frontier-sql", "🛠️  Importing {hash:?}");
		let _ = indexer_backend
			.insert_block_metadata(client.clone(), hash)
			.await
			.map_err(|e| {
				log::error!(target: "frontier-sql", "{e}");
			});
		log::debug!(target: "frontier-sql", "Inserted block metadata  {hash:?}");
		indexer_backend.index_block_logs(client.clone(), hash).await;

		if let Ok(Some(header)) = blockchain_backend.header(hash) {
			let parent_hash = header.parent_hash();
			hashes.push(*parent_hash);
		}
	}
}

/// Canonicalizes the database by setting the `is_canon` field for the retracted blocks to `0`,
/// and `1` if they are enacted.
async fn canonicalize_blocks<Block: BlockT<Hash = H256>>(
	indexer_backend: Arc<fc_db::sql::Backend<Block>>,
	common: H256,
	enacted: Vec<H256>,
	retracted: Vec<H256>,
) {
	if (indexer_backend.canonicalize(&retracted, &enacted).await).is_err() {
		log::error!(
			target: "frontier-sql",
			"❌  Canonicalization failed for common ancestor {}, potentially corrupted db. Retracted: {:?}, Enacted: {:?}",
			common,
			retracted,
			enacted,
		);
	}
}

/// Attempts to index any missing blocks that are in the past. This fixes any gaps that may
/// be present in the indexing strategy, since the indexer only walks the parent hashes until
/// it finds the first ancestor that has already been indexed.
async fn index_missing_blocks<Block, Client, Backend>(
	client: Arc<Client>,
	substrate_backend: Arc<Backend>,
	indexer_backend: Arc<fc_db::sql::Backend<Block>>,
) where
	Block: BlockT<Hash = H256>,
	Client: ProvideRuntimeApi<Block>,
	Client::Api: EthereumRuntimeRPCApi<Block>,
	Client: HeaderBackend<Block> + StorageProvider<Block, Backend> + 'static,
	Backend: BackendT<Block> + 'static,
	Backend::State: StateBackend<BlakeTwo256>,
{
	if let Some(block_number) = indexer_backend.get_first_missing_canon_block().await {
		log::debug!(target: "frontier-sql", "Missing {block_number:?}");
		if block_number == 0 {
			index_genesis_block(client.clone(), indexer_backend.clone()).await;
		} else if let Ok(Some(block_hash)) = client.hash(block_number.unique_saturated_into()) {
			log::debug!(
				target: "frontier-sql",
				"Indexing past canonical blocks from #{} {:?}",
				block_number,
				block_hash,
			);
			index_canonical_block_and_ancestors(
				client.clone(),
				substrate_backend.clone(),
				indexer_backend.clone(),
				block_hash,
			)
			.await;
		} else {
			log::debug!(target: "frontier-sql", "Failed retrieving hash for block #{block_number}");
		}
	}
}

/// Attempts to index any missing blocks that are in the past. This fixes any gaps that may
/// be present in the indexing strategy, since the indexer only walks the parent hashes until
/// it finds the first ancestor that has already been indexed.
async fn index_genesis_block<Block, Client, Backend>(
	client: Arc<Client>,
	indexer_backend: Arc<fc_db::sql::Backend<Block>>,
) where
	Block: BlockT<Hash = H256>,
	Client: ProvideRuntimeApi<Block>,
	Client::Api: EthereumRuntimeRPCApi<Block>,
	Client: HeaderBackend<Block> + StorageProvider<Block, Backend> + 'static,
	Backend: BackendT<Block> + 'static,
	Backend::State: StateBackend<BlakeTwo256>,
{
	log::info!(
		target: "frontier-sql",
		"Import genesis",
	);
	if let Ok(Some(substrate_genesis_hash)) = indexer_backend
		.insert_genesis_block_metadata(client.clone())
		.await
		.map_err(|e| {
			log::error!(target: "frontier-sql", "💔  Cannot sync genesis block: {e}");
		}) {
		log::debug!(target: "frontier-sql", "Imported genesis block {substrate_genesis_hash:?}");
	}
}

#[cfg(test)]
mod test {
	use super::*;

	use std::{
		collections::BTreeMap,
		path::Path,
		sync::{Arc, Mutex},
	};

	use futures::executor;
	use scale_codec::Encode;
	use sqlx::Row;
	use tempfile::tempdir;
	// Substrate
	use sc_block_builder::BlockBuilderProvider;
	use sc_client_api::{BlockchainEvents, HeaderBackend};
	use sp_consensus::BlockOrigin;
	use sp_core::{H160, H256, U256};
	use sp_io::hashing::twox_128;
	use sp_runtime::{
		generic::{DigestItem, Header},
		traits::BlakeTwo256,
	};
	use substrate_test_runtime_client::{
		prelude::*, DefaultTestClientBuilderExt, TestClientBuilder, TestClientBuilderExt,
	};
	// Frontier
	use fc_storage::{OverrideHandle, SchemaV3Override, StorageOverride};
	use fp_storage::{
		EthereumStorageSchema, ETHEREUM_CURRENT_RECEIPTS, PALLET_ETHEREUM, PALLET_ETHEREUM_SCHEMA,
	};

	type OpaqueBlock = sp_runtime::generic::Block<
		Header<u64, BlakeTwo256>,
		substrate_test_runtime_client::runtime::Extrinsic,
	>;

	struct TestSyncOracleNotSyncing;
	impl sp_consensus::SyncOracle for TestSyncOracleNotSyncing {
		fn is_major_syncing(&self) -> bool {
			false
		}
		fn is_offline(&self) -> bool {
			false
		}
	}

	fn storage_prefix_build(module: &[u8], storage: &[u8]) -> Vec<u8> {
		[twox_128(module), twox_128(storage)].concat().to_vec()
	}

	fn ethereum_digest() -> DigestItem {
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
		let ethereum_transactions: Vec<ethereum::TransactionV2> = vec![];
		let ethereum_block = ethereum::Block::new(partial_header, ethereum_transactions, vec![]);
		DigestItem::Consensus(
			fp_consensus::FRONTIER_ENGINE_ID,
			fp_consensus::PostLog::Hashes(fp_consensus::Hashes::from_block(ethereum_block))
				.encode(),
		)
	}

	#[tokio::test]
	async fn interval_indexing_works() {
		let tmp = tempdir().expect("create a temporary directory");
		// Initialize storage with schema V3
		let builder = TestClientBuilder::new().add_extra_storage(
			PALLET_ETHEREUM_SCHEMA.to_vec(),
			Encode::encode(&EthereumStorageSchema::V3),
		);
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
		// Indexer backend
		let indexer_backend = fc_db::sql::Backend::new(
			fc_db::sql::BackendConfig::Sqlite(fc_db::sql::SqliteBackendConfig {
				path: Path::new("sqlite:///")
					.join(tmp.path())
					.join("test.db3")
					.to_str()
					.unwrap(),
				create_if_missing: true,
				cache_size: 204800,
				thread_count: 4,
			}),
			100,
			None,
			overrides.clone(),
		)
		.await
		.expect("indexer pool to be created");
		// Pool
		let pool = indexer_backend.pool().clone();

		// Create 10 blocks, 2 receipts each, 1 log per receipt
		let mut logs: Vec<(i32, fc_db::sql::Log)> = vec![];
		for block_number in 1..11 {
			// New block including pallet ethereum block digest
			let mut builder = client.new_block(Default::default()).unwrap();
			builder
				.push_deposit_log_digest_item(ethereum_digest())
				.expect("deposit log");
			// Addresses
			let address_1 = H160::repeat_byte(0x01);
			let address_2 = H160::repeat_byte(0x02);
			// Topics
			let topics_1_1 = H256::repeat_byte(0x01);
			let topics_1_2 = H256::repeat_byte(0x02);
			let topics_2_1 = H256::repeat_byte(0x03);
			let topics_2_2 = H256::repeat_byte(0x04);
			let topics_2_3 = H256::repeat_byte(0x05);
			let topics_2_4 = H256::repeat_byte(0x06);

			let receipts = Encode::encode(&vec![
				ethereum::ReceiptV3::EIP1559(ethereum::EIP1559ReceiptData {
					status_code: 0u8,
					used_gas: U256::zero(),
					logs_bloom: ethereum_types::Bloom::zero(),
					logs: vec![ethereum::Log {
						address: address_1,
						topics: vec![topics_1_1, topics_1_2],
						data: vec![],
					}],
				}),
				ethereum::ReceiptV3::EIP1559(ethereum::EIP1559ReceiptData {
					status_code: 0u8,
					used_gas: U256::zero(),
					logs_bloom: ethereum_types::Bloom::zero(),
					logs: vec![ethereum::Log {
						address: address_2,
						topics: vec![topics_2_1, topics_2_2, topics_2_3, topics_2_4],
						data: vec![],
					}],
				}),
			]);
			builder
				.push_storage_change(
					storage_prefix_build(PALLET_ETHEREUM, ETHEREUM_CURRENT_RECEIPTS),
					Some(receipts),
				)
				.unwrap();
			let block = builder.build().unwrap().block;
			let block_hash = block.header.hash();
			executor::block_on(client.import(BlockOrigin::Own, block)).unwrap();
			logs.push((
				block_number as i32,
				fc_db::sql::Log {
					address: address_1.as_bytes().to_owned(),
					topic_1: Some(topics_1_1.as_bytes().to_owned()),
					topic_2: Some(topics_1_2.as_bytes().to_owned()),
					topic_3: None,
					topic_4: None,
					log_index: 0i32,
					transaction_index: 0i32,
					substrate_block_hash: block_hash.as_bytes().to_owned(),
				},
			));
			logs.push((
				block_number as i32,
				fc_db::sql::Log {
					address: address_2.as_bytes().to_owned(),
					topic_1: Some(topics_2_1.as_bytes().to_owned()),
					topic_2: Some(topics_2_2.as_bytes().to_owned()),
					topic_3: Some(topics_2_3.as_bytes().to_owned()),
					topic_4: Some(topics_2_4.as_bytes().to_owned()),
					log_index: 0i32,
					transaction_index: 1i32,
					substrate_block_hash: block_hash.as_bytes().to_owned(),
				},
			));
		}

		let test_sync_oracle = TestSyncOracleNotSyncing {};
		let pubsub_notification_sinks: EthereumBlockNotificationSinks<
			EthereumBlockNotification<OpaqueBlock>,
		> = Default::default();
		let pubsub_notification_sinks = Arc::new(pubsub_notification_sinks);

		let pubsub_notification_sinks_inner = pubsub_notification_sinks.clone();

		// Spawn worker after creating the blocks will resolve the interval future.
		// Because the SyncWorker is spawned at service level, in the real world this will only
		// happen when we are in major syncing (where there is lack of import notificatons).
		tokio::task::spawn(async move {
			crate::sql::SyncWorker::run(
				client.clone(),
				backend.clone(),
				Arc::new(indexer_backend),
				client.clone().import_notification_stream(),
				SyncWorkerConfig {
					read_notification_timeout: Duration::from_secs(1),
					check_indexed_blocks_interval: Duration::from_secs(60),
				},
				SyncStrategy::Parachain,
				Arc::new(test_sync_oracle),
				pubsub_notification_sinks_inner,
			)
			.await
		});

		// Enough time for interval to run
		futures_timer::Delay::new(std::time::Duration::from_millis(1500)).await;

		// Query db
		let db_logs = sqlx::query(
			"SELECT
					b.block_number,
					address,
					topic_1,
					topic_2,
					topic_3,
					topic_4,
					log_index,
					transaction_index,
					a.substrate_block_hash
				FROM logs AS a INNER JOIN blocks AS b ON a.substrate_block_hash = b.substrate_block_hash
				ORDER BY b.block_number ASC, log_index ASC, transaction_index ASC",
		)
		.fetch_all(&pool)
		.await
		.expect("test query result")
		.iter()
		.map(|row| {
			let block_number = row.get::<i32, _>(0);
			let address = row.get::<Vec<u8>, _>(1);
			let topic_1 = row.get::<Option<Vec<u8>>, _>(2);
			let topic_2 = row.get::<Option<Vec<u8>>, _>(3);
			let topic_3 = row.get::<Option<Vec<u8>>, _>(4);
			let topic_4 = row.get::<Option<Vec<u8>>, _>(5);
			let log_index = row.get::<i32, _>(6);
			let transaction_index = row.get::<i32, _>(7);
			let substrate_block_hash = row.get::<Vec<u8>, _>(8);
			(
				block_number,
				fc_db::sql::Log {
					address,
					topic_1,
					topic_2,
					topic_3,
					topic_4,
					log_index,
					transaction_index,
					substrate_block_hash,
				},
			)
		})
		.collect::<Vec<(i32, fc_db::sql::Log)>>();

		// Expect the db to contain 20 rows. 10 blocks, 2 logs each.
		// Db data is sorted ASC by block_number, log_index and transaction_index.
		// This is necessary because indexing is done from tip to genesis.
		// Expect the db resultset to be equal to the locally produced Log vector.
		assert_eq!(db_logs, logs);
	}

	#[tokio::test]
	async fn notification_indexing_works() {
		let tmp = tempdir().expect("create a temporary directory");
		// Initialize storage with schema V3
		let builder = TestClientBuilder::new().add_extra_storage(
			PALLET_ETHEREUM_SCHEMA.to_vec(),
			Encode::encode(&EthereumStorageSchema::V3),
		);
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
		// Indexer backend
		let indexer_backend = fc_db::sql::Backend::new(
			fc_db::sql::BackendConfig::Sqlite(fc_db::sql::SqliteBackendConfig {
				path: Path::new("sqlite:///")
					.join(tmp.path())
					.join("test.db3")
					.to_str()
					.unwrap(),
				create_if_missing: true,
				cache_size: 204800,
				thread_count: 4,
			}),
			100,
			None,
			overrides.clone(),
		)
		.await
		.expect("indexer pool to be created");
		// Pool
		let pool = indexer_backend.pool().clone();

		let test_sync_oracle = TestSyncOracleNotSyncing {};
		let pubsub_notification_sinks: EthereumBlockNotificationSinks<
			EthereumBlockNotification<OpaqueBlock>,
		> = Default::default();
		let pubsub_notification_sinks = Arc::new(pubsub_notification_sinks);

		let pubsub_notification_sinks_inner = pubsub_notification_sinks.clone();

		// Spawn worker after creating the blocks will resolve the interval future.
		// Because the SyncWorker is spawned at service level, in the real world this will only
		// happen when we are in major syncing (where there is lack of import notifications).
		let notification_stream = client.clone().import_notification_stream();
		let client_inner = client.clone();
		tokio::task::spawn(async move {
			crate::sql::SyncWorker::run(
				client_inner,
				backend.clone(),
				Arc::new(indexer_backend),
				notification_stream,
				SyncWorkerConfig {
					read_notification_timeout: Duration::from_secs(10),
					check_indexed_blocks_interval: Duration::from_secs(60),
				},
				SyncStrategy::Parachain,
				Arc::new(test_sync_oracle),
				pubsub_notification_sinks_inner,
			)
			.await
		});

		// Create 10 blocks, 2 receipts each, 1 log per receipt
		let mut logs: Vec<(i32, fc_db::sql::Log)> = vec![];
		for block_number in 1..11 {
			// New block including pallet ethereum block digest
			let mut builder = client.new_block(Default::default()).unwrap();
			builder
				.push_deposit_log_digest_item(ethereum_digest())
				.expect("deposit log");
			// Addresses
			let address_1 = H160::random();
			let address_2 = H160::random();
			// Topics
			let topics_1_1 = H256::random();
			let topics_1_2 = H256::random();
			let topics_2_1 = H256::random();
			let topics_2_2 = H256::random();
			let topics_2_3 = H256::random();
			let topics_2_4 = H256::random();

			let receipts = Encode::encode(&vec![
				ethereum::ReceiptV3::EIP1559(ethereum::EIP1559ReceiptData {
					status_code: 0u8,
					used_gas: U256::zero(),
					logs_bloom: ethereum_types::Bloom::zero(),
					logs: vec![ethereum::Log {
						address: address_1,
						topics: vec![topics_1_1, topics_1_2],
						data: vec![],
					}],
				}),
				ethereum::ReceiptV3::EIP1559(ethereum::EIP1559ReceiptData {
					status_code: 0u8,
					used_gas: U256::zero(),
					logs_bloom: ethereum_types::Bloom::zero(),
					logs: vec![ethereum::Log {
						address: address_2,
						topics: vec![topics_2_1, topics_2_2, topics_2_3, topics_2_4],
						data: vec![],
					}],
				}),
			]);
			builder
				.push_storage_change(
					storage_prefix_build(PALLET_ETHEREUM, ETHEREUM_CURRENT_RECEIPTS),
					Some(receipts),
				)
				.unwrap();
			let block = builder.build().unwrap().block;
			let block_hash = block.header.hash();
			executor::block_on(client.import(BlockOrigin::Own, block)).unwrap();
			logs.push((
				block_number as i32,
				fc_db::sql::Log {
					address: address_1.as_bytes().to_owned(),
					topic_1: Some(topics_1_1.as_bytes().to_owned()),
					topic_2: Some(topics_1_2.as_bytes().to_owned()),
					topic_3: None,
					topic_4: None,
					log_index: 0i32,
					transaction_index: 0i32,
					substrate_block_hash: block_hash.as_bytes().to_owned(),
				},
			));
			logs.push((
				block_number as i32,
				fc_db::sql::Log {
					address: address_2.as_bytes().to_owned(),
					topic_1: Some(topics_2_1.as_bytes().to_owned()),
					topic_2: Some(topics_2_2.as_bytes().to_owned()),
					topic_3: Some(topics_2_3.as_bytes().to_owned()),
					topic_4: Some(topics_2_4.as_bytes().to_owned()),
					log_index: 0i32,
					transaction_index: 1i32,
					substrate_block_hash: block_hash.as_bytes().to_owned(),
				},
			));
			// Let's not notify too quickly
			futures_timer::Delay::new(std::time::Duration::from_millis(100)).await;
		}

		// Query db
		let db_logs = sqlx::query(
			"SELECT
					b.block_number,
					address,
					topic_1,
					topic_2,
					topic_3,
					topic_4,
					log_index,
					transaction_index,
					a.substrate_block_hash
				FROM logs AS a INNER JOIN blocks AS b ON a.substrate_block_hash = b.substrate_block_hash
				ORDER BY b.block_number ASC, log_index ASC, transaction_index ASC",
		)
		.fetch_all(&pool)
		.await
		.expect("test query result")
		.iter()
		.map(|row| {
			let block_number = row.get::<i32, _>(0);
			let address = row.get::<Vec<u8>, _>(1);
			let topic_1 = row.get::<Option<Vec<u8>>, _>(2);
			let topic_2 = row.get::<Option<Vec<u8>>, _>(3);
			let topic_3 = row.get::<Option<Vec<u8>>, _>(4);
			let topic_4 = row.get::<Option<Vec<u8>>, _>(5);
			let log_index = row.get::<i32, _>(6);
			let transaction_index = row.get::<i32, _>(7);
			let substrate_block_hash = row.get::<Vec<u8>, _>(8);
			(
				block_number,
				fc_db::sql::Log {
					address,
					topic_1,
					topic_2,
					topic_3,
					topic_4,
					log_index,
					transaction_index,
					substrate_block_hash,
				},
			)
		})
		.collect::<Vec<(i32, fc_db::sql::Log)>>();

		// Expect the db to contain 20 rows. 10 blocks, 2 logs each.
		// Db data is sorted ASC by block_number, log_index and transaction_index.
		// This is necessary because indexing is done from tip to genesis.
		// Expect the db resultset to be equal to the locally produced Log vector.
		assert_eq!(db_logs, logs);
	}

	#[tokio::test]
	async fn canonicalize_works() {
		let tmp = tempdir().expect("create a temporary directory");
		// Initialize storage with schema V3
		let builder = TestClientBuilder::new().add_extra_storage(
			PALLET_ETHEREUM_SCHEMA.to_vec(),
			Encode::encode(&EthereumStorageSchema::V3),
		);
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
		// Indexer backend
		let indexer_backend = fc_db::sql::Backend::new(
			fc_db::sql::BackendConfig::Sqlite(fc_db::sql::SqliteBackendConfig {
				path: Path::new("sqlite:///")
					.join(tmp.path())
					.join("test.db3")
					.to_str()
					.unwrap(),
				create_if_missing: true,
				cache_size: 204800,
				thread_count: 4,
			}),
			100,
			None,
			overrides.clone(),
		)
		.await
		.expect("indexer pool to be created");

		// Pool
		let pool = indexer_backend.pool().clone();

		// Spawn indexer task
		let test_sync_oracle = TestSyncOracleNotSyncing {};
		let pubsub_notification_sinks: EthereumBlockNotificationSinks<
			EthereumBlockNotification<OpaqueBlock>,
		> = Default::default();
		let pubsub_notification_sinks = Arc::new(pubsub_notification_sinks);

		let pubsub_notification_sinks_inner = pubsub_notification_sinks.clone();

		let notification_stream = client.clone().import_notification_stream();
		let client_inner = client.clone();
		tokio::task::spawn(async move {
			crate::sql::SyncWorker::run(
				client_inner,
				backend.clone(),
				Arc::new(indexer_backend),
				notification_stream,
				SyncWorkerConfig {
					read_notification_timeout: Duration::from_secs(10),
					check_indexed_blocks_interval: Duration::from_secs(60),
				},
				SyncStrategy::Parachain,
				Arc::new(test_sync_oracle),
				pubsub_notification_sinks_inner,
			)
			.await
		});

		// Create 10 blocks saving the common ancestor for branching.
		let mut parent_hash = client
			.hash(sp_runtime::traits::Zero::zero())
			.unwrap()
			.expect("genesis hash");
		let mut common_ancestor = parent_hash;
		let mut hashes_to_be_orphaned: Vec<H256> = vec![];
		for block_number in 1..11 {
			// New block including pallet ethereum block digest
			let mut builder = client
				.new_block_at(parent_hash, Default::default(), false)
				.unwrap();
			builder
				.push_deposit_log_digest_item(ethereum_digest())
				.expect("deposit log");
			let block = builder.build().unwrap().block;
			let block_hash = block.header.hash();
			executor::block_on(client.import(BlockOrigin::Own, block)).unwrap();
			if block_number == 8 {
				common_ancestor = block_hash;
			}
			if block_number == 9 || block_number == 10 {
				hashes_to_be_orphaned.push(block_hash);
			}
			parent_hash = block_hash;
			// Let's not notify too quickly
			futures_timer::Delay::new(std::time::Duration::from_millis(100)).await;
		}

		// Test all blocks are initially canon.
		let mut res = sqlx::query("SELECT is_canon FROM blocks")
			.fetch_all(&pool)
			.await
			.expect("test query result")
			.iter()
			.map(|row| row.get::<i32, _>(0))
			.collect::<Vec<i32>>();

		assert_eq!(res.len(), 10);
		res.dedup();
		assert_eq!(res.len(), 1);

		// Create the new longest chain, 10 more blocks on top of the common ancestor.
		parent_hash = common_ancestor;
		for _ in 1..11 {
			// New block including pallet ethereum block digest
			let mut builder = client
				.new_block_at(parent_hash, Default::default(), false)
				.unwrap();
			builder
				.push_deposit_log_digest_item(ethereum_digest())
				.expect("deposit log");
			let block = builder.build().unwrap().block;
			let block_hash = block.header.hash();
			executor::block_on(client.import(BlockOrigin::Own, block)).unwrap();
			parent_hash = block_hash;
			// Let's not notify too quickly
			futures_timer::Delay::new(std::time::Duration::from_millis(100)).await;
		}

		// Test the reorged chain is correctly indexed.
		let res = sqlx::query("SELECT substrate_block_hash, is_canon, block_number FROM blocks")
			.fetch_all(&pool)
			.await
			.expect("test query result")
			.iter()
			.map(|row| {
				let substrate_block_hash = H256::from_slice(&row.get::<Vec<u8>, _>(0)[..]);
				let is_canon = row.get::<i32, _>(1);
				let block_number = row.get::<i32, _>(2);
				(substrate_block_hash, is_canon, block_number)
			})
			.collect::<Vec<(H256, i32, i32)>>();

		// 20 blocks in total
		assert_eq!(res.len(), 20);

		// 18 of which are canon
		let canon = res
			.clone()
			.into_iter()
			.filter_map(|it| if it.1 == 1 { Some(it) } else { None })
			.collect::<Vec<(H256, i32, i32)>>();
		assert_eq!(canon.len(), 18);

		// and 2 of which are the originally tracked as orphaned
		let not_canon = res
			.clone()
			.into_iter()
			.filter_map(|it| if it.1 == 0 { Some(it.0) } else { None })
			.collect::<Vec<H256>>();
		assert_eq!(not_canon.len(), hashes_to_be_orphaned.len());
		assert!(not_canon.iter().all(|h| hashes_to_be_orphaned.contains(h)));
	}

	#[tokio::test]
	async fn resuming_from_last_indexed_block_works() {
		let tmp = tempdir().expect("create a temporary directory");
		// Initialize storage with schema V3
		let builder = TestClientBuilder::new().add_extra_storage(
			PALLET_ETHEREUM_SCHEMA.to_vec(),
			Encode::encode(&EthereumStorageSchema::V3),
		);
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
		// Indexer backend
		let indexer_backend = fc_db::sql::Backend::new(
			fc_db::sql::BackendConfig::Sqlite(fc_db::sql::SqliteBackendConfig {
				path: Path::new("sqlite:///")
					.join(tmp.path())
					.join("test.db3")
					.to_str()
					.unwrap(),
				create_if_missing: true,
				cache_size: 204800,
				thread_count: 4,
			}),
			100,
			None,
			overrides.clone(),
		)
		.await
		.expect("indexer pool to be created");

		// Pool
		let pool = indexer_backend.pool().clone();

		// Create 5 blocks, storing them newest first.
		let mut parent_hash = client
			.hash(sp_runtime::traits::Zero::zero())
			.unwrap()
			.expect("genesis hash");
		let mut best_block_hashes: Vec<H256> = vec![];
		for _block_number in 1..=5 {
			let mut builder = client
				.new_block_at(parent_hash, Default::default(), false)
				.unwrap();
			builder
				.push_deposit_log_digest_item(ethereum_digest())
				.expect("deposit log");
			let block = builder.build().unwrap().block;
			let block_hash = block.header.hash();
			executor::block_on(client.import(BlockOrigin::Own, block)).unwrap();
			best_block_hashes.insert(0, block_hash.clone());
			parent_hash = block_hash;
		}

		// Mark the block as canon and indexed
		let block_resume_at = best_block_hashes[0];
		sqlx::query("INSERT INTO blocks(substrate_block_hash, ethereum_block_hash, ethereum_storage_schema, block_number, is_canon) VALUES (?, ?, ?, 5, 1)")
			.bind(block_resume_at.as_bytes())
			.bind(H256::zero().as_bytes())
			.bind(H256::zero().as_bytes())
			.execute(&pool)
			.await
			.expect("sql query must succeed");
		sqlx::query("INSERT INTO sync_status(substrate_block_hash, status) VALUES (?, 1)")
			.bind(block_resume_at.as_bytes())
			.execute(&pool)
			.await
			.expect("sql query must succeed");

		// Spawn indexer task
		let test_sync_oracle = TestSyncOracleNotSyncing {};
		let pubsub_notification_sinks: EthereumBlockNotificationSinks<
			EthereumBlockNotification<OpaqueBlock>,
		> = Default::default();
		let pubsub_notification_sinks = Arc::new(pubsub_notification_sinks);

		let pubsub_notification_sinks_inner = pubsub_notification_sinks.clone();

		let client_inner = client.clone();
		tokio::task::spawn(async move {
			crate::sql::SyncWorker::run(
				client_inner,
				backend.clone(),
				Arc::new(indexer_backend),
				client.clone().import_notification_stream(),
				SyncWorkerConfig {
					read_notification_timeout: Duration::from_secs(10),
					check_indexed_blocks_interval: Duration::from_secs(60),
				},
				SyncStrategy::Parachain,
				Arc::new(test_sync_oracle),
				pubsub_notification_sinks_inner,
			)
			.await
		});
		// Enough time for indexing
		futures_timer::Delay::new(std::time::Duration::from_millis(1500)).await;

		// Test the reorged chain is correctly indexed.
		let actual_imported_blocks =
			sqlx::query("SELECT substrate_block_hash, is_canon, block_number FROM blocks")
				.fetch_all(&pool)
				.await
				.expect("test query result")
				.iter()
				.map(|row| H256::from_slice(&row.get::<Vec<u8>, _>(0)[..]))
				.collect::<Vec<H256>>();
		let expected_imported_blocks = best_block_hashes.clone();
		assert_eq!(expected_imported_blocks, actual_imported_blocks);
	}

	struct TestSyncOracle {
		sync_status: Arc<Mutex<bool>>,
	}
	impl sp_consensus::SyncOracle for TestSyncOracle {
		fn is_major_syncing(&self) -> bool {
			*self.sync_status.lock().expect("failed getting lock")
		}
		fn is_offline(&self) -> bool {
			false
		}
	}

	struct TestSyncOracleWrapper {
		oracle: Arc<TestSyncOracle>,
		sync_status: Arc<Mutex<bool>>,
	}
	impl TestSyncOracleWrapper {
		fn new() -> Self {
			let sync_status = Arc::new(Mutex::new(false));
			TestSyncOracleWrapper {
				oracle: Arc::new(TestSyncOracle {
					sync_status: sync_status.clone(),
				}),
				sync_status,
			}
		}
		fn set_sync_status(&mut self, value: bool) {
			*self.sync_status.lock().expect("failed getting lock") = value;
		}
	}

	#[tokio::test]
	async fn sync_strategy_normal_indexes_best_blocks_if_not_major_sync() {
		let tmp = tempdir().expect("create a temporary directory");
		let builder = TestClientBuilder::new().add_extra_storage(
			PALLET_ETHEREUM_SCHEMA.to_vec(),
			Encode::encode(&EthereumStorageSchema::V3),
		);
		let backend = builder.backend();
		let (client, _) =
			builder.build_with_native_executor::<frontier_template_runtime::RuntimeApi, _>(None);
		let mut client = Arc::new(client);
		let mut overrides_map = BTreeMap::new();
		overrides_map.insert(
			EthereumStorageSchema::V3,
			Box::new(SchemaV3Override::new(client.clone())) as Box<dyn StorageOverride<_>>,
		);
		let overrides = Arc::new(OverrideHandle {
			schemas: overrides_map,
			fallback: Box::new(SchemaV3Override::new(client.clone())),
		});
		let indexer_backend = fc_db::sql::Backend::new(
			fc_db::sql::BackendConfig::Sqlite(fc_db::sql::SqliteBackendConfig {
				path: Path::new("sqlite:///")
					.join(tmp.path())
					.join("test.db3")
					.to_str()
					.unwrap(),
				create_if_missing: true,
				cache_size: 204800,
				thread_count: 4,
			}),
			100,
			None,
			overrides.clone(),
		)
		.await
		.expect("indexer pool to be created");

		// Pool
		let pool = indexer_backend.pool().clone();

		// Spawn indexer task
		let pubsub_notification_sinks: crate::EthereumBlockNotificationSinks<
			crate::EthereumBlockNotification<OpaqueBlock>,
		> = Default::default();
		let pubsub_notification_sinks = Arc::new(pubsub_notification_sinks);
		let mut sync_oracle_wrapper = TestSyncOracleWrapper::new();
		let sync_oracle = sync_oracle_wrapper.oracle.clone();
		let client_inner = client.clone();
		tokio::task::spawn(async move {
			crate::sql::SyncWorker::run(
				client_inner.clone(),
				backend.clone(),
				Arc::new(indexer_backend),
				client_inner.import_notification_stream(),
				SyncWorkerConfig {
					read_notification_timeout: Duration::from_secs(10),
					check_indexed_blocks_interval: Duration::from_secs(60),
				},
				SyncStrategy::Normal,
				Arc::new(sync_oracle),
				pubsub_notification_sinks.clone(),
			)
			.await
		});
		// Enough time for startup
		futures_timer::Delay::new(std::time::Duration::from_millis(200)).await;

		// Import 3 blocks as part of normal operation, storing them oldest first.
		sync_oracle_wrapper.set_sync_status(false);
		let mut parent_hash = client
			.hash(sp_runtime::traits::Zero::zero())
			.unwrap()
			.expect("genesis hash");
		let mut best_block_hashes: Vec<H256> = vec![];
		for _block_number in 1..=3 {
			let mut builder = client
				.new_block_at(parent_hash, Default::default(), false)
				.unwrap();
			builder
				.push_deposit_log_digest_item(ethereum_digest())
				.expect("deposit log");
			let block = builder.build().unwrap().block;
			let block_hash = block.header.hash();

			executor::block_on(client.import(BlockOrigin::Own, block)).unwrap();
			best_block_hashes.push(block_hash.clone());
			parent_hash = block_hash;
		}

		// Enough time for indexing
		futures_timer::Delay::new(std::time::Duration::from_millis(3000)).await;

		// Test the chain is correctly indexed.
		let actual_imported_blocks =
			sqlx::query("SELECT substrate_block_hash, is_canon, block_number FROM blocks")
				.fetch_all(&pool)
				.await
				.expect("test query result")
				.iter()
				.map(|row| H256::from_slice(&row.get::<Vec<u8>, _>(0)[..]))
				.collect::<Vec<H256>>();
		let expected_imported_blocks = best_block_hashes.clone();
		assert_eq!(expected_imported_blocks, actual_imported_blocks);
	}

	#[tokio::test]
	async fn sync_strategy_normal_ignores_non_best_block_if_not_major_sync() {
		let tmp = tempdir().expect("create a temporary directory");
		let builder = TestClientBuilder::new().add_extra_storage(
			PALLET_ETHEREUM_SCHEMA.to_vec(),
			Encode::encode(&EthereumStorageSchema::V3),
		);
		let backend = builder.backend();
		let (client, _) =
			builder.build_with_native_executor::<frontier_template_runtime::RuntimeApi, _>(None);
		let mut client = Arc::new(client);
		let mut overrides_map = BTreeMap::new();
		overrides_map.insert(
			EthereumStorageSchema::V3,
			Box::new(SchemaV3Override::new(client.clone())) as Box<dyn StorageOverride<_>>,
		);
		let overrides = Arc::new(OverrideHandle {
			schemas: overrides_map,
			fallback: Box::new(SchemaV3Override::new(client.clone())),
		});
		let indexer_backend = fc_db::sql::Backend::new(
			fc_db::sql::BackendConfig::Sqlite(fc_db::sql::SqliteBackendConfig {
				path: Path::new("sqlite:///")
					.join(tmp.path())
					.join("test.db3")
					.to_str()
					.unwrap(),
				create_if_missing: true,
				cache_size: 204800,
				thread_count: 4,
			}),
			100,
			None,
			overrides.clone(),
		)
		.await
		.expect("indexer pool to be created");

		// Pool
		let pool = indexer_backend.pool().clone();

		// Spawn indexer task
		let pubsub_notification_sinks: crate::EthereumBlockNotificationSinks<
			crate::EthereumBlockNotification<OpaqueBlock>,
		> = Default::default();
		let pubsub_notification_sinks = Arc::new(pubsub_notification_sinks);
		let mut sync_oracle_wrapper = TestSyncOracleWrapper::new();
		let sync_oracle = sync_oracle_wrapper.oracle.clone();
		let client_inner = client.clone();
		tokio::task::spawn(async move {
			crate::sql::SyncWorker::run(
				client_inner.clone(),
				backend.clone(),
				Arc::new(indexer_backend),
				client_inner.import_notification_stream(),
				SyncWorkerConfig {
					read_notification_timeout: Duration::from_secs(10),
					check_indexed_blocks_interval: Duration::from_secs(60),
				},
				SyncStrategy::Normal,
				Arc::new(sync_oracle),
				pubsub_notification_sinks.clone(),
			)
			.await
		});
		// Enough time for startup
		futures_timer::Delay::new(std::time::Duration::from_millis(200)).await;

		// Import 3 blocks as part of normal operation, storing them oldest first.
		sync_oracle_wrapper.set_sync_status(false);
		let mut parent_hash = client
			.hash(sp_runtime::traits::Zero::zero())
			.unwrap()
			.expect("genesis hash");
		let mut best_block_hashes: Vec<H256> = vec![];
		for _block_number in 1..=3 {
			let mut builder = client
				.new_block_at(parent_hash, Default::default(), false)
				.unwrap();
			builder
				.push_deposit_log_digest_item(ethereum_digest())
				.expect("deposit log");
			let block = builder.build().unwrap().block;
			let block_hash = block.header.hash();

			executor::block_on(client.import(BlockOrigin::Own, block)).unwrap();
			best_block_hashes.push(block_hash.clone());
			parent_hash = block_hash;
		}

		// create non-best block
		let mut builder = client
			.new_block_at(best_block_hashes[0], Default::default(), false)
			.unwrap();
		builder
			.push_deposit_log_digest_item(ethereum_digest())
			.expect("deposit log");
		let block = builder.build().unwrap().block;

		executor::block_on(client.import(BlockOrigin::Own, block)).unwrap();

		// Enough time for indexing
		futures_timer::Delay::new(std::time::Duration::from_millis(3000)).await;

		// Test the chain is correctly indexed.
		let actual_imported_blocks =
			sqlx::query("SELECT substrate_block_hash, is_canon, block_number FROM blocks")
				.fetch_all(&pool)
				.await
				.expect("test query result")
				.iter()
				.map(|row| H256::from_slice(&row.get::<Vec<u8>, _>(0)[..]))
				.collect::<Vec<H256>>();
		let expected_imported_blocks = best_block_hashes.clone();
		assert_eq!(expected_imported_blocks, actual_imported_blocks);
	}

	#[tokio::test]
	async fn sync_strategy_parachain_indexes_best_blocks_if_not_major_sync() {
		let tmp = tempdir().expect("create a temporary directory");
		let builder = TestClientBuilder::new().add_extra_storage(
			PALLET_ETHEREUM_SCHEMA.to_vec(),
			Encode::encode(&EthereumStorageSchema::V3),
		);
		let backend = builder.backend();
		let (client, _) =
			builder.build_with_native_executor::<frontier_template_runtime::RuntimeApi, _>(None);
		let mut client = Arc::new(client);
		let mut overrides_map = BTreeMap::new();
		overrides_map.insert(
			EthereumStorageSchema::V3,
			Box::new(SchemaV3Override::new(client.clone())) as Box<dyn StorageOverride<_>>,
		);
		let overrides = Arc::new(OverrideHandle {
			schemas: overrides_map,
			fallback: Box::new(SchemaV3Override::new(client.clone())),
		});
		let indexer_backend = fc_db::sql::Backend::new(
			fc_db::sql::BackendConfig::Sqlite(fc_db::sql::SqliteBackendConfig {
				path: Path::new("sqlite:///")
					.join(tmp.path())
					.join("test.db3")
					.to_str()
					.unwrap(),
				create_if_missing: true,
				cache_size: 204800,
				thread_count: 4,
			}),
			100,
			None,
			overrides.clone(),
		)
		.await
		.expect("indexer pool to be created");

		// Pool
		let pool = indexer_backend.pool().clone();

		// Spawn indexer task
		let pubsub_notification_sinks: crate::EthereumBlockNotificationSinks<
			crate::EthereumBlockNotification<OpaqueBlock>,
		> = Default::default();
		let pubsub_notification_sinks = Arc::new(pubsub_notification_sinks);
		let mut sync_oracle_wrapper = TestSyncOracleWrapper::new();
		let sync_oracle = sync_oracle_wrapper.oracle.clone();
		let client_inner = client.clone();
		tokio::task::spawn(async move {
			crate::sql::SyncWorker::run(
				client_inner.clone(),
				backend.clone(),
				Arc::new(indexer_backend),
				client_inner.import_notification_stream(),
				SyncWorkerConfig {
					read_notification_timeout: Duration::from_secs(10),
					check_indexed_blocks_interval: Duration::from_secs(60),
				},
				SyncStrategy::Parachain,
				Arc::new(sync_oracle),
				pubsub_notification_sinks.clone(),
			)
			.await
		});
		// Enough time for startup
		futures_timer::Delay::new(std::time::Duration::from_millis(200)).await;

		// Import 3 blocks as part of normal operation, storing them oldest first.
		sync_oracle_wrapper.set_sync_status(false);
		let mut parent_hash = client
			.hash(sp_runtime::traits::Zero::zero())
			.unwrap()
			.expect("genesis hash");
		let mut best_block_hashes: Vec<H256> = vec![];
		for _block_number in 1..=3 {
			let mut builder = client
				.new_block_at(parent_hash, Default::default(), false)
				.unwrap();
			builder
				.push_deposit_log_digest_item(ethereum_digest())
				.expect("deposit log");
			let block = builder.build().unwrap().block;
			let block_hash = block.header.hash();

			executor::block_on(client.import(BlockOrigin::Own, block)).unwrap();
			best_block_hashes.push(block_hash.clone());
			parent_hash = block_hash;
		}

		// Enough time for indexing
		futures_timer::Delay::new(std::time::Duration::from_millis(3000)).await;

		// Test the chain is correctly indexed.
		let actual_imported_blocks =
			sqlx::query("SELECT substrate_block_hash, is_canon, block_number FROM blocks")
				.fetch_all(&pool)
				.await
				.expect("test query result")
				.iter()
				.map(|row| H256::from_slice(&row.get::<Vec<u8>, _>(0)[..]))
				.collect::<Vec<H256>>();
		let expected_imported_blocks = best_block_hashes.clone();
		assert_eq!(expected_imported_blocks, actual_imported_blocks);
	}

	#[tokio::test]
	async fn sync_strategy_parachain_ignores_non_best_blocks_if_not_major_sync() {
		let tmp = tempdir().expect("create a temporary directory");
		let builder = TestClientBuilder::new().add_extra_storage(
			PALLET_ETHEREUM_SCHEMA.to_vec(),
			Encode::encode(&EthereumStorageSchema::V3),
		);
		let backend = builder.backend();
		let (client, _) =
			builder.build_with_native_executor::<frontier_template_runtime::RuntimeApi, _>(None);
		let mut client = Arc::new(client);
		let mut overrides_map = BTreeMap::new();
		overrides_map.insert(
			EthereumStorageSchema::V3,
			Box::new(SchemaV3Override::new(client.clone())) as Box<dyn StorageOverride<_>>,
		);
		let overrides = Arc::new(OverrideHandle {
			schemas: overrides_map,
			fallback: Box::new(SchemaV3Override::new(client.clone())),
		});
		let indexer_backend = fc_db::sql::Backend::new(
			fc_db::sql::BackendConfig::Sqlite(fc_db::sql::SqliteBackendConfig {
				path: Path::new("sqlite:///")
					.join(tmp.path())
					.join("test.db3")
					.to_str()
					.unwrap(),
				create_if_missing: true,
				cache_size: 204800,
				thread_count: 4,
			}),
			100,
			None,
			overrides.clone(),
		)
		.await
		.expect("indexer pool to be created");

		// Pool
		let pool = indexer_backend.pool().clone();

		// Spawn indexer task
		let pubsub_notification_sinks: crate::EthereumBlockNotificationSinks<
			crate::EthereumBlockNotification<OpaqueBlock>,
		> = Default::default();
		let pubsub_notification_sinks = Arc::new(pubsub_notification_sinks);
		let mut sync_oracle_wrapper = TestSyncOracleWrapper::new();
		let sync_oracle = sync_oracle_wrapper.oracle.clone();
		let client_inner = client.clone();
		tokio::task::spawn(async move {
			crate::sql::SyncWorker::run(
				client_inner.clone(),
				backend.clone(),
				Arc::new(indexer_backend),
				client_inner.import_notification_stream(),
				SyncWorkerConfig {
					read_notification_timeout: Duration::from_secs(10),
					check_indexed_blocks_interval: Duration::from_secs(60),
				},
				SyncStrategy::Parachain,
				Arc::new(sync_oracle),
				pubsub_notification_sinks.clone(),
			)
			.await
		});
		// Enough time for startup
		futures_timer::Delay::new(std::time::Duration::from_millis(200)).await;

		// Import 3 blocks as part of normal operation, storing them oldest first.
		sync_oracle_wrapper.set_sync_status(false);
		let mut parent_hash = client
			.hash(sp_runtime::traits::Zero::zero())
			.unwrap()
			.expect("genesis hash");
		let mut best_block_hashes: Vec<H256> = vec![];
		for _block_number in 1..=3 {
			let mut builder = client
				.new_block_at(parent_hash, Default::default(), false)
				.unwrap();
			builder
				.push_deposit_log_digest_item(ethereum_digest())
				.expect("deposit log");
			let block = builder.build().unwrap().block;
			let block_hash = block.header.hash();

			executor::block_on(client.import(BlockOrigin::Own, block)).unwrap();
			best_block_hashes.push(block_hash.clone());
			parent_hash = block_hash;
		}

		// create non-best block
		let mut builder = client
			.new_block_at(best_block_hashes[0], Default::default(), false)
			.unwrap();
		builder
			.push_deposit_log_digest_item(ethereum_digest())
			.expect("deposit log");
		let block = builder.build().unwrap().block;

		executor::block_on(client.import(BlockOrigin::Own, block)).unwrap();

		// Enough time for indexing
		futures_timer::Delay::new(std::time::Duration::from_millis(3000)).await;

		// Test the chain is correctly indexed.
		let actual_imported_blocks =
			sqlx::query("SELECT substrate_block_hash, is_canon, block_number FROM blocks")
				.fetch_all(&pool)
				.await
				.expect("test query result")
				.iter()
				.map(|row| H256::from_slice(&row.get::<Vec<u8>, _>(0)[..]))
				.collect::<Vec<H256>>();
		let expected_imported_blocks = best_block_hashes.clone();
		assert_eq!(expected_imported_blocks, actual_imported_blocks);
	}

	#[tokio::test]
	async fn sync_strategy_normal_ignores_best_blocks_if_major_sync() {
		let tmp = tempdir().expect("create a temporary directory");
		let builder = TestClientBuilder::new().add_extra_storage(
			PALLET_ETHEREUM_SCHEMA.to_vec(),
			Encode::encode(&EthereumStorageSchema::V3),
		);
		let backend = builder.backend();
		let (client, _) =
			builder.build_with_native_executor::<frontier_template_runtime::RuntimeApi, _>(None);
		let mut client = Arc::new(client);
		let mut overrides_map = BTreeMap::new();
		overrides_map.insert(
			EthereumStorageSchema::V3,
			Box::new(SchemaV3Override::new(client.clone())) as Box<dyn StorageOverride<_>>,
		);
		let overrides = Arc::new(OverrideHandle {
			schemas: overrides_map,
			fallback: Box::new(SchemaV3Override::new(client.clone())),
		});
		let indexer_backend = fc_db::sql::Backend::new(
			fc_db::sql::BackendConfig::Sqlite(fc_db::sql::SqliteBackendConfig {
				path: Path::new("sqlite:///")
					.join(tmp.path())
					.join("test.db3")
					.to_str()
					.unwrap(),
				create_if_missing: true,
				cache_size: 204800,
				thread_count: 4,
			}),
			100,
			None,
			overrides.clone(),
		)
		.await
		.expect("indexer pool to be created");

		// Pool
		let pool = indexer_backend.pool().clone();

		// Spawn indexer task
		let pubsub_notification_sinks: crate::EthereumBlockNotificationSinks<
			crate::EthereumBlockNotification<OpaqueBlock>,
		> = Default::default();
		let pubsub_notification_sinks = Arc::new(pubsub_notification_sinks);
		let mut sync_oracle_wrapper = TestSyncOracleWrapper::new();
		let sync_oracle = sync_oracle_wrapper.oracle.clone();
		let client_inner = client.clone();
		tokio::task::spawn(async move {
			crate::sql::SyncWorker::run(
				client_inner.clone(),
				backend.clone(),
				Arc::new(indexer_backend),
				client_inner.import_notification_stream(),
				SyncWorkerConfig {
					read_notification_timeout: Duration::from_secs(10),
					check_indexed_blocks_interval: Duration::from_secs(60),
				},
				SyncStrategy::Normal,
				Arc::new(sync_oracle),
				pubsub_notification_sinks.clone(),
			)
			.await
		});
		// Enough time for startup
		futures_timer::Delay::new(std::time::Duration::from_millis(200)).await;

		// Import 3 blocks as part of initial network sync, storing them oldest first.
		sync_oracle_wrapper.set_sync_status(true);
		let mut parent_hash = client
			.hash(sp_runtime::traits::Zero::zero())
			.unwrap()
			.expect("genesis hash");
		let mut best_block_hashes: Vec<H256> = vec![];
		for _block_number in 1..=3 {
			let mut builder = client
				.new_block_at(parent_hash, Default::default(), false)
				.unwrap();
			builder
				.push_deposit_log_digest_item(ethereum_digest())
				.expect("deposit log");
			let block = builder.build().unwrap().block;
			let block_hash = block.header.hash();

			executor::block_on(client.import(BlockOrigin::NetworkInitialSync, block)).unwrap();
			best_block_hashes.push(block_hash.clone());
			parent_hash = block_hash;
		}

		// Enough time for indexing
		futures_timer::Delay::new(std::time::Duration::from_millis(3000)).await;

		// Test the chain is correctly indexed.
		let actual_imported_blocks =
			sqlx::query("SELECT substrate_block_hash, is_canon, block_number FROM blocks")
				.fetch_all(&pool)
				.await
				.expect("test query result")
				.iter()
				.map(|row| H256::from_slice(&row.get::<Vec<u8>, _>(0)[..]))
				.collect::<Vec<H256>>();
		let expected_imported_blocks = Vec::<H256>::new();
		assert_eq!(expected_imported_blocks, actual_imported_blocks);
	}

	#[tokio::test]
	async fn sync_strategy_parachain_ignores_best_blocks_if_major_sync() {
		let tmp = tempdir().expect("create a temporary directory");
		let builder = TestClientBuilder::new().add_extra_storage(
			PALLET_ETHEREUM_SCHEMA.to_vec(),
			Encode::encode(&EthereumStorageSchema::V3),
		);
		let backend = builder.backend();
		let (client, _) =
			builder.build_with_native_executor::<frontier_template_runtime::RuntimeApi, _>(None);
		let mut client = Arc::new(client);
		let mut overrides_map = BTreeMap::new();
		overrides_map.insert(
			EthereumStorageSchema::V3,
			Box::new(SchemaV3Override::new(client.clone())) as Box<dyn StorageOverride<_>>,
		);
		let overrides = Arc::new(OverrideHandle {
			schemas: overrides_map,
			fallback: Box::new(SchemaV3Override::new(client.clone())),
		});
		let indexer_backend = fc_db::sql::Backend::new(
			fc_db::sql::BackendConfig::Sqlite(fc_db::sql::SqliteBackendConfig {
				path: Path::new("sqlite:///")
					.join(tmp.path())
					.join("test.db3")
					.to_str()
					.unwrap(),
				create_if_missing: true,
				cache_size: 204800,
				thread_count: 4,
			}),
			100,
			None,
			overrides.clone(),
		)
		.await
		.expect("indexer pool to be created");

		// Pool
		let pool = indexer_backend.pool().clone();

		// Spawn indexer task
		let pubsub_notification_sinks: crate::EthereumBlockNotificationSinks<
			crate::EthereumBlockNotification<OpaqueBlock>,
		> = Default::default();
		let pubsub_notification_sinks = Arc::new(pubsub_notification_sinks);
		let mut sync_oracle_wrapper = TestSyncOracleWrapper::new();
		let sync_oracle = sync_oracle_wrapper.oracle.clone();
		let client_inner = client.clone();
		tokio::task::spawn(async move {
			crate::sql::SyncWorker::run(
				client_inner.clone(),
				backend.clone(),
				Arc::new(indexer_backend),
				client_inner.import_notification_stream(),
				SyncWorkerConfig {
					read_notification_timeout: Duration::from_secs(10),
					check_indexed_blocks_interval: Duration::from_secs(60),
				},
				SyncStrategy::Parachain,
				Arc::new(sync_oracle),
				pubsub_notification_sinks.clone(),
			)
			.await
		});
		// Enough time for startup
		futures_timer::Delay::new(std::time::Duration::from_millis(200)).await;

		// Import 3 blocks as part of initial network sync, storing them oldest first.
		sync_oracle_wrapper.set_sync_status(true);
		let mut parent_hash = client
			.hash(sp_runtime::traits::Zero::zero())
			.unwrap()
			.expect("genesis hash");
		let mut best_block_hashes: Vec<H256> = vec![];
		for _block_number in 1..=3 {
			let mut builder = client
				.new_block_at(parent_hash, Default::default(), false)
				.unwrap();
			builder
				.push_deposit_log_digest_item(ethereum_digest())
				.expect("deposit log");
			let block = builder.build().unwrap().block;
			let block_hash = block.header.hash();

			executor::block_on(client.import(BlockOrigin::NetworkInitialSync, block)).unwrap();
			best_block_hashes.push(block_hash.clone());
			parent_hash = block_hash;
		}

		// Enough time for indexing
		futures_timer::Delay::new(std::time::Duration::from_millis(3000)).await;

		// Test the chain is correctly indexed.
		let actual_imported_blocks =
			sqlx::query("SELECT substrate_block_hash, is_canon, block_number FROM blocks")
				.fetch_all(&pool)
				.await
				.expect("test query result")
				.iter()
				.map(|row| H256::from_slice(&row.get::<Vec<u8>, _>(0)[..]))
				.collect::<Vec<H256>>();
		let expected_imported_blocks = Vec::<H256>::new();
		assert_eq!(expected_imported_blocks, actual_imported_blocks);
	}
}
