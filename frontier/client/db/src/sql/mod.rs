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

use std::{cmp::Ordering, collections::HashSet, num::NonZeroU32, str::FromStr, sync::Arc};

use futures::TryStreamExt;
use scale_codec::{Decode, Encode};
use sqlx::{
	query::Query,
	sqlite::{
		SqliteArguments, SqliteConnectOptions, SqlitePool, SqlitePoolOptions, SqliteQueryResult,
	},
	ConnectOptions, Error, Execute, QueryBuilder, Row, Sqlite,
};
// Substrate
use sc_client_api::backend::{Backend as BackendT, StateBackend, StorageProvider};
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_core::{H160, H256};
use sp_runtime::{
	generic::BlockId,
	traits::{BlakeTwo256, Block as BlockT, Header as HeaderT, UniqueSaturatedInto, Zero},
};
// Frontier
use fc_storage::OverrideHandle;
use fp_consensus::{FindLogError, Hashes, Log as ConsensusLog, PostLog, PreLog};
use fp_rpc::EthereumRuntimeRPCApi;
use fp_storage::{EthereumStorageSchema, PALLET_ETHEREUM_SCHEMA};

use crate::{BackendReader, FilteredLog};

/// Maximum number to topics allowed to be filtered upon
const MAX_TOPIC_COUNT: u16 = 4;

/// Represents a log item.
#[derive(Debug, Eq, PartialEq)]
pub struct Log {
	pub address: Vec<u8>,
	pub topic_1: Option<Vec<u8>>,
	pub topic_2: Option<Vec<u8>>,
	pub topic_3: Option<Vec<u8>>,
	pub topic_4: Option<Vec<u8>>,
	pub log_index: i32,
	pub transaction_index: i32,
	pub substrate_block_hash: Vec<u8>,
}

/// Represents the block metadata.
#[derive(Eq, PartialEq)]
struct BlockMetadata {
	pub substrate_block_hash: H256,
	pub block_number: i32,
	pub post_hashes: fp_consensus::Hashes,
	pub schema: EthereumStorageSchema,
	pub is_canon: i32,
}

/// Represents the Sqlite connection options that are
/// used to establish a database connection.
#[derive(Debug)]
pub struct SqliteBackendConfig<'a> {
	pub path: &'a str,
	pub create_if_missing: bool,
	pub thread_count: u32,
	pub cache_size: u64,
}

/// Represents the indexed status of a block and if it's canon or not.
#[derive(Debug, Default)]
pub struct BlockIndexedStatus {
	pub indexed: bool,
	pub canon: bool,
}

/// Represents the backend configurations.
#[derive(Debug)]
pub enum BackendConfig<'a> {
	Sqlite(SqliteBackendConfig<'a>),
}

#[derive(Clone)]
pub struct Backend<Block: BlockT> {
	/// The Sqlite connection.
	pool: SqlitePool,

	/// The additional overrides for the logs handler.
	overrides: Arc<OverrideHandle<Block>>,

	/// The number of allowed operations for the Sqlite filter call.
	/// A value of `0` disables the timeout.
	num_ops_timeout: i32,
}

impl<Block: BlockT> Backend<Block>
where
	Block: BlockT<Hash = H256> + Send + Sync,
{
	/// Creates a new instance of the SQL backend.
	pub async fn new(
		config: BackendConfig<'_>,
		pool_size: u32,
		num_ops_timeout: Option<NonZeroU32>,
		overrides: Arc<OverrideHandle<Block>>,
	) -> Result<Self, Error> {
		let any_pool = SqlitePoolOptions::new()
			.max_connections(pool_size)
			.connect_lazy_with(Self::connect_options(&config)?.disable_statement_logging());
		let _ = Self::create_database_if_not_exists(&any_pool).await?;
		let _ = Self::create_indexes_if_not_exist(&any_pool).await?;
		Ok(Self {
			pool: any_pool,
			overrides,
			num_ops_timeout: num_ops_timeout
				.map(|n| n.get())
				.unwrap_or(0)
				.try_into()
				.unwrap_or(i32::MAX),
		})
	}

	fn connect_options(config: &BackendConfig) -> Result<SqliteConnectOptions, Error> {
		match config {
			BackendConfig::Sqlite(config) => {
				log::info!(target: "frontier-sql", "📑 Connection configuration: {config:?}");
				let config = sqlx::sqlite::SqliteConnectOptions::from_str(config.path)?
					.create_if_missing(config.create_if_missing)
					// https://www.sqlite.org/pragma.html#pragma_busy_timeout
					.busy_timeout(std::time::Duration::from_secs(8))
					// 200MB, https://www.sqlite.org/pragma.html#pragma_cache_size
					.pragma("cache_size", format!("-{}", config.cache_size))
					// https://www.sqlite.org/pragma.html#pragma_analysis_limit
					.pragma("analysis_limit", "1000")
					// https://www.sqlite.org/pragma.html#pragma_threads
					.pragma("threads", config.thread_count.to_string())
					// https://www.sqlite.org/pragma.html#pragma_threads
					.pragma("temp_store", "memory")
					// https://www.sqlite.org/wal.html
					.journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
					// https://www.sqlite.org/pragma.html#pragma_synchronous
					.synchronous(sqlx::sqlite::SqliteSynchronous::Normal);
				Ok(config)
			}
		}
	}

	/// Get the underlying Sqlite pool.
	pub fn pool(&self) -> &SqlitePool {
		&self.pool
	}

	/// Canonicalize the indexed blocks, marking/demarking them as canon based on the
	/// provided `retracted` and `enacted` values.
	pub async fn canonicalize(&self, retracted: &[H256], enacted: &[H256]) -> Result<(), Error> {
		let mut tx = self.pool().begin().await?;

		// Retracted
		let mut builder: QueryBuilder<Sqlite> =
			QueryBuilder::new("UPDATE blocks SET is_canon = 0 WHERE substrate_block_hash IN (");
		let mut retracted_hashes = builder.separated(", ");
		for hash in retracted.iter() {
			let hash = hash.as_bytes();
			retracted_hashes.push_bind(hash);
		}
		retracted_hashes.push_unseparated(")");
		let query = builder.build();
		query.execute(&mut *tx).await?;

		// Enacted
		let mut builder: QueryBuilder<Sqlite> =
			QueryBuilder::new("UPDATE blocks SET is_canon = 1 WHERE substrate_block_hash IN (");
		let mut enacted_hashes = builder.separated(", ");
		for hash in enacted.iter() {
			let hash = hash.as_bytes();
			enacted_hashes.push_bind(hash);
		}
		enacted_hashes.push_unseparated(")");
		let query = builder.build();
		query.execute(&mut *tx).await?;

		tx.commit().await
	}

	/// Index the block metadata for the genesis block.
	pub async fn insert_genesis_block_metadata<Client, BE>(
		&self,
		client: Arc<Client>,
	) -> Result<Option<H256>, Error>
	where
		Client: StorageProvider<Block, BE> + HeaderBackend<Block> + Send + Sync + 'static,
		Client: ProvideRuntimeApi<Block>,
		Client::Api: EthereumRuntimeRPCApi<Block>,
		BE: BackendT<Block> + 'static,
		BE::State: StateBackend<BlakeTwo256>,
	{
		let id = BlockId::Number(Zero::zero());
		let substrate_genesis_hash = client
			.expect_block_hash_from_id(&id)
			.map_err(|_| Error::Protocol("Cannot resolve genesis hash".to_string()))?;
		let maybe_substrate_hash: Option<H256> = if let Ok(Some(_)) =
			client.header(substrate_genesis_hash)
		{
			let has_api = client
				.runtime_api()
				.has_api_with::<dyn EthereumRuntimeRPCApi<Block>, _>(
					substrate_genesis_hash,
					|version| version >= 1,
				)
				.expect("runtime api reachable");

			log::debug!(target: "frontier-sql", "Index genesis block, has_api={has_api}, hash={substrate_genesis_hash:?}");

			if has_api {
				// The chain has frontier support from genesis.
				// Read from the runtime and store the block metadata.
				let ethereum_block = client
					.runtime_api()
					.current_block(substrate_genesis_hash)
					.expect("runtime api reachable")
					.expect("ethereum genesis block");

				let schema =
					Self::onchain_storage_schema(client.as_ref(), substrate_genesis_hash).encode();
				let ethereum_block_hash = ethereum_block.header.hash().as_bytes().to_owned();
				let substrate_block_hash = substrate_genesis_hash.as_bytes();
				let block_number = 0i32;
				let is_canon = 1i32;

				let _ = sqlx::query(
					"INSERT OR IGNORE INTO blocks(
						ethereum_block_hash,
						substrate_block_hash,
						block_number,
						ethereum_storage_schema,
						is_canon)
					VALUES (?, ?, ?, ?, ?)",
				)
				.bind(ethereum_block_hash)
				.bind(substrate_block_hash)
				.bind(block_number)
				.bind(schema)
				.bind(is_canon)
				.execute(self.pool())
				.await?;
			}
			Some(substrate_genesis_hash)
		} else {
			None
		};
		Ok(maybe_substrate_hash)
	}

	fn insert_block_metadata_inner<Client, BE>(
		client: Arc<Client>,
		hash: H256,
		overrides: Arc<OverrideHandle<Block>>,
	) -> Result<BlockMetadata, Error>
	where
		Client: StorageProvider<Block, BE> + HeaderBackend<Block> + Send + Sync + 'static,
		BE: BackendT<Block> + 'static,
		BE::State: StateBackend<BlakeTwo256>,
	{
		log::trace!(target: "frontier-sql", "🛠️  [Metadata] Retrieving digest data for block {hash:?}");
		if let Ok(Some(header)) = client.header(hash) {
			match fp_consensus::find_log(header.digest()) {
				Ok(log) => {
					let schema = Self::onchain_storage_schema(client.as_ref(), hash);
					let log_hashes = match log {
						ConsensusLog::Post(PostLog::Hashes(post_hashes)) => post_hashes,
						ConsensusLog::Post(PostLog::Block(block)) => Hashes::from_block(block),
						ConsensusLog::Post(PostLog::BlockHash(expect_eth_block_hash)) => {
							let ethereum_block = overrides
								.schemas
								.get(&schema)
								.unwrap_or(&overrides.fallback)
								.current_block(hash);
							match ethereum_block {
								Some(block) => {
									let got_eth_block_hash = block.header.hash();
									if got_eth_block_hash != expect_eth_block_hash {
										return Err(Error::Protocol(format!(
											"Ethereum block hash mismatch: \
											frontier consensus digest ({expect_eth_block_hash:?}), \
											db state ({got_eth_block_hash:?})"
										)));
									} else {
										Hashes::from_block(block)
									}
								}
								None => {
									return Err(Error::Protocol(format!(
										"Missing ethereum block for hash mismatch {expect_eth_block_hash:?}"
									)))
								}
							}
						}
						ConsensusLog::Pre(PreLog::Block(block)) => Hashes::from_block(block),
					};

					let header_number = *header.number();
					let block_number =
						UniqueSaturatedInto::<u32>::unique_saturated_into(header_number) as i32;
					let is_canon = match client.hash(header_number) {
						Ok(Some(inner_hash)) => (inner_hash == hash) as i32,
						Ok(None) => {
							log::debug!(target: "frontier-sql", "[Metadata] Missing header for block #{block_number} ({hash:?})");
							0
						}
						Err(err) => {
							log::debug!(
								target: "frontier-sql",
								"[Metadata] Failed to retrieve header for block #{block_number} ({hash:?}): {err:?}",
							);
							0
						}
					};

					log::trace!(
						target: "frontier-sql",
						"[Metadata] Prepared block metadata for #{block_number} ({hash:?}) canon={is_canon}",
					);
					Ok(BlockMetadata {
						substrate_block_hash: hash,
						block_number,
						post_hashes: log_hashes,
						schema,
						is_canon,
					})
				}
				Err(FindLogError::NotFound) => Err(Error::Protocol(format!(
					"[Metadata] No logs found for hash {hash:?}",
				))),
				Err(FindLogError::MultipleLogs) => Err(Error::Protocol(format!(
					"[Metadata] Multiple logs found for hash {hash:?}",
				))),
			}
		} else {
			Err(Error::Protocol(format!(
				"[Metadata] Failed retrieving header for hash {hash:?}"
			)))
		}
	}

	/// Insert the block metadata for the provided block hashes.
	pub async fn insert_block_metadata<Client, BE>(
		&self,
		client: Arc<Client>,
		hash: H256,
	) -> Result<(), Error>
	where
		Client: StorageProvider<Block, BE> + HeaderBackend<Block> + Send + Sync + 'static,
		BE: BackendT<Block> + 'static,
		BE::State: StateBackend<BlakeTwo256>,
	{
		// Spawn a blocking task to get block metadata from substrate backend.
		let overrides = self.overrides.clone();
		let metadata = tokio::task::spawn_blocking(move || {
			Self::insert_block_metadata_inner(client.clone(), hash, overrides)
		})
		.await
		.map_err(|_| Error::Protocol("tokio blocking metadata task failed".to_string()))??;

		let mut tx = self.pool().begin().await?;

		log::debug!(
			target: "frontier-sql",
			"🛠️  [Metadata] Starting execution of statements on db transaction"
		);
		let post_hashes = metadata.post_hashes;
		let ethereum_block_hash = post_hashes.block_hash.as_bytes();
		let substrate_block_hash = metadata.substrate_block_hash.as_bytes();
		let schema = metadata.schema.encode();
		let block_number = metadata.block_number;
		let is_canon = metadata.is_canon;

		let _ = sqlx::query(
			"INSERT OR IGNORE INTO blocks(
					ethereum_block_hash,
					substrate_block_hash,
					block_number,
					ethereum_storage_schema,
					is_canon)
				VALUES (?, ?, ?, ?, ?)",
		)
		.bind(ethereum_block_hash)
		.bind(substrate_block_hash)
		.bind(block_number)
		.bind(schema)
		.bind(is_canon)
		.execute(&mut *tx)
		.await?;
		for (i, &transaction_hash) in post_hashes.transaction_hashes.iter().enumerate() {
			let ethereum_transaction_hash = transaction_hash.as_bytes();
			let ethereum_transaction_index = i as i32;
			log::trace!(
				target: "frontier-sql",
				"[Metadata] Inserting TX for block #{block_number} - {transaction_hash:?} index {ethereum_transaction_index}",
			);
			let _ = sqlx::query(
				"INSERT OR IGNORE INTO transactions(
						ethereum_transaction_hash,
						substrate_block_hash,
						ethereum_block_hash,
						ethereum_transaction_index)
					VALUES (?, ?, ?, ?)",
			)
			.bind(ethereum_transaction_hash)
			.bind(substrate_block_hash)
			.bind(ethereum_block_hash)
			.bind(ethereum_transaction_index)
			.execute(&mut *tx)
			.await?;
		}

		sqlx::query("INSERT INTO sync_status(substrate_block_hash) VALUES (?)")
			.bind(hash.as_bytes())
			.execute(&mut *tx)
			.await?;

		log::debug!(target: "frontier-sql", "[Metadata] Ready to commit");
		tx.commit().await
	}

	/// Index the logs for the newly indexed blocks upto a `max_pending_blocks` value.
	pub async fn index_block_logs<Client, BE>(&self, client: Arc<Client>, block_hash: Block::Hash)
	where
		Client: StorageProvider<Block, BE> + HeaderBackend<Block> + Send + Sync + 'static,
		BE: BackendT<Block> + 'static,
		BE::State: StateBackend<BlakeTwo256>,
	{
		let pool = self.pool().clone();
		let overrides = self.overrides.clone();
		let _ = async {
			// The overarching db transaction for the task.
			// Due to the async nature of this task, the same work is likely to happen
			// more than once. For example when a new batch is scheduled when the previous one
			// didn't finished yet and the new batch happens to select the same substrate
			// block hashes for the update.
			// That is expected, we are exchanging extra work for *acid*ity.
			// There is no case of unique constrain violation or race condition as already
			// existing entries are ignored.
			let mut tx = pool.begin().await?;
			// Update statement returning the substrate block hashes for this batch.
			match sqlx::query(
				"UPDATE sync_status
			SET status = 1
			WHERE substrate_block_hash IN
				(SELECT substrate_block_hash
				FROM sync_status
				WHERE status = 0 AND substrate_block_hash = ?) RETURNING substrate_block_hash",
			)
			.bind(block_hash.as_bytes())
			.fetch_one(&mut *tx)
			.await
			{
				Ok(_) => {
					// Spawn a blocking task to get log data from substrate backend.
					let logs = tokio::task::spawn_blocking(move || {
						Self::get_logs(client.clone(), overrides, block_hash)
					})
					.await
					.map_err(|_| Error::Protocol("tokio blocking task failed".to_string()))?;

					for log in logs {
						let _ = sqlx::query(
							"INSERT OR IGNORE INTO logs(
						address,
						topic_1,
						topic_2,
						topic_3,
						topic_4,
						log_index,
						transaction_index,
						substrate_block_hash)
					VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
						)
						.bind(log.address)
						.bind(log.topic_1)
						.bind(log.topic_2)
						.bind(log.topic_3)
						.bind(log.topic_4)
						.bind(log.log_index)
						.bind(log.transaction_index)
						.bind(log.substrate_block_hash)
						.execute(&mut *tx)
						.await?;
					}
					Ok(tx.commit().await?)
				}
				Err(e) => Err(e),
			}
		}
		.await
		.map_err(|e| {
			log::error!(target: "frontier-sql", "{e}");
		});
		// https://www.sqlite.org/pragma.html#pragma_optimize
		let _ = sqlx::query("PRAGMA optimize").execute(&pool).await;
		log::debug!(target: "frontier-sql", "Batch committed");
	}

	fn get_logs<Client, BE>(
		client: Arc<Client>,
		overrides: Arc<OverrideHandle<Block>>,
		substrate_block_hash: H256,
	) -> Vec<Log>
	where
		Client: StorageProvider<Block, BE> + HeaderBackend<Block> + Send + Sync + 'static,
		BE: BackendT<Block> + 'static,
		BE::State: StateBackend<BlakeTwo256>,
	{
		let mut logs: Vec<Log> = vec![];
		let mut transaction_count: usize = 0;
		let mut log_count: usize = 0;
		let schema = Self::onchain_storage_schema(client.as_ref(), substrate_block_hash);
		let handler = overrides
			.schemas
			.get(&schema)
			.unwrap_or(&overrides.fallback);

		let receipts = handler
			.current_receipts(substrate_block_hash)
			.unwrap_or_default();

		transaction_count += receipts.len();
		for (transaction_index, receipt) in receipts.iter().enumerate() {
			let receipt_logs = match receipt {
				ethereum::ReceiptV3::Legacy(d)
				| ethereum::ReceiptV3::EIP2930(d)
				| ethereum::ReceiptV3::EIP1559(d) => &d.logs,
			};
			let transaction_index = transaction_index as i32;
			log_count += receipt_logs.len();
			for (log_index, log) in receipt_logs.iter().enumerate() {
				logs.push(Log {
					address: log.address.as_bytes().to_owned(),
					topic_1: log.topics.get(0).map(|l| l.as_bytes().to_owned()),
					topic_2: log.topics.get(1).map(|l| l.as_bytes().to_owned()),
					topic_3: log.topics.get(2).map(|l| l.as_bytes().to_owned()),
					topic_4: log.topics.get(3).map(|l| l.as_bytes().to_owned()),
					log_index: log_index as i32,
					transaction_index,
					substrate_block_hash: substrate_block_hash.as_bytes().to_owned(),
				});
			}
		}
		log::debug!(
			target: "frontier-sql",
			"Ready to commit {log_count} logs from {transaction_count} transactions"
		);
		logs
	}

	fn onchain_storage_schema<Client, BE>(client: &Client, at: Block::Hash) -> EthereumStorageSchema
	where
		Client: StorageProvider<Block, BE> + HeaderBackend<Block> + Send + Sync + 'static,
		BE: BackendT<Block> + 'static,
		BE::State: StateBackend<BlakeTwo256>,
	{
		match client.storage(at, &sp_storage::StorageKey(PALLET_ETHEREUM_SCHEMA.to_vec())) {
			Ok(Some(bytes)) => Decode::decode(&mut &bytes.0[..])
				.ok()
				.unwrap_or(EthereumStorageSchema::Undefined),
			_ => EthereumStorageSchema::Undefined,
		}
	}

	/// Retrieves the status if a block has been already indexed.
	pub async fn is_block_indexed(&self, block_hash: Block::Hash) -> bool {
		sqlx::query("SELECT substrate_block_hash FROM sync_status WHERE substrate_block_hash = ?")
			.bind(block_hash.as_bytes().to_owned())
			.fetch_optional(self.pool())
			.await
			.map(|r| r.is_some())
			.unwrap_or(false)
	}

	/// Retrieves the status if a block is indexed and if also marked as canon.
	pub async fn block_indexed_and_canon_status(
		&self,
		block_hash: Block::Hash,
	) -> BlockIndexedStatus {
		sqlx::query(
			"SELECT b.is_canon FROM sync_status AS s
			INNER JOIN blocks AS b
			ON s.substrate_block_hash = b.substrate_block_hash
			WHERE s.substrate_block_hash = ?",
		)
		.bind(block_hash.as_bytes().to_owned())
		.fetch_optional(self.pool())
		.await
		.map(|result| {
			result
				.map(|row| {
					let is_canon: i32 = row.get(0);
					BlockIndexedStatus {
						indexed: true,
						canon: is_canon != 0,
					}
				})
				.unwrap_or_default()
		})
		.unwrap_or_default()
	}

	/// Sets the provided block as canon.
	pub async fn set_block_as_canon(&self, block_hash: H256) -> Result<SqliteQueryResult, Error> {
		sqlx::query("UPDATE blocks SET is_canon = 1 WHERE substrate_block_hash = ?")
			.bind(block_hash.as_bytes())
			.execute(self.pool())
			.await
	}

	/// Retrieves the first missing canonical block number in decreasing order that hasn't been indexed yet.
	/// If no unindexed block exists or the table or the rows do not exist, then the function
	/// returns `None`.
	pub async fn get_first_missing_canon_block(&self) -> Option<u32> {
		match sqlx::query(
			"SELECT b1.block_number-1
			FROM blocks as b1
			WHERE b1.block_number > 0 AND b1.is_canon=1 AND NOT EXISTS (
				SELECT 1 FROM blocks AS b2
				WHERE b2.block_number = b1.block_number-1
				AND b1.is_canon=1
				AND b2.is_canon=1
			)
			ORDER BY block_number LIMIT 1",
		)
		.fetch_optional(self.pool())
		.await
		{
			Ok(result) => {
				if let Some(row) = result {
					let block_number: u32 = row.get(0);
					return Some(block_number);
				}
			}
			Err(err) => {
				log::debug!(target: "frontier-sql", "Failed retrieving missing block {err:?}");
			}
		}

		None
	}

	/// Retrieves the first pending canonical block hash in decreasing order that hasn't had
	// its logs indexed yet. If no unindexed block exists or the table or the rows do not exist,
	/// then the function returns `None`.
	pub async fn get_first_pending_canon_block(&self) -> Option<H256> {
		match sqlx::query(
			"SELECT s.substrate_block_hash FROM sync_status AS s
			INNER JOIN blocks as b
			ON s.substrate_block_hash = b.substrate_block_hash
			WHERE b.is_canon = 1 AND s.status = 0
			ORDER BY b.block_number LIMIT 1",
		)
		.fetch_optional(self.pool())
		.await
		{
			Ok(result) => {
				if let Some(row) = result {
					let block_hash_bytes: Vec<u8> = row.get(0);
					let block_hash = H256::from_slice(&block_hash_bytes[..]);
					return Some(block_hash);
				}
			}
			Err(err) => {
				log::debug!(target: "frontier-sql", "Failed retrieving missing block {err:?}");
			}
		}

		None
	}

	/// Retrieve the block hash for the last indexed canon block.
	pub async fn get_last_indexed_canon_block(&self) -> Result<H256, Error> {
		let row = sqlx::query(
			"SELECT b.substrate_block_hash FROM blocks AS b
			INNER JOIN sync_status AS s
			ON s.substrate_block_hash = b.substrate_block_hash
			WHERE b.is_canon=1 AND s.status = 1
			ORDER BY b.id DESC LIMIT 1",
		)
		.fetch_one(self.pool())
		.await?;
		Ok(H256::from_slice(
			&row.try_get::<Vec<u8>, _>(0).unwrap_or_default()[..],
		))
	}

	/// Create the Sqlite database if it does not already exist.
	async fn create_database_if_not_exists(pool: &SqlitePool) -> Result<SqliteQueryResult, Error> {
		sqlx::query(
			"BEGIN;
			CREATE TABLE IF NOT EXISTS logs (
				id INTEGER PRIMARY KEY,
				address BLOB NOT NULL,
				topic_1 BLOB,
				topic_2 BLOB,
				topic_3 BLOB,
				topic_4 BLOB,
				log_index INTEGER NOT NULL,
				transaction_index INTEGER NOT NULL,
				substrate_block_hash BLOB NOT NULL,
				UNIQUE (
					log_index,
					transaction_index,
					substrate_block_hash
				)
			);
			CREATE TABLE IF NOT EXISTS sync_status (
				id INTEGER PRIMARY KEY,
				substrate_block_hash BLOB NOT NULL,
				status INTEGER DEFAULT 0 NOT NULL,
				UNIQUE (
					substrate_block_hash
				)
			);
			CREATE TABLE IF NOT EXISTS blocks (
				id INTEGER PRIMARY KEY,
				block_number INTEGER NOT NULL,
				ethereum_block_hash BLOB NOT NULL,
				substrate_block_hash BLOB NOT NULL,
				ethereum_storage_schema BLOB NOT NULL,
				is_canon INTEGER NOT NULL,
				UNIQUE (
					ethereum_block_hash,
					substrate_block_hash
				)
			);
			CREATE TABLE IF NOT EXISTS transactions (
				id INTEGER PRIMARY KEY,
				ethereum_transaction_hash BLOB NOT NULL,
				substrate_block_hash BLOB NOT NULL,
				ethereum_block_hash BLOB NOT NULL,
				ethereum_transaction_index INTEGER NOT NULL,
				UNIQUE (
					ethereum_transaction_hash,
					substrate_block_hash
				)
			);
			COMMIT;",
		)
		.execute(pool)
		.await
	}

	/// Create the Sqlite database indices if it does not already exist.
	async fn create_indexes_if_not_exist(pool: &SqlitePool) -> Result<SqliteQueryResult, Error> {
		sqlx::query(
			"BEGIN;
			CREATE INDEX IF NOT EXISTS logs_main_idx ON logs (
				address,
				topic_1,
				topic_2,
				topic_3,
				topic_4
			);
			CREATE INDEX IF NOT EXISTS logs_substrate_index ON logs (
				substrate_block_hash
			);
			CREATE INDEX IF NOT EXISTS blocks_number_index ON blocks (
				block_number
			);
			CREATE INDEX IF NOT EXISTS blocks_substrate_index ON blocks (
				substrate_block_hash
			);
			CREATE INDEX IF NOT EXISTS eth_block_hash_idx ON blocks (
				ethereum_block_hash
			);
			CREATE INDEX IF NOT EXISTS eth_tx_hash_idx ON transactions (
				ethereum_transaction_hash
			);
			CREATE INDEX IF NOT EXISTS eth_tx_hash_2_idx ON transactions (
				ethereum_block_hash,
				ethereum_transaction_index
			);
			COMMIT;",
		)
		.execute(pool)
		.await
	}
}

#[async_trait::async_trait]
impl<Block: BlockT<Hash = H256>> BackendReader<Block> for Backend<Block> {
	async fn block_hash(
		&self,
		ethereum_block_hash: &H256,
	) -> Result<Option<Vec<Block::Hash>>, String> {
		let ethereum_block_hash = ethereum_block_hash.as_bytes();
		let res =
			sqlx::query("SELECT substrate_block_hash FROM blocks WHERE ethereum_block_hash = ?")
				.bind(ethereum_block_hash)
				.fetch_all(&self.pool)
				.await
				.ok()
				.map(|rows| {
					rows.iter()
						.map(|row| {
							H256::from_slice(&row.try_get::<Vec<u8>, _>(0).unwrap_or_default()[..])
						})
						.collect()
				});
		Ok(res)
	}

	async fn transaction_metadata(
		&self,
		ethereum_transaction_hash: &H256,
	) -> Result<Vec<crate::TransactionMetadata<Block>>, String> {
		let ethereum_transaction_hash = ethereum_transaction_hash.as_bytes();
		let out = sqlx::query(
			"SELECT
				substrate_block_hash, ethereum_block_hash, ethereum_transaction_index
			FROM transactions WHERE ethereum_transaction_hash = ?",
		)
		.bind(ethereum_transaction_hash)
		.fetch_all(&self.pool)
		.await
		.unwrap_or_default()
		.iter()
		.map(|row| {
			let substrate_block_hash =
				H256::from_slice(&row.try_get::<Vec<u8>, _>(0).unwrap_or_default()[..]);
			let ethereum_block_hash =
				H256::from_slice(&row.try_get::<Vec<u8>, _>(1).unwrap_or_default()[..]);
			let ethereum_transaction_index = row.try_get::<i32, _>(2).unwrap_or_default() as u32;
			crate::TransactionMetadata {
				block_hash: substrate_block_hash,
				ethereum_block_hash,
				ethereum_index: ethereum_transaction_index,
			}
		})
		.collect();

		Ok(out)
	}

	async fn filter_logs(
		&self,
		from_block: u64,
		to_block: u64,
		addresses: Vec<H160>,
		topics: Vec<Vec<Option<H256>>>,
	) -> Result<Vec<FilteredLog<Block>>, String> {
		let mut unique_topics: [HashSet<H256>; 4] = [
			HashSet::new(),
			HashSet::new(),
			HashSet::new(),
			HashSet::new(),
		];
		for topic_combination in topics.into_iter() {
			for (topic_index, topic) in topic_combination.into_iter().enumerate() {
				if topic_index == MAX_TOPIC_COUNT as usize {
					return Err("Invalid topic input. Maximum length is 4.".to_string());
				}

				if let Some(topic) = topic {
					unique_topics[topic_index].insert(topic);
				}
			}
		}

		let log_key = format!(
			"{}-{}-{:?}-{:?}",
			from_block, to_block, addresses, unique_topics
		);
		let mut qb = QueryBuilder::new("");
		let query = build_query(&mut qb, from_block, to_block, addresses, unique_topics);
		let sql = query.sql();

		let mut conn = self
			.pool()
			.acquire()
			.await
			.map_err(|err| format!("failed acquiring sqlite connection: {}", err))?;
		let log_key2 = log_key.clone();
		conn.lock_handle()
			.await
			.map_err(|err| format!("{:?}", err))?
			.set_progress_handler(self.num_ops_timeout, move || {
				log::debug!(target: "frontier-sql", "Sqlite progress_handler triggered for {log_key2}");
				false
			});
		log::debug!(target: "frontier-sql", "Query: {sql:?} - {log_key}");

		let mut out: Vec<FilteredLog<Block>> = vec![];
		let mut rows = query.fetch(&mut *conn);
		let maybe_err = loop {
			match rows.try_next().await {
				Ok(Some(row)) => {
					// Substrate block hash
					let substrate_block_hash =
						H256::from_slice(&row.try_get::<Vec<u8>, _>(0).unwrap_or_default()[..]);
					// Ethereum block hash
					let ethereum_block_hash =
						H256::from_slice(&row.try_get::<Vec<u8>, _>(1).unwrap_or_default()[..]);
					// Block number
					let block_number = row.try_get::<i32, _>(2).unwrap_or_default() as u32;
					// Ethereum storage schema
					let ethereum_storage_schema: EthereumStorageSchema =
						Decode::decode(&mut &row.try_get::<Vec<u8>, _>(3).unwrap_or_default()[..])
							.map_err(|_| {
								"Cannot decode EthereumStorageSchema for block".to_string()
							})?;
					// Transaction index
					let transaction_index = row.try_get::<i32, _>(4).unwrap_or_default() as u32;
					// Log index
					let log_index = row.try_get::<i32, _>(5).unwrap_or_default() as u32;
					out.push(FilteredLog {
						substrate_block_hash,
						ethereum_block_hash,
						block_number,
						ethereum_storage_schema,
						transaction_index,
						log_index,
					});
				}
				Ok(None) => break None, // no more rows
				Err(err) => break Some(err),
			};
		};
		drop(rows);
		conn.lock_handle()
			.await
			.map_err(|err| format!("{:?}", err))?
			.remove_progress_handler();

		if let Some(err) = maybe_err {
			log::error!(target: "frontier-sql", "Failed to query sql db: {err:?} - {log_key}");
			return Err("Failed to query sql db with statement".to_string());
		}

		log::info!(target: "frontier-sql", "FILTER remove handler - {log_key}");
		Ok(out)
	}

	fn is_indexed(&self) -> bool {
		true
	}
}

/// Build a SQL query to retrieve a list of logs given certain constraints.
fn build_query<'a>(
	qb: &'a mut QueryBuilder<Sqlite>,
	from_block: u64,
	to_block: u64,
	addresses: Vec<H160>,
	topics: [HashSet<H256>; 4],
) -> Query<'a, Sqlite, SqliteArguments<'a>> {
	qb.push(
		"
SELECT
	l.substrate_block_hash,
	b.ethereum_block_hash,
	b.block_number,
	b.ethereum_storage_schema,
	l.transaction_index,
	l.log_index
FROM logs AS l
INNER JOIN blocks AS b
ON (b.block_number BETWEEN ",
	);
	qb.separated(" AND ")
		.push_bind(from_block as i64)
		.push_bind(to_block as i64)
		.push_unseparated(")");
	qb.push(" AND b.substrate_block_hash = l.substrate_block_hash")
		.push(" AND b.is_canon = 1")
		.push("\nWHERE 1");

	if !addresses.is_empty() {
		qb.push(" AND l.address IN (");
		let mut qb_addr = qb.separated(", ");
		addresses.iter().for_each(|addr| {
			qb_addr.push_bind(addr.as_bytes().to_owned());
		});
		qb_addr.push_unseparated(")");
	}

	for (i, topic_options) in topics.iter().enumerate() {
		match topic_options.len().cmp(&1) {
			Ordering::Greater => {
				qb.push(format!(" AND l.topic_{} IN (", i + 1));
				let mut qb_topic = qb.separated(", ");
				topic_options.iter().for_each(|t| {
					qb_topic.push_bind(t.as_bytes().to_owned());
				});
				qb_topic.push_unseparated(")");
			}
			Ordering::Equal => {
				qb.push(format!(" AND l.topic_{} = ", i + 1)).push_bind(
					topic_options
						.iter()
						.next()
						.expect("length is 1, must exist; qed")
						.as_bytes()
						.to_owned(),
				);
			}
			Ordering::Less => {}
		}
	}

	qb.push(
		"
ORDER BY b.block_number ASC, l.transaction_index ASC, l.log_index ASC
LIMIT 10001",
	);

	qb.build()
}

#[cfg(test)]
mod test {
	use super::*;

	use std::{collections::BTreeMap, path::Path};

	use maplit::hashset;
	use scale_codec::Encode;
	use sqlx::{sqlite::SqliteRow, QueryBuilder, Row, SqlitePool};
	use tempfile::tempdir;
	// Substrate
	use sp_core::{H160, H256};
	use sp_runtime::{
		generic::{Block, Header},
		traits::BlakeTwo256,
	};
	use substrate_test_runtime_client::{
		DefaultTestClientBuilderExt, TestClientBuilder, TestClientBuilderExt,
	};
	// Frontier
	use fc_storage::{OverrideHandle, SchemaV3Override, StorageOverride};
	use fp_storage::{EthereumStorageSchema, PALLET_ETHEREUM_SCHEMA};

	type OpaqueBlock =
		Block<Header<u64, BlakeTwo256>, substrate_test_runtime_client::runtime::Extrinsic>;

	struct TestFilter {
		pub from_block: u64,
		pub to_block: u64,
		pub addresses: Vec<H160>,
		pub topics: Vec<Vec<Option<H256>>>,
		pub expected_result: Vec<FilteredLog<OpaqueBlock>>,
	}

	#[derive(Debug, Clone)]
	struct Log {
		block_number: u32,
		address: H160,
		topics: [H256; 4],
		substrate_block_hash: H256,
		ethereum_block_hash: H256,
		transaction_index: u32,
		log_index: u32,
	}

	#[allow(unused)]
	struct TestData {
		backend: super::Backend<OpaqueBlock>,
		alice: H160,
		bob: H160,
		topics_a: H256,
		topics_b: H256,
		topics_c: H256,
		topics_d: H256,
		substrate_hash_1: H256,
		substrate_hash_2: H256,
		substrate_hash_3: H256,
		ethereum_hash_1: H256,
		ethereum_hash_2: H256,
		ethereum_hash_3: H256,
		log_1_abcd_0_0_alice: Log,
		log_1_dcba_1_0_alice: Log,
		log_1_badc_2_0_alice: Log,
		log_2_abcd_0_0_bob: Log,
		log_2_dcba_1_0_bob: Log,
		log_2_badc_2_0_bob: Log,
		log_3_abcd_0_0_bob: Log,
		log_3_dcba_1_0_bob: Log,
		log_3_badc_2_0_bob: Log,
	}

	impl From<Log> for FilteredLog<OpaqueBlock> {
		fn from(value: Log) -> Self {
			Self {
				substrate_block_hash: value.substrate_block_hash,
				ethereum_block_hash: value.ethereum_block_hash,
				block_number: value.block_number,
				ethereum_storage_schema: EthereumStorageSchema::V3,
				transaction_index: value.transaction_index,
				log_index: value.log_index,
			}
		}
	}

	async fn prepare() -> TestData {
		let tmp = tempdir().expect("create a temporary directory");
		// Initialize storage with schema V3
		let builder = TestClientBuilder::new().add_extra_storage(
			PALLET_ETHEREUM_SCHEMA.to_vec(),
			Encode::encode(&EthereumStorageSchema::V3),
		);
		// Client
		let (client, _) = builder
			.build_with_native_executor::<substrate_test_runtime_client::runtime::RuntimeApi, _>(
				None,
			);
		let client = Arc::new(client);
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
		let indexer_backend = super::Backend::new(
			super::BackendConfig::Sqlite(super::SqliteBackendConfig {
				path: Path::new("sqlite:///")
					.join(tmp.path())
					.join("test.db3")
					.to_str()
					.unwrap(),
				create_if_missing: true,
				cache_size: 20480,
				thread_count: 4,
			}),
			1,
			None,
			overrides.clone(),
		)
		.await
		.expect("indexer pool to be created");

		// Prepare test db data
		// Addresses
		let alice = H160::repeat_byte(0x01);
		let bob = H160::repeat_byte(0x02);
		// Topics
		let topics_a = H256::repeat_byte(0x01);
		let topics_b = H256::repeat_byte(0x02);
		let topics_c = H256::repeat_byte(0x03);
		let topics_d = H256::repeat_byte(0x04);
		// Substrate block hashes
		let substrate_hash_1 = H256::repeat_byte(0x05);
		let substrate_hash_2 = H256::repeat_byte(0x06);
		let substrate_hash_3 = H256::repeat_byte(0x07);
		// Ethereum block hashes
		let ethereum_hash_1 = H256::repeat_byte(0x08);
		let ethereum_hash_2 = H256::repeat_byte(0x09);
		let ethereum_hash_3 = H256::repeat_byte(0x0a);
		// Ethereum storage schema
		let ethereum_storage_schema = EthereumStorageSchema::V3;

		let block_entries = vec![
			// Block 1
			(
				1i32,
				ethereum_hash_1,
				substrate_hash_1,
				ethereum_storage_schema,
			),
			// Block 2
			(
				2i32,
				ethereum_hash_2,
				substrate_hash_2,
				ethereum_storage_schema,
			),
			// Block 3
			(
				3i32,
				ethereum_hash_3,
				substrate_hash_3,
				ethereum_storage_schema,
			),
		];
		let mut builder = QueryBuilder::new(
			"INSERT INTO blocks(
				block_number,
				ethereum_block_hash,
				substrate_block_hash,
				ethereum_storage_schema,
				is_canon
			)",
		);
		builder.push_values(block_entries, |mut b, entry| {
			let block_number = entry.0;
			let ethereum_block_hash = entry.1.as_bytes().to_owned();
			let substrate_block_hash = entry.2.as_bytes().to_owned();
			let ethereum_storage_schema = entry.3.encode();

			b.push_bind(block_number);
			b.push_bind(ethereum_block_hash);
			b.push_bind(substrate_block_hash);
			b.push_bind(ethereum_storage_schema);
			b.push_bind(1i32);
		});
		let query = builder.build();
		let _ = query
			.execute(indexer_backend.pool())
			.await
			.expect("insert should succeed");

		// log_{BLOCK}_{TOPICS}_{LOG_INDEX}_{TX_INDEX}
		let log_1_abcd_0_0_alice = Log {
			block_number: 1,
			address: alice,
			topics: [topics_a, topics_b, topics_c, topics_d],
			log_index: 0,
			transaction_index: 0,
			substrate_block_hash: substrate_hash_1,
			ethereum_block_hash: ethereum_hash_1,
		};
		let log_1_dcba_1_0_alice = Log {
			block_number: 1,
			address: alice,
			topics: [topics_d, topics_c, topics_b, topics_a],
			log_index: 1,
			transaction_index: 0,
			substrate_block_hash: substrate_hash_1,
			ethereum_block_hash: ethereum_hash_1,
		};
		let log_1_badc_2_0_alice = Log {
			block_number: 1,
			address: alice,
			topics: [topics_b, topics_a, topics_d, topics_c],
			log_index: 2,
			transaction_index: 0,
			substrate_block_hash: substrate_hash_1,
			ethereum_block_hash: ethereum_hash_1,
		};
		let log_2_abcd_0_0_bob = Log {
			block_number: 2,
			address: bob,
			topics: [topics_a, topics_b, topics_c, topics_d],
			log_index: 0,
			transaction_index: 0,
			substrate_block_hash: substrate_hash_2,
			ethereum_block_hash: ethereum_hash_2,
		};
		let log_2_dcba_1_0_bob = Log {
			block_number: 2,
			address: bob,
			topics: [topics_d, topics_c, topics_b, topics_a],
			log_index: 1,
			transaction_index: 0,
			substrate_block_hash: substrate_hash_2,
			ethereum_block_hash: ethereum_hash_2,
		};
		let log_2_badc_2_0_bob = Log {
			block_number: 2,
			address: bob,
			topics: [topics_b, topics_a, topics_d, topics_c],
			log_index: 2,
			transaction_index: 0,
			substrate_block_hash: substrate_hash_2,
			ethereum_block_hash: ethereum_hash_2,
		};

		let log_3_abcd_0_0_bob = Log {
			block_number: 3,
			address: bob,
			topics: [topics_a, topics_b, topics_c, topics_d],
			log_index: 0,
			transaction_index: 0,
			substrate_block_hash: substrate_hash_3,
			ethereum_block_hash: ethereum_hash_3,
		};
		let log_3_dcba_1_0_bob = Log {
			block_number: 3,
			address: bob,
			topics: [topics_d, topics_c, topics_b, topics_a],
			log_index: 1,
			transaction_index: 0,
			substrate_block_hash: substrate_hash_3,
			ethereum_block_hash: ethereum_hash_3,
		};
		let log_3_badc_2_0_bob = Log {
			block_number: 3,
			address: bob,
			topics: [topics_b, topics_a, topics_d, topics_c],
			log_index: 2,
			transaction_index: 0,
			substrate_block_hash: substrate_hash_3,
			ethereum_block_hash: ethereum_hash_3,
		};

		let log_entries = vec![
			// Block 1
			log_1_abcd_0_0_alice.clone(),
			log_1_dcba_1_0_alice.clone(),
			log_1_badc_2_0_alice.clone(),
			// Block 2
			log_2_abcd_0_0_bob.clone(),
			log_2_dcba_1_0_bob.clone(),
			log_2_badc_2_0_bob.clone(),
			// Block 3
			log_3_abcd_0_0_bob.clone(),
			log_3_dcba_1_0_bob.clone(),
			log_3_badc_2_0_bob.clone(),
		];

		let mut builder: QueryBuilder<sqlx::Sqlite> = QueryBuilder::new(
			"INSERT INTO logs(
				address,
				topic_1,
				topic_2,
				topic_3,
				topic_4,
				log_index,
				transaction_index,
				substrate_block_hash
			)",
		);
		builder.push_values(log_entries, |mut b, entry| {
			let address = entry.address.as_bytes().to_owned();
			let topic_1 = entry.topics[0].as_bytes().to_owned();
			let topic_2 = entry.topics[1].as_bytes().to_owned();
			let topic_3 = entry.topics[2].as_bytes().to_owned();
			let topic_4 = entry.topics[3].as_bytes().to_owned();
			let log_index = entry.log_index;
			let transaction_index = entry.transaction_index;
			let substrate_block_hash = entry.substrate_block_hash.as_bytes().to_owned();

			b.push_bind(address);
			b.push_bind(topic_1);
			b.push_bind(topic_2);
			b.push_bind(topic_3);
			b.push_bind(topic_4);
			b.push_bind(log_index);
			b.push_bind(transaction_index);
			b.push_bind(substrate_block_hash);
		});
		let query = builder.build();
		let _ = query.execute(indexer_backend.pool()).await;

		TestData {
			alice,
			bob,
			topics_a,
			topics_b,
			topics_c,
			topics_d,
			substrate_hash_1,
			substrate_hash_2,
			substrate_hash_3,
			ethereum_hash_1,
			ethereum_hash_2,
			ethereum_hash_3,
			backend: indexer_backend,
			log_1_abcd_0_0_alice,
			log_1_dcba_1_0_alice,
			log_1_badc_2_0_alice,
			log_2_abcd_0_0_bob,
			log_2_dcba_1_0_bob,
			log_2_badc_2_0_bob,
			log_3_abcd_0_0_bob,
			log_3_dcba_1_0_bob,
			log_3_badc_2_0_bob,
		}
	}

	async fn run_test_case(
		backend: super::Backend<OpaqueBlock>,
		test_case: &TestFilter,
	) -> Result<Vec<FilteredLog<OpaqueBlock>>, String> {
		backend
			.filter_logs(
				test_case.from_block,
				test_case.to_block,
				test_case.addresses.clone(),
				test_case.topics.clone(),
			)
			.await
	}

	async fn assert_blocks_canon(pool: &SqlitePool, expected: Vec<(H256, u32)>) {
		let actual: Vec<(H256, u32)> =
			sqlx::query("SELECT substrate_block_hash, is_canon FROM blocks")
				.map(|row: SqliteRow| (H256::from_slice(&row.get::<Vec<u8>, _>(0)[..]), row.get(1)))
				.fetch_all(pool)
				.await
				.expect("sql query must succeed");
		assert_eq!(expected, actual);
	}

	#[tokio::test]
	async fn genesis_works() {
		let TestData { backend, .. } = prepare().await;
		let filter = TestFilter {
			from_block: 0,
			to_block: 0,
			addresses: vec![],
			topics: vec![],
			expected_result: vec![],
		};
		let result = run_test_case(backend, &filter).await.expect("must succeed");
		assert_eq!(result, filter.expected_result);
	}

	#[tokio::test]
	async fn unsanitized_input_works() {
		let TestData { backend, .. } = prepare().await;
		let filter = TestFilter {
			from_block: 0,
			to_block: 0,
			addresses: vec![],
			topics: vec![vec![None], vec![None, None, None]],
			expected_result: vec![],
		};
		let result = run_test_case(backend, &filter).await.expect("must succeed");
		assert_eq!(result, filter.expected_result);
	}

	#[tokio::test]
	async fn invalid_topic_input_size_fails() {
		let TestData {
			backend, topics_a, ..
		} = prepare().await;
		let filter = TestFilter {
			from_block: 0,
			to_block: 0,
			addresses: vec![],
			topics: vec![
				vec![Some(topics_a), None, None, None, None],
				vec![Some(topics_a), None, None, None],
			],
			expected_result: vec![],
		};
		run_test_case(backend, &filter)
			.await
			.expect_err("Invalid topic input. Maximum length is 4.");
	}

	#[tokio::test]
	async fn test_malformed_topic_cleans_invalid_options() {
		let TestData {
			backend,
			topics_a,
			topics_b,
			topics_d,
			log_1_badc_2_0_alice,
			..
		} = prepare().await;

		// [(a,null,b), (a, null), (d,null), null] -> [(a,b), a, d]
		let filter = TestFilter {
			from_block: 0,
			to_block: 1,
			addresses: vec![],
			topics: vec![
				vec![Some(topics_a), None, Some(topics_d)],
				vec![None], // not considered
				vec![Some(topics_b), Some(topics_a), None],
				vec![None, None, None, None], // not considered
			],
			expected_result: vec![log_1_badc_2_0_alice.into()],
		};
		let result = run_test_case(backend, &filter).await.expect("must succeed");
		assert_eq!(result, filter.expected_result);
	}

	#[tokio::test]
	async fn block_range_works() {
		let TestData {
			backend,
			log_1_abcd_0_0_alice,
			log_1_dcba_1_0_alice,
			log_1_badc_2_0_alice,
			log_2_abcd_0_0_bob,
			log_2_dcba_1_0_bob,
			log_2_badc_2_0_bob,
			..
		} = prepare().await;

		let filter = TestFilter {
			from_block: 0,
			to_block: 2,
			addresses: vec![],
			topics: vec![],
			expected_result: vec![
				log_1_abcd_0_0_alice.into(),
				log_1_dcba_1_0_alice.into(),
				log_1_badc_2_0_alice.into(),
				log_2_abcd_0_0_bob.into(),
				log_2_dcba_1_0_bob.into(),
				log_2_badc_2_0_bob.into(),
			],
		};
		let result = run_test_case(backend, &filter).await.expect("must succeed");
		assert_eq!(result, filter.expected_result);
	}

	#[tokio::test]
	async fn address_filter_works() {
		let TestData {
			backend,
			alice,
			log_1_abcd_0_0_alice,
			log_1_dcba_1_0_alice,
			log_1_badc_2_0_alice,
			..
		} = prepare().await;
		let filter = TestFilter {
			from_block: 0,
			to_block: 3,
			addresses: vec![alice],
			topics: vec![],
			expected_result: vec![
				log_1_abcd_0_0_alice.into(),
				log_1_dcba_1_0_alice.into(),
				log_1_badc_2_0_alice.into(),
			],
		};
		let result = run_test_case(backend, &filter).await.expect("must succeed");
		assert_eq!(result, filter.expected_result);
	}

	#[tokio::test]
	async fn topic_filter_works() {
		let TestData {
			backend,
			topics_d,
			log_1_dcba_1_0_alice,
			log_2_dcba_1_0_bob,
			log_3_dcba_1_0_bob,
			..
		} = prepare().await;
		let filter = TestFilter {
			from_block: 0,
			to_block: 3,
			addresses: vec![],
			topics: vec![vec![Some(topics_d)]],
			expected_result: vec![
				log_1_dcba_1_0_alice.into(),
				log_2_dcba_1_0_bob.into(),
				log_3_dcba_1_0_bob.into(),
			],
		};
		let result = run_test_case(backend, &filter).await.expect("must succeed");
		assert_eq!(result, filter.expected_result);
	}

	#[tokio::test]
	async fn test_filters_address_and_topic() {
		let TestData {
			backend,
			bob,
			topics_b,
			log_2_badc_2_0_bob,
			log_3_badc_2_0_bob,
			..
		} = prepare().await;
		let filter = TestFilter {
			from_block: 0,
			to_block: 3,
			addresses: vec![bob],
			topics: vec![vec![Some(topics_b)]],
			expected_result: vec![log_2_badc_2_0_bob.into(), log_3_badc_2_0_bob.into()],
		};
		let result = run_test_case(backend, &filter).await.expect("must succeed");
		assert_eq!(result, filter.expected_result);
	}

	#[tokio::test]
	async fn test_filters_multi_address_and_topic() {
		let TestData {
			backend,
			alice,
			bob,
			topics_b,
			log_1_badc_2_0_alice,
			log_2_badc_2_0_bob,
			log_3_badc_2_0_bob,
			..
		} = prepare().await;
		let filter = TestFilter {
			from_block: 0,
			to_block: 3,
			addresses: vec![alice, bob],
			topics: vec![vec![Some(topics_b)]],
			expected_result: vec![
				log_1_badc_2_0_alice.into(),
				log_2_badc_2_0_bob.into(),
				log_3_badc_2_0_bob.into(),
			],
		};
		let result = run_test_case(backend, &filter).await.expect("must succeed");
		assert_eq!(result, filter.expected_result);
	}

	#[tokio::test]
	async fn test_filters_multi_address_and_multi_topic() {
		let TestData {
			backend,
			alice,
			bob,
			topics_a,
			topics_b,
			log_1_abcd_0_0_alice,
			log_2_abcd_0_0_bob,
			log_3_abcd_0_0_bob,
			..
		} = prepare().await;
		let filter = TestFilter {
			from_block: 0,
			to_block: 3,
			addresses: vec![alice, bob],
			topics: vec![vec![Some(topics_a), Some(topics_b)]],
			expected_result: vec![
				log_1_abcd_0_0_alice.into(),
				log_2_abcd_0_0_bob.into(),
				log_3_abcd_0_0_bob.into(),
			],
		};
		let result = run_test_case(backend, &filter).await.expect("must succeed");
		assert_eq!(result, filter.expected_result);
	}

	#[tokio::test]
	async fn filter_with_topic_wildcards_works() {
		let TestData {
			backend,
			alice,
			bob,
			topics_d,
			topics_b,
			log_1_dcba_1_0_alice,
			log_2_dcba_1_0_bob,
			log_3_dcba_1_0_bob,
			..
		} = prepare().await;
		let filter = TestFilter {
			from_block: 0,
			to_block: 3,
			addresses: vec![alice, bob],
			topics: vec![vec![Some(topics_d), None, Some(topics_b)]],
			expected_result: vec![
				log_1_dcba_1_0_alice.into(),
				log_2_dcba_1_0_bob.into(),
				log_3_dcba_1_0_bob.into(),
			],
		};
		let result = run_test_case(backend, &filter).await.expect("must succeed");
		assert_eq!(result, filter.expected_result);
	}

	#[tokio::test]
	async fn trailing_wildcard_is_useless_but_works() {
		let TestData {
			alice,
			backend,
			topics_b,
			log_1_dcba_1_0_alice,
			..
		} = prepare().await;
		let filter = TestFilter {
			from_block: 0,
			to_block: 1,
			addresses: vec![alice],
			topics: vec![vec![None, None, Some(topics_b), None]],
			expected_result: vec![log_1_dcba_1_0_alice.into()],
		};
		let result = run_test_case(backend, &filter).await.expect("must succeed");
		assert_eq!(result, filter.expected_result);
	}

	#[tokio::test]
	async fn filter_with_multi_topic_options_works() {
		let TestData {
			backend,
			topics_a,
			topics_d,
			log_1_abcd_0_0_alice,
			log_1_dcba_1_0_alice,
			log_2_abcd_0_0_bob,
			log_2_dcba_1_0_bob,
			log_3_abcd_0_0_bob,
			log_3_dcba_1_0_bob,
			..
		} = prepare().await;
		let filter = TestFilter {
			from_block: 0,
			to_block: 3,
			addresses: vec![],
			topics: vec![
				vec![Some(topics_a)],
				vec![Some(topics_d)],
				vec![Some(topics_d)], // duplicate, ignored
			],
			expected_result: vec![
				log_1_abcd_0_0_alice.into(),
				log_1_dcba_1_0_alice.into(),
				log_2_abcd_0_0_bob.into(),
				log_2_dcba_1_0_bob.into(),
				log_3_abcd_0_0_bob.into(),
				log_3_dcba_1_0_bob.into(),
			],
		};
		let result = run_test_case(backend, &filter).await.expect("must succeed");
		assert_eq!(result, filter.expected_result);
	}

	#[tokio::test]
	async fn filter_with_multi_topic_options_and_wildcards_works() {
		let TestData {
			backend,
			bob,
			topics_a,
			topics_b,
			topics_c,
			topics_d,
			log_2_dcba_1_0_bob,
			log_2_badc_2_0_bob,
			log_3_dcba_1_0_bob,
			log_3_badc_2_0_bob,
			..
		} = prepare().await;
		let filter = TestFilter {
			from_block: 0,
			to_block: 3,
			addresses: vec![bob],
			// Product on input [null,null,(b,d),(a,c)].
			topics: vec![
				vec![None, None, Some(topics_b), Some(topics_a)],
				vec![None, None, Some(topics_b), Some(topics_c)],
				vec![None, None, Some(topics_d), Some(topics_a)],
				vec![None, None, Some(topics_d), Some(topics_c)],
			],
			expected_result: vec![
				log_2_dcba_1_0_bob.into(),
				log_2_badc_2_0_bob.into(),
				log_3_dcba_1_0_bob.into(),
				log_3_badc_2_0_bob.into(),
			],
		};
		let result = run_test_case(backend, &filter).await.expect("must succeed");
		assert_eq!(result, filter.expected_result);
	}

	#[tokio::test]
	async fn test_canonicalize_sets_canon_flag_for_redacted_and_enacted_blocks_correctly() {
		let TestData {
			backend,
			substrate_hash_1,
			substrate_hash_2,
			substrate_hash_3,
			..
		} = prepare().await;

		// set block #1 to non canon
		sqlx::query("UPDATE blocks SET is_canon = 0 WHERE substrate_block_hash = ?")
			.bind(substrate_hash_1.as_bytes())
			.execute(backend.pool())
			.await
			.expect("sql query must succeed");
		assert_blocks_canon(
			backend.pool(),
			vec![
				(substrate_hash_1, 0),
				(substrate_hash_2, 1),
				(substrate_hash_3, 1),
			],
		)
		.await;

		backend
			.canonicalize(&[substrate_hash_2], &[substrate_hash_1])
			.await
			.expect("must succeed");

		assert_blocks_canon(
			backend.pool(),
			vec![
				(substrate_hash_1, 1),
				(substrate_hash_2, 0),
				(substrate_hash_3, 1),
			],
		)
		.await;
	}

	#[test]
	fn test_query_should_be_generated_correctly() {
		use sqlx::Execute;

		let from_block: u64 = 100;
		let to_block: u64 = 500;
		let addresses: Vec<H160> = vec![
			H160::repeat_byte(0x01),
			H160::repeat_byte(0x02),
			H160::repeat_byte(0x03),
		];
		let topics = [
			hashset![
				H256::repeat_byte(0x01),
				H256::repeat_byte(0x02),
				H256::repeat_byte(0x03),
			],
			hashset![H256::repeat_byte(0x04), H256::repeat_byte(0x05),],
			hashset![],
			hashset![H256::repeat_byte(0x06)],
		];

		let expected_query_sql = "
SELECT
	l.substrate_block_hash,
	b.ethereum_block_hash,
	b.block_number,
	b.ethereum_storage_schema,
	l.transaction_index,
	l.log_index
FROM logs AS l
INNER JOIN blocks AS b
ON (b.block_number BETWEEN ? AND ?) AND b.substrate_block_hash = l.substrate_block_hash AND b.is_canon = 1
WHERE 1 AND l.address IN (?, ?, ?) AND l.topic_1 IN (?, ?, ?) AND l.topic_2 IN (?, ?) AND l.topic_4 = ?
ORDER BY b.block_number ASC, l.transaction_index ASC, l.log_index ASC
LIMIT 10001";

		let mut qb = QueryBuilder::new("");
		let actual_query_sql =
			super::build_query(&mut qb, from_block, to_block, addresses, topics).sql();
		assert_eq!(expected_query_sql, actual_query_sql);
	}
}
