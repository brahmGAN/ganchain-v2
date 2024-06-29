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

//! EVM stack-based runner.

use evm::{
	backend::Backend as BackendT,
	executor::stack::{Accessed, StackExecutor, StackState as StackStateT, StackSubstateMetadata},
	gasometer::{GasCost, StorageTarget},
	ExitError, ExitReason, Opcode, Transfer,
};
// Substrate
use frame_support::{
	traits::{
		tokens::{currency::Currency, ExistenceRequirement},
		Get, Time,
	},
	weights::Weight,
};
use sp_core::{H160, H256, U256};
use sp_runtime::traits::UniqueSaturatedInto;
use sp_std::{
	boxed::Box,
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	marker::PhantomData,
	mem,
	vec::Vec,
};
// Frontier
use fp_evm::{
	AccessedStorage, CallInfo, CreateInfo, ExecutionInfoV2, IsPrecompileResult, Log, PrecompileSet,
	Vicinity, WeightInfo, ACCOUNT_BASIC_PROOF_SIZE, ACCOUNT_CODES_METADATA_PROOF_SIZE,
	ACCOUNT_STORAGE_PROOF_SIZE, IS_EMPTY_CHECK_PROOF_SIZE, WRITE_PROOF_SIZE,
};

use crate::{
	runner::Runner as RunnerT, AccountCodes, AccountCodesMetadata, AccountStorages, AddressMapping,
	BalanceOf, BlockHashMapping, Config, Error, Event, FeeCalculator, OnChargeEVMTransaction,
	OnCreate, Pallet, RunnerError,
};

#[cfg(feature = "forbid-evm-reentrancy")]
environmental::thread_local_impl!(static IN_EVM: environmental::RefCell<bool> = environmental::RefCell::new(false));

#[derive(Default)]
pub struct Runner<T: Config> {
	_marker: PhantomData<T>,
}

impl<T: Config> Runner<T>
where
	BalanceOf<T>: TryFrom<U256> + Into<U256>,
{
	#[allow(clippy::let_and_return)]
	/// Execute an already validated EVM operation.
	fn execute<'config, 'precompiles, F, R>(
		source: H160,
		value: U256,
		gas_limit: u64,
		max_fee_per_gas: Option<U256>,
		max_priority_fee_per_gas: Option<U256>,
		config: &'config evm::Config,
		precompiles: &'precompiles T::PrecompilesType,
		is_transactional: bool,
		weight_limit: Option<Weight>,
		proof_size_base_cost: Option<u64>,
		f: F,
	) -> Result<ExecutionInfoV2<R>, RunnerError<Error<T>>>
	where
		F: FnOnce(
			&mut StackExecutor<
				'config,
				'precompiles,
				SubstrateStackState<'_, 'config, T>,
				T::PrecompilesType,
			>,
		) -> (ExitReason, R),
		R: Default,
	{
		let (base_fee, weight) = T::FeeCalculator::min_gas_price();

		#[cfg(feature = "forbid-evm-reentrancy")]
		if IN_EVM.with(|in_evm| in_evm.replace(true)) {
			return Err(RunnerError {
				error: Error::<T>::Reentrancy,
				weight,
			});
		}

		let res = Self::execute_inner(
			source,
			value,
			gas_limit,
			max_fee_per_gas,
			max_priority_fee_per_gas,
			config,
			precompiles,
			is_transactional,
			f,
			base_fee,
			weight,
			weight_limit,
			proof_size_base_cost,
		);

		// Set IN_EVM to false
		// We should make sure that this line is executed whatever the execution path.
		#[cfg(feature = "forbid-evm-reentrancy")]
		let _ = IN_EVM.with(|in_evm| in_evm.take());

		res
	}

	// Execute an already validated EVM operation.
	fn execute_inner<'config, 'precompiles, F, R>(
		source: H160,
		value: U256,
		mut gas_limit: u64,
		max_fee_per_gas: Option<U256>,
		max_priority_fee_per_gas: Option<U256>,
		config: &'config evm::Config,
		precompiles: &'precompiles T::PrecompilesType,
		is_transactional: bool,
		f: F,
		base_fee: U256,
		weight: Weight,
		weight_limit: Option<Weight>,
		proof_size_base_cost: Option<u64>,
	) -> Result<ExecutionInfoV2<R>, RunnerError<Error<T>>>
	where
		F: FnOnce(
			&mut StackExecutor<
				'config,
				'precompiles,
				SubstrateStackState<'_, 'config, T>,
				T::PrecompilesType,
			>,
		) -> (ExitReason, R),
		R: Default,
	{
		// Used to record the external costs in the evm through the StackState implementation
		let maybe_weight_info =
			WeightInfo::new_from_weight_limit(weight_limit, proof_size_base_cost).map_err(
				|_| RunnerError {
					error: Error::<T>::Undefined,
					weight,
				},
			)?;
		// The precompile check is only used for transactional invocations. However, here we always
		// execute the check, because the check has side effects.
		match precompiles.is_precompile(source, gas_limit) {
			IsPrecompileResult::Answer { extra_cost, .. } => {
				gas_limit = gas_limit.saturating_sub(extra_cost);
			}
			IsPrecompileResult::OutOfGas => {
				return Ok(ExecutionInfoV2 {
					exit_reason: ExitError::OutOfGas.into(),
					value: Default::default(),
					used_gas: fp_evm::UsedGas {
						standard: gas_limit.into(),
						effective: gas_limit.into(),
					},
					weight_info: maybe_weight_info,
					logs: Default::default(),
				})
			}
		};

		// Only check the restrictions of EIP-3607 if the source of the EVM operation is from an external transaction.
		// If the source of this EVM operation is from an internal call, like from `eth_call` or `eth_estimateGas` RPC,
		// we will skip the checks for the EIP-3607.
		//
		// EIP-3607: https://eips.ethereum.org/EIPS/eip-3607
		// Do not allow transactions for which `tx.sender` has any code deployed.
		if is_transactional && !<AccountCodes<T>>::get(source).is_empty() {
			return Err(RunnerError {
				error: Error::<T>::TransactionMustComeFromEOA,
				weight,
			});
		}

		let (total_fee_per_gas, _actual_priority_fee_per_gas) =
			match (max_fee_per_gas, max_priority_fee_per_gas, is_transactional) {
				// Zero max_fee_per_gas for validated transactional calls exist in XCM -> EVM
				// because fees are already withdrawn in the xcm-executor.
				(Some(max_fee), _, true) if max_fee.is_zero() => (U256::zero(), U256::zero()),
				// With no tip, we pay exactly the base_fee
				(Some(_), None, _) => (base_fee, U256::zero()),
				// With tip, we include as much of the tip on top of base_fee that we can, never
				// exceeding max_fee_per_gas
				(Some(max_fee_per_gas), Some(max_priority_fee_per_gas), _) => {
					let actual_priority_fee_per_gas = max_fee_per_gas
						.saturating_sub(base_fee)
						.min(max_priority_fee_per_gas);
					(
						base_fee.saturating_add(actual_priority_fee_per_gas),
						actual_priority_fee_per_gas,
					)
				}
				// Gas price check is skipped for non-transactional calls that don't
				// define a `max_fee_per_gas` input.
				(None, _, false) => (Default::default(), U256::zero()),
				// Unreachable, previously validated. Handle gracefully.
				_ => {
					return Err(RunnerError {
						error: Error::<T>::GasPriceTooLow,
						weight,
					})
				}
			};

		// After eip-1559 we make sure the account can pay both the evm execution and priority fees.
		let total_fee =
			total_fee_per_gas
				.checked_mul(U256::from(gas_limit))
				.ok_or(RunnerError {
					error: Error::<T>::FeeOverflow,
					weight,
				})?;

		// Deduct fee from the `source` account. Returns `None` if `total_fee` is Zero.
		let fee = T::OnChargeTransaction::withdraw_fee(&source, total_fee)
			.map_err(|e| RunnerError { error: e, weight })?;

		// Execute the EVM call.
		let vicinity = Vicinity {
			gas_price: base_fee,
			origin: source,
		};

		let metadata = StackSubstateMetadata::new(gas_limit, config);
		let state = SubstrateStackState::new(&vicinity, metadata, maybe_weight_info);
		let mut executor = StackExecutor::new_with_precompiles(state, config, precompiles);

		let (reason, retv) = f(&mut executor);

		// Post execution.
		let used_gas = executor.used_gas();
		let effective_gas = match executor.state().weight_info() {
			Some(weight_info) => U256::from(sp_std::cmp::max(
				used_gas,
				weight_info
					.proof_size_usage
					.unwrap_or_default()
					.saturating_mul(T::GasLimitPovSizeRatio::get()),
			)),
			_ => used_gas.into(),
		};
		let actual_fee = effective_gas.saturating_mul(total_fee_per_gas);
		let actual_base_fee = effective_gas.saturating_mul(base_fee);

		log::debug!(
			target: "evm",
			"Execution {:?} [source: {:?}, value: {}, gas_limit: {}, actual_fee: {}, used_gas: {}, effective_gas: {}, base_fee: {}, total_fee_per_gas: {}, is_transactional: {}]",
			reason,
			source,
			value,
			gas_limit,
			actual_fee,
			used_gas,
			effective_gas,
			base_fee,
			total_fee_per_gas,
			is_transactional
		);
		// The difference between initially withdrawn and the actual cost is refunded.
		//
		// Considered the following request:
		// +-----------+---------+--------------+
		// | Gas_limit | Max_Fee | Max_Priority |
		// +-----------+---------+--------------+
		// |        20 |      10 |            6 |
		// +-----------+---------+--------------+
		//
		// And execution:
		// +----------+----------+
		// | Gas_used | Base_Fee |
		// +----------+----------+
		// |        5 |        2 |
		// +----------+----------+
		//
		// Initially withdrawn 10 * 20 = 200.
		// Actual cost (2 + 6) * 5 = 40.
		// Refunded 200 - 40 = 160.
		// Tip 5 * 6 = 30.
		// Burned 200 - (160 + 30) = 10. Which is equivalent to gas_used * base_fee.
		let actual_priority_fee = T::OnChargeTransaction::correct_and_deposit_fee(
			&source,
			// Actual fee after evm execution, including tip.
			actual_fee,
			// Base fee.
			actual_base_fee,
			// Fee initially withdrawn.
			fee,
		);
		T::OnChargeTransaction::pay_priority_fee(actual_priority_fee);

		let state = executor.into_state();

		for address in &state.substate.deletes {
			log::debug!(
				target: "evm",
				"Deleting account at {:?}",
				address
			);
			Pallet::<T>::remove_account(address)
		}

		for log in &state.substate.logs {
			log::trace!(
				target: "evm",
				"Inserting log for {:?}, topics ({}) {:?}, data ({}): {:?}]",
				log.address,
				log.topics.len(),
				log.topics,
				log.data.len(),
				log.data
			);
			Pallet::<T>::deposit_event(Event::<T>::Log {
				log: Log {
					address: log.address,
					topics: log.topics.clone(),
					data: log.data.clone(),
				},
			});
		}

		Ok(ExecutionInfoV2 {
			value: retv,
			exit_reason: reason,
			used_gas: fp_evm::UsedGas {
				standard: used_gas.into(),
				effective: effective_gas,
			},
			weight_info: state.weight_info(),
			logs: state.substate.logs,
		})
	}
}

impl<T: Config> RunnerT<T> for Runner<T>
where
	BalanceOf<T>: TryFrom<U256> + Into<U256>,
{
	type Error = Error<T>;

	fn validate(
		source: H160,
		target: Option<H160>,
		input: Vec<u8>,
		value: U256,
		gas_limit: u64,
		max_fee_per_gas: Option<U256>,
		max_priority_fee_per_gas: Option<U256>,
		nonce: Option<U256>,
		access_list: Vec<(H160, Vec<H256>)>,
		is_transactional: bool,
		weight_limit: Option<Weight>,
		proof_size_base_cost: Option<u64>,
		evm_config: &evm::Config,
	) -> Result<(), RunnerError<Self::Error>> {
		let (base_fee, mut weight) = T::FeeCalculator::min_gas_price();
		let (source_account, inner_weight) = Pallet::<T>::account_basic(&source);
		weight = weight.saturating_add(inner_weight);

		let _ = fp_evm::CheckEvmTransaction::<Self::Error>::new(
			fp_evm::CheckEvmTransactionConfig {
				evm_config,
				block_gas_limit: T::BlockGasLimit::get(),
				base_fee,
				chain_id: T::ChainId::get(),
				is_transactional,
			},
			fp_evm::CheckEvmTransactionInput {
				chain_id: Some(T::ChainId::get()),
				to: target,
				input,
				nonce: nonce.unwrap_or(source_account.nonce),
				gas_limit: gas_limit.into(),
				gas_price: None,
				max_fee_per_gas,
				max_priority_fee_per_gas,
				value,
				access_list,
			},
			weight_limit,
			proof_size_base_cost,
		)
		.validate_in_block_for(&source_account)
		.and_then(|v| v.with_base_fee())
		.and_then(|v| v.with_balance_for(&source_account))
		.map_err(|error| RunnerError { error, weight })?;
		Ok(())
	}

	fn call(
		source: H160,
		target: H160,
		input: Vec<u8>,
		value: U256,
		gas_limit: u64,
		max_fee_per_gas: Option<U256>,
		max_priority_fee_per_gas: Option<U256>,
		nonce: Option<U256>,
		access_list: Vec<(H160, Vec<H256>)>,
		is_transactional: bool,
		validate: bool,
		weight_limit: Option<Weight>,
		proof_size_base_cost: Option<u64>,
		config: &evm::Config,
	) -> Result<CallInfo, RunnerError<Self::Error>> {
		if validate {
			Self::validate(
				source,
				Some(target),
				input.clone(),
				value,
				gas_limit,
				max_fee_per_gas,
				max_priority_fee_per_gas,
				nonce,
				access_list.clone(),
				is_transactional,
				weight_limit,
				proof_size_base_cost,
				config,
			)?;
		}
		let precompiles = T::PrecompilesValue::get();
		Self::execute(
			source,
			value,
			gas_limit,
			max_fee_per_gas,
			max_priority_fee_per_gas,
			config,
			&precompiles,
			is_transactional,
			weight_limit,
			proof_size_base_cost,
			|executor| executor.transact_call(source, target, value, input, gas_limit, access_list),
		)
	}

	fn create(
		source: H160,
		init: Vec<u8>,
		value: U256,
		gas_limit: u64,
		max_fee_per_gas: Option<U256>,
		max_priority_fee_per_gas: Option<U256>,
		nonce: Option<U256>,
		access_list: Vec<(H160, Vec<H256>)>,
		is_transactional: bool,
		validate: bool,
		weight_limit: Option<Weight>,
		proof_size_base_cost: Option<u64>,
		config: &evm::Config,
	) -> Result<CreateInfo, RunnerError<Self::Error>> {
		if validate {
			Self::validate(
				source,
				None,
				init.clone(),
				value,
				gas_limit,
				max_fee_per_gas,
				max_priority_fee_per_gas,
				nonce,
				access_list.clone(),
				is_transactional,
				weight_limit,
				proof_size_base_cost,
				config,
			)?;
		}
		let precompiles = T::PrecompilesValue::get();
		Self::execute(
			source,
			value,
			gas_limit,
			max_fee_per_gas,
			max_priority_fee_per_gas,
			config,
			&precompiles,
			is_transactional,
			weight_limit,
			proof_size_base_cost,
			|executor| {
				let address = executor.create_address(evm::CreateScheme::Legacy { caller: source });
				T::OnCreate::on_create(source, address);
				let (reason, _) =
					executor.transact_create(source, value, init, gas_limit, access_list);
				(reason, address)
			},
		)
	}

	fn create2(
		source: H160,
		init: Vec<u8>,
		salt: H256,
		value: U256,
		gas_limit: u64,
		max_fee_per_gas: Option<U256>,
		max_priority_fee_per_gas: Option<U256>,
		nonce: Option<U256>,
		access_list: Vec<(H160, Vec<H256>)>,
		is_transactional: bool,
		validate: bool,
		weight_limit: Option<Weight>,
		proof_size_base_cost: Option<u64>,
		config: &evm::Config,
	) -> Result<CreateInfo, RunnerError<Self::Error>> {
		if validate {
			Self::validate(
				source,
				None,
				init.clone(),
				value,
				gas_limit,
				max_fee_per_gas,
				max_priority_fee_per_gas,
				nonce,
				access_list.clone(),
				is_transactional,
				weight_limit,
				proof_size_base_cost,
				config,
			)?;
		}
		let precompiles = T::PrecompilesValue::get();
		let code_hash = H256::from(sp_io::hashing::keccak_256(&init));
		Self::execute(
			source,
			value,
			gas_limit,
			max_fee_per_gas,
			max_priority_fee_per_gas,
			config,
			&precompiles,
			is_transactional,
			weight_limit,
			proof_size_base_cost,
			|executor| {
				let address = executor.create_address(evm::CreateScheme::Create2 {
					caller: source,
					code_hash,
					salt,
				});
				T::OnCreate::on_create(source, address);
				let (reason, _) =
					executor.transact_create2(source, value, init, salt, gas_limit, access_list);
				(reason, address)
			},
		)
	}
}

struct SubstrateStackSubstate<'config> {
	metadata: StackSubstateMetadata<'config>,
	deletes: BTreeSet<H160>,
	logs: Vec<Log>,
	parent: Option<Box<SubstrateStackSubstate<'config>>>,
}

impl<'config> SubstrateStackSubstate<'config> {
	pub fn metadata(&self) -> &StackSubstateMetadata<'config> {
		&self.metadata
	}

	pub fn metadata_mut(&mut self) -> &mut StackSubstateMetadata<'config> {
		&mut self.metadata
	}

	pub fn enter(&mut self, gas_limit: u64, is_static: bool) {
		let mut entering = Self {
			metadata: self.metadata.spit_child(gas_limit, is_static),
			parent: None,
			deletes: BTreeSet::new(),
			logs: Vec::new(),
		};
		mem::swap(&mut entering, self);

		self.parent = Some(Box::new(entering));

		sp_io::storage::start_transaction();
	}

	pub fn exit_commit(&mut self) -> Result<(), ExitError> {
		let mut exited = *self.parent.take().expect("Cannot commit on root substate");
		mem::swap(&mut exited, self);

		self.metadata.swallow_commit(exited.metadata)?;
		self.logs.append(&mut exited.logs);
		self.deletes.append(&mut exited.deletes);

		sp_io::storage::commit_transaction();
		Ok(())
	}

	pub fn exit_revert(&mut self) -> Result<(), ExitError> {
		let mut exited = *self.parent.take().expect("Cannot discard on root substate");
		mem::swap(&mut exited, self);
		self.metadata.swallow_revert(exited.metadata)?;

		sp_io::storage::rollback_transaction();
		Ok(())
	}

	pub fn exit_discard(&mut self) -> Result<(), ExitError> {
		let mut exited = *self.parent.take().expect("Cannot discard on root substate");
		mem::swap(&mut exited, self);
		self.metadata.swallow_discard(exited.metadata)?;

		sp_io::storage::rollback_transaction();
		Ok(())
	}

	pub fn deleted(&self, address: H160) -> bool {
		if self.deletes.contains(&address) {
			return true;
		}

		if let Some(parent) = self.parent.as_ref() {
			return parent.deleted(address);
		}

		false
	}

	pub fn set_deleted(&mut self, address: H160) {
		self.deletes.insert(address);
	}

	pub fn log(&mut self, address: H160, topics: Vec<H256>, data: Vec<u8>) {
		self.logs.push(Log {
			address,
			topics,
			data,
		});
	}

	fn recursive_is_cold<F: Fn(&Accessed) -> bool>(&self, f: &F) -> bool {
		let local_is_accessed = self.metadata.accessed().as_ref().map(f).unwrap_or(false);
		if local_is_accessed {
			false
		} else {
			self.parent
				.as_ref()
				.map(|p| p.recursive_is_cold(f))
				.unwrap_or(true)
		}
	}
}

#[derive(Default, Clone, Eq, PartialEq)]
pub struct Recorded {
	account_codes: Vec<H160>,
	account_storages: BTreeMap<(H160, H256), bool>,
}

/// Substrate backend for EVM.
pub struct SubstrateStackState<'vicinity, 'config, T> {
	vicinity: &'vicinity Vicinity,
	substate: SubstrateStackSubstate<'config>,
	original_storage: BTreeMap<(H160, H256), H256>,
	recorded: Recorded,
	weight_info: Option<WeightInfo>,
	_marker: PhantomData<T>,
}

impl<'vicinity, 'config, T: Config> SubstrateStackState<'vicinity, 'config, T> {
	/// Create a new backend with given vicinity.
	pub fn new(
		vicinity: &'vicinity Vicinity,
		metadata: StackSubstateMetadata<'config>,
		weight_info: Option<WeightInfo>,
	) -> Self {
		Self {
			vicinity,
			substate: SubstrateStackSubstate {
				metadata,
				deletes: BTreeSet::new(),
				logs: Vec::new(),
				parent: None,
			},
			_marker: PhantomData,
			original_storage: BTreeMap::new(),
			recorded: Default::default(),
			weight_info,
		}
	}

	pub fn weight_info(&self) -> Option<WeightInfo> {
		self.weight_info
	}

	pub fn recorded(&self) -> &Recorded {
		&self.recorded
	}

	pub fn info_mut(&mut self) -> (&mut Option<WeightInfo>, &mut Recorded) {
		(&mut self.weight_info, &mut self.recorded)
	}
}

impl<'vicinity, 'config, T: Config> BackendT for SubstrateStackState<'vicinity, 'config, T>
where
	BalanceOf<T>: TryFrom<U256> + Into<U256>,
{
	fn gas_price(&self) -> U256 {
		self.vicinity.gas_price
	}
	fn origin(&self) -> H160 {
		self.vicinity.origin
	}

	fn block_hash(&self, number: U256) -> H256 {
		if number > U256::from(u32::MAX) {
			H256::default()
		} else {
			T::BlockHashMapping::block_hash(number.as_u32())
		}
	}

	fn block_number(&self) -> U256 {
		let number: u128 = frame_system::Pallet::<T>::block_number().unique_saturated_into();
		U256::from(number)
	}

	fn block_coinbase(&self) -> H160 {
		Pallet::<T>::find_author()
	}

	fn block_timestamp(&self) -> U256 {
		let now: u128 = T::Timestamp::now().unique_saturated_into();
		U256::from(now / 1000)
	}

	fn block_difficulty(&self) -> U256 {
		U256::zero()
	}

	fn block_randomness(&self) -> Option<H256> {
		None
	}

	fn block_gas_limit(&self) -> U256 {
		T::BlockGasLimit::get()
	}

	fn block_base_fee_per_gas(&self) -> U256 {
		let (base_fee, _) = T::FeeCalculator::min_gas_price();
		base_fee
	}

	fn chain_id(&self) -> U256 {
		U256::from(T::ChainId::get())
	}

	fn exists(&self, _address: H160) -> bool {
		true
	}

	fn basic(&self, address: H160) -> evm::backend::Basic {
		let (account, _) = Pallet::<T>::account_basic(&address);

		evm::backend::Basic {
			balance: account.balance,
			nonce: account.nonce,
		}
	}

	fn code(&self, address: H160) -> Vec<u8> {
		<AccountCodes<T>>::get(address)
	}

	fn storage(&self, address: H160, index: H256) -> H256 {
		<AccountStorages<T>>::get(address, index)
	}

	fn original_storage(&self, address: H160, index: H256) -> Option<H256> {
		Some(
			self.original_storage
				.get(&(address, index))
				.cloned()
				.unwrap_or_else(|| self.storage(address, index)),
		)
	}
}

impl<'vicinity, 'config, T: Config> StackStateT<'config>
	for SubstrateStackState<'vicinity, 'config, T>
where
	BalanceOf<T>: TryFrom<U256> + Into<U256>,
{
	fn metadata(&self) -> &StackSubstateMetadata<'config> {
		self.substate.metadata()
	}

	fn metadata_mut(&mut self) -> &mut StackSubstateMetadata<'config> {
		self.substate.metadata_mut()
	}

	fn enter(&mut self, gas_limit: u64, is_static: bool) {
		self.substate.enter(gas_limit, is_static)
	}

	fn exit_commit(&mut self) -> Result<(), ExitError> {
		self.substate.exit_commit()
	}

	fn exit_revert(&mut self) -> Result<(), ExitError> {
		self.substate.exit_revert()
	}

	fn exit_discard(&mut self) -> Result<(), ExitError> {
		self.substate.exit_discard()
	}

	fn is_empty(&self, address: H160) -> bool {
		Pallet::<T>::is_account_empty(&address)
	}

	fn deleted(&self, address: H160) -> bool {
		self.substate.deleted(address)
	}

	fn inc_nonce(&mut self, address: H160) -> Result<(), ExitError> {
		let account_id = T::AddressMapping::into_account_id(address);
		frame_system::Pallet::<T>::inc_account_nonce(&account_id);
		Ok(())
	}

	fn set_storage(&mut self, address: H160, index: H256, value: H256) {
		// We cache the current value if this is the first time we modify it
		// in the transaction.
		use sp_std::collections::btree_map::Entry::Vacant;
		if let Vacant(e) = self.original_storage.entry((address, index)) {
			let original = <AccountStorages<T>>::get(address, index);
			// No need to cache if same value.
			if original != value {
				e.insert(original);
			}
		}

		// Then we insert or remove the entry based on the value.
		if value == H256::default() {
			log::debug!(
				target: "evm",
				"Removing storage for {:?} [index: {:?}]",
				address,
				index,
			);
			<AccountStorages<T>>::remove(address, index);
		} else {
			log::debug!(
				target: "evm",
				"Updating storage for {:?} [index: {:?}, value: {:?}]",
				address,
				index,
				value,
			);
			<AccountStorages<T>>::insert(address, index, value);
		}
	}

	fn reset_storage(&mut self, address: H160) {
		#[allow(deprecated)]
		let _ = <AccountStorages<T>>::remove_prefix(address, None);
	}

	fn log(&mut self, address: H160, topics: Vec<H256>, data: Vec<u8>) {
		self.substate.log(address, topics, data)
	}

	fn set_deleted(&mut self, address: H160) {
		self.substate.set_deleted(address)
	}

	fn set_code(&mut self, address: H160, code: Vec<u8>) {
		log::debug!(
			target: "evm",
			"Inserting code ({} bytes) at {:?}",
			code.len(),
			address
		);
		Pallet::<T>::create_account(address, code);
	}

	fn transfer(&mut self, transfer: Transfer) -> Result<(), ExitError> {

		let source = T::AddressMapping::into_account_id(transfer.source);
		let target = T::AddressMapping::into_account_id(transfer.target);


		// log::info!("source {:#?}", source);
		// log::info!("target {:#?}", target);


		T::Currency::transfer(
			&source,
			&target,
			transfer
				.value
				.try_into()
				.map_err(|_| ExitError::OutOfFund)?,
			ExistenceRequirement::AllowDeath,
		)
		.map_err(|_| ExitError::OutOfFund)
	}

	fn reset_balance(&mut self, _address: H160) {
		// Do nothing on reset balance in Substrate.
		//
		// This function exists in EVM because a design issue
		// (arguably a bug) in SELFDESTRUCT that can cause total
		// issuance to be reduced. We do not need to replicate this.
	}

	fn touch(&mut self, _address: H160) {
		// Do nothing on touch in Substrate.
		//
		// EVM pallet considers all accounts to exist, and distinguish
		// only empty and non-empty accounts. This avoids many of the
		// subtle issues in EIP-161.
	}

	fn is_cold(&self, address: H160) -> bool {
		self.substate
			.recursive_is_cold(&|a| a.accessed_addresses.contains(&address))
	}

	fn is_storage_cold(&self, address: H160, key: H256) -> bool {
		self.substate
			.recursive_is_cold(&|a: &Accessed| a.accessed_storage.contains(&(address, key)))
	}

	fn code_size(&self, address: H160) -> U256 {
		U256::from(<Pallet<T>>::account_code_metadata(address).size)
	}

	fn code_hash(&self, address: H160) -> H256 {
		<Pallet<T>>::account_code_metadata(address).hash
	}

	fn record_external_operation(&mut self, op: evm::ExternalOperation) -> Result<(), ExitError> {
		let size_limit: u64 = self
			.metadata()
			.gasometer()
			.config()
			.create_contract_limit
			.unwrap_or_default() as u64;

		let (weight_info, recorded) = self.info_mut();

		if let Some(weight_info) = weight_info {
			match op {
				evm::ExternalOperation::AccountBasicRead => {
					weight_info.try_record_proof_size_or_fail(ACCOUNT_BASIC_PROOF_SIZE)?
				}
				evm::ExternalOperation::AddressCodeRead(address) => {
					let maybe_record = !recorded.account_codes.contains(&address);
					// Skip if the address has been already recorded this block
					if maybe_record {
						// First we record account emptiness check.
						// Transfers to EOAs with standard 21_000 gas limit are able to
						// pay for this pov size.
						weight_info.try_record_proof_size_or_fail(IS_EMPTY_CHECK_PROOF_SIZE)?;

						if <AccountCodes<T>>::decode_len(address).unwrap_or(0) == 0 {
							return Ok(());
						}
						// Try to record fixed sized `AccountCodesMetadata` read
						// Tentatively 16 + 20 + 40
						weight_info
							.try_record_proof_size_or_fail(ACCOUNT_CODES_METADATA_PROOF_SIZE)?;
						if let Some(meta) = <AccountCodesMetadata<T>>::get(address) {
							weight_info.try_record_proof_size_or_fail(meta.size)?;
						} else {
							// If it does not exist, try to record `create_contract_limit` first.
							weight_info.try_record_proof_size_or_fail(size_limit)?;
							let meta = Pallet::<T>::account_code_metadata(address);
							let actual_size = meta.size;
							// Refund if applies
							weight_info.refund_proof_size(size_limit.saturating_sub(actual_size));
						}
						recorded.account_codes.push(address);
					}
				}
				evm::ExternalOperation::IsEmpty => {
					weight_info.try_record_proof_size_or_fail(IS_EMPTY_CHECK_PROOF_SIZE)?
				}
				evm::ExternalOperation::Write => {
					weight_info.try_record_proof_size_or_fail(WRITE_PROOF_SIZE)?
				}
			};
		}
		Ok(())
	}

	fn record_external_dynamic_opcode_cost(
		&mut self,
		opcode: Opcode,
		_gas_cost: GasCost,
		target: evm::gasometer::StorageTarget,
	) -> Result<(), ExitError> {
		// If account code or storage slot is in the overlay it is already accounted for and early exit
		let mut accessed_storage: Option<AccessedStorage> = match target {
			StorageTarget::Address(address) => {
				if self.recorded().account_codes.contains(&address) {
					return Ok(());
				} else {
					Some(AccessedStorage::AccountCodes(address))
				}
			}
			StorageTarget::Slot(address, index) => {
				if self
					.recorded()
					.account_storages
					.contains_key(&(address, index))
				{
					return Ok(());
				} else {
					Some(AccessedStorage::AccountStorages((address, index)))
				}
			}
			_ => None,
		};

		let size_limit: u64 = self
			.metadata()
			.gasometer()
			.config()
			.create_contract_limit
			.unwrap_or_default() as u64;

		let (weight_info, recorded) = {
			let (weight_info, recorded) = self.info_mut();
			if let Some(weight_info) = weight_info {
				(weight_info, recorded)
			} else {
				return Ok(());
			}
		};

		// Record ref_time first
		// TODO benchmark opcodes, until this is done we do used_gas to weight conversion for ref_time

		// Record proof_size
		// Return if proof size recording is disabled
		let proof_size_limit = if let Some(proof_size_limit) = weight_info.proof_size_limit {
			proof_size_limit
		} else {
			return Ok(());
		};

		let mut maybe_record_and_refund = |with_empty_check: bool| -> Result<(), ExitError> {
			let address = if let Some(AccessedStorage::AccountCodes(address)) = accessed_storage {
				address
			} else {
				// This must be unreachable, a valid target must be set.
				// TODO decide how do we want to gracefully handle.
				return Err(ExitError::OutOfGas);
			};
			// First try to record fixed sized `AccountCodesMetadata` read
			// Tentatively 20 + 8 + 32
			let mut base_cost = ACCOUNT_CODES_METADATA_PROOF_SIZE;
			if with_empty_check {
				base_cost = base_cost.saturating_add(IS_EMPTY_CHECK_PROOF_SIZE);
			}
			weight_info.try_record_proof_size_or_fail(base_cost)?;
			if let Some(meta) = <AccountCodesMetadata<T>>::get(address) {
				weight_info.try_record_proof_size_or_fail(meta.size)?;
			} else {
				// If it does not exist, try to record `create_contract_limit` first.
				weight_info.try_record_proof_size_or_fail(size_limit)?;
				let meta = Pallet::<T>::account_code_metadata(address);
				let actual_size = meta.size;
				// Refund if applies
				weight_info.refund_proof_size(size_limit.saturating_sub(actual_size));
			}
			recorded.account_codes.push(address);
			// Already recorded, return
			Ok(())
		};

		// Proof size is fixed length for writes (a 32-byte hash in a merkle trie), and
		// the full key/value for reads. For read and writes over the same storage, the full value
		// is included.
		// For cold reads involving code (call, callcode, staticcall and delegatecall):
		//	- We depend on https://github.com/paritytech/frontier/pull/893
		//	- Try to get the cached size or compute it on the fly
		//	- We record the actual size after caching, refunding the difference between it and the initially deducted
		//	contract size limit.
		let opcode_proof_size = match opcode {
			// Basic account fixed length
			Opcode::BALANCE => {
				accessed_storage = None;
				U256::from(ACCOUNT_BASIC_PROOF_SIZE)
			}
			Opcode::EXTCODESIZE | Opcode::EXTCODECOPY | Opcode::EXTCODEHASH => {
				return maybe_record_and_refund(false)
			}
			Opcode::CALLCODE | Opcode::CALL | Opcode::DELEGATECALL | Opcode::STATICCALL => {
				return maybe_record_and_refund(true)
			}
			// (H160, H256) double map blake2 128 concat key size (68) + value 32
			Opcode::SLOAD => U256::from(ACCOUNT_STORAGE_PROOF_SIZE),
			Opcode::SSTORE => {
				let (address, index) =
					if let Some(AccessedStorage::AccountStorages((address, index))) =
						accessed_storage
					{
						(address, index)
					} else {
						// This must be unreachable, a valid target must be set.
						// TODO decide how do we want to gracefully handle.
						return Err(ExitError::OutOfGas);
					};
				let mut cost = WRITE_PROOF_SIZE;
				let maybe_record = !recorded.account_storages.contains_key(&(address, index));
				// If the slot is yet to be accessed we charge for it, as the evm reads
				// it prior to the opcode execution.
				// Skip if the address and index has been already recorded this block.
				if maybe_record {
					cost = cost.saturating_add(ACCOUNT_STORAGE_PROOF_SIZE);
				}
				U256::from(cost)
			}
			// Fixed trie 32 byte hash
			Opcode::CREATE | Opcode::CREATE2 => U256::from(WRITE_PROOF_SIZE),
			// When calling SUICIDE a target account will receive the self destructing
			// address's balance. We need to account for both:
			//	- Target basic account read
			//	- 5 bytes of `decode_len`
			Opcode::SUICIDE => {
				accessed_storage = None;
				U256::from(IS_EMPTY_CHECK_PROOF_SIZE)
			}
			// Rest of dynamic opcodes that do not involve proof size recording, do nothing
			_ => return Ok(()),
		};

		if opcode_proof_size > U256::from(u64::MAX) {
			weight_info.try_record_proof_size_or_fail(proof_size_limit)?;
			return Err(ExitError::OutOfGas);
		}

		// Cache the storage access
		match accessed_storage {
			Some(AccessedStorage::AccountStorages((address, index))) => {
				recorded.account_storages.insert((address, index), true);
			}
			Some(AccessedStorage::AccountCodes(address)) => {
				recorded.account_codes.push(address);
			}
			_ => {}
		}

		// Record cost
		self.record_external_cost(None, Some(opcode_proof_size.low_u64()))?;
		Ok(())
	}

	fn record_external_cost(
		&mut self,
		ref_time: Option<u64>,
		proof_size: Option<u64>,
	) -> Result<(), ExitError> {
		let weight_info = if let (Some(weight_info), _) = self.info_mut() {
			weight_info
		} else {
			return Ok(());
		};
		// Record ref_time first
		// TODO benchmark opcodes, until this is done we do used_gas to weight conversion for ref_time
		if let Some(amount) = ref_time {
			weight_info.try_record_ref_time_or_fail(amount)?;
		}
		if let Some(amount) = proof_size {
			weight_info.try_record_proof_size_or_fail(amount)?;
		}
		Ok(())
	}

	fn refund_external_cost(&mut self, ref_time: Option<u64>, proof_size: Option<u64>) {
		if let Some(mut weight_info) = self.weight_info {
			if let Some(amount) = ref_time {
				weight_info.refund_ref_time(amount);
			}
			if let Some(amount) = proof_size {
				weight_info.refund_proof_size(amount);
			}
		}
	}
}

#[cfg(feature = "forbid-evm-reentrancy")]
#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{MockPrecompileSet, Test};
	use evm::ExitSucceed;

	macro_rules! assert_matches {
		( $left:expr, $(|)? $( $pattern:pat_param )|+ $( if $guard: expr )? $(,)? ) => {
			match $left {
				$( $pattern )|+ $( if $guard )? => {}
				ref left_val => panic!("assertion failed: `{:?}` does not match `{}`",
					left_val, stringify!($($pattern)|+ $(if $guard)?))
			}
		};
		( $left:expr, $(|)? $( $pattern:pat_param )|+ $( if $guard: expr )?, $($arg:tt)+ ) => {
			match $left {
				$( $pattern )|+ $( if $guard )? => {}
				ref left_val => panic!("assertion failed: `{:?}` does not match `{}`",
					left_val, stringify!($($pattern)|+ $(if $guard)?))
			}
		};
	}

	#[test]
	fn test_evm_reentrancy() {
		let config = evm::Config::istanbul();

		// Should fail with the appropriate error if there is reentrancy
		let res = Runner::<Test>::execute(
			H160::default(),
			U256::default(),
			100_000,
			None,
			None,
			&config,
			&MockPrecompileSet,
			false,
			|_| {
				let res = Runner::<Test>::execute(
					H160::default(),
					U256::default(),
					100_000,
					None,
					None,
					&config,
					&MockPrecompileSet,
					false,
					|_| (ExitReason::Succeed(ExitSucceed::Stopped), ()),
				);
				assert_matches!(
					res,
					Err(RunnerError {
						error: Error::<Test>::Reentrancy,
						..
					})
				);
				(ExitReason::Error(ExitError::CallTooDeep), ())
			},
		);
		assert_matches!(
			res,
			Ok(ExecutionInfoV2 {
				exit_reason: ExitReason::Error(ExitError::CallTooDeep),
				..
			})
		);

		// Should succeed if there is no reentrancy
		let res = Runner::<Test>::execute(
			H160::default(),
			U256::default(),
			100_000,
			None,
			None,
			&config,
			&MockPrecompileSet,
			false,
			|_| (ExitReason::Succeed(ExitSucceed::Stopped), ()),
		);
		assert!(res.is_ok());
	}
}
