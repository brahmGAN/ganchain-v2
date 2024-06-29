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

mod overrides;
pub use self::overrides::*;

use std::{collections::BTreeMap, sync::Arc};

use scale_codec::Decode;
// Substrate
use sc_client_api::{backend::Backend, StorageProvider};
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::Block as BlockT;
use sp_storage::StorageKey;
// Frontier
use fp_rpc::EthereumRuntimeRPCApi;
use fp_storage::{EthereumStorageSchema, PALLET_ETHEREUM_SCHEMA};

pub fn overrides_handle<B, C, BE>(client: Arc<C>) -> Arc<OverrideHandle<B>>
where
	B: BlockT,
	C: ProvideRuntimeApi<B>,
	C::Api: EthereumRuntimeRPCApi<B>,
	C: HeaderBackend<B> + StorageProvider<B, BE> + 'static,
	BE: Backend<B> + 'static,
{
	let mut overrides_map = BTreeMap::new();
	overrides_map.insert(
		EthereumStorageSchema::V1,
		Box::new(SchemaV1Override::new(client.clone())) as Box<dyn StorageOverride<_>>,
	);
	overrides_map.insert(
		EthereumStorageSchema::V2,
		Box::new(SchemaV2Override::new(client.clone())) as Box<dyn StorageOverride<_>>,
	);
	overrides_map.insert(
		EthereumStorageSchema::V3,
		Box::new(SchemaV3Override::new(client.clone())) as Box<dyn StorageOverride<_>>,
	);

	Arc::new(OverrideHandle {
		schemas: overrides_map,
		fallback: Box::new(RuntimeApiStorageOverride::<B, C>::new(client)),
	})
}

pub fn onchain_storage_schema<B: BlockT, C, BE>(client: &C, hash: B::Hash) -> EthereumStorageSchema
where
	B: BlockT,
	C: HeaderBackend<B> + StorageProvider<B, BE>,
	BE: Backend<B>,
{
	match client.storage(hash, &StorageKey(PALLET_ETHEREUM_SCHEMA.to_vec())) {
		Ok(Some(bytes)) => Decode::decode(&mut &bytes.0[..])
			.ok()
			.unwrap_or(EthereumStorageSchema::Undefined),
		_ => EthereumStorageSchema::Undefined,
	}
}
