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

#![allow(clippy::format_in_format_args)]

use std::{
	fs,
	io::{self, Read},
	path::PathBuf,
};

use serde::de::DeserializeOwned;
use serde_json::Deserializer;
// Substrate
use sp_runtime::traits::Block as BlockT;

use super::{DbValue, Operation};

pub fn maybe_deserialize_value<B: BlockT>(
	operation: &Operation,
	value: Option<&PathBuf>,
) -> sc_cli::Result<Option<DbValue<B::Hash>>> {
	fn parse_db_values<H: DeserializeOwned, I: Read + Send>(
		input: I,
	) -> sc_cli::Result<Option<DbValue<H>>> {
		let mut stream_deser = Deserializer::from_reader(input).into_iter::<DbValue<H>>();
		if let Some(Ok(value)) = stream_deser.next() {
			Ok(Some(value))
		} else {
			Err("Failed to deserialize value data".into())
		}
	}

	if let Operation::Create | Operation::Update = operation {
		match &value {
			Some(filename) => parse_db_values::<B::Hash, _>(fs::File::open(filename)?),
			None => {
				let mut buffer = String::new();
				let res = parse_db_values(io::stdin());
				let _ = io::stdin().read_line(&mut buffer);
				res
			}
		}
	} else {
		Ok(None)
	}
}

/// Messaging and prompt.
pub trait FrontierDbMessage {
	fn key_value_error<K: core::fmt::Debug, V: core::fmt::Debug>(
		&self,
		key: K,
		value: &V,
	) -> sc_cli::Error {
		format!(
			"Key `{:?}` and Value `{:?}` are not compatible with this operation",
			key, value
		)
		.into()
	}

	fn key_column_error<K: core::fmt::Debug, V: core::fmt::Debug>(
		&self,
		key: K,
		value: &V,
	) -> sc_cli::Error {
		format!(
			"Key `{:?}` and Column `{:?}` are not compatible with this operation",
			key, value
		)
		.into()
	}

	fn key_not_empty_error<K: core::fmt::Debug>(&self, key: K) -> sc_cli::Error {
		format!("Operation not allowed for non-empty Key `{:?}`", key).into()
	}

	fn one_to_many_error(&self) -> sc_cli::Error {
		"One-to-many operation not allowed".into()
	}

	#[cfg(not(test))]
	fn confirmation_prompt<K: core::fmt::Debug, V: core::fmt::Debug>(
		&self,
		operation: &Operation,
		key: K,
		existing_value: &V,
		new_value: &V,
	) -> sc_cli::Result<()> {
		println!(
			"{}",
			format!(
				r#"
			---------------------------------------------
			Operation: {:?}
			Key: {:?}
			Existing value: {:?}
			New value: {:?}
			---------------------------------------------
			Type `confirm` and press [Enter] to confirm:
		"#,
				operation, key, existing_value, new_value
			)
		);

		let mut buffer = String::new();
		io::stdin().read_line(&mut buffer)?;
		if buffer.trim() != "confirm" {
			return Err("-- Cancel exit --".into());
		}
		Ok(())
	}

	#[cfg(test)]
	fn confirmation_prompt<K: core::fmt::Debug, V: core::fmt::Debug>(
		&self,
		_operation: &Operation,
		_key: K,
		_existing_value: &V,
		_new_value: &V,
	) -> sc_cli::Result<()> {
		Ok(())
	}
}
