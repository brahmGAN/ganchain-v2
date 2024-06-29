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

#![cfg_attr(not(feature = "std"), no_std)]
#![deny(unused_crate_dependencies)]

mod checked_extrinsic;
mod unchecked_extrinsic;

pub use crate::{
	checked_extrinsic::{CheckedExtrinsic, CheckedSignature},
	unchecked_extrinsic::UncheckedExtrinsic,
};

use sp_runtime::{
	traits::{DispatchInfoOf, Dispatchable, PostDispatchInfoOf},
	transaction_validity::{TransactionValidity, TransactionValidityError},
};

/// A call that has self-contained functions. A self-contained
/// function is something that has its signature embedded in its call.
pub trait SelfContainedCall: Dispatchable {
	/// Validated signature info.
	type SignedInfo;

	/// Returns whether the current call is a self-contained function.
	fn is_self_contained(&self) -> bool;
	/// Check signatures of a self-contained function. Returns `None`
	/// if the function is not a self-contained.
	fn check_self_contained(&self) -> Option<Result<Self::SignedInfo, TransactionValidityError>>;
	/// Validate a self-contained function. Returns `None` if the
	/// function is not a self-contained.
	fn validate_self_contained(
		&self,
		info: &Self::SignedInfo,
		dispatch_info: &DispatchInfoOf<Self>,
		len: usize,
	) -> Option<TransactionValidity>;
	/// Do any pre-flight stuff for a self-contained call.
	///
	/// Note this function by default delegates to `validate_self_contained`, so that
	/// all checks performed for the transaction queue are also performed during
	/// the dispatch phase (applying the extrinsic).
	///
	/// If you ever override this function, you need to make sure to always
	/// perform the same validation as in `validate_self_contained`.
	///
	/// Returns `None` if the function is not a self-contained.
	fn pre_dispatch_self_contained(
		&self,
		info: &Self::SignedInfo,
		dispatch_info: &DispatchInfoOf<Self>,
		len: usize,
	) -> Option<Result<(), TransactionValidityError>> {
		self.validate_self_contained(info, dispatch_info, len)
			.map(|res| res.map(|_| ()))
	}
	/// Apply a self-contained function. Returns `None` if the
	/// function is not a self-contained.
	fn apply_self_contained(
		self,
		info: Self::SignedInfo,
	) -> Option<sp_runtime::DispatchResultWithInfo<PostDispatchInfoOf<Self>>>;
}
