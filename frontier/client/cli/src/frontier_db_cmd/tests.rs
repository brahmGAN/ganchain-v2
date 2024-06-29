// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
// This file is part of Frontier.
//
// Copyright (c) 2020 Parity Technologies (UK) Ltd.
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

use std::{collections::HashMap, path::PathBuf, str::FromStr, sync::Arc};

use ethereum_types::H256;
use futures::executor;
use scale_codec::Encode;
use serde::Serialize;
use tempfile::tempdir;
// Substrate
use sc_block_builder::BlockBuilderProvider;
use sc_cli::DatabasePruningMode;
use sp_blockchain::HeaderBackend;
use sp_consensus::BlockOrigin;
use sp_io::hashing::twox_128;
use sp_runtime::{
	generic::{Block, Header},
	traits::{BlakeTwo256, Block as BlockT},
};
use substrate_test_runtime_client::{
	BlockBuilderExt, ClientBlockImportExt, ClientExt, DefaultTestClientBuilderExt,
	TestClientBuilder,
};
// Frontier
use fp_storage::{EthereumStorageSchema, ETHEREUM_CURRENT_TRANSACTION_STATUS, PALLET_ETHEREUM};
use frontier_template_runtime::RuntimeApi;

use crate::frontier_db_cmd::{Column, FrontierDbCmd, Operation};

type OpaqueBlock =
	Block<Header<u64, BlakeTwo256>, substrate_test_runtime_client::runtime::Extrinsic>;

pub fn open_frontier_backend<Block: BlockT, C: HeaderBackend<Block>>(
	client: Arc<C>,
	path: PathBuf,
) -> Result<Arc<fc_db::kv::Backend<Block>>, String> {
	Ok(Arc::new(fc_db::kv::Backend::<Block>::new(
		client,
		&fc_db::kv::DatabaseSettings {
			source: sc_client_db::DatabaseSource::RocksDb {
				path,
				cache_size: 0,
			},
		},
	)?))
}

fn storage_prefix_build(module: &[u8], storage: &[u8]) -> Vec<u8> {
	[twox_128(module), twox_128(storage)].concat().to_vec()
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum TestValue {
	Schema(HashMap<H256, EthereumStorageSchema>),
	Tips(Vec<<OpaqueBlock as BlockT>::Hash>),
	Commitment(<OpaqueBlock as BlockT>::Hash),
}

fn cmd(key: String, value: Option<PathBuf>, operation: Operation, column: Column) -> FrontierDbCmd {
	FrontierDbCmd {
		operation,
		column,
		key,
		value,
		shared_params: sc_cli::SharedParams {
			chain: None,
			dev: true,
			base_path: None,
			log: vec![],
			disable_log_color: true,
			enable_log_reloading: true,
			tracing_targets: None,
			tracing_receiver: sc_cli::arg_enums::TracingReceiver::Log,
			detailed_log_output: false,
		},
		pruning_params: sc_cli::PruningParams {
			state_pruning: Some(DatabasePruningMode::Archive),
			blocks_pruning: DatabasePruningMode::Archive,
		},
	}
}

fn schema_test_value() -> TestValue {
	let mut inner = HashMap::new();
	inner.insert(H256::default(), EthereumStorageSchema::V1);
	TestValue::Schema(inner)
}

fn tips_test_value() -> TestValue {
	TestValue::Tips(vec![H256::default()])
}

fn test_json_file(tmp: &tempfile::TempDir, value: &TestValue) -> PathBuf {
	let test_value_path = tmp.path().join("test.json");
	std::fs::write(
		test_value_path.clone(),
		serde_json::to_string_pretty(value).unwrap(),
	)
	.expect("write test value json file");
	test_value_path
}

#[test]
fn schema_create_success_if_value_is_empty() {
	let tmp = tempdir().expect("create a temporary directory");
	// Write some data in a temp file.
	let test_value_path = test_json_file(&tmp, &schema_test_value());

	// Test client.
	let (client, _) = TestClientBuilder::new().build_with_native_executor::<RuntimeApi, _>(None);
	let client = Arc::new(client);
	// Create a temporary frontier secondary DB.
	let backend = open_frontier_backend::<OpaqueBlock, _>(client.clone(), tmp.into_path())
		.expect("a temporary db was created");

	assert_eq!(backend.meta().ethereum_schema(), Ok(None));

	// Run the command
	assert!(cmd(
		":ethereum_schema_cache".to_string(),
		Some(test_value_path),
		Operation::Create,
		Column::Meta
	)
	.run(client, backend.clone())
	.is_ok());

	assert_eq!(
		backend.meta().ethereum_schema(),
		Ok(Some(vec![(EthereumStorageSchema::V1, H256::default())]))
	);
}

#[test]
fn schema_create_fails_if_value_is_not_empty() {
	let tmp = tempdir().expect("create a temporary directory");
	// Write some data in a temp file.
	let test_value_path = test_json_file(&tmp, &schema_test_value());

	// Test client.
	let (client, _) = TestClientBuilder::new().build_with_native_executor::<RuntimeApi, _>(None);
	let client = Arc::new(client);
	// Create a temporary frontier secondary DB.
	let backend = open_frontier_backend::<OpaqueBlock, _>(client.clone(), tmp.into_path())
		.expect("a temporary db was created");

	let data_before = vec![(EthereumStorageSchema::V2, H256::default())];

	let _ = backend
		.meta()
		.write_ethereum_schema(data_before.clone())
		.expect("data inserted in temporary db");

	// Run the command
	assert!(cmd(
		":ethereum_schema_cache".to_string(),
		Some(test_value_path),
		Operation::Create,
		Column::Meta
	)
	.run(client, backend.clone())
	.is_err());

	let data_after = backend.meta().ethereum_schema().unwrap().unwrap();
	assert_eq!(data_after, data_before);
}

#[test]
fn schema_read_works() {
	let tmp = tempdir().expect("create a temporary directory");
	// Test client.
	let (client, _) = TestClientBuilder::new().build_with_native_executor::<RuntimeApi, _>(None);
	let client = Arc::new(client);
	// Create a temporary frontier secondary DB.
	let backend = open_frontier_backend::<OpaqueBlock, _>(client.clone(), tmp.into_path())
		.expect("a temporary db was created");

	assert_eq!(backend.meta().ethereum_schema(), Ok(None));

	let data = vec![(EthereumStorageSchema::V2, H256::default())];

	let _ = backend
		.meta()
		.write_ethereum_schema(data.clone())
		.expect("data inserted in temporary db");

	// Run the command
	assert!(cmd(
		":ethereum_schema_cache".to_string(),
		None,
		Operation::Read,
		Column::Meta
	)
	.run(client, backend.clone())
	.is_ok());
}

#[test]
fn schema_update_works() {
	let tmp = tempdir().expect("create a temporary directory");
	// Write some data in a temp file.
	let test_value_path = test_json_file(&tmp, &schema_test_value());
	// Test client.
	let (client, _) = TestClientBuilder::new().build_with_native_executor::<RuntimeApi, _>(None);
	let client = Arc::new(client);
	// Create a temporary frontier secondary DB.
	let backend = open_frontier_backend::<OpaqueBlock, _>(client.clone(), tmp.into_path())
		.expect("a temporary db was created");

	assert_eq!(backend.meta().ethereum_schema(), Ok(None));
	// Run the command
	assert!(cmd(
		":ethereum_schema_cache".to_string(),
		Some(test_value_path),
		Operation::Update,
		Column::Meta
	)
	.run(client, backend.clone())
	.is_ok());

	assert_eq!(
		backend.meta().ethereum_schema(),
		Ok(Some(vec![(EthereumStorageSchema::V1, H256::default())]))
	);
}

#[test]
fn schema_delete_works() {
	let tmp = tempdir().expect("create a temporary directory");
	// Test client.
	let (client, _) = TestClientBuilder::new().build_with_native_executor::<RuntimeApi, _>(None);
	let client = Arc::new(client);
	// Create a temporary frontier secondary DB.
	let backend = open_frontier_backend::<OpaqueBlock, _>(client.clone(), tmp.into_path())
		.expect("a temporary db was created");

	let data = vec![(EthereumStorageSchema::V2, H256::default())];

	let _ = backend
		.meta()
		.write_ethereum_schema(data.clone())
		.expect("data inserted in temporary db");
	// Run the command
	assert!(cmd(
		":ethereum_schema_cache".to_string(),
		None,
		Operation::Delete,
		Column::Meta
	)
	.run(client, backend.clone())
	.is_ok());

	assert_eq!(backend.meta().ethereum_schema(), Ok(Some(vec![])));
}

#[test]
fn tips_create_success_if_value_is_empty() {
	let tmp = tempdir().expect("create a temporary directory");
	// Write some data in a temp file.
	let test_value_path = test_json_file(&tmp, &tips_test_value());
	// Test client.
	let (client, _) = TestClientBuilder::new().build_with_native_executor::<RuntimeApi, _>(None);
	let client = Arc::new(client);
	// Create a temporary frontier secondary DB.
	let backend = open_frontier_backend::<OpaqueBlock, _>(client.clone(), tmp.into_path())
		.expect("a temporary db was created");

	assert_eq!(backend.meta().current_syncing_tips(), Ok(vec![]));
	// Run the command
	assert!(cmd(
		"CURRENT_SYNCING_TIPS".to_string(),
		Some(test_value_path),
		Operation::Create,
		Column::Meta
	)
	.run(client, backend.clone())
	.is_ok());

	assert_eq!(
		backend.meta().current_syncing_tips(),
		Ok(vec![H256::default()])
	);
}

#[test]
fn tips_create_fails_if_value_is_not_empty() {
	let tmp = tempdir().expect("create a temporary directory");
	// Write some data in a temp file.
	let test_value_path = test_json_file(&tmp, &tips_test_value());
	// Test client.
	let (client, _) = TestClientBuilder::new().build_with_native_executor::<RuntimeApi, _>(None);
	let client = Arc::new(client);
	// Create a temporary frontier secondary DB.
	let backend = open_frontier_backend::<OpaqueBlock, _>(client.clone(), tmp.into_path())
		.expect("a temporary db was created");

	let data_before = vec![H256::default()];

	let _ = backend
		.meta()
		.write_current_syncing_tips(data_before.clone())
		.expect("data inserted in temporary db");
	// Run the command
	assert!(cmd(
		"CURRENT_SYNCING_TIPS".to_string(),
		Some(test_value_path),
		Operation::Create,
		Column::Meta
	)
	.run(client, backend.clone())
	.is_err());

	let data_after = backend.meta().current_syncing_tips().unwrap();
	assert_eq!(data_after, data_before);
}

#[test]
fn tips_read_works() {
	let tmp = tempdir().expect("create a temporary directory");
	// Test client.
	let (client, _) = TestClientBuilder::new().build_with_native_executor::<RuntimeApi, _>(None);
	let client = Arc::new(client);
	// Create a temporary frontier secondary DB.
	let backend = open_frontier_backend::<OpaqueBlock, _>(client.clone(), tmp.into_path())
		.expect("a temporary db was created");

	assert_eq!(backend.meta().current_syncing_tips(), Ok(vec![]));

	let data = vec![H256::default()];

	let _ = backend
		.meta()
		.write_current_syncing_tips(data.clone())
		.expect("data inserted in temporary db");
	// Run the command
	assert!(cmd(
		"CURRENT_SYNCING_TIPS".to_string(),
		None,
		Operation::Read,
		Column::Meta
	)
	.run(client, backend.clone())
	.is_ok());
}

#[test]
fn tips_update_works() {
	let tmp = tempdir().expect("create a temporary directory");
	// Write some data in a temp file.
	let test_value_path = test_json_file(&tmp, &tips_test_value());
	// Test client.
	let (client, _) = TestClientBuilder::new().build_with_native_executor::<RuntimeApi, _>(None);
	let client = Arc::new(client);
	// Create a temporary frontier secondary DB.
	let backend = open_frontier_backend::<OpaqueBlock, _>(client.clone(), tmp.into_path())
		.expect("a temporary db was created");

	assert_eq!(backend.meta().current_syncing_tips(), Ok(vec![]));
	// Run the command
	assert!(cmd(
		"CURRENT_SYNCING_TIPS".to_string(),
		Some(test_value_path),
		Operation::Update,
		Column::Meta
	)
	.run(client, backend.clone())
	.is_ok());

	assert_eq!(
		backend.meta().current_syncing_tips(),
		Ok(vec![H256::default()])
	);
}

#[test]
fn tips_delete_works() {
	let tmp = tempdir().expect("create a temporary directory");
	// Test client.
	let (client, _) = TestClientBuilder::new().build_with_native_executor::<RuntimeApi, _>(None);
	let client = Arc::new(client);
	// Create a temporary frontier secondary DB.
	let backend = open_frontier_backend::<OpaqueBlock, _>(client.clone(), tmp.into_path())
		.expect("a temporary db was created");

	let data = vec![H256::default()];

	let _ = backend
		.meta()
		.write_current_syncing_tips(data.clone())
		.expect("data inserted in temporary db");
	// Run the command
	assert!(cmd(
		"CURRENT_SYNCING_TIPS".to_string(),
		None,
		Operation::Delete,
		Column::Meta
	)
	.run(client, backend.clone())
	.is_ok());

	assert_eq!(backend.meta().current_syncing_tips(), Ok(vec![]));
}

#[test]
fn non_existent_meta_static_keys_are_no_op() {
	let tmp = tempdir().expect("create a temporary directory");
	// Write some data in a temp file.
	let test_value_path = test_json_file(&tmp, &schema_test_value());
	// Test client.
	let (client, _) = TestClientBuilder::new().build_with_native_executor::<RuntimeApi, _>(None);
	let client = Arc::new(client);
	// Create a temporary frontier secondary DB.
	let backend = open_frontier_backend::<OpaqueBlock, _>(client.clone(), tmp.into_path())
		.expect("a temporary db was created");
	let client = client;

	let data = vec![(EthereumStorageSchema::V1, H256::default())];

	let _ = backend
		.meta()
		.write_ethereum_schema(data.clone())
		.expect("data inserted in temporary db");

	// Run the Create command
	assert!(cmd(
		":foo".to_string(),
		Some(test_value_path.clone()),
		Operation::Create,
		Column::Meta
	)
	.run(Arc::clone(&client), backend.clone())
	.is_err());

	assert_eq!(
		backend.meta().ethereum_schema(),
		Ok(Some(vec![(EthereumStorageSchema::V1, H256::default())]))
	);

	// Run the Read command
	assert!(cmd(":foo".to_string(), None, Operation::Read, Column::Meta)
		.run(Arc::clone(&client), backend.clone())
		.is_err());

	// Run the Update command
	assert!(cmd(
		":foo".to_string(),
		Some(test_value_path),
		Operation::Update,
		Column::Meta
	)
	.run(Arc::clone(&client), backend.clone())
	.is_err());

	assert_eq!(
		backend.meta().ethereum_schema(),
		Ok(Some(vec![(EthereumStorageSchema::V1, H256::default())]))
	);

	// Run the Delete command
	assert!(
		cmd(":foo".to_string(), None, Operation::Delete, Column::Meta)
			.run(Arc::clone(&client), backend.clone())
			.is_err()
	);

	assert_eq!(
		backend.meta().ethereum_schema(),
		Ok(Some(vec![(EthereumStorageSchema::V1, H256::default())]))
	);
}

#[test]
fn not_deserializable_input_value_is_no_op() {
	let tmp = tempdir().expect("create a temporary directory");
	// Write some data in a temp file.
	let test_value_path = tmp.path().join("test.json");

	std::fs::write(
		test_value_path.clone(),
		serde_json::to_string("im_not_allowed_here").unwrap(),
	)
	.expect("write test value json file");
	// Test client.
	let (client, _) = TestClientBuilder::new().build_with_native_executor::<RuntimeApi, _>(None);
	let client = Arc::new(client);
	// Create a temporary frontier secondary DB.
	let backend = open_frontier_backend::<OpaqueBlock, _>(client.clone(), tmp.into_path())
		.expect("a temporary db was created");
	let client = client;

	// Run the Create command
	assert!(cmd(
		":ethereum_schema_cache".to_string(),
		Some(test_value_path.clone()),
		Operation::Create,
		Column::Meta
	)
	.run(Arc::clone(&client), backend.clone())
	.is_err());

	assert_eq!(backend.meta().ethereum_schema(), Ok(None));

	// Run the Update command
	assert!(cmd(
		":ethereum_schema_cache".to_string(),
		Some(test_value_path),
		Operation::Update,
		Column::Meta
	)
	.run(Arc::clone(&client), backend.clone())
	.is_err());

	assert_eq!(backend.meta().ethereum_schema(), Ok(None));
}

#[ignore]
#[test]
fn commitment_create() {
	let tmp = tempdir().expect("create a temporary directory");

	// Test client.
	let (c, _) = TestClientBuilder::new().build_with_native_executor::<RuntimeApi, _>(None);
	let mut client = Arc::new(c);

	// Get some transaction status.
	let t1 = fp_rpc::TransactionStatus::default();
	let t1_hash = t1.transaction_hash;
	let statuses = vec![t1];

	// Build a block and fill the pallet-ethereum status.
	let key = storage_prefix_build(PALLET_ETHEREUM, ETHEREUM_CURRENT_TRANSACTION_STATUS);
	let mut builder = client.new_block(Default::default()).unwrap();
	builder
		.push_storage_change(key, Some(statuses.encode()))
		.unwrap();
	let block = builder.build().unwrap().block;
	let block_hash = block.header.hash();
	executor::block_on(client.import(BlockOrigin::Own, block)).unwrap();

	// Set the substrate block hash as the value for the command.
	let test_value_path = test_json_file(&tmp, &TestValue::Commitment(block_hash));

	// Create a temporary frontier secondary DB.
	let backend = open_frontier_backend::<OpaqueBlock, _>(client.clone(), tmp.into_path())
		.expect("a temporary db was created");

	// Run the command using some ethereum block hash as key.
	let ethereum_block_hash = H256::default();
	assert!(cmd(
		format!("{:?}", ethereum_block_hash),
		Some(test_value_path.clone()),
		Operation::Create,
		Column::Block
	)
	.run(Arc::clone(&client), backend.clone())
	.is_ok());

	// Expect the ethereum and substrate block hashes to be mapped.
	assert_eq!(
		backend.mapping().block_hash(&ethereum_block_hash),
		Ok(Some(vec![block_hash]))
	);

	// Expect the offchain-stored transaction metadata to match the one we stored in the runtime.
	let expected_transaction_metadata = fc_db::TransactionMetadata {
		block_hash,
		ethereum_block_hash,
		ethereum_index: 0,
	};
	assert_eq!(
		backend.mapping().transaction_metadata(&t1_hash),
		Ok(vec![expected_transaction_metadata])
	);

	// Expect a second command run to fail, as the key is not empty anymore.
	assert!(cmd(
		format!("{:?}", ethereum_block_hash),
		Some(test_value_path),
		Operation::Create,
		Column::Block
	)
	.run(Arc::clone(&client), backend.clone())
	.is_err());
}

#[ignore]
#[test]
fn commitment_update() {
	let tmp = tempdir().expect("create a temporary directory");

	// Test client.
	let (c, _) = TestClientBuilder::new().build_with_native_executor::<RuntimeApi, _>(None);
	let mut client = Arc::new(c);

	// Get some transaction status.
	let t1 = fp_rpc::TransactionStatus::default();
	let mut t2 = fp_rpc::TransactionStatus::default();
	t2.transaction_hash =
		H256::from_str("0x2200000000000000000000000000000000000000000000000000000000000000")
			.unwrap();
	let t1_hash = t1.transaction_hash;
	let t2_hash = t2.transaction_hash;
	let statuses_a1 = vec![t1.clone()];
	let statuses_a2 = vec![t1, t2];

	let key = storage_prefix_build(PALLET_ETHEREUM, ETHEREUM_CURRENT_TRANSACTION_STATUS);

	// First we create block and insert data in the offchain db.

	// Build a block A1 and fill the pallet-ethereum status.
	let mut builder = client
		.new_block_at(client.genesis_hash(), Default::default(), false)
		.unwrap();
	builder
		.push_storage_change(key.clone(), Some(statuses_a1.encode()))
		.unwrap();
	let block_a1 = builder.build().unwrap().block;
	let block_a1_hash = block_a1.header.hash();
	executor::block_on(client.import(BlockOrigin::Own, block_a1)).unwrap();

	// Set the substrate block hash as the value for the command.
	let test_value_path = test_json_file(&tmp, &TestValue::Commitment(block_a1_hash));

	// Create a temporary frontier secondary DB.
	let backend = open_frontier_backend::<OpaqueBlock, _>(client.clone(), tmp.into_path())
		.expect("a temporary db was created");

	// Run the command using some ethereum block hash as key.
	let ethereum_block_hash = H256::default();
	assert!(cmd(
		format!("{:?}", ethereum_block_hash),
		Some(test_value_path.clone()),
		Operation::Create,
		Column::Block
	)
	.run(Arc::clone(&client), backend.clone())
	.is_ok());

	// Expect the ethereum and substrate block hashes to be mapped.
	assert_eq!(
		backend.mapping().block_hash(&ethereum_block_hash),
		Ok(Some(vec![block_a1_hash]))
	);

	// Expect the offchain-stored transaction metadata to match the one we stored in the runtime.
	let expected_transaction_metadata_a1_t1 = fc_db::TransactionMetadata {
		block_hash: block_a1_hash,
		ethereum_block_hash,
		ethereum_index: 0,
	};
	assert_eq!(
		backend.mapping().transaction_metadata(&t1_hash),
		Ok(vec![expected_transaction_metadata_a1_t1.clone()])
	);

	// Next we create a new block and update the offchain db.

	// Build a block A2 and fill the pallet-ethereum status.
	let tmp = tempdir().expect("create a temporary directory");

	let mut builder = client
		.new_block_at(client.genesis_hash(), Default::default(), false)
		.unwrap();
	builder
		.push_storage_change(key, Some(statuses_a2.encode()))
		.unwrap();
	let block_a2 = builder.build().unwrap().block;
	let block_a2_hash = block_a2.header.hash();
	executor::block_on(client.import(BlockOrigin::Own, block_a2)).unwrap();

	// Set the substrate block hash as the value for the command.
	let test_value_path = test_json_file(&tmp, &TestValue::Commitment(block_a2_hash));

	// Run the command using some ethereum block hash as key.
	let ethereum_block_hash = H256::default();
	assert!(cmd(
		format!("{:?}", ethereum_block_hash),
		Some(test_value_path.clone()),
		Operation::Update,
		Column::Block
	)
	.run(Arc::clone(&client), backend.clone())
	.is_ok());

	// Expect the ethereum and substrate block hashes to be mapped.
	assert_eq!(
		backend.mapping().block_hash(&ethereum_block_hash),
		Ok(Some(vec![block_a1_hash, block_a2_hash]))
	);

	// Expect the offchain-stored transaction metadata to have data for both blocks.
	let expected_transaction_metadata_a2_t1 = fc_db::TransactionMetadata {
		block_hash: block_a2_hash,
		ethereum_block_hash,
		ethereum_index: 0,
	};
	let expected_transaction_metadata_a2_t2 = fc_db::TransactionMetadata {
		block_hash: block_a2_hash,
		ethereum_block_hash,
		ethereum_index: 1,
	};
	assert_eq!(
		backend.mapping().transaction_metadata(&t1_hash),
		Ok(vec![
			expected_transaction_metadata_a1_t1,
			expected_transaction_metadata_a2_t1
		])
	);
	assert_eq!(
		backend.mapping().transaction_metadata(&t2_hash),
		Ok(vec![expected_transaction_metadata_a2_t2])
	);
}

#[ignore]
#[test]
fn mapping_read_works() {
	let tmp = tempdir().expect("create a temporary directory");

	// Test client.
	let (c, _) = TestClientBuilder::new().build_with_native_executor::<RuntimeApi, _>(None);
	let mut client = Arc::new(c);

	// Get some transaction status.
	let t1 = fp_rpc::TransactionStatus::default();
	let t1_hash = t1.transaction_hash;
	let statuses = vec![t1];

	// Build a block and fill the pallet-ethereum status.
	let key = storage_prefix_build(PALLET_ETHEREUM, ETHEREUM_CURRENT_TRANSACTION_STATUS);
	let mut builder = client.new_block(Default::default()).unwrap();
	builder
		.push_storage_change(key, Some(statuses.encode()))
		.unwrap();
	let block = builder.build().unwrap().block;
	let block_hash = block.header.hash();
	executor::block_on(client.import(BlockOrigin::Own, block)).unwrap();

	// Set the substrate block hash as the value for the command.
	let test_value_path = test_json_file(&tmp, &TestValue::Commitment(block_hash));

	// Create a temporary frontier secondary DB.
	let backend = open_frontier_backend::<OpaqueBlock, _>(client.clone(), tmp.into_path())
		.expect("a temporary db was created");

	// Create command using some ethereum block hash as key.
	let ethereum_block_hash = H256::default();
	assert!(cmd(
		format!("{:?}", ethereum_block_hash),
		Some(test_value_path.clone()),
		Operation::Create,
		Column::Block,
	)
	.run(Arc::clone(&client), backend.clone())
	.is_ok());

	// Read block command.
	assert!(cmd(
		format!("{:?}", ethereum_block_hash),
		None,
		Operation::Read,
		Column::Block
	)
	.run(Arc::clone(&client), backend.clone())
	.is_ok());

	// Read transaction command.
	assert!(cmd(
		format!("{:?}", t1_hash),
		None,
		Operation::Read,
		Column::Transaction
	)
	.run(Arc::clone(&client), backend.clone())
	.is_ok());
}
