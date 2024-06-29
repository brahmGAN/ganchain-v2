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
#![allow(clippy::too_many_arguments)]

pub mod kv;
#[cfg(feature = "sql")]
pub mod sql;

use sp_api::BlockT;

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum SyncStrategy {
	Normal,
	Parachain,
}

pub type EthereumBlockNotificationSinks<T> =
	parking_lot::Mutex<Vec<sc_utils::mpsc::TracingUnboundedSender<T>>>;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct EthereumBlockNotification<Block: BlockT> {
	pub is_new_best: bool,
	pub hash: Block::Hash,
}
