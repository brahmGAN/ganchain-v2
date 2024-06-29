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

use std::sync::Arc;

use jsonrpsee::core::RpcResult;
// Substrate
use sc_network::{NetworkPeers, NetworkService};
use sc_network_common::ExHashT;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::Block as BlockT;
// Frontier
use fc_rpc_core::{types::PeerCount, NetApiServer};
use fp_rpc::EthereumRuntimeRPCApi;

use crate::internal_err;

/// Net API implementation.
pub struct Net<B: BlockT, C, H: ExHashT> {
	client: Arc<C>,
	network: Arc<NetworkService<B, H>>,
	peer_count_as_hex: bool,
}

impl<B: BlockT, C, H: ExHashT> Net<B, C, H> {
	pub fn new(
		client: Arc<C>,
		network: Arc<NetworkService<B, H>>,
		peer_count_as_hex: bool,
	) -> Self {
		Self {
			client,
			network,
			peer_count_as_hex,
		}
	}
}

impl<B, C, H: ExHashT> NetApiServer for Net<B, C, H>
where
	B: BlockT,
	C: ProvideRuntimeApi<B>,
	C::Api: EthereumRuntimeRPCApi<B>,
	C: HeaderBackend<B> + 'static,
{
	fn version(&self) -> RpcResult<String> {
		let hash = self.client.info().best_hash;
		Ok(self
			.client
			.runtime_api()
			.chain_id(hash)
			.map_err(|_| internal_err("fetch runtime chain id failed"))?
			.to_string())
	}

	fn peer_count(&self) -> RpcResult<PeerCount> {
		let peer_count = self.network.sync_num_connected();
		Ok(match self.peer_count_as_hex {
			true => PeerCount::String(format!("0x{:x}", peer_count)),
			false => PeerCount::U32(peer_count as u32),
		})
	}

	fn is_listening(&self) -> RpcResult<bool> {
		Ok(true)
	}
}
