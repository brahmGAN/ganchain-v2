// SPDX-License-Identifier: Apache-2.0
// This file is part of Frontier.
//
// Copyright (c) 2017-2020 Parity Technologies (UK) Ltd.
//
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

use frame_support::{
	dispatch::{DispatchInfo, GetDispatchInfo},
	traits::ExtrinsicCall,
};
use scale_codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{
		self, Checkable, Extrinsic, ExtrinsicMetadata, IdentifyAccount, MaybeDisplay, Member,
		SignedExtension,
	},
	transaction_validity::{InvalidTransaction, TransactionValidityError},
	OpaqueExtrinsic, RuntimeDebug,
};

use crate::{CheckedExtrinsic, CheckedSignature, SelfContainedCall};

/// A extrinsic right from the external world. This is unchecked and so
/// can contain a signature.
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
pub struct UncheckedExtrinsic<Address, Call, Signature, Extra: SignedExtension>(
	pub sp_runtime::generic::UncheckedExtrinsic<Address, Call, Signature, Extra>,
);

impl<Address, Call, Signature, Extra: SignedExtension>
	UncheckedExtrinsic<Address, Call, Signature, Extra>
{
	/// New instance of a signed extrinsic aka "transaction".
	pub fn new_signed(function: Call, signed: Address, signature: Signature, extra: Extra) -> Self {
		Self(sp_runtime::generic::UncheckedExtrinsic::new_signed(
			function, signed, signature, extra,
		))
	}

	/// New instance of an unsigned extrinsic aka "inherent".
	pub fn new_unsigned(function: Call) -> Self {
		Self(sp_runtime::generic::UncheckedExtrinsic::new_unsigned(
			function,
		))
	}
}

impl<Address, Call, Signature, Extra> Extrinsic
	for UncheckedExtrinsic<Address, Call, Signature, Extra>
where
	Address: TypeInfo,
	Call: SelfContainedCall + TypeInfo,
	Signature: TypeInfo,
	Extra: SignedExtension,
{
	type Call = Call;

	type SignaturePayload = (Address, Signature, Extra);

	fn is_signed(&self) -> Option<bool> {
		if self.0.function.is_self_contained() {
			Some(true)
		} else {
			self.0.is_signed()
		}
	}

	fn new(function: Call, signed_data: Option<Self::SignaturePayload>) -> Option<Self> {
		sp_runtime::generic::UncheckedExtrinsic::new(function, signed_data).map(Self)
	}
}

impl<Address, AccountId, Call, Signature, Extra, Lookup> Checkable<Lookup>
	for UncheckedExtrinsic<Address, Call, Signature, Extra>
where
	Address: Member + MaybeDisplay,
	Call: Encode + Member + SelfContainedCall,
	Signature: Member + traits::Verify,
	<Signature as traits::Verify>::Signer: IdentifyAccount<AccountId = AccountId>,
	Extra: SignedExtension<AccountId = AccountId>,
	AccountId: Member + MaybeDisplay,
	Lookup: traits::Lookup<Source = Address, Target = AccountId>,
{
	type Checked =
		CheckedExtrinsic<AccountId, Call, Extra, <Call as SelfContainedCall>::SignedInfo>;

	fn check(self, lookup: &Lookup) -> Result<Self::Checked, TransactionValidityError> {
		if self.0.function.is_self_contained() {
			if self.0.signature.is_some() {
				return Err(TransactionValidityError::Invalid(
					InvalidTransaction::BadProof,
				));
			}

			let signed_info = self.0.function.check_self_contained().ok_or(
				TransactionValidityError::Invalid(InvalidTransaction::BadProof),
			)??;
			Ok(CheckedExtrinsic {
				signed: CheckedSignature::SelfContained(signed_info),
				function: self.0.function,
			})
		} else {
			let checked = Checkable::<Lookup>::check(self.0, lookup)?;
			Ok(CheckedExtrinsic {
				signed: match checked.signed {
					Some((id, extra)) => CheckedSignature::Signed(id, extra),
					None => CheckedSignature::Unsigned,
				},
				function: checked.function,
			})
		}
	}

	#[cfg(feature = "try-runtime")]
	fn unchecked_into_checked_i_know_what_i_am_doing(
		self,
		lookup: &Lookup,
	) -> Result<Self::Checked, TransactionValidityError> {
		if self.0.function.is_self_contained() {
			match self.0.function.check_self_contained() {
				Some(signed_info) => Ok(CheckedExtrinsic {
					signed: match signed_info {
						Ok(info) => CheckedSignature::SelfContained(info),
						_ => CheckedSignature::Unsigned,
					},
					function: self.0.function,
				}),
				None => Ok(CheckedExtrinsic {
					signed: CheckedSignature::Unsigned,
					function: self.0.function,
				}),
			}
		} else {
			let checked =
				Checkable::<Lookup>::unchecked_into_checked_i_know_what_i_am_doing(self.0, lookup)?;
			Ok(CheckedExtrinsic {
				signed: match checked.signed {
					Some((id, extra)) => CheckedSignature::Signed(id, extra),
					None => CheckedSignature::Unsigned,
				},
				function: checked.function,
			})
		}
	}
}

impl<Address, Call, Signature, Extra> ExtrinsicMetadata
	for UncheckedExtrinsic<Address, Call, Signature, Extra>
where
	Extra: SignedExtension,
{
	const VERSION: u8 = <sp_runtime::generic::UncheckedExtrinsic<Address, Call, Signature, Extra> as ExtrinsicMetadata>::VERSION;
	type SignedExtensions = Extra;
}

impl<Address, Call, Signature, Extra> ExtrinsicCall
	for UncheckedExtrinsic<Address, Call, Signature, Extra>
where
	Address: TypeInfo,
	Call: SelfContainedCall + TypeInfo,
	Signature: TypeInfo,
	Extra: SignedExtension,
{
	fn call(&self) -> &Self::Call {
		&self.0.function
	}
}

impl<Address, Call: GetDispatchInfo, Signature, Extra> GetDispatchInfo
	for UncheckedExtrinsic<Address, Call, Signature, Extra>
where
	Extra: SignedExtension,
{
	fn get_dispatch_info(&self) -> DispatchInfo {
		self.0.function.get_dispatch_info()
	}
}

#[cfg(feature = "serde")]
impl<Address: Encode, Signature: Encode, Call: Encode, Extra: SignedExtension> serde::Serialize
	for UncheckedExtrinsic<Address, Call, Signature, Extra>
{
	fn serialize<S>(&self, seq: S) -> Result<S::Ok, S::Error>
	where
		S: ::serde::Serializer,
	{
		self.0.serialize(seq)
	}
}

#[cfg(feature = "serde")]
impl<'a, Address: Decode, Signature: Decode, Call: Decode, Extra: SignedExtension>
	serde::Deserialize<'a> for UncheckedExtrinsic<Address, Call, Signature, Extra>
{
	fn deserialize<D>(de: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'a>,
	{
		<sp_runtime::generic::UncheckedExtrinsic<Address, Call, Signature, Extra>>::deserialize(de)
			.map(Self)
	}
}

impl<Address, Call, Signature, Extra> From<UncheckedExtrinsic<Address, Call, Signature, Extra>>
	for OpaqueExtrinsic
where
	Address: Encode,
	Signature: Encode,
	Call: Encode,
	Extra: SignedExtension,
{
	fn from(extrinsic: UncheckedExtrinsic<Address, Call, Signature, Extra>) -> Self {
		extrinsic.0.into()
	}
}
