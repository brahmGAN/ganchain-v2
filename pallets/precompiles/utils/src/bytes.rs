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

use super::*;
use alloc::borrow::ToOwned;
pub use alloc::string::String;
use sp_core::{ConstU32, Get};

type ConstU32Max = ConstU32<{ u32::MAX }>;

pub type UnboundedBytes = BoundedBytesString<BytesKind, ConstU32Max>;
pub type BoundedBytes<S> = BoundedBytesString<BytesKind, S>;

pub type UnboundedString = BoundedBytesString<StringKind, ConstU32Max>;
pub type BoundedString<S> = BoundedBytesString<StringKind, S>;

trait Kind {
	fn signature() -> String;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BytesKind;

impl Kind for BytesKind {
	fn signature() -> String {
		String::from("bytes")
	}
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StringKind;

impl Kind for StringKind {
	fn signature() -> String {
		String::from("string")
	}
}

/// The `bytes/string` type of Solidity.
/// It is different from `Vec<u8>` which will be serialized with padding for each `u8` element
/// of the array, while `Bytes` is tightly packed.
#[derive(Debug)]
pub struct BoundedBytesString<K, S> {
	data: Vec<u8>,
	_phantom: PhantomData<(K, S)>,
}

impl<K: Kind, S: Get<u32>> Clone for BoundedBytesString<K, S> {
	fn clone(&self) -> Self {
		Self { data: self.data.clone(), _phantom: PhantomData }
	}
}

impl<K1, S1, K2, S2> PartialEq<BoundedBytesString<K2, S2>> for BoundedBytesString<K1, S1> {
	fn eq(&self, other: &BoundedBytesString<K2, S2>) -> bool {
		self.data.eq(&other.data)
	}
}

impl<K, S> Eq for BoundedBytesString<K, S> {}

impl<K, S: Get<u32>> BoundedBytesString<K, S> {
	pub fn as_bytes(&self) -> &[u8] {
		&self.data
	}

	pub fn as_str(&self) -> Result<&str, sp_std::str::Utf8Error> {
		sp_std::str::from_utf8(&self.data)
	}
}

impl<K: Kind, S: Get<u32>> EvmData for BoundedBytesString<K, S> {
	fn read(reader: &mut EvmDataReader) -> EvmResult<Self> {
		let mut inner_reader = reader.read_pointer()?;

		// Read bytes/string size.
		let array_size: usize = inner_reader
			.read::<U256>()
			.map_err(|_| revert("length, out of bounds"))?
			.try_into()
			.map_err(|_| revert("length, value too large"))?;

		if array_size > S::get() as usize {
			return Err(revert("length, value too large").into())
		}

		let data = inner_reader.read_raw_bytes(array_size)?;

		let bytes = Self { data: data.to_owned(), _phantom: PhantomData };

		Ok(bytes)
	}

	fn write(writer: &mut EvmDataWriter, value: Self) {
		let value: Vec<_> = value.into();
		let length = value.len();

		// Pad the data.
		// Leave it as is if a multiple of 32, otherwise pad to next
		// multiple or 32.
		let chunks = length / 32;
		let padded_size = match length % 32 {
			0 => chunks * 32,
			_ => (chunks + 1) * 32,
		};

		let mut value = value.to_vec();
		value.resize(padded_size, 0);

		writer.write_pointer(EvmDataWriter::new().write(U256::from(length)).write(value).build());
	}

	fn has_static_size() -> bool {
		false
	}
}

// BytesString <=> Vec/&[u8]

impl<K, S> From<BoundedBytesString<K, S>> for Vec<u8> {
	fn from(value: BoundedBytesString<K, S>) -> Self {
		value.data
	}
}

impl<K, S> From<Vec<u8>> for BoundedBytesString<K, S> {
	fn from(value: Vec<u8>) -> Self {
		Self { data: value, _phantom: PhantomData }
	}
}

impl<K, S> From<&[u8]> for BoundedBytesString<K, S> {
	fn from(value: &[u8]) -> Self {
		Self { data: value.to_vec(), _phantom: PhantomData }
	}
}

impl<K, S, const N: usize> From<[u8; N]> for BoundedBytesString<K, S> {
	fn from(value: [u8; N]) -> Self {
		Self { data: value.to_vec(), _phantom: PhantomData }
	}
}

impl<K, S, const N: usize> From<&[u8; N]> for BoundedBytesString<K, S> {
	fn from(value: &[u8; N]) -> Self {
		Self { data: value.to_vec(), _phantom: PhantomData }
	}
}

// BytesString <=> String/str

impl<K, S> TryFrom<BoundedBytesString<K, S>> for String {
	type Error = alloc::string::FromUtf8Error;

	fn try_from(value: BoundedBytesString<K, S>) -> Result<Self, Self::Error> {
		alloc::string::String::from_utf8(value.data)
	}
}

impl<K, S> From<&str> for BoundedBytesString<K, S> {
	fn from(value: &str) -> Self {
		Self { data: value.as_bytes().into(), _phantom: PhantomData }
	}
}

impl<K, S> From<String> for BoundedBytesString<K, S> {
	fn from(value: String) -> Self {
		Self { data: value.as_bytes().into(), _phantom: PhantomData }
	}
}
