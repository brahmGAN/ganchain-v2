// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
// This file is part of Frontier.
//
// Copyright (c) 2021-2022 Parity Technologies (UK) Ltd.
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

mod parity_db_adapter;
mod upgrade;
mod utils;

use std::{
	marker::PhantomData,
	path::{Path, PathBuf},
	sync::Arc,
};

use parking_lot::Mutex;
use scale_codec::{Decode, Encode};
// Substrate
pub use sc_client_db::DatabaseSource;
use sp_blockchain::HeaderBackend;
use sp_core::H256;
pub use sp_database::Database;
use sp_runtime::traits::Block as BlockT;
// Frontier
use fp_storage::{EthereumStorageSchema, PALLET_ETHEREUM_SCHEMA_CACHE};

use crate::TransactionMetadata;

const DB_HASH_LEN: usize = 32;
/// Hash type that this backend uses for the database.
pub type DbHash = [u8; DB_HASH_LEN];

/// Database settings.
pub struct DatabaseSettings {
	/// Where to find the database.
	pub source: DatabaseSource,
}

pub(crate) mod columns {
	pub const NUM_COLUMNS: u32 = 4;

	pub const META: u32 = 0;
	pub const BLOCK_MAPPING: u32 = 1;
	pub const TRANSACTION_MAPPING: u32 = 2;
	pub const SYNCED_MAPPING: u32 = 3;
}

pub mod static_keys {
	pub const CURRENT_SYNCING_TIPS: &[u8] = b"CURRENT_SYNCING_TIPS";
}

#[derive(Clone)]
pub struct Backend<Block: BlockT> {
	meta: Arc<MetaDb<Block>>,
	mapping: Arc<MappingDb<Block>>,
}

#[async_trait::async_trait]
impl<Block: BlockT> crate::BackendReader<Block> for Backend<Block> {
	async fn block_hash(
		&self,
		ethereum_block_hash: &H256,
	) -> Result<Option<Vec<Block::Hash>>, String> {
		self.mapping().block_hash(ethereum_block_hash)
	}
	async fn transaction_metadata(
		&self,
		ethereum_transaction_hash: &H256,
	) -> Result<Vec<TransactionMetadata<Block>>, String> {
		self.mapping()
			.transaction_metadata(ethereum_transaction_hash)
	}
	async fn filter_logs(
		&self,
		_from_block: u64,
		_to_block: u64,
		_addresses: Vec<sp_core::H160>,
		_topics: Vec<Vec<Option<H256>>>,
	) -> Result<Vec<crate::FilteredLog<Block>>, String> {
		Err("KeyValue db does not index logs".into())
	}

	fn is_indexed(&self) -> bool {
		false
	}
}

/// Returns the frontier database directory.
pub fn frontier_database_dir(db_config_dir: &Path, db_path: &str) -> PathBuf {
	db_config_dir.join("frontier").join(db_path)
}

impl<Block: BlockT> Backend<Block> {
	pub fn open<C: HeaderBackend<Block>>(
		client: Arc<C>,
		database: &DatabaseSource,
		db_config_dir: &Path,
	) -> Result<Self, String> {
		Self::new(
			client,
			&DatabaseSettings {
				source: match database {
					DatabaseSource::Auto { .. } => DatabaseSource::Auto {
						rocksdb_path: frontier_database_dir(db_config_dir, "db"),
						paritydb_path: frontier_database_dir(db_config_dir, "paritydb"),
						cache_size: 0,
					},
					#[cfg(feature = "rocksdb")]
					DatabaseSource::RocksDb { .. } => DatabaseSource::RocksDb {
						path: frontier_database_dir(db_config_dir, "db"),
						cache_size: 0,
					},
					DatabaseSource::ParityDb { .. } => DatabaseSource::ParityDb {
						path: frontier_database_dir(db_config_dir, "paritydb"),
					},
					_ => {
						return Err(
							"Supported db sources: `auto` | `rocksdb` | `paritydb`".to_string()
						)
					}
				},
			},
		)
	}

	pub fn new<C: HeaderBackend<Block>>(
		client: Arc<C>,
		config: &DatabaseSettings,
	) -> Result<Self, String> {
		let db = utils::open_database::<Block, C>(client, config)?;

		Ok(Self {
			mapping: Arc::new(MappingDb {
				db: db.clone(),
				write_lock: Arc::new(Mutex::new(())),
				_marker: PhantomData,
			}),
			meta: Arc::new(MetaDb {
				db: db.clone(),
				_marker: PhantomData,
			}),
		})
	}

	pub fn mapping(&self) -> &Arc<MappingDb<Block>> {
		&self.mapping
	}

	pub fn meta(&self) -> &Arc<MetaDb<Block>> {
		&self.meta
	}
}

pub struct MetaDb<Block: BlockT> {
	db: Arc<dyn Database<DbHash>>,
	_marker: PhantomData<Block>,
}

impl<Block: BlockT> MetaDb<Block> {
	pub fn current_syncing_tips(&self) -> Result<Vec<Block::Hash>, String> {
		match self.db.get(
			crate::columns::META,
			crate::static_keys::CURRENT_SYNCING_TIPS,
		) {
			Some(raw) => {
				Ok(Vec::<Block::Hash>::decode(&mut &raw[..]).map_err(|e| format!("{:?}", e))?)
			}
			None => Ok(Vec::new()),
		}
	}

	pub fn write_current_syncing_tips(&self, tips: Vec<Block::Hash>) -> Result<(), String> {
		let mut transaction = sp_database::Transaction::new();

		transaction.set(
			crate::columns::META,
			crate::static_keys::CURRENT_SYNCING_TIPS,
			&tips.encode(),
		);

		self.db
			.commit(transaction)
			.map_err(|e| format!("{:?}", e))?;

		Ok(())
	}

	pub fn ethereum_schema(&self) -> Result<Option<Vec<(EthereumStorageSchema, H256)>>, String> {
		match self
			.db
			.get(crate::columns::META, &PALLET_ETHEREUM_SCHEMA_CACHE.encode())
		{
			Some(raw) => Ok(Some(
				Decode::decode(&mut &raw[..]).map_err(|e| format!("{:?}", e))?,
			)),
			None => Ok(None),
		}
	}

	pub fn write_ethereum_schema(
		&self,
		new_cache: Vec<(EthereumStorageSchema, H256)>,
	) -> Result<(), String> {
		let mut transaction = sp_database::Transaction::new();

		transaction.set(
			crate::columns::META,
			&PALLET_ETHEREUM_SCHEMA_CACHE.encode(),
			&new_cache.encode(),
		);

		self.db
			.commit(transaction)
			.map_err(|e| format!("{:?}", e))?;

		Ok(())
	}
}

#[derive(Debug)]
pub struct MappingCommitment<Block: BlockT> {
	pub block_hash: Block::Hash,
	pub ethereum_block_hash: H256,
	pub ethereum_transaction_hashes: Vec<H256>,
}

pub struct MappingDb<Block: BlockT> {
	db: Arc<dyn Database<DbHash>>,
	write_lock: Arc<Mutex<()>>,
	_marker: PhantomData<Block>,
}

impl<Block: BlockT> MappingDb<Block> {
	pub fn is_synced(&self, block_hash: &Block::Hash) -> Result<bool, String> {
		match self
			.db
			.get(crate::columns::SYNCED_MAPPING, &block_hash.encode())
		{
			Some(raw) => Ok(bool::decode(&mut &raw[..]).map_err(|e| format!("{:?}", e))?),
			None => Ok(false),
		}
	}

	pub fn block_hash(
		&self,
		ethereum_block_hash: &H256,
	) -> Result<Option<Vec<Block::Hash>>, String> {
		match self
			.db
			.get(crate::columns::BLOCK_MAPPING, &ethereum_block_hash.encode())
		{
			Some(raw) => Ok(Some(
				Vec::<Block::Hash>::decode(&mut &raw[..]).map_err(|e| format!("{:?}", e))?,
			)),
			None => Ok(None),
		}
	}

	pub fn transaction_metadata(
		&self,
		ethereum_transaction_hash: &H256,
	) -> Result<Vec<TransactionMetadata<Block>>, String> {
		match self.db.get(
			crate::columns::TRANSACTION_MAPPING,
			&ethereum_transaction_hash.encode(),
		) {
			Some(raw) => Ok(Vec::<TransactionMetadata<Block>>::decode(&mut &raw[..])
				.map_err(|e| format!("{:?}", e))?),
			None => Ok(Vec::new()),
		}
	}

	pub fn write_none(&self, block_hash: Block::Hash) -> Result<(), String> {
		let _lock = self.write_lock.lock();

		let mut transaction = sp_database::Transaction::new();

		transaction.set(
			crate::columns::SYNCED_MAPPING,
			&block_hash.encode(),
			&true.encode(),
		);

		self.db
			.commit(transaction)
			.map_err(|e| format!("{:?}", e))?;

		Ok(())
	}

	pub fn write_hashes(&self, commitment: MappingCommitment<Block>) -> Result<(), String> {
		let _lock = self.write_lock.lock();

		let mut transaction = sp_database::Transaction::new();

		let substrate_hashes = match self.block_hash(&commitment.ethereum_block_hash) {
			Ok(Some(mut data)) => {
				if !data.contains(&commitment.block_hash) {
					data.push(commitment.block_hash);
					log::warn!(
						target: "fc-db",
						"Possible equivocation at ethereum block hash {} {:?}",
						&commitment.ethereum_block_hash,
						&data
					);
				}
				data
			}
			_ => vec![commitment.block_hash],
		};

		transaction.set(
			crate::columns::BLOCK_MAPPING,
			&commitment.ethereum_block_hash.encode(),
			&substrate_hashes.encode(),
		);

		for (i, ethereum_transaction_hash) in commitment
			.ethereum_transaction_hashes
			.into_iter()
			.enumerate()
		{
			let mut metadata = self.transaction_metadata(&ethereum_transaction_hash)?;
			metadata.push(TransactionMetadata::<Block> {
				block_hash: commitment.block_hash,
				ethereum_block_hash: commitment.ethereum_block_hash,
				ethereum_index: i as u32,
			});
			transaction.set(
				crate::columns::TRANSACTION_MAPPING,
				&ethereum_transaction_hash.encode(),
				&metadata.encode(),
			);
		}

		transaction.set(
			crate::columns::SYNCED_MAPPING,
			&commitment.block_hash.encode(),
			&true.encode(),
		);

		self.db
			.commit(transaction)
			.map_err(|e| format!("{:?}", e))?;

		Ok(())
	}
}
