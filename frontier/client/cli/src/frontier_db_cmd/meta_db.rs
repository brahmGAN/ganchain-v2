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

use std::{
	collections::HashMap,
	str::{self, FromStr},
	sync::Arc,
};

use ethereum_types::H256;
use serde::Deserialize;
// Substrate
use sp_runtime::traits::Block as BlockT;

use super::{utils::FrontierDbMessage, FrontierDbCmd, Operation};

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum MetaValue<H> {
	Tips(Vec<H>),
	Schema(HashMap<H256, fp_storage::EthereumStorageSchema>),
}

#[derive(Clone, Copy, Debug)]
pub enum MetaKey {
	Tips,
	Schema,
}

impl FromStr for MetaKey {
	type Err = sc_cli::Error;

	// A convenience function to verify the user input is known.
	fn from_str(input: &str) -> Result<MetaKey, Self::Err> {
		let tips = str::from_utf8(fc_db::kv::static_keys::CURRENT_SYNCING_TIPS).unwrap();
		let schema = str::from_utf8(fp_storage::PALLET_ETHEREUM_SCHEMA_CACHE).unwrap();
		match input {
			x if x == tips => Ok(MetaKey::Tips),
			y if y == schema => Ok(MetaKey::Schema),
			_ => Err(format!("`{:?}` is not a meta column static key", input).into()),
		}
	}
}

pub struct MetaDb<'a, B: BlockT> {
	cmd: &'a FrontierDbCmd,
	backend: Arc<fc_db::kv::Backend<B>>,
}

impl<'a, B: BlockT> MetaDb<'a, B> {
	pub fn new(cmd: &'a FrontierDbCmd, backend: Arc<fc_db::kv::Backend<B>>) -> Self {
		Self { cmd, backend }
	}

	pub fn query(&self, key: &MetaKey, value: &Option<MetaValue<B::Hash>>) -> sc_cli::Result<()> {
		match self.cmd.operation {
			Operation::Create => match (key, value) {
				// Insert data to the meta column, static tips key.
				(MetaKey::Tips, Some(MetaValue::Tips(hashes))) => {
					if self.backend.meta().current_syncing_tips()?.is_empty() {
						self.backend
							.meta()
							.write_current_syncing_tips(hashes.clone())?;
					} else {
						return Err(self.key_not_empty_error(key));
					}
				}
				// Insert data to the meta column, static schema cache key.
				(MetaKey::Schema, Some(MetaValue::Schema(schema_map))) => {
					if self.backend.meta().ethereum_schema()?.is_none() {
						let data = schema_map
							.iter()
							.map(|(key, value)| (*value, *key))
							.collect::<Vec<(fp_storage::EthereumStorageSchema, H256)>>();
						self.backend.meta().write_ethereum_schema(data)?;
					} else {
						return Err(self.key_not_empty_error(key));
					}
				}
				_ => return Err(self.key_value_error(key, value)),
			},
			Operation::Read => match key {
				// Read meta column, static tips key.
				MetaKey::Tips => {
					let value = self.backend.meta().current_syncing_tips()?;
					println!("{:?}", value);
				}
				// Read meta column, static schema cache key.
				MetaKey::Schema => {
					let value = self.backend.meta().ethereum_schema()?;
					println!("{:?}", value);
				}
			},
			Operation::Update => match (key, value) {
				// Update the static tips key's value.
				(MetaKey::Tips, Some(MetaValue::Tips(new_value))) => {
					let value = self.backend.meta().current_syncing_tips()?;
					self.confirmation_prompt(&self.cmd.operation, key, &value, new_value)?;
					self.backend
						.meta()
						.write_current_syncing_tips(new_value.clone())?;
				}
				// Update the static schema cache key's value.
				(MetaKey::Schema, Some(MetaValue::Schema(schema_map))) => {
					let value = self.backend.meta().ethereum_schema()?;
					let new_value = schema_map
						.iter()
						.map(|(key, value)| (*value, *key))
						.collect::<Vec<(fp_storage::EthereumStorageSchema, H256)>>();
					self.confirmation_prompt(
						&self.cmd.operation,
						key,
						&value,
						&Some(new_value.clone()),
					)?;
					self.backend.meta().write_ethereum_schema(new_value)?;
				}
				_ => return Err(self.key_value_error(key, value)),
			},
			Operation::Delete => match key {
				// Deletes the static tips key's value.
				MetaKey::Tips => {
					let value = self.backend.meta().current_syncing_tips()?;
					self.confirmation_prompt(&self.cmd.operation, key, &value, &vec![])?;
					self.backend.meta().write_current_syncing_tips(vec![])?;
				}
				// Deletes the static schema cache key's value.
				MetaKey::Schema => {
					let value = self.backend.meta().ethereum_schema()?;
					self.confirmation_prompt(&self.cmd.operation, key, &value, &Some(vec![]))?;
					self.backend.meta().write_ethereum_schema(vec![])?;
				}
			},
		}
		Ok(())
	}
}

impl<'a, B: BlockT> FrontierDbMessage for MetaDb<'a, B> {}
