// This file is part of Astar.

// Copyright (C) 2019-2023 Stake Technologies Pte.Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later

// Astar is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Astar is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Astar. If not, see <http://www.gnu.org/licenses/>.

#![cfg_attr(not(feature = "std"), no_std)]

use fp_evm::{PrecompileHandle, PrecompileOutput};
use pallet_evm::Precompile;
use sp_core::ecdsa;
use sp_std::{marker::PhantomData, prelude::*};

use precompile_utils::{
	succeed, Bytes, EvmDataWriter, EvmResult, FunctionModifier, PrecompileHandleExt,
};

// #[cfg(test)]
// mod mock;
// #[cfg(test)]
// mod tests;

#[precompile_utils::generate_function_selector]
#[derive(Debug, PartialEq)]
pub enum Action {
	Verify = "verify(bytes,bytes,bytes)",
}

/// A precompile to wrap substrate ecdsa functions.
pub struct SubstrateEcdsaPrecompile<Runtime>(PhantomData<Runtime>);

impl<Runtime: pallet_evm::Config> Precompile for SubstrateEcdsaPrecompile<Runtime> {
	fn execute(handle: &mut impl PrecompileHandle) -> EvmResult<PrecompileOutput> {
		log::trace!(target: "substrate-ecdsa-precompile", "In SubstrateEcdsa precompile");

		let selector = handle.read_selector()?;

		handle.check_function_modifier(FunctionModifier::View)?;

		match selector {
			// Dispatchables
			Action::Verify => Self::verify(handle),
		}
	}
}

impl<Runtime: pallet_evm::Config> SubstrateEcdsaPrecompile<Runtime> {
	fn verify(handle: &mut impl PrecompileHandle) -> EvmResult<PrecompileOutput> {
		let mut input = handle.read_input()?;
		input.expect_arguments(3)?;

		// Parse arguments
		let public_bytes: Vec<u8> = input.read::<Bytes>()?.into();
		let signature_bytes: Vec<u8> = input.read::<Bytes>()?.into();
		let message: Vec<u8> = input.read::<Bytes>()?.into();

		// Parse public key
		let public = if let Ok(public) = ecdsa::Public::try_from(&public_bytes[..]) {
			public
		} else {
			// Return `false` if public key length is wrong
			return Ok(succeed(EvmDataWriter::new().write(false).build()))
		};

		// Parse signature
		let signature_opt = ecdsa::Signature::from_slice(&signature_bytes[..]);

		let signature = if let Some(sig) = signature_opt {
			sig
		} else {
			// Return `false` if signature length is wrong
			return Ok(succeed(EvmDataWriter::new().write(false).build()))
		};

		log::trace!(
			target: "substrate-ecdsa-precompile",
			"Verify signature {:?} for public {:?} and message {:?}",
			signature, public, message,
		);

		let is_confirmed = sp_io::crypto::ecdsa_verify(&signature, &message[..], &public);

		log::trace!(
			target: "substrate-ecdsa-precompile",
			"Verified signature {:?} is {:?}",
			signature, is_confirmed,
		);

		Ok(succeed(EvmDataWriter::new().write(is_confirmed).build()))
	}
}
