// Copyright (C) 2019-2023 Stake Technologies Pte.Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Ethereum prefixed signatures compatibility instances.

use parity_scale_codec::{Decode, Encode};
use sp_core::ecdsa;
use sp_io::{crypto::secp256k1_ecdsa_recover_compressed, hashing::keccak_256};
use sp_runtime::{
	traits::{IdentifyAccount, Lazy, Verify},
	MultiSignature,
};
use sp_std::prelude::*;

/// Ethereum-compatible signature type.
#[derive(Encode, Decode, PartialEq, Eq, Clone, scale_info::TypeInfo)]
pub struct EthereumSignature(pub [u8; 65]);

impl sp_std::fmt::Debug for EthereumSignature {
	fn fmt(&self, f: &mut sp_std::fmt::Formatter<'_>) -> sp_std::fmt::Result {
		write!(f, "EthereumSignature({:?})", &self.0[..])
	}
}

impl From<ecdsa::Signature> for EthereumSignature {
	fn from(signature: ecdsa::Signature) -> Self {
		Self(signature.into())
	}
}

impl sp_std::convert::TryFrom<Vec<u8>> for EthereumSignature {
	type Error = ();

	fn try_from(data: Vec<u8>) -> Result<Self, Self::Error> {
		if data.len() == 65 {
			let mut inner = [0u8; 65];
			inner.copy_from_slice(&data[..]);
			Ok(EthereumSignature(inner))
		} else {
			Err(())
		}
	}
}

/// Constructs the message that Ethereum RPC's `personal_sign` and `eth_sign` would sign.
///
/// Note: sign message hash to escape of message length estimation.
pub fn signable_message(what: &[u8]) -> Vec<u8> {
	let hash = keccak_256(what);
	let mut v = b"\x19Ethereum Signed Message:\n32".to_vec();
	v.extend_from_slice(&hash[..]);
	v
}

/// Attempts to recover the Ethereum public key from a message signature signed by using
/// the Ethereum RPC's `personal_sign` and `eth_sign`.
impl Verify for EthereumSignature {
	type Signer = <MultiSignature as Verify>::Signer;

	fn verify<L: Lazy<[u8]>>(
		&self,
		mut msg: L,
		account: &<Self::Signer as IdentifyAccount>::AccountId,
	) -> bool {
		let msg = keccak_256(&signable_message(msg.get()));
		match secp256k1_ecdsa_recover_compressed(&self.0, &msg).ok() {
			Some(public) => {
				let signer = Self::Signer::from(ecdsa::Public::from_raw(public));
				*account == signer.into_account()
			},
			None => false,
		}
	}
}

#[test]
fn verify_should_works() {
	use hex_literal::hex;
	use sp_core::{ecdsa, Pair};

	let msg = "test eth signed message";
	let pair = ecdsa::Pair::from_seed(&hex![
		"ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
	]);
	let account = <MultiSignature as Verify>::Signer::from(pair.public()).into_account();
	let signature = EthereumSignature(hex!["f5d5cc953828e3fb0d81f3176d88fa5c73d3ad3dc4bc7a8061b03a6db2cd73337778df75a1443e8c642f6ceae0db39b90c321ac270ad7836695cae76f703f3031c"]);
	assert_eq!(signature.verify(msg.as_ref(), &account), true);
}
