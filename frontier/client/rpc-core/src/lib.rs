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

#![deny(unused_crate_dependencies)]

pub mod types;

mod eth;
mod eth_pubsub;
mod net;
#[cfg(feature = "txpool")]
mod txpool;
mod web3;

#[cfg(feature = "txpool")]
pub use self::txpool::TxPoolApiServer;
pub use self::{
	eth::{EthApiServer, EthFilterApiServer},
	eth_pubsub::EthPubSubApiServer,
	net::NetApiServer,
	web3::Web3ApiServer,
};
