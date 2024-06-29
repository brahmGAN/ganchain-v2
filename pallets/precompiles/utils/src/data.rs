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

use crate::{revert, EvmResult};

use alloc::borrow::ToOwned;
use core::{any::type_name, marker::PhantomData, ops::Range};
use impl_trait_for_tuples::impl_for_tuples;
use sp_core::{Get, H160, H256, U256};
use sp_std::{convert::TryInto, vec, vec::Vec};

/// The `address` type of Solidity.
/// H160 could represent 2 types of data (bytes20 and address) that are not encoded the same way.
/// To avoid issues writing H160 is thus not supported.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Address(pub H160);

impl From<H160> for Address {
	fn from(a: H160) -> Address {
		Address(a)
	}
}

impl From<Address> for H160 {
	fn from(a: Address) -> H160 {
		a.0
	}
}

/// The `bytes`/`string` type of Solidity.
/// It is different from `Vec<u8>` which will be serialized with padding for each `u8` element
/// of the array, while `Bytes` is tightly packed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Bytes(pub Vec<u8>);

impl Bytes {
	/// Interpret as `bytes`.
	pub fn as_bytes(&self) -> &[u8] {
		&self.0
	}

	/// Interpret as `string`.
	/// Can fail if the string is not valid UTF8.
	pub fn as_str(&self) -> Result<&str, sp_std::str::Utf8Error> {
		sp_std::str::from_utf8(&self.0)
	}
}

impl From<&[u8]> for Bytes {
	fn from(a: &[u8]) -> Self {
		Self(a.to_owned())
	}
}

impl From<&str> for Bytes {
	fn from(a: &str) -> Self {
		a.as_bytes().into()
	}
}

impl From<Bytes> for Vec<u8> {
	fn from(b: Bytes) -> Vec<u8> {
		b.0
	}
}

/// Wrapper around an EVM input slice, helping to parse it.
/// Provide functions to parse common types.
#[derive(Clone, Copy, Debug)]
pub struct EvmDataReader<'a> {
	input: &'a [u8],
	cursor: usize,
}

impl<'a> EvmDataReader<'a> {
	/// Create a new input parser.
	pub fn new(input: &'a [u8]) -> Self {
		Self { input, cursor: 0 }
	}

	/// Create a new input parser from a selector-initial input.
	pub fn read_selector<T>(input: &'a [u8]) -> EvmResult<T>
	where
		T: num_enum::TryFromPrimitive<Primitive = u32>,
	{
		if input.len() < 4 {
			return Err(revert("tried to parse selector out of bounds"))
		}

		let mut buffer = [0u8; 4];
		buffer.copy_from_slice(&input[0..4]);
		let selector = T::try_from_primitive(u32::from_be_bytes(buffer)).map_err(|_| {
			log::trace!(
				target: "precompile-utils",
				"Failed to match function selector for {}",
				type_name::<T>()
			);
			revert("unknown selector")
		})?;

		Ok(selector)
	}

	/// Create a new input parser from a selector-initial input.
	pub fn new_skip_selector(input: &'a [u8]) -> EvmResult<Self> {
		if input.len() < 4 {
			return Err(revert("input is too short"))
		}

		Ok(Self::new(&input[4..]))
	}

	/// Check the input has at least the correct amount of arguments before the end (32 bytes values).
	pub fn expect_arguments(&self, args: usize) -> EvmResult {
		if self.input.len() >= self.cursor + args * 32 {
			Ok(())
		} else {
			Err(revert("input doesn't match expected length"))
		}
	}

	/// Read data from the input.
	pub fn read<T: EvmData>(&mut self) -> EvmResult<T> {
		T::read(self)
	}

	/// Read raw bytes from the input.
	/// Doesn't handle any alignment checks, prefer using `read` instead of possible.
	/// Returns an error if trying to parse out of bounds.
	pub fn read_raw_bytes(&mut self, len: usize) -> EvmResult<&[u8]> {
		let range = self.move_cursor(len)?;

		let data = self
			.input
			.get(range)
			.ok_or_else(|| revert("tried to parse raw bytes out of bounds"))?;

		Ok(data)
	}

	/// Reads a pointer, returning a reader targetting the pointed location.
	pub fn read_pointer(&mut self) -> EvmResult<Self> {
		let offset: usize = self
			.read::<U256>()
			.map_err(|_| revert("tried to parse array offset out of bounds"))?
			.try_into()
			.map_err(|_| revert("array offset is too large"))?;

		if offset >= self.input.len() {
			return Err(revert("pointer points out of bounds"))
		}

		Ok(Self { input: &self.input[offset..], cursor: 0 })
	}

	/// Read remaining bytes
	pub fn read_till_end(&mut self) -> EvmResult<&[u8]> {
		let range = self.move_cursor(self.input.len() - self.cursor)?;

		let data = self
			.input
			.get(range)
			.ok_or_else(|| revert("tried to parse raw bytes out of bounds"))?;

		Ok(data)
	}

	/// Move the reading cursor with provided length, and return a range from the previous cursor
	/// location to the new one.
	/// Checks cursor overflows.
	fn move_cursor(&mut self, len: usize) -> EvmResult<Range<usize>> {
		let start = self.cursor;
		let end = self
			.cursor
			.checked_add(len)
			.ok_or_else(|| revert("data reading cursor overflow"))?;

		self.cursor = end;

		Ok(start..end)
	}
}

/// Help build an EVM input/output data.
///
/// Functions takes `self` to allow chaining all calls like
/// `EvmDataWriter::new().write(...).write(...).build()`.
/// While it could be more ergonomic to take &mut self, this would
/// prevent to have a `build` function that don't clone the output.
#[derive(Clone, Debug)]
pub struct EvmDataWriter {
	pub(crate) data: Vec<u8>,
	offset_data: Vec<OffsetDatum>,
	selector: Option<u32>,
}

#[derive(Clone, Debug)]
struct OffsetDatum {
	// Offset location in the container data.
	offset_position: usize,
	// Data pointed by the offset that must be inserted at the end of container data.
	data: Vec<u8>,
	// Inside of arrays, the offset is not from the start of array data (length), but from the start
	// of the item. This shift allow to correct this.
	offset_shift: usize,
}

impl EvmDataWriter {
	/// Creates a new empty output builder (without selector).
	pub fn new() -> Self {
		Self { data: vec![], offset_data: vec![], selector: None }
	}

	/// Creates a new empty output builder with provided selector.
	/// Selector will only be appended before the data when calling
	/// `build` to not mess with the offsets.
	pub fn new_with_selector(selector: impl Into<u32>) -> Self {
		Self { data: vec![], offset_data: vec![], selector: Some(selector.into()) }
	}

	/// Return the built data.
	pub fn build(mut self) -> Vec<u8> {
		Self::bake_offsets(&mut self.data, self.offset_data);

		if let Some(selector) = self.selector {
			let mut output = selector.to_be_bytes().to_vec();
			output.append(&mut self.data);
			output
		} else {
			self.data
		}
	}

	/// Add offseted data at the end of this writer's data, updating the offsets.
	fn bake_offsets(output: &mut Vec<u8>, offsets: Vec<OffsetDatum>) {
		for mut offset_datum in offsets {
			let offset_position = offset_datum.offset_position;
			let offset_position_end = offset_position + 32;

			// The offset is the distance between the start of the data and the
			// start of the pointed data (start of a struct, length of an array).
			// Offsets in inner data are relative to the start of their respective "container".
			// However in arrays the "container" is actually the item itself instead of the whole
			// array, which is corrected by `offset_shift`.
			let free_space_offset = output.len() - offset_datum.offset_shift;

			// Override dummy offset to the offset it will be in the final output.
			U256::from(free_space_offset)
				.to_big_endian(&mut output[offset_position..offset_position_end]);

			// Append this data at the end of the current output.
			output.append(&mut offset_datum.data);
		}
	}

	/// Write arbitrary bytes.
	/// Doesn't handle any alignement checks, prefer using `write` instead if possible.
	fn write_raw_bytes(mut self, value: &[u8]) -> Self {
		self.data.extend_from_slice(value);
		self
	}

	/// Write data of requested type.
	pub fn write<T: EvmData>(mut self, value: T) -> Self {
		T::write(&mut self, value);
		self
	}

	/// Writes a pointer to given data.
	/// The data will be appended when calling `build`.
	/// Initially write a dummy value as offset in this writer's data, which will be replaced by
	/// the correct offset once the pointed data is appended.
	///
	/// Takes `&mut self` since its goal is to be used inside `EvmData` impl and not in chains.
	pub fn write_pointer(&mut self, data: Vec<u8>) {
		let offset_position = self.data.len();
		H256::write(self, H256::repeat_byte(0xff));

		self.offset_data.push(OffsetDatum { offset_position, data, offset_shift: 0 });
	}
}

impl Default for EvmDataWriter {
	fn default() -> Self {
		Self::new()
	}
}

/// Data that can be converted from and to EVM data types.
pub trait EvmData: Sized {
	fn read(reader: &mut EvmDataReader) -> EvmResult<Self>;
	fn write(writer: &mut EvmDataWriter, value: Self);
	fn has_static_size() -> bool;
	fn is_explicit_tuple() -> bool {
		false
	}
}
/// Encode the value into its Solidity ABI format.
/// If `T` is a tuple it is encoded as a Solidity tuple with dynamic-size offset.
fn encode<T: EvmData>(value: T) -> Vec<u8> {
	EvmDataWriter::new().write(value).build()
}

/// Encode the value into its Solidity ABI format.
/// If `T` is a tuple every element is encoded without a prefixed offset.
/// It matches the encoding of Solidity function arguments and return value, or event data.
pub fn encode_arguments<T: EvmData>(value: T) -> Vec<u8> {
	let output = encode(value);
	if T::is_explicit_tuple() && !T::has_static_size() {
		output[32..].to_vec()
	} else {
		output
	}
}

pub use self::{encode_arguments as encode_return_value, encode_arguments as encode_event_data};

#[impl_for_tuples(1, 18)]
impl EvmData for Tuple {
	fn has_static_size() -> bool {
		for_tuples!(#( Tuple::has_static_size() )&*)
	}

	fn read(reader: &mut EvmDataReader) -> EvmResult<Self> {
		if !Self::has_static_size() {
			let reader = &mut reader.read_pointer()?;
			Ok(for_tuples!( ( #( reader.read::<Tuple>()? ),* ) ))
		} else {
			Ok(for_tuples!( ( #( reader.read::<Tuple>()? ),* ) ))
		}
	}

	fn write(writer: &mut EvmDataWriter, value: Self) {
		if !Self::has_static_size() {
			let mut inner_writer = EvmDataWriter::new();
			for_tuples!( #( Tuple::write(&mut inner_writer, value.Tuple); )* );
			writer.write_pointer(inner_writer.build());
		} else {
			for_tuples!( #( Tuple::write(writer, value.Tuple); )* );
		}
	}
}

impl EvmData for H256 {
	fn read(reader: &mut EvmDataReader) -> EvmResult<Self> {
		let range = reader.move_cursor(32)?;

		let data = reader
			.input
			.get(range)
			.ok_or_else(|| revert("tried to parse H256 out of bounds"))?;

		Ok(H256::from_slice(data))
	}

	fn write(writer: &mut EvmDataWriter, value: Self) {
		writer.data.extend_from_slice(value.as_bytes());
	}

	fn has_static_size() -> bool {
		true
	}
}

impl EvmData for Address {
	fn read(reader: &mut EvmDataReader) -> EvmResult<Self> {
		let range = reader.move_cursor(32)?;

		let data = reader
			.input
			.get(range)
			.ok_or_else(|| revert("tried to parse H160 out of bounds"))?;

		Ok(H160::from_slice(&data[12..32]).into())
	}

	fn write(writer: &mut EvmDataWriter, value: Self) {
		H256::write(writer, value.0.into());
	}

	fn has_static_size() -> bool {
		true
	}
}

impl EvmData for U256 {
	fn read(reader: &mut EvmDataReader) -> EvmResult<Self> {
		let range = reader.move_cursor(32)?;

		let data = reader
			.input
			.get(range)
			.ok_or_else(|| revert("tried to parse U256 out of bounds"))?;

		Ok(U256::from_big_endian(data))
	}

	fn write(writer: &mut EvmDataWriter, value: Self) {
		let mut buffer = [0u8; 32];
		value.to_big_endian(&mut buffer);
		writer.data.extend_from_slice(&buffer);
	}

	fn has_static_size() -> bool {
		true
	}
}

macro_rules! impl_evmdata_for_uints {
	($($uint:ty, )*) => {
		$(
			impl EvmData for $uint {
				fn read(reader: &mut EvmDataReader) -> EvmResult<Self> {
					let range = reader.move_cursor(32)?;

					let data = reader
						.input
						.get(range)
						.ok_or_else(|| revert(alloc::format!(
							"tried to parse {} out of bounds", core::any::type_name::<Self>()
						)))?;

					let mut buffer = [0u8; core::mem::size_of::<Self>()];
					buffer.copy_from_slice(&data[32 - core::mem::size_of::<Self>()..]);
					Ok(Self::from_be_bytes(buffer))
				}

				fn write(writer: &mut EvmDataWriter, value: Self) {
					let mut buffer = [0u8; 32];
					buffer[32 - core::mem::size_of::<Self>()..].copy_from_slice(&value.to_be_bytes());
					writer.data.extend_from_slice(&buffer);
				}

				fn has_static_size() -> bool {
					true
				}
			}
		)*
	};
}

impl_evmdata_for_uints!(u16, u32, u64, u128,);

// The implementation for u8 is specific, for performance reasons.
impl EvmData for u8 {
	fn read(reader: &mut EvmDataReader) -> EvmResult<Self> {
		let range = reader.move_cursor(32)?;

		let data = reader
			.input
			.get(range)
			.ok_or_else(|| revert("tried to parse u64 out of bounds"))?;

		Ok(data[31])
	}

	fn write(writer: &mut EvmDataWriter, value: Self) {
		let mut buffer = [0u8; 32];
		buffer[31] = value;

		writer.data.extend_from_slice(&buffer);
	}

	fn has_static_size() -> bool {
		true
	}
}

impl EvmData for bool {
	fn read(reader: &mut EvmDataReader) -> EvmResult<Self> {
		let h256 = H256::read(reader).map_err(|_| revert("tried to parse bool out of bounds"))?;

		Ok(!h256.is_zero())
	}

	fn write(writer: &mut EvmDataWriter, value: Self) {
		let mut buffer = [0u8; 32];
		if value {
			buffer[31] = 1;
		}

		writer.data.extend_from_slice(&buffer);
	}

	fn has_static_size() -> bool {
		true
	}
}

impl<T: EvmData> EvmData for Vec<T> {
	fn read(reader: &mut EvmDataReader) -> EvmResult<Self> {
		let mut inner_reader = reader.read_pointer()?;

		let array_size: usize = inner_reader
			.read::<U256>()
			.map_err(|_| revert("tried to parse array length out of bounds"))?
			.try_into()
			.map_err(|_| revert("array length is too large"))?;

		let mut array = vec![];

		let mut item_reader = EvmDataReader {
			input: inner_reader
				.input
				.get(32..)
				.ok_or_else(|| revert("try to read array items out of bound"))?,
			cursor: 0,
		};

		for _ in 0..array_size {
			array.push(item_reader.read()?);
		}

		Ok(array)
	}

	fn write(writer: &mut EvmDataWriter, value: Self) {
		let mut inner_writer = EvmDataWriter::new().write(U256::from(value.len()));

		for inner in value {
			// Any offset in items are relative to the start of the item instead of the
			// start of the array. However if there is offseted data it must but appended after
			// all items (offsets) are written. We thus need to rely on `compute_offsets` to do
			// that, and must store a "shift" to correct the offsets.
			let shift = inner_writer.data.len();
			let item_writer = EvmDataWriter::new().write(inner);

			inner_writer = inner_writer.write_raw_bytes(&item_writer.data);
			for mut offset_datum in item_writer.offset_data {
				offset_datum.offset_shift += 32;
				offset_datum.offset_position += shift;
				inner_writer.offset_data.push(offset_datum);
			}
		}

		writer.write_pointer(inner_writer.build());
	}

	fn has_static_size() -> bool {
		false
	}
}

impl EvmData for Bytes {
	fn read(reader: &mut EvmDataReader) -> EvmResult<Self> {
		let mut inner_reader = reader.read_pointer()?;

		// Read bytes/string size.
		let array_size: usize = inner_reader
			.read::<U256>()
			.map_err(|_| revert("tried to parse bytes/string length out of bounds"))?
			.try_into()
			.map_err(|_| revert("bytes/string length is too large"))?;

		// Get valid range over the bytes data.
		let range = inner_reader.move_cursor(array_size)?;

		let data = inner_reader
			.input
			.get(range)
			.ok_or_else(|| revert("tried to parse bytes/string out of bounds"))?;

		let bytes = Self(data.to_owned());

		Ok(bytes)
	}

	fn write(writer: &mut EvmDataWriter, value: Self) {
		let length = value.0.len();

		// Pad the data.
		// Leave it as is if a multiple of 32, otherwise pad to next
		// multiple or 32.
		let chunks = length / 32;
		let padded_size = match length % 32 {
			0 => chunks * 32,
			_ => (chunks + 1) * 32,
		};

		let mut value = value.0.to_vec();
		value.resize(padded_size, 0);

		writer.write_pointer(
			EvmDataWriter::new().write(U256::from(length)).write_raw_bytes(&value).build(),
		);
	}

	fn has_static_size() -> bool {
		false
	}
}

/// Wrapper around a Vec that provides a max length bound on read.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundedVec<T, S> {
	inner: Vec<T>,
	_phantom: PhantomData<S>,
}

impl<T: EvmData, S: Get<u32>> EvmData for BoundedVec<T, S> {
	fn read(reader: &mut EvmDataReader) -> EvmResult<Self> {
		let mut inner_reader = reader.read_pointer()?;

		let array_size: usize = inner_reader
			.read::<U256>()
			.map_err(|_| revert("out of bounds: length of array"))?
			.try_into()
			.map_err(|_| revert("value too large : Array has more than max items allowed"))?;

		if array_size > S::get() as usize {
			return Err(revert("value too large : Array has more than max items allowed").into())
		}

		let mut array = vec![];

		let mut item_reader = EvmDataReader {
			input: inner_reader
				.input
				.get(32..)
				.ok_or_else(|| revert("read out of bounds: array content"))?,
			cursor: 0,
		};

		for _ in 0..array_size {
			array.push(item_reader.read()?);
		}

		Ok(BoundedVec { inner: array, _phantom: PhantomData })
	}

	fn write(writer: &mut EvmDataWriter, value: Self) {
		let value: Vec<_> = value.into();
		let mut inner_writer = EvmDataWriter::new().write(U256::from(value.len()));

		for inner in value {
			// Any offset in items are relative to the start of the item instead of the
			// start of the array. However if there is offseted data it must but appended after
			// all items (offsets) are written. We thus need to rely on `compute_offsets` to do
			// that, and must store a "shift" to correct the offsets.
			let shift = inner_writer.data.len();
			let item_writer = EvmDataWriter::new().write(inner);

			inner_writer = inner_writer.write_raw_bytes(&item_writer.data);
			for mut offset_datum in item_writer.offset_data {
				offset_datum.offset_shift += 32;
				offset_datum.offset_position += shift;
				inner_writer.offset_data.push(offset_datum);
			}
		}

		writer.write_pointer(inner_writer.build());
	}

	fn has_static_size() -> bool {
		false
	}
}

impl<T, S> From<Vec<T>> for BoundedVec<T, S> {
	fn from(value: Vec<T>) -> Self {
		BoundedVec { inner: value, _phantom: PhantomData }
	}
}

impl<T: Clone, S> From<&[T]> for BoundedVec<T, S> {
	fn from(value: &[T]) -> Self {
		BoundedVec { inner: value.to_vec(), _phantom: PhantomData }
	}
}

impl<T: Clone, S, const N: usize> From<[T; N]> for BoundedVec<T, S> {
	fn from(value: [T; N]) -> Self {
		BoundedVec { inner: value.to_vec(), _phantom: PhantomData }
	}
}

impl<T, S> From<BoundedVec<T, S>> for Vec<T> {
	fn from(value: BoundedVec<T, S>) -> Self {
		value.inner
	}
}
