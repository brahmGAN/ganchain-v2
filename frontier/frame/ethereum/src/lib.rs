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

//! # Ethereum pallet
//!
//! The Ethereum pallet works together with EVM pallet to provide full emulation
//! for Ethereum block processing.

// Ensure we're `no_std` when compiling for Wasm.
#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::comparison_chain, clippy::large_enum_variant)]
#![warn(unused_crate_dependencies)]

#[cfg(all(feature = "std", test))]
mod mock;
#[cfg(all(feature = "std", test))]
mod tests;

use ethereum_types::{Bloom, BloomInput, H160, H256, H64, U256};
use evm::ExitReason;
use fp_consensus::{PostLog, PreLog, FRONTIER_ENGINE_ID};
use fp_ethereum::{
	TransactionData, TransactionValidationError, ValidatedTransaction as ValidatedTransactionT,
};
use fp_evm::{
	CallOrCreateInfo, CheckEvmTransaction, CheckEvmTransactionConfig, InvalidEvmTransactionError,
};
use fp_storage::{EthereumStorageSchema, PALLET_ETHEREUM_SCHEMA};
use frame_support::{
	codec::{Decode, Encode, MaxEncodedLen},
	dispatch::{DispatchInfo, DispatchResultWithPostInfo, Pays, PostDispatchInfo},
	scale_info::TypeInfo,
	traits::{EnsureOrigin, Get, PalletInfoAccess, Time},
	weights::Weight,
};
use frame_system::{pallet_prelude::OriginFor, CheckWeight, WeightInfo};
use pallet_evm::{BlockHashMapping, FeeCalculator, GasWeightMapping, Runner};
use sp_runtime::{
	generic::DigestItem,
	traits::{DispatchInfoOf, Dispatchable, One, Saturating, UniqueSaturatedInto, Zero},
	transaction_validity::{
		InvalidTransaction, TransactionValidity, TransactionValidityError, ValidTransactionBuilder,
	},
	DispatchErrorWithPostInfo, RuntimeDebug, SaturatedConversion,
};
use sp_std::{marker::PhantomData, prelude::*};

pub use ethereum::{
	AccessListItem, BlockV2 as Block, LegacyTransactionMessage, Log, ReceiptV3 as Receipt,
	TransactionAction, TransactionV2 as Transaction,
};
pub use fp_rpc::TransactionStatus;

#[derive(Clone, Eq, PartialEq, RuntimeDebug)]
#[derive(Encode, Decode, MaxEncodedLen, TypeInfo)]
pub enum RawOrigin {
	EthereumTransaction(H160),
}

pub fn ensure_ethereum_transaction<OuterOrigin>(o: OuterOrigin) -> Result<H160, &'static str>
where
	OuterOrigin: Into<Result<RawOrigin, OuterOrigin>>,
{
	match o.into() {
		Ok(RawOrigin::EthereumTransaction(n)) => Ok(n),
		_ => Err("bad origin: expected to be an Ethereum transaction"),
	}
}

pub struct EnsureEthereumTransaction;
impl<O: Into<Result<RawOrigin, O>> + From<RawOrigin>> EnsureOrigin<O>
	for EnsureEthereumTransaction
{
	type Success = H160;
	fn try_origin(o: O) -> Result<Self::Success, O> {
		o.into().map(|o| match o {
			RawOrigin::EthereumTransaction(id) => id,
		})
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin() -> Result<O, ()> {
		Ok(O::from(RawOrigin::EthereumTransaction(Default::default())))
	}
}

impl<T> Call<T>
where
	OriginFor<T>: Into<Result<RawOrigin, OriginFor<T>>>,
	T: Send + Sync + Config,
	T::RuntimeCall: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>,
{
	pub fn is_self_contained(&self) -> bool {
		matches!(self, Call::transact { .. })
	}

	pub fn check_self_contained(&self) -> Option<Result<H160, TransactionValidityError>> {
		if let Call::transact { transaction } = self {
			let check = || {
				let origin = Pallet::<T>::recover_signer(transaction).ok_or(
					InvalidTransaction::Custom(TransactionValidationError::InvalidSignature as u8),
				)?;

				Ok(origin)
			};

			Some(check())
		} else {
			None
		}
	}

	pub fn pre_dispatch_self_contained(
		&self,
		origin: &H160,
		dispatch_info: &DispatchInfoOf<T::RuntimeCall>,
		len: usize,
	) -> Option<Result<(), TransactionValidityError>> {
		if let Call::transact { transaction } = self {
			if let Err(e) = CheckWeight::<T>::do_pre_dispatch(dispatch_info, len) {
				return Some(Err(e));
			}

			Some(Pallet::<T>::validate_transaction_in_block(
				*origin,
				transaction,
			))
		} else {
			None
		}
	}

	pub fn validate_self_contained(
		&self,
		origin: &H160,
		dispatch_info: &DispatchInfoOf<T::RuntimeCall>,
		len: usize,
	) -> Option<TransactionValidity> {
		if let Call::transact { transaction } = self {
			if let Err(e) = CheckWeight::<T>::do_validate(dispatch_info, len) {
				return Some(Err(e));
			}

			Some(Pallet::<T>::validate_transaction_in_pool(
				*origin,
				transaction,
			))
		} else {
			None
		}
	}
}

#[derive(Copy, Clone, Eq, PartialEq, Default)]
pub enum PostLogContent {
	#[default]
	BlockAndTxnHashes,
	OnlyBlockHash,
}

pub use self::pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::origin]
	pub type Origin = RawOrigin;

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_evm::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		/// How Ethereum state root is calculated.
		type StateRoot: Get<H256>;
		/// What's included in the PostLog.
		type PostLogContent: Get<PostLogContent>;
		/// The maximum length of the extra data in the Executed event.
		type ExtraDataLength: Get<u32>;
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_finalize(n: BlockNumberFor<T>) {
			<Pallet<T>>::store_block(
				match fp_consensus::find_pre_log(&frame_system::Pallet::<T>::digest()) {
					Ok(_) => None,
					Err(_) => Some(T::PostLogContent::get()),
				},
				U256::from(UniqueSaturatedInto::<u128>::unique_saturated_into(
					frame_system::Pallet::<T>::block_number(),
				)),
			);
			// move block hash pruning window by one block
			let block_hash_count = T::BlockHashCount::get();
			let to_remove = n
				.saturating_sub(block_hash_count)
				.saturating_sub(One::one());
			// keep genesis hash
			if !to_remove.is_zero() {
				<BlockHash<T>>::remove(U256::from(
					UniqueSaturatedInto::<u32>::unique_saturated_into(to_remove),
				));
			}
			Pending::<T>::kill();
		}

		fn on_initialize(_: BlockNumberFor<T>) -> Weight {
			let mut weight = T::SystemWeightInfo::kill_storage(1);

			// If the digest contain an existing ethereum block(encoded as PreLog), If contains,
			// execute the imported block firstly and disable transact dispatch function.
			if let Ok(log) = fp_consensus::find_pre_log(&frame_system::Pallet::<T>::digest()) {
				let PreLog::Block(block) = log;

				for transaction in block.transactions {
					let source = Self::recover_signer(&transaction).expect(
						"pre-block transaction signature invalid; the block cannot be built",
					);

					Self::validate_transaction_in_block(source, &transaction).expect(
						"pre-block transaction verification failed; the block cannot be built",
					);
					let r = Self::apply_validated_transaction(source, transaction)
						.expect("pre-block apply transaction failed; the block cannot be built");

					weight = weight.saturating_add(r.actual_weight.unwrap_or_default());
				}
			}
			// Account for `on_finalize` weight:
			//	- read: frame_system::Pallet::<T>::digest()
			//	- read: frame_system::Pallet::<T>::block_number()
			//	- write: <Pallet<T>>::store_block()
			//	- write: <BlockHash<T>>::remove()
			weight.saturating_add(T::DbWeight::get().reads_writes(2, 2))
		}

		fn on_runtime_upgrade() -> Weight {
			frame_support::storage::unhashed::put::<EthereumStorageSchema>(
				PALLET_ETHEREUM_SCHEMA,
				&EthereumStorageSchema::V3,
			);

			T::DbWeight::get().writes(1)
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T>
	where
		OriginFor<T>: Into<Result<RawOrigin, OriginFor<T>>>,
	{
		/// Transact an Ethereum transaction.
		#[pallet::call_index(0)]
		#[pallet::weight({
			let without_base_extrinsic_weight = true;
			<T as pallet_evm::Config>::GasWeightMapping::gas_to_weight({
				let transaction_data: TransactionData = transaction.into();
				transaction_data.gas_limit.unique_saturated_into()
			}, without_base_extrinsic_weight)
		})]
		pub fn transact(
			origin: OriginFor<T>,
			transaction: Transaction,
		) -> DispatchResultWithPostInfo {
			let source = ensure_ethereum_transaction(origin)?;
			// Disable transact functionality if PreLog exist.
			assert!(
				fp_consensus::find_pre_log(&frame_system::Pallet::<T>::digest()).is_err(),
				"pre log already exists; block is invalid",
			);

			Self::apply_validated_transaction(source, transaction)
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event {
		/// An ethereum transaction was successfully executed.
		Executed {
			from: H160,
			to: H160,
			transaction_hash: H256,
			exit_reason: ExitReason,
			extra_data: Vec<u8>,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Signature is invalid.
		InvalidSignature,
		/// Pre-log is present, therefore transact is not allowed.
		PreLogExists,
	}

	/// Current building block's transactions and receipts.
	#[pallet::storage]
	pub(super) type Pending<T: Config> =
		StorageValue<_, Vec<(Transaction, TransactionStatus, Receipt)>, ValueQuery>;

	/// The current Ethereum block.
	#[pallet::storage]
	pub type CurrentBlock<T: Config> = StorageValue<_, ethereum::BlockV2>;

	/// The current Ethereum receipts.
	#[pallet::storage]
	pub type CurrentReceipts<T: Config> = StorageValue<_, Vec<Receipt>>;

	/// The current transaction statuses.
	#[pallet::storage]
	pub type CurrentTransactionStatuses<T: Config> = StorageValue<_, Vec<TransactionStatus>>;

	// Mapping for block number and hashes.
	#[pallet::storage]
	pub type BlockHash<T: Config> = StorageMap<_, Twox64Concat, U256, H256, ValueQuery>;

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T> {
		#[serde(skip)]
		pub _marker: PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			<Pallet<T>>::store_block(None, U256::zero());
			frame_support::storage::unhashed::put::<EthereumStorageSchema>(
				PALLET_ETHEREUM_SCHEMA,
				&EthereumStorageSchema::V3,
			);
		}
	}
}

impl<T: Config> Pallet<T> {
	/// The call wrapped in the extrinsic is part of the PoV, record this as a base cost for the size of the proof.
	fn proof_size_base_cost(transaction: &Transaction) -> u64 {
		transaction
			.encode()
			.len()
			// pallet index
			.saturating_add(1)
			// call index
			.saturating_add(1) as u64
	}

	fn recover_signer(transaction: &Transaction) -> Option<H160> {
		let mut sig = [0u8; 65];
		let mut msg = [0u8; 32];
		match transaction {
			Transaction::Legacy(t) => {
				sig[0..32].copy_from_slice(&t.signature.r()[..]);
				sig[32..64].copy_from_slice(&t.signature.s()[..]);
				sig[64] = t.signature.standard_v();
				msg.copy_from_slice(
					&ethereum::LegacyTransactionMessage::from(t.clone()).hash()[..],
				);
			}
			Transaction::EIP2930(t) => {
				sig[0..32].copy_from_slice(&t.r[..]);
				sig[32..64].copy_from_slice(&t.s[..]);
				sig[64] = t.odd_y_parity as u8;
				msg.copy_from_slice(
					&ethereum::EIP2930TransactionMessage::from(t.clone()).hash()[..],
				);
			}
			Transaction::EIP1559(t) => {
				sig[0..32].copy_from_slice(&t.r[..]);
				sig[32..64].copy_from_slice(&t.s[..]);
				sig[64] = t.odd_y_parity as u8;
				msg.copy_from_slice(
					&ethereum::EIP1559TransactionMessage::from(t.clone()).hash()[..],
				);
			}
		}
		let pubkey = sp_io::crypto::secp256k1_ecdsa_recover(&sig, &msg).ok()?;
		Some(H160::from(H256::from(sp_io::hashing::keccak_256(&pubkey))))
	}

	fn store_block(post_log: Option<PostLogContent>, block_number: U256) {
		let mut transactions = Vec::new();
		let mut statuses = Vec::new();
		let mut receipts = Vec::new();
		let mut logs_bloom = Bloom::default();
		let mut cumulative_gas_used = U256::zero();
		for (transaction, status, receipt) in Pending::<T>::get() {
			transactions.push(transaction);
			statuses.push(status);
			receipts.push(receipt.clone());
			let (logs, used_gas) = match receipt {
				Receipt::Legacy(d) | Receipt::EIP2930(d) | Receipt::EIP1559(d) => {
					(d.logs.clone(), d.used_gas)
				}
			};
			cumulative_gas_used = used_gas;
			Self::logs_bloom(logs, &mut logs_bloom);
		}

		let ommers = Vec::<ethereum::Header>::new();
		let receipts_root = ethereum::util::ordered_trie_root(
			receipts.iter().map(ethereum::EnvelopedEncodable::encode),
		);
		let partial_header = ethereum::PartialHeader {
			parent_hash: if block_number > U256::zero() {
				BlockHash::<T>::get(block_number - 1)
			} else {
				H256::default()
			},
			beneficiary: pallet_evm::Pallet::<T>::find_author(),
			state_root: T::StateRoot::get(),
			receipts_root,
			logs_bloom,
			difficulty: U256::zero(),
			number: block_number,
			gas_limit: T::BlockGasLimit::get(),
			gas_used: cumulative_gas_used,
			timestamp: T::Timestamp::now().unique_saturated_into(),
			extra_data: Vec::new(),
			mix_hash: H256::default(),
			nonce: H64::default(),
		};
		let block = ethereum::Block::new(partial_header, transactions.clone(), ommers);

		CurrentBlock::<T>::put(block.clone());
		CurrentReceipts::<T>::put(receipts.clone());
		CurrentTransactionStatuses::<T>::put(statuses.clone());
		BlockHash::<T>::insert(block_number, block.header.hash());

		match post_log {
			Some(PostLogContent::BlockAndTxnHashes) => {
				let digest = DigestItem::Consensus(
					FRONTIER_ENGINE_ID,
					PostLog::Hashes(fp_consensus::Hashes::from_block(block)).encode(),
				);
				frame_system::Pallet::<T>::deposit_log(digest);
			}
			Some(PostLogContent::OnlyBlockHash) => {
				let digest = DigestItem::Consensus(
					FRONTIER_ENGINE_ID,
					PostLog::BlockHash(block.header.hash()).encode(),
				);
				frame_system::Pallet::<T>::deposit_log(digest);
			}
			None => { /* do nothing*/ }
		}
	}

	fn logs_bloom(logs: Vec<Log>, bloom: &mut Bloom) {
		for log in logs {
			bloom.accrue(BloomInput::Raw(&log.address[..]));
			for topic in log.topics {
				bloom.accrue(BloomInput::Raw(&topic[..]));
			}
		}
	}

	// Controls that must be performed by the pool.
	// The controls common with the State Transition Function (STF) are in
	// the function `validate_transaction_common`.
	fn validate_transaction_in_pool(
		origin: H160,
		transaction: &Transaction,
	) -> TransactionValidity {
		let transaction_data: TransactionData = transaction.into();
		let transaction_nonce = transaction_data.nonce;

		let (weight_limit, proof_size_base_cost) =
			match <T as pallet_evm::Config>::GasWeightMapping::gas_to_weight(
				transaction_data.gas_limit.unique_saturated_into(),
				true,
			) {
				weight_limit if weight_limit.proof_size() > 0 => (
					Some(weight_limit),
					Some(Self::proof_size_base_cost(transaction)),
				),
				_ => (None, None),
			};

		let (base_fee, _) = T::FeeCalculator::min_gas_price();
		let (who, _) = pallet_evm::Pallet::<T>::account_basic(&origin);

		let _ = CheckEvmTransaction::<InvalidTransactionWrapper>::new(
			CheckEvmTransactionConfig {
				evm_config: T::config(),
				block_gas_limit: T::BlockGasLimit::get(),
				base_fee,
				chain_id: T::ChainId::get(),
				is_transactional: true,
			},
			transaction_data.clone().into(),
			weight_limit,
			proof_size_base_cost,
		)
		.validate_in_pool_for(&who)
		.and_then(|v| v.with_chain_id())
		.and_then(|v| v.with_base_fee())
		.and_then(|v| v.with_balance_for(&who))
		.map_err(|e| e.0)?;

		let priority = match (
			transaction_data.gas_price,
			transaction_data.max_fee_per_gas,
			transaction_data.max_priority_fee_per_gas,
		) {
			// Legacy or EIP-2930 transaction.
			// Handle priority here. On legacy transaction everything in gas_price except
			// the current base_fee is considered a tip to the miner and thus the priority.
			(Some(gas_price), None, None) => {
				gas_price.saturating_sub(base_fee).unique_saturated_into()
			}
			// EIP-1559 transaction without tip.
			(None, Some(_), None) => 0,
			// EIP-1559 transaction with tip.
			(None, Some(max_fee_per_gas), Some(max_priority_fee_per_gas)) => max_fee_per_gas
				.saturating_sub(base_fee)
				.min(max_priority_fee_per_gas)
				.unique_saturated_into(),
			// Unreachable because already validated. Gracefully handle.
			_ => return Err(InvalidTransaction::Payment.into()),
		};

		// The tag provides and requires must be filled correctly according to the nonce.
		let mut builder = ValidTransactionBuilder::default()
			.and_provides((origin, transaction_nonce))
			.priority(priority);

		// In the context of the pool, a transaction with
		// too high a nonce is still considered valid
		if transaction_nonce > who.nonce {
			if let Some(prev_nonce) = transaction_nonce.checked_sub(1.into()) {
				builder = builder.and_requires((origin, prev_nonce))
			}
		}

		builder.build()
	}

	fn apply_validated_transaction(
		source: H160,
		transaction: Transaction,
	) -> DispatchResultWithPostInfo {
		let (to, _, info) = Self::execute(source, &transaction, None)?;

		let pending = Pending::<T>::get();
		let transaction_hash = transaction.hash();
		let transaction_index = pending.len() as u32;

		let (reason, status, weight_info, used_gas, dest, extra_data) = match info {
			CallOrCreateInfo::Call(info) => (
				info.exit_reason.clone(),
				TransactionStatus {
					transaction_hash,
					transaction_index,
					from: source,
					to,
					contract_address: None,
					logs: info.logs.clone(),
					logs_bloom: {
						let mut bloom: Bloom = Bloom::default();
						Self::logs_bloom(info.logs, &mut bloom);
						bloom
					},
				},
				info.weight_info,
				info.used_gas,
				to,
				match info.exit_reason {
					ExitReason::Revert(_) => {
						const LEN_START: usize = 36;
						const MESSAGE_START: usize = 68;

						let data = info.value;
						let data_len = data.len();
						if data_len > MESSAGE_START {
							let message_len = U256::from(&data[LEN_START..MESSAGE_START])
								.saturated_into::<usize>();
							let message_end = MESSAGE_START.saturating_add(
								message_len.min(T::ExtraDataLength::get() as usize),
							);

							if data_len >= message_end {
								data[MESSAGE_START..message_end].to_vec()
							} else {
								data
							}
						} else {
							data
						}
					}
					_ => vec![],
				},
			),
			CallOrCreateInfo::Create(info) => (
				info.exit_reason,
				TransactionStatus {
					transaction_hash,
					transaction_index,
					from: source,
					to,
					contract_address: Some(info.value),
					logs: info.logs.clone(),
					logs_bloom: {
						let mut bloom: Bloom = Bloom::default();
						Self::logs_bloom(info.logs, &mut bloom);
						bloom
					},
				},
				info.weight_info,
				info.used_gas,
				Some(info.value),
				Vec::new(),
			),
		};

		let receipt = {
			let status_code: u8 = match reason {
				ExitReason::Succeed(_) => 1,
				_ => 0,
			};
			let logs_bloom = status.logs_bloom;
			let logs = status.clone().logs;
			let cumulative_gas_used = if let Some((_, _, receipt)) = pending.last() {
				match receipt {
					Receipt::Legacy(d) | Receipt::EIP2930(d) | Receipt::EIP1559(d) => {
						d.used_gas.saturating_add(used_gas.effective)
					}
				}
			} else {
				used_gas.effective
			};
			match &transaction {
				Transaction::Legacy(_) => Receipt::Legacy(ethereum::EIP658ReceiptData {
					status_code,
					used_gas: cumulative_gas_used,
					logs_bloom,
					logs,
				}),
				Transaction::EIP2930(_) => Receipt::EIP2930(ethereum::EIP2930ReceiptData {
					status_code,
					used_gas: cumulative_gas_used,
					logs_bloom,
					logs,
				}),
				Transaction::EIP1559(_) => Receipt::EIP1559(ethereum::EIP2930ReceiptData {
					status_code,
					used_gas: cumulative_gas_used,
					logs_bloom,
					logs,
				}),
			}
		};

		Pending::<T>::append((transaction, status, receipt));

		Self::deposit_event(Event::Executed {
			from: source,
			to: dest.unwrap_or_default(),
			transaction_hash,
			exit_reason: reason,
			extra_data,
		});

		Ok(PostDispatchInfo {
			actual_weight: {
				let mut gas_to_weight = T::GasWeightMapping::gas_to_weight(
					sp_std::cmp::max(
						used_gas.standard.unique_saturated_into(),
						used_gas.effective.unique_saturated_into(),
					),
					true,
				);
				if let Some(weight_info) = weight_info {
					if let Some(proof_size_usage) = weight_info.proof_size_usage {
						*gas_to_weight.proof_size_mut() = proof_size_usage;
					}
				}
				Some(gas_to_weight)
			},
			pays_fee: Pays::No,
		})
	}

	/// Get current block hash
	pub fn current_block_hash() -> Option<H256> {
		<CurrentBlock<T>>::get().map(|block| block.header.hash())
	}

	/// Execute an Ethereum transaction.
	pub fn execute(
		from: H160,
		transaction: &Transaction,
		config: Option<evm::Config>,
	) -> Result<
		(Option<H160>, Option<H160>, CallOrCreateInfo),
		DispatchErrorWithPostInfo<PostDispatchInfo>,
	> {
		let (
			input,
			value,
			gas_limit,
			max_fee_per_gas,
			max_priority_fee_per_gas,
			nonce,
			action,
			access_list,
		) = {
			match transaction {
				// max_fee_per_gas and max_priority_fee_per_gas in legacy and 2930 transactions is
				// the provided gas_price.
				Transaction::Legacy(t) => (
					t.input.clone(),
					t.value,
					t.gas_limit,
					Some(t.gas_price),
					Some(t.gas_price),
					Some(t.nonce),
					t.action,
					Vec::new(),
				),
				Transaction::EIP2930(t) => {
					let access_list: Vec<(H160, Vec<H256>)> = t
						.access_list
						.iter()
						.map(|item| (item.address, item.storage_keys.clone()))
						.collect();
					(
						t.input.clone(),
						t.value,
						t.gas_limit,
						Some(t.gas_price),
						Some(t.gas_price),
						Some(t.nonce),
						t.action,
						access_list,
					)
				}
				Transaction::EIP1559(t) => {
					let access_list: Vec<(H160, Vec<H256>)> = t
						.access_list
						.iter()
						.map(|item| (item.address, item.storage_keys.clone()))
						.collect();
					(
						t.input.clone(),
						t.value,
						t.gas_limit,
						Some(t.max_fee_per_gas),
						Some(t.max_priority_fee_per_gas),
						Some(t.nonce),
						t.action,
						access_list,
					)
				}
			}
		};

		let is_transactional = true;
		let validate = false;

		let (proof_size_base_cost, weight_limit) =
			match <T as pallet_evm::Config>::GasWeightMapping::gas_to_weight(
				gas_limit.unique_saturated_into(),
				true,
			) {
				weight_limit if weight_limit.proof_size() > 0 => (
					Some(Self::proof_size_base_cost(transaction)),
					Some(weight_limit),
				),
				_ => (None, None),
			};
		match action {
			ethereum::TransactionAction::Call(target) => {
				let res = match T::Runner::call(
					from,
					target,
					input,
					value,
					gas_limit.unique_saturated_into(),
					max_fee_per_gas,
					max_priority_fee_per_gas,
					nonce,
					access_list,
					is_transactional,
					validate,
					weight_limit,
					proof_size_base_cost,
					config.as_ref().unwrap_or_else(|| T::config()),
				) {
					Ok(res) => res,
					Err(e) => {
						return Err(DispatchErrorWithPostInfo {
							post_info: PostDispatchInfo {
								actual_weight: Some(e.weight),
								pays_fee: Pays::Yes,
							},
							error: e.error.into(),
						})
					}
				};

				Ok((Some(target), None, CallOrCreateInfo::Call(res)))
			}
			ethereum::TransactionAction::Create => {
				let res = match T::Runner::create(
					from,
					input,
					value,
					gas_limit.unique_saturated_into(),
					max_fee_per_gas,
					max_priority_fee_per_gas,
					nonce,
					access_list,
					is_transactional,
					validate,
					weight_limit,
					proof_size_base_cost,
					config.as_ref().unwrap_or_else(|| T::config()),
				) {
					Ok(res) => res,
					Err(e) => {
						return Err(DispatchErrorWithPostInfo {
							post_info: PostDispatchInfo {
								actual_weight: Some(e.weight),
								pays_fee: Pays::Yes,
							},
							error: e.error.into(),
						})
					}
				};

				Ok((None, Some(res.value), CallOrCreateInfo::Create(res)))
			}
		}
	}

	/// Validate an Ethereum transaction already in block
	///
	/// This function must be called during the pre-dispatch phase
	/// (just before applying the extrinsic).
	pub fn validate_transaction_in_block(
		origin: H160,
		transaction: &Transaction,
	) -> Result<(), TransactionValidityError> {
		let transaction_data: TransactionData = transaction.into();

		let (base_fee, _) = T::FeeCalculator::min_gas_price();
		let (who, _) = pallet_evm::Pallet::<T>::account_basic(&origin);

		let (weight_limit, proof_size_base_cost) =
			match <T as pallet_evm::Config>::GasWeightMapping::gas_to_weight(
				transaction_data.gas_limit.unique_saturated_into(),
				true,
			) {
				weight_limit if weight_limit.proof_size() > 0 => (
					Some(weight_limit),
					Some(Self::proof_size_base_cost(transaction)),
				),
				_ => (None, None),
			};

		let _ = CheckEvmTransaction::<InvalidTransactionWrapper>::new(
			CheckEvmTransactionConfig {
				evm_config: T::config(),
				block_gas_limit: T::BlockGasLimit::get(),
				base_fee,
				chain_id: T::ChainId::get(),
				is_transactional: true,
			},
			transaction_data.into(),
			weight_limit,
			proof_size_base_cost,
		)
		.validate_in_block_for(&who)
		.and_then(|v| v.with_chain_id())
		.and_then(|v| v.with_base_fee())
		.and_then(|v| v.with_balance_for(&who))
		.map_err(|e| TransactionValidityError::Invalid(e.0))?;

		Ok(())
	}

	pub fn migrate_block_v0_to_v2() -> Weight {
		let db_weights = T::DbWeight::get();
		let mut weight: Weight = db_weights.reads(1);
		let item = b"CurrentBlock";
		let block_v0 = frame_support::storage::migration::get_storage_value::<ethereum::BlockV0>(
			Self::name().as_bytes(),
			item,
			&[],
		);
		if let Some(block_v0) = block_v0 {
			weight = weight.saturating_add(db_weights.writes(1));
			let block_v2: ethereum::BlockV2 = block_v0.into();
			frame_support::storage::migration::put_storage_value::<ethereum::BlockV2>(
				Self::name().as_bytes(),
				item,
				&[],
				block_v2,
			);
		}
		weight
	}

	#[cfg(feature = "try-runtime")]
	pub fn pre_migrate_block_v2() -> Result<Vec<u8>, &'static str> {
		let item = b"CurrentBlock";
		let block_v0 = frame_support::storage::migration::get_storage_value::<ethereum::BlockV0>(
			Self::name().as_bytes(),
			item,
			&[],
		);
		if let Some(block_v0) = block_v0 {
			Ok((
				block_v0.header.number,
				block_v0.header.parent_hash,
				block_v0.transactions.len() as u64,
			)
				.encode())
		} else {
			Ok(Vec::new())
		}
	}

	#[cfg(feature = "try-runtime")]
	pub fn post_migrate_block_v2(v0_data: Vec<u8>) -> Result<(), &'static str> {
		let (v0_number, v0_parent_hash, v0_transaction_len): (U256, H256, u64) = Decode::decode(
			&mut v0_data.as_slice(),
		)
		.expect("the state parameter should be something that was generated by pre_upgrade");
		let item = b"CurrentBlock";
		let block_v2 = frame_support::storage::migration::get_storage_value::<ethereum::BlockV2>(
			Self::name().as_bytes(),
			item,
			&[],
		);

		assert!(block_v2.is_some());

		let block_v2 = block_v2.unwrap();
		assert_eq!(block_v2.header.number, v0_number);
		assert_eq!(block_v2.header.parent_hash, v0_parent_hash);
		assert_eq!(block_v2.transactions.len() as u64, v0_transaction_len);
		Ok(())
	}
}

pub struct ValidatedTransaction<T>(PhantomData<T>);
impl<T: Config> ValidatedTransactionT for ValidatedTransaction<T> {
	fn apply(source: H160, transaction: Transaction) -> DispatchResultWithPostInfo {
		Pallet::<T>::apply_validated_transaction(source, transaction)
	}
}

#[derive(Eq, PartialEq, Clone, RuntimeDebug)]
pub enum ReturnValue {
	Bytes(Vec<u8>),
	Hash(H160),
}

pub struct IntermediateStateRoot<T>(PhantomData<T>);
impl<T: Config> Get<H256> for IntermediateStateRoot<T> {
	fn get() -> H256 {
		let version = T::Version::get().state_version();
		H256::decode(&mut &sp_io::storage::root(version)[..])
			.expect("Node is configured to use the same hash; qed")
	}
}

/// Returns the Ethereum block hash by number.
pub struct EthereumBlockHashMapping<T>(PhantomData<T>);
impl<T: Config> BlockHashMapping for EthereumBlockHashMapping<T> {
	fn block_hash(number: u32) -> H256 {
		BlockHash::<T>::get(U256::from(number))
	}
}

pub struct InvalidTransactionWrapper(InvalidTransaction);

impl From<InvalidEvmTransactionError> for InvalidTransactionWrapper {
	fn from(validation_error: InvalidEvmTransactionError) -> Self {
		match validation_error {
			InvalidEvmTransactionError::GasLimitTooLow => InvalidTransactionWrapper(
				InvalidTransaction::Custom(TransactionValidationError::GasLimitTooLow as u8),
			),
			InvalidEvmTransactionError::GasLimitTooHigh => InvalidTransactionWrapper(
				InvalidTransaction::Custom(TransactionValidationError::GasLimitTooHigh as u8),
			),
			InvalidEvmTransactionError::GasPriceTooLow => {
				InvalidTransactionWrapper(InvalidTransaction::Payment)
			}
			InvalidEvmTransactionError::PriorityFeeTooHigh => InvalidTransactionWrapper(
				InvalidTransaction::Custom(TransactionValidationError::MaxFeePerGasTooLow as u8),
			),
			InvalidEvmTransactionError::BalanceTooLow => {
				InvalidTransactionWrapper(InvalidTransaction::Payment)
			}
			InvalidEvmTransactionError::TxNonceTooLow => {
				InvalidTransactionWrapper(InvalidTransaction::Stale)
			}
			InvalidEvmTransactionError::TxNonceTooHigh => {
				InvalidTransactionWrapper(InvalidTransaction::Future)
			}
			InvalidEvmTransactionError::InvalidPaymentInput => {
				InvalidTransactionWrapper(InvalidTransaction::Payment)
			}
			InvalidEvmTransactionError::InvalidChainId => InvalidTransactionWrapper(
				InvalidTransaction::Custom(TransactionValidationError::InvalidChainId as u8),
			),
		}
	}
}
