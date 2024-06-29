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

#![deny(unused_crate_dependencies)]

use scale_codec::{Decode, Encode};
// Substrate
pub use sc_client_db::DatabaseSource;
use sp_core::H256;
use sp_runtime::traits::Block as BlockT;

pub mod kv;
use kv::{columns, static_keys};

#[cfg(feature = "sql")]
pub mod sql;

#[derive(Clone)]
pub enum Backend<Block: BlockT> {
	KeyValue(kv::Backend<Block>),
	#[cfg(feature = "sql")]
	Sql(sql::Backend<Block>),
}

#[derive(Clone, Encode, Debug, Decode, Eq, PartialEq)]
pub struct TransactionMetadata<Block: BlockT> {
	pub block_hash: Block::Hash,
	pub ethereum_block_hash: H256,
	pub ethereum_index: u32,
}

#[derive(Debug, Eq, PartialEq)]
pub struct FilteredLog<Block: BlockT> {
	pub substrate_block_hash: Block::Hash,
	pub ethereum_block_hash: H256,
	pub block_number: u32,
	pub ethereum_storage_schema: fp_storage::EthereumStorageSchema,
	pub transaction_index: u32,
	pub log_index: u32,
}

#[async_trait::async_trait]
pub trait BackendReader<Block: BlockT> {
	async fn block_hash(
		&self,
		ethereum_block_hash: &H256,
	) -> Result<Option<Vec<Block::Hash>>, String>;

	async fn transaction_metadata(
		&self,
		ethereum_transaction_hash: &H256,
	) -> Result<Vec<TransactionMetadata<Block>>, String>;

	async fn filter_logs(
		&self,
		from_block: u64,
		to_block: u64,
		addresses: Vec<sp_core::H160>,
		topics: Vec<Vec<Option<H256>>>,
	) -> Result<Vec<FilteredLog<Block>>, String>;

	fn is_indexed(&self) -> bool;
}
