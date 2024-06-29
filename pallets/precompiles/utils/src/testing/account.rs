// This file is part of Astar.

// Copyright 2019-2022 PureStake Inc.
// Copyright (C) 2022-2023 Stake Technologies Pte.Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Utils package, originally developed by Purestake Inc.
// Utils package used in Astar Network in terms of GPLv3.
//
// Utils is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Utils is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Utils.  If not, see <http://www.gnu.org/licenses/>.
use pallet_evm::AddressMapping;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_core::{Decode, Encode, MaxEncodedLen, H160, H256};

#[derive(
	Eq,
	PartialEq,
	Ord,
	PartialOrd,
	Clone,
	Encode,
	Decode,
	Debug,
	MaxEncodedLen,
	TypeInfo,
	Serialize,
	Deserialize,
	derive_more::Display,
)]
pub struct MockAccount(pub H160);

impl MockAccount {
	pub fn from_u64(v: u64) -> Self {
		H160::from_low_u64_be(v).into()
	}

	pub fn zero() -> Self {
		H160::zero().into()
	}

	pub fn has_prefix(&self, prefix: &[u8]) -> bool {
		&self.0[0..4] == prefix
	}

	pub fn has_prefix_u32(&self, prefix: u32) -> bool {
		self.0[0..4] == prefix.to_be_bytes()
	}

	pub fn without_prefix(&self) -> u128 {
		u128::from_be_bytes(<[u8; 16]>::try_from(&self.0[4..20]).expect("slice have len 16"))
	}
}

impl From<MockAccount> for H160 {
	fn from(account: MockAccount) -> H160 {
		account.0
	}
}

impl From<MockAccount> for [u8; 20] {
	fn from(account: MockAccount) -> [u8; 20] {
		let x: H160 = account.into();
		x.into()
	}
}

impl From<MockAccount> for H256 {
	fn from(x: MockAccount) -> H256 {
		let x: H160 = x.into();
		x.into()
	}
}

impl From<H160> for MockAccount {
	fn from(address: H160) -> MockAccount {
		MockAccount(address)
	}
}

impl From<[u8; 20]> for MockAccount {
	fn from(address: [u8; 20]) -> MockAccount {
		let x: H160 = address.into();
		MockAccount(x)
	}
}

impl AddressMapping<MockAccount> for MockAccount {
	fn into_account_id(address: H160) -> MockAccount {
		address.into()
	}
}

#[macro_export]
macro_rules! mock_account {
	($name:ident, $convert:expr) => {
		pub struct $name;
		mock_account!(# $name, $convert);
	};
	($name:ident ( $($field:ty),* ), $convert:expr) => {
		pub struct $name($(pub $field),*);
		mock_account!(# $name, $convert);
	};
	(# $name:ident, $convert:expr) => {
		impl From<$name> for MockAccount {
			fn from(value: $name) -> MockAccount {
				$convert(value)
			}
		}

		impl From<$name> for sp_core::H160 {
			fn from(value: $name) -> sp_core::H160 {
				MockAccount::from(value).into()
			}
		}

		impl From<$name> for sp_core::H256 {
			fn from(value: $name) -> sp_core::H256 {
				MockAccount::from(value).into()
			}
		}
	};
}

mock_account!(Zero, |_| MockAccount::zero());
mock_account!(Alice, |_| H160::repeat_byte(0xAA).into());
mock_account!(Bob, |_| H160::repeat_byte(0xBB).into());
mock_account!(Charlie, |_| H160::repeat_byte(0xCC).into());
mock_account!(David, |_| H160::repeat_byte(0xDD).into());

mock_account!(Precompile1, |_| MockAccount::from_u64(1));

mock_account!(CryptoAlith, |_| H160::from(hex_literal::hex!(
	"f24FF3a9CF04c71Dbc94D0b566f7A27B94566cac"
))
.into());
mock_account!(CryptoBaltathar, |_| H160::from(hex_literal::hex!(
	"3Cd0A705a2DC65e5b1E1205896BaA2be8A07c6e0"
))
.into());
mock_account!(CryptoCarleth, |_| H160::from(hex_literal::hex!(
	"798d4Ba9baf0064Ec19eB4F0a1a45785ae9D6DFc"
))
.into());

mock_account!(AddressInPrefixedSet(u32, u128), |value: AddressInPrefixedSet| {
	let prefix: u32 = value.0;
	let index: u128 = value.1;

	let mut buffer = Vec::with_capacity(20); // 160 bits

	buffer.extend_from_slice(&prefix.to_be_bytes());
	buffer.extend_from_slice(&index.to_be_bytes());

	assert_eq!(buffer.len(), 20, "address buffer should have len of 20");

	H160::from_slice(&buffer).into()
});

pub fn alith_secret_key() -> [u8; 32] {
	hex_literal::hex!("5fb92d6e98884f76de468fa3f6278f8807c48bebc13595d45af5bdc4da702133")
}

pub fn baltathar_secret_key() -> [u8; 32] {
	hex_literal::hex!("8075991ce870b93a8870eca0c0f91913d12f47948ca0fd25b49c6fa7cdbeee8b")
}

pub fn charleth_secret_key() -> [u8; 32] {
	hex_literal::hex!("0b6e18cafb6ed99687ec547bd28139cafdd2bffe70e6b688025de6b445aa5c5b")
}
