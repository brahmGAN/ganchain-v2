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

use std::{
	fmt, fs,
	io::{self, ErrorKind, Read, Write},
	path::{Path, PathBuf},
	sync::Arc,
};

use scale_codec::{Decode, Encode};
// Substrate
use sc_client_db::DatabaseSource;
use sp_blockchain::HeaderBackend;
use sp_core::H256;
use sp_runtime::traits::Block as BlockT;

/// Version file name.
const VERSION_FILE_NAME: &str = "db_version";

/// Current db version.
const CURRENT_VERSION: u32 = 2;

/// Number of columns in each version.
const _V1_NUM_COLUMNS: u32 = 4;
const V2_NUM_COLUMNS: u32 = 4;

/// Database upgrade errors.
#[derive(Debug)]
pub(crate) enum UpgradeError {
	/// Database version cannot be read from existing db_version file.
	UnknownDatabaseVersion,
	/// Database version no longer supported.
	UnsupportedVersion(u32),
	/// Database version comes from future version of the client.
	FutureDatabaseVersion(u32),
	/// Common io error.
	Io(io::Error),
}

pub(crate) type UpgradeResult<T> = Result<T, UpgradeError>;

pub(crate) struct UpgradeVersion1To2Summary {
	pub success: u32,
	pub error: Vec<H256>,
}

impl From<io::Error> for UpgradeError {
	fn from(err: io::Error) -> Self {
		UpgradeError::Io(err)
	}
}

impl fmt::Display for UpgradeError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			UpgradeError::UnknownDatabaseVersion => {
				write!(
					f,
					"Database version cannot be read from existing db_version file"
				)
			}
			UpgradeError::UnsupportedVersion(version) => {
				write!(f, "Database version no longer supported: {}", version)
			}
			UpgradeError::FutureDatabaseVersion(version) => {
				write!(
					f,
					"Database version comes from future version of the client: {}",
					version
				)
			}
			UpgradeError::Io(err) => write!(f, "Io error: {}", err),
		}
	}
}

/// Upgrade database to current version.
pub(crate) fn upgrade_db<Block: BlockT, C: HeaderBackend<Block>>(
	client: Arc<C>,
	db_path: &Path,
	source: &DatabaseSource,
) -> UpgradeResult<()> {
	let db_version = current_version(db_path)?;
	match db_version {
		0 => return Err(UpgradeError::UnsupportedVersion(db_version)),
		1 => {
			let summary: UpgradeVersion1To2Summary = match source {
				DatabaseSource::ParityDb { .. } => {
					migrate_1_to_2_parity_db::<Block, C>(client, db_path)?
				}
				#[cfg(feature = "rocksdb")]
				DatabaseSource::RocksDb { .. } => migrate_1_to_2_rocks_db::<Block, C>(client, db_path)?,
				_ => panic!("DatabaseSource required for upgrade ParityDb | RocksDb"),
			};
			if !summary.error.is_empty() {
				panic!(
					"Inconsistent migration from version 1 to 2. Failed on {:?}",
					summary.error
				);
			} else {
				log::info!("✔️ Successful Frontier DB migration from version 1 to version 2 ({:?} entries).", summary.success);
			}
		}
		CURRENT_VERSION => (),
		_ => return Err(UpgradeError::FutureDatabaseVersion(db_version)),
	}
	update_version(db_path)?;
	Ok(())
}

/// Reads current database version from the file at given path.
/// If the file does not exist it gets created with version 1.
pub(crate) fn current_version(path: &Path) -> UpgradeResult<u32> {
	match fs::File::open(version_file_path(path)) {
		Err(ref err) if err.kind() == ErrorKind::NotFound => {
			fs::create_dir_all(path)?;
			let mut file = fs::File::create(version_file_path(path))?;
			file.write_all(format!("{}", CURRENT_VERSION).as_bytes())?;
			Ok(CURRENT_VERSION)
		}
		Err(_) => Err(UpgradeError::UnknownDatabaseVersion),
		Ok(mut file) => {
			let mut s = String::new();
			file.read_to_string(&mut s)
				.map_err(|_| UpgradeError::UnknownDatabaseVersion)?;
			s.parse::<u32>()
				.map_err(|_| UpgradeError::UnknownDatabaseVersion)
		}
	}
}

/// Writes current database version to the file.
/// Creates a new file if the version file does not exist yet.
pub(crate) fn update_version(path: &Path) -> io::Result<()> {
	fs::create_dir_all(path)?;
	let mut file = fs::File::create(version_file_path(path))?;
	file.write_all(format!("{}", CURRENT_VERSION).as_bytes())?;
	Ok(())
}

/// Returns the version file path.
fn version_file_path(path: &Path) -> PathBuf {
	let mut file_path = path.to_owned();
	file_path.push(VERSION_FILE_NAME);
	file_path
}

/// Migration from version1 to version2:
/// - The format of the Ethereum<>Substrate block mapping changed to support equivocation.
/// - Migrating schema from One-to-one to One-to-many (EthHash: Vec<SubstrateHash>) relationship.
#[cfg(feature = "rocksdb")]
pub(crate) fn migrate_1_to_2_rocks_db<Block: BlockT, C: HeaderBackend<Block>>(
	client: Arc<C>,
	db_path: &Path,
) -> UpgradeResult<UpgradeVersion1To2Summary> {
	log::info!("🔨 Running Frontier DB migration from version 1 to version 2. Please wait.");
	let mut res = UpgradeVersion1To2Summary {
		success: 0,
		error: vec![],
	};
	// Process a batch of hashes in a single db transaction
	#[rustfmt::skip]
	let mut process_chunk = |
		db: &kvdb_rocksdb::Database,
		ethereum_hashes: &[smallvec::SmallVec<[u8; 32]>]
	| -> UpgradeResult<()> {
		let mut transaction = db.transaction();
		for ethereum_hash in ethereum_hashes {
			let mut maybe_error = true;
			if let Some(substrate_hash) = db.get(super::columns::BLOCK_MAPPING, ethereum_hash)? {
				// Only update version1 data
				let decoded = Vec::<Block::Hash>::decode(&mut &substrate_hash[..]);
				if decoded.is_err() || decoded.unwrap().is_empty() {
					// Verify the substrate hash is part of the canonical chain.
					if let Ok(Some(number)) = client.number(Block::Hash::decode(&mut &substrate_hash[..]).unwrap()) {
						if let Ok(Some(hash)) = client.hash(number) {
							transaction.put_vec(
								super::columns::BLOCK_MAPPING,
								ethereum_hash,
								vec![hash].encode(),
							);
							res.success += 1;
							maybe_error = false;
						}
					}
				} else {
					// If version 2 data, we just consider this hash a success.
					// This can happen if the process was closed in the middle of the migration.
					res.success += 1;
					maybe_error = false;
				}
			}
			if maybe_error {
				res.error.push(H256::from_slice(ethereum_hash));
			}
		}
		db.write(transaction)
			.map_err(|_| io::Error::new(ErrorKind::Other, "Failed to commit on migrate_1_to_2"))?;
		log::debug!(
			target: "fc-db-upgrade",
			"🔨 Success {}, error {}.",
			res.success,
			res.error.len()
		);
		Ok(())
	};

	let db_cfg = kvdb_rocksdb::DatabaseConfig::with_columns(V2_NUM_COLUMNS);
	let db = kvdb_rocksdb::Database::open(&db_cfg, db_path)?;

	// Get all the block hashes we need to update
	let ethereum_hashes: Vec<_> = db
		.iter(super::columns::BLOCK_MAPPING)
		.filter_map(|entry| entry.map_or(None, |r| Some(r.0)))
		.collect();

	// Read and update each entry in db transaction batches
	const CHUNK_SIZE: usize = 10_000;
	let chunks = ethereum_hashes.chunks(CHUNK_SIZE);
	let all_len = ethereum_hashes.len();
	for (i, chunk) in chunks.enumerate() {
		process_chunk(&db, chunk)?;
		log::debug!(
			target: "fc-db-upgrade",
			"🔨 Processed {} of {} entries.",
			(CHUNK_SIZE * (i + 1)),
			all_len
		);
	}
	Ok(res)
}

pub(crate) fn migrate_1_to_2_parity_db<Block: BlockT, C: HeaderBackend<Block>>(
	client: Arc<C>,
	db_path: &Path,
) -> UpgradeResult<UpgradeVersion1To2Summary> {
	log::info!("🔨 Running Frontier DB migration from version 1 to version 2. Please wait.");
	let mut res = UpgradeVersion1To2Summary {
		success: 0,
		error: vec![],
	};
	// Process a batch of hashes in a single db transaction
	#[rustfmt::skip]
	let mut process_chunk = |
		db: &parity_db::Db,
		ethereum_hashes: &[Vec<u8>]
	| -> UpgradeResult<()> {
		let mut transaction = vec![];
		for ethereum_hash in ethereum_hashes {
			let mut maybe_error = true;
			if let Some(substrate_hash) = db.get(super::columns::BLOCK_MAPPING as u8, ethereum_hash).map_err(|_|
				io::Error::new(ErrorKind::Other, "Key does not exist")
			)? {
				// Only update version1 data
				let decoded = Vec::<Block::Hash>::decode(&mut &substrate_hash[..]);
				if decoded.is_err() || decoded.unwrap().is_empty() {
					// Verify the substrate hash is part of the canonical chain.
					if let Ok(Some(number)) = client.number(Block::Hash::decode(&mut &substrate_hash[..]).unwrap()) {
						if let Ok(Some(hash)) = client.hash(number) {
							transaction.push((
								super::columns::BLOCK_MAPPING as u8,
								ethereum_hash,
								Some(vec![hash].encode()),
							));
							res.success += 1;
							maybe_error = false;
						}
					}
				}
			}
			if maybe_error {
				res.error.push(H256::from_slice(ethereum_hash));
			}
		}
		db.commit(transaction)
			.map_err(|_| io::Error::new(ErrorKind::Other, "Failed to commit on migrate_1_to_2"))?;
		Ok(())
	};

	let mut db_cfg = parity_db::Options::with_columns(db_path, V2_NUM_COLUMNS as u8);
	db_cfg.columns[super::columns::BLOCK_MAPPING as usize].btree_index = true;

	let db = parity_db::Db::open_or_create(&db_cfg)
		.map_err(|_| io::Error::new(ErrorKind::Other, "Failed to open db"))?;

	// Get all the block hashes we need to update
	let ethereum_hashes: Vec<_> = match db.iter(super::columns::BLOCK_MAPPING as u8) {
		Ok(mut iter) => {
			let mut hashes = vec![];
			while let Ok(Some((k, _))) = iter.next() {
				hashes.push(k);
			}
			hashes
		}
		Err(_) => vec![],
	};
	// Read and update each entry in db transaction batches
	const CHUNK_SIZE: usize = 10_000;
	let chunks = ethereum_hashes.chunks(CHUNK_SIZE);
	for chunk in chunks {
		process_chunk(&db, chunk)?;
	}
	Ok(res)
}

#[cfg(test)]
mod tests {
	use futures::executor;
	use sc_block_builder::BlockBuilderProvider;
	use sp_consensus::BlockOrigin;
	use substrate_test_runtime_client::{
		prelude::*, DefaultTestClientBuilderExt, TestClientBuilder,
	};

	use std::{
		io::{Read, Write},
		sync::Arc,
	};

	use crate::kv::DatabaseSettings;
	use scale_codec::Encode;
	use sp_blockchain::HeaderBackend;
	use sp_core::H256;
	use sp_runtime::{
		generic::{Block, Header},
		traits::{BlakeTwo256, Block as BlockT},
	};
	use tempfile::tempdir;

	type OpaqueBlock =
		Block<Header<u64, BlakeTwo256>, substrate_test_runtime_client::runtime::Extrinsic>;

	pub fn open_frontier_backend<Block: BlockT, C: HeaderBackend<Block>>(
		client: Arc<C>,
		setting: &crate::kv::DatabaseSettings,
	) -> Result<Arc<crate::kv::Backend<Block>>, String> {
		Ok(Arc::new(crate::kv::Backend::<Block>::new(client, setting)?))
	}

	#[cfg_attr(not(feature = "rocksdb"), ignore)]
	#[test]
	fn upgrade_1_to_2_works() {
		let settings: Vec<DatabaseSettings> = vec![
			// Rocks db
			#[cfg(feature = "rocksdb")]
			crate::kv::DatabaseSettings {
				source: sc_client_db::DatabaseSource::RocksDb {
					path: tempdir()
						.expect("create a temporary directory")
						.path()
						.to_owned(),
					cache_size: 0,
				},
			},
			// Parity db
			crate::kv::DatabaseSettings {
				source: sc_client_db::DatabaseSource::ParityDb {
					path: tempdir()
						.expect("create a temporary directory")
						.path()
						.to_owned(),
				},
			},
		];

		for setting in settings {
			let (client, _) = TestClientBuilder::new()
				.build_with_native_executor::<substrate_test_runtime_client::runtime::RuntimeApi, _>(
				None,
			);
			let mut client = Arc::new(client);

			// Genesis block
			let mut builder = client.new_block(Default::default()).unwrap();
			builder.push_storage_change(vec![1], None).unwrap();
			let block = builder.build().unwrap().block;
			let mut previous_canon_block_hash = block.header.hash();
			executor::block_on(client.import(BlockOrigin::Own, block)).unwrap();

			let path = setting.source.path().unwrap();

			let mut ethereum_hashes = vec![];
			let mut substrate_hashes = vec![];
			let mut transaction_hashes = vec![];
			{
				// Create a temporary frontier secondary DB.
				let backend = open_frontier_backend::<OpaqueBlock, _>(client.clone(), &setting)
					.expect("a temporary db was created");

				// Fill the tmp db with some data
				let mut transaction = sp_database::Transaction::new();
				for _ in 0..50 {
					// Ethereum hash
					let ethhash = H256::random();
					// Create two branches, and map the orphan one.
					// Keep track of the canon hash to later verify the migration replaced it.
					// A1
					let mut builder = client
						.new_block_at(previous_canon_block_hash, Default::default(), false)
						.unwrap();
					builder.push_storage_change(vec![1], None).unwrap();
					let block = builder.build().unwrap().block;
					let next_canon_block_hash = block.header.hash();
					executor::block_on(client.import(BlockOrigin::Own, block)).unwrap();
					// A2
					let mut builder = client
						.new_block_at(previous_canon_block_hash, Default::default(), false)
						.unwrap();
					builder.push_storage_change(vec![2], None).unwrap();
					let block = builder.build().unwrap().block;
					let orphan_block_hash = block.header.hash();
					executor::block_on(client.import(BlockOrigin::Own, block)).unwrap();

					// Track canon hash
					ethereum_hashes.push(ethhash);
					substrate_hashes.push(next_canon_block_hash);
					// Set orphan hash block mapping
					transaction.set(
						crate::kv::columns::BLOCK_MAPPING,
						&ethhash.encode(),
						&orphan_block_hash.encode(),
					);
					// Test also that one-to-many transaction data is not affected by the migration logic.
					// Map a transaction to both canon and orphan block hashes. This is what would have
					// happened in case of fork or equivocation.
					let eth_tx_hash = H256::random();
					let mut metadata = vec![];
					for hash in vec![next_canon_block_hash, orphan_block_hash].iter() {
						metadata.push(crate::kv::TransactionMetadata::<OpaqueBlock> {
							block_hash: *hash,
							ethereum_block_hash: ethhash,
							ethereum_index: 0u32,
						});
					}
					transaction.set(
						crate::kv::columns::TRANSACTION_MAPPING,
						&eth_tx_hash.encode(),
						&metadata.encode(),
					);
					transaction_hashes.push(eth_tx_hash);
					previous_canon_block_hash = next_canon_block_hash;
				}
				let _ = backend.mapping().db.commit(transaction);
			}

			// Writes version 1 to file.
			let _ = std::fs::create_dir_all(&path).expect("db path created");
			let mut version_path = path.to_owned();
			version_path.push("db_version");
			let mut version_file =
				std::fs::File::create(version_path).expect("db version file path created");
			version_file
				.write_all(format!("{}", 1).as_bytes())
				.expect("write version 1");

			// Upgrade database from version 1 to 2
			let _ = super::upgrade_db::<OpaqueBlock, _>(client.clone(), &path, &setting.source);

			// Check data after migration
			let backend = open_frontier_backend::<OpaqueBlock, _>(client, &setting)
				.expect("a temporary db was created");
			for (i, original_ethereum_hash) in ethereum_hashes.iter().enumerate() {
				let canon_substrate_block_hash = substrate_hashes.get(i).expect("Block hash");
				let mapped_block = backend
					.mapping()
					.block_hash(original_ethereum_hash)
					.unwrap()
					.unwrap();
				// All entries now hold a single element Vec
				assert_eq!(mapped_block.len(), 1);
				// The Vec holds the canon block hash
				assert_eq!(mapped_block.first(), Some(canon_substrate_block_hash));
				// Transaction hash still holds canon block data
				let mapped_transaction = backend
					.mapping()
					.transaction_metadata(transaction_hashes.get(i).expect("Transaction hash"))
					.unwrap();
				assert!(mapped_transaction
					.into_iter()
					.any(|tx| tx.block_hash == *canon_substrate_block_hash));
			}

			// Upgrade db version file
			assert_eq!(super::current_version(&path).expect("version"), 2u32);
		}
	}

	#[cfg(feature = "rocksdb")]
	#[test]
	fn create_db_with_current_version_works() {
		let tmp = tempdir().expect("create a temporary directory");

		let (client, _) = TestClientBuilder::new()
			.build_with_native_executor::<substrate_test_runtime_client::runtime::RuntimeApi, _>(
			None,
		);
		let client = Arc::new(client);

		let setting = crate::kv::DatabaseSettings {
			source: sc_client_db::DatabaseSource::RocksDb {
				path: tmp.path().to_owned(),
				cache_size: 0,
			},
		};
		let path = setting.source.path().unwrap();
		let _ = super::upgrade_db::<OpaqueBlock, _>(client.clone(), &path, &setting.source);

		let mut file =
			std::fs::File::open(crate::kv::upgrade::version_file_path(&path)).expect("file exist");

		let mut s = String::new();
		file.read_to_string(&mut s).expect("read file contents");
		assert_eq!(s.parse::<u32>().expect("parse file contents"), 2u32);
	}
}
