use crate::{AccountId, AssetId};

use frame_support::ensure;
use sp_std::marker::PhantomData;

use pallet_assets::AssetsCallback;
use pallet_evm_precompile_assets_erc20::AddressToAssetId;

/// Revert opt code. It's inserted at the precompile addresses, to make them functional in EVM.
pub const EVM_REVERT_CODE: &[u8] = &[0x60, 0x00, 0x60, 0x00, 0xfd];

/// Handler for automatic revert code registration.
///
/// When an asset is created, it automatically becomes available to the EVM via an `ERC20-like` interface.
/// In order for the precompile to work, dedicated asset address needs to have the revert code registered, otherwise the call will fail.
///
/// It is important to note that if the dedicated asset EVM address is already taken, asset creation should fail.
/// After asset has been destroyed, it is also safe to remove the revert code and free the address for future usage.
pub struct EvmRevertCodeHandler<A, R>(PhantomData<(A, R)>);
impl<A, R> AssetsCallback<AssetId, AccountId> for EvmRevertCodeHandler<A, R>
where
	A: AddressToAssetId<AssetId>,
	R: pallet_evm::Config,
{
	fn created(id: &AssetId, _: &AccountId) -> Result<(), ()> {
		let address = A::asset_id_to_address(*id);
		// In case of collision, we need to cancel the asset creation.
		ensure!(!pallet_evm::AccountCodes::<R>::contains_key(&address), ());
		pallet_evm::AccountCodes::<R>::insert(address, EVM_REVERT_CODE.to_vec());
		Ok(())
	}

	fn destroyed(id: &AssetId) -> Result<(), ()> {
		let address = A::asset_id_to_address(*id);
		pallet_evm::AccountCodes::<R>::remove(address);
		Ok(())
	}
}

// #[cfg(feature = "runtime-benchmarks")]
// /// Benchmark helper for `pallet-assets`.
// pub struct AssetsBenchmarkHelper;
// impl<AssetIdParameter: From<u128>> pallet_assets::BenchmarkHelper<AssetIdParameter>
//     for AssetsBenchmarkHelper
// {
//     fn create_asset_id_parameter(id: u32) -> AssetIdParameter {
//         AssetId::from(id).into()
//     }
// }
