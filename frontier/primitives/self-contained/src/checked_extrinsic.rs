// SPDX-License-Identifier: Apache-2.0
// This file is part of Frontier.
//
// Copyright (c) 2020-2022 Parity Technologies (UK) Ltd.
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

use frame_support::dispatch::{DispatchInfo, GetDispatchInfo};
use sp_runtime::{
	traits::{
		self, DispatchInfoOf, Dispatchable, MaybeDisplay, Member, PostDispatchInfoOf,
		SignedExtension, ValidateUnsigned,
	},
	transaction_validity::{
		InvalidTransaction, TransactionSource, TransactionValidity, TransactionValidityError,
	},
	RuntimeDebug,
};

use crate::SelfContainedCall;

#[derive(Clone, Eq, PartialEq, RuntimeDebug)]
pub enum CheckedSignature<AccountId, Extra, SelfContainedSignedInfo> {
	Signed(AccountId, Extra),
	Unsigned,
	SelfContained(SelfContainedSignedInfo),
}

/// Definition of something that the external world might want to say; its
/// existence implies that it has been checked and is good, particularly with
/// regards to the signature.
#[derive(Clone, Eq, PartialEq, RuntimeDebug)]
pub struct CheckedExtrinsic<AccountId, Call, Extra, SelfContainedSignedInfo> {
	/// Who this purports to be from and the number of extrinsics have come before
	/// from the same signer, if anyone (note this is not a signature).
	pub signed: CheckedSignature<AccountId, Extra, SelfContainedSignedInfo>,

	/// The function that should be called.
	pub function: Call,
}

impl<AccountId, Call: GetDispatchInfo, Extra, SelfContainedSignedInfo> GetDispatchInfo
	for CheckedExtrinsic<AccountId, Call, Extra, SelfContainedSignedInfo>
{
	fn get_dispatch_info(&self) -> DispatchInfo {
		self.function.get_dispatch_info()
	}
}

impl<AccountId, Call, Extra, SelfContainedSignedInfo, Origin> traits::Applyable
	for CheckedExtrinsic<AccountId, Call, Extra, SelfContainedSignedInfo>
where
	AccountId: Member + MaybeDisplay,
	Call: Member
		+ Dispatchable<RuntimeOrigin = Origin>
		+ SelfContainedCall<SignedInfo = SelfContainedSignedInfo>,
	Extra: SignedExtension<AccountId = AccountId, Call = Call>,
	Origin: From<Option<AccountId>>,
	SelfContainedSignedInfo: Send + Sync + 'static,
{
	type Call = Call;

	fn validate<U: ValidateUnsigned<Call = Self::Call>>(
		&self,
		// TODO [#5006;ToDr] should source be passed to `SignedExtension`s?
		// Perhaps a change for 2.0 to avoid breaking too much APIs?
		source: TransactionSource,
		info: &DispatchInfoOf<Self::Call>,
		len: usize,
	) -> TransactionValidity {
		match &self.signed {
			CheckedSignature::Signed(id, extra) => {
				Extra::validate(extra, id, &self.function, info, len)
			}
			CheckedSignature::Unsigned => {
				let valid = Extra::validate_unsigned(&self.function, info, len)?;
				let unsigned_validation = U::validate_unsigned(source, &self.function)?;
				Ok(valid.combine_with(unsigned_validation))
			}
			CheckedSignature::SelfContained(signed_info) => self
				.function
				.validate_self_contained(signed_info, info, len)
				.ok_or(TransactionValidityError::Invalid(
					InvalidTransaction::BadProof,
				))?,
		}
	}

	fn apply<U: ValidateUnsigned<Call = Self::Call>>(
		self,
		info: &DispatchInfoOf<Self::Call>,
		len: usize,
	) -> sp_runtime::ApplyExtrinsicResultWithInfo<PostDispatchInfoOf<Self::Call>> {
		match self.signed {
			CheckedSignature::Signed(id, extra) => {
				let pre = Extra::pre_dispatch(extra, &id, &self.function, info, len)?;
				let maybe_who = Some(id);
				let res = self.function.dispatch(Origin::from(maybe_who));
				let post_info = match res {
					Ok(info) => info,
					Err(err) => err.post_info,
				};
				Extra::post_dispatch(
					Some(pre),
					info,
					&post_info,
					len,
					&res.map(|_| ()).map_err(|e| e.error),
				)?;
				Ok(res)
			}
			CheckedSignature::Unsigned => {
				Extra::pre_dispatch_unsigned(&self.function, info, len)?;
				U::pre_dispatch(&self.function)?;
				let maybe_who = None;
				let res = self.function.dispatch(Origin::from(maybe_who));
				let post_info = match res {
					Ok(info) => info,
					Err(err) => err.post_info,
				};
				Extra::post_dispatch(
					None,
					info,
					&post_info,
					len,
					&res.map(|_| ()).map_err(|e| e.error),
				)?;
				Ok(res)
			}
			CheckedSignature::SelfContained(signed_info) => {
				// If pre-dispatch fail, the block must be considered invalid
				self.function
					.pre_dispatch_self_contained(&signed_info, info, len)
					.ok_or(TransactionValidityError::Invalid(
						InvalidTransaction::BadProof,
					))??;
				let res = self.function.apply_self_contained(signed_info).ok_or(
					TransactionValidityError::Invalid(InvalidTransaction::BadProof),
				)?;
				let post_info = match res {
					Ok(info) => info,
					Err(err) => err.post_info,
				};
				Extra::post_dispatch(
					None,
					info,
					&post_info,
					len,
					&res.map(|_| ()).map_err(|e| e.error),
				)?;
				Ok(res)
			}
		}
	}
}
