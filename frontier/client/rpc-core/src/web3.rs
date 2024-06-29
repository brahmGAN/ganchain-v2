// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
// This file is part of Frontier.
//
// Copyright (c) 2015-2022 Parity Technologies (UK) Ltd.
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

//! Web3 rpc interface.

use ethereum_types::H256;
use jsonrpsee::{core::RpcResult, proc_macros::rpc};

use crate::types::Bytes;

/// Web3 rpc interface.
#[rpc(server)]
pub trait Web3Api {
	/// Returns current client version.
	#[method(name = "web3_clientVersion")]
	fn client_version(&self) -> RpcResult<String>;

	/// Returns sha3 of the given data
	#[method(name = "web3_sha3")]
	fn sha3(&self, input: Bytes) -> RpcResult<H256>;
}
