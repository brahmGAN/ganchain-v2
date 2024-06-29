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

// Copyright 2019-2022 PureStake Inc.
// Copyright 2022      Stake Technologies
// This file is part of AssetsERC20 package, originally developed by Purestake Inc.
// AssetsERC20 package used in Astar Network in terms of GPLv3.
//
// AssetsERC20 is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// AssetsERC20 is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with AssetsERC20.  If not, see <http://www.gnu.org/licenses/>.
use frame_support::assert_ok;
use std::str::from_utf8;

use crate::mock::*;
use crate::*;

use precompile_utils::{testing::*, EvmDataWriter, LogsBuilder};
use sha3::{Digest, Keccak256};

fn precompiles() -> Erc20AssetsPrecompileSet<Runtime> {
    PrecompilesValue::get()
}

#[test]
fn selector_less_than_four_bytes() {
    ExtBuilder::default().build().execute_with(|| {
        assert_ok!(Assets::force_create(
            RuntimeOrigin::root(),
            0u128,
            Account::Alice.into(),
            true,
            1
        ));
        // This selector is only three bytes long when four are required.
        precompiles()
            .prepare_test(Account::Alice, Account::AssetId(0u128), vec![1u8, 2u8, 3u8])
            .execute_reverts(|output| output == b"tried to parse selector out of bounds");
    });
}

#[test]
fn no_selector_exists_but_length_is_right() {
    ExtBuilder::default().build().execute_with(|| {
        assert_ok!(Assets::force_create(
            RuntimeOrigin::root(),
            0u128,
            Account::Alice.into(),
            true,
            1
        ));

        precompiles()
            .prepare_test(
                Account::Alice,
                Account::AssetId(0u128),
                vec![1u8, 2u8, 3u8, 4u8],
            )
            .execute_reverts(|output| output == b"unknown selector");
    });
}

#[test]
fn selectors() {
    assert_eq!(Action::BalanceOf as u32, 0x70a08231);
    assert_eq!(Action::TotalSupply as u32, 0x18160ddd);
    assert_eq!(Action::Approve as u32, 0x095ea7b3);
    assert_eq!(Action::Allowance as u32, 0xdd62ed3e);
    assert_eq!(Action::Transfer as u32, 0xa9059cbb);
    assert_eq!(Action::TransferFrom as u32, 0x23b872dd);
    assert_eq!(Action::Name as u32, 0x06fdde03);
    assert_eq!(Action::Symbol as u32, 0x95d89b41);
    assert_eq!(Action::Decimals as u32, 0x313ce567);
    assert_eq!(Action::MinimumBalance as u32, 0xb9d1d49b);
    assert_eq!(Action::Mint as u32, 0x40c10f19);
    assert_eq!(Action::Burn as u32, 0x9dc29fac);

    assert_eq!(
        crate::SELECTOR_LOG_TRANSFER,
        &Keccak256::digest(b"Transfer(address,address,uint256)")[..]
    );

    assert_eq!(
        crate::SELECTOR_LOG_APPROVAL,
        &Keccak256::digest(b"Approval(address,address,uint256)")[..]
    );
}

#[test]
fn get_total_supply() {
    ExtBuilder::default()
        .with_balances(vec![(Account::Alice, 1000), (Account::Bob, 2500)])
        .build()
        .execute_with(|| {
            assert_ok!(Assets::force_create(
                RuntimeOrigin::root(),
                0u128,
                Account::Alice.into(),
                true,
                1
            ));
            assert_ok!(Assets::mint(
                RuntimeOrigin::signed(Account::Alice),
                0u128,
                Account::Alice.into(),
                1000
            ));

            precompiles()
                .prepare_test(
                    Account::Alice,
                    Account::AssetId(0u128),
                    EvmDataWriter::new_with_selector(Action::TotalSupply).build(),
                )
                .expect_cost(0) // TODO: Test db read/write costs
                .expect_no_logs()
                .execute_returns(EvmDataWriter::new().write(U256::from(1000u64)).build());
        });
}

#[test]
fn get_balances_known_user() {
    ExtBuilder::default()
        .with_balances(vec![(Account::Alice, 1000)])
        .build()
        .execute_with(|| {
            assert_ok!(Assets::force_create(
                RuntimeOrigin::root(),
                0u128,
                Account::Alice.into(),
                true,
                1
            ));
            assert_ok!(Assets::mint(
                RuntimeOrigin::signed(Account::Alice),
                0u128,
                Account::Alice.into(),
                1000
            ));

            precompiles()
                .prepare_test(
                    Account::Alice,
                    Account::AssetId(0u128),
                    EvmDataWriter::new_with_selector(Action::BalanceOf)
                        .write(Address(Account::Alice.into()))
                        .build(),
                )
                .expect_cost(0) // TODO: Test db read/write costs
                .expect_no_logs()
                .execute_returns(EvmDataWriter::new().write(U256::from(1000u64)).build());
        });
}

#[test]
fn get_balances_unknown_user() {
    ExtBuilder::default()
        .with_balances(vec![(Account::Alice, 1000)])
        .build()
        .execute_with(|| {
            assert_ok!(Assets::force_create(
                RuntimeOrigin::root(),
                0u128,
                Account::Alice.into(),
                true,
                1
            ));

            precompiles()
                .prepare_test(
                    Account::Alice,
                    Account::AssetId(0u128),
                    EvmDataWriter::new_with_selector(Action::BalanceOf)
                        .write(Address(Account::Bob.into()))
                        .build(),
                )
                .expect_cost(0) // TODO: Test db read/write costs
                .expect_no_logs()
                .execute_returns(EvmDataWriter::new().write(U256::from(0u64)).build());
        });
}

#[test]
fn approve() {
    ExtBuilder::default()
        .with_balances(vec![(Account::Alice, 1000)])
        .build()
        .execute_with(|| {
            assert_ok!(Assets::force_create(
                RuntimeOrigin::root(),
                0u128,
                Account::Alice.into(),
                true,
                1
            ));
            assert_ok!(Assets::mint(
                RuntimeOrigin::signed(Account::Alice),
                0u128,
                Account::Alice.into(),
                1000
            ));

            precompiles()
                .prepare_test(
                    Account::Alice,
                    Account::AssetId(0u128),
                    EvmDataWriter::new_with_selector(Action::Approve)
                        .write(Address(Account::Bob.into()))
                        .write(U256::from(500))
                        .build(),
                )
                .expect_log(LogsBuilder::new(Account::AssetId(0u128).into()).log3(
                    SELECTOR_LOG_APPROVAL,
                    Account::Alice,
                    Account::Bob,
                    EvmDataWriter::new().write(U256::from(500)).build(),
                ))
                .execute_returns(EvmDataWriter::new().write(true).build());
        });
}

#[test]
fn approve_saturating() {
    ExtBuilder::default()
        .with_balances(vec![(Account::Alice, 1000)])
        .build()
        .execute_with(|| {
            assert_ok!(Assets::force_create(
                RuntimeOrigin::root(),
                0u128,
                Account::Alice.into(),
                true,
                1
            ));
            assert_ok!(Assets::mint(
                RuntimeOrigin::signed(Account::Alice),
                0u128,
                Account::Alice.into(),
                1000
            ));

            precompiles()
                .prepare_test(
                    Account::Alice,
                    Account::AssetId(0u128),
                    EvmDataWriter::new_with_selector(Action::Approve)
                        .write(Address(Account::Bob.into()))
                        .write(U256::MAX)
                        .build(),
                )
                .expect_log(LogsBuilder::new(Account::AssetId(0u128).into()).log3(
                    SELECTOR_LOG_APPROVAL,
                    Account::Alice,
                    Account::Bob,
                    EvmDataWriter::new().write(U256::MAX).build(),
                ))
                .execute_returns(EvmDataWriter::new().write(true).build());

            precompiles()
                .prepare_test(
                    Account::Alice,
                    Account::AssetId(0u128),
                    EvmDataWriter::new_with_selector(Action::Allowance)
                        .write(Address(Account::Alice.into()))
                        .write(Address(Account::Bob.into()))
                        .build(),
                )
                .expect_cost(0u64)
                .expect_no_logs()
                .execute_returns(EvmDataWriter::new().write(U256::from(u128::MAX)).build());
        });
}

#[test]
fn check_allowance_existing() {
    ExtBuilder::default()
        .with_balances(vec![(Account::Alice, 1000)])
        .build()
        .execute_with(|| {
            assert_ok!(Assets::force_create(
                RuntimeOrigin::root(),
                0u128,
                Account::Alice.into(),
                true,
                1
            ));
            assert_ok!(Assets::mint(
                RuntimeOrigin::signed(Account::Alice),
                0u128,
                Account::Alice.into(),
                1000
            ));

            precompiles()
                .prepare_test(
                    Account::Alice,
                    Account::AssetId(0u128),
                    EvmDataWriter::new_with_selector(Action::Approve)
                        .write(Address(Account::Bob.into()))
                        .write(U256::from(500))
                        .build(),
                )
                .execute_some();

            precompiles()
                .prepare_test(
                    Account::Alice,
                    Account::AssetId(0u128),
                    EvmDataWriter::new_with_selector(Action::Allowance)
                        .write(Address(Account::Alice.into()))
                        .write(Address(Account::Bob.into()))
                        .build(),
                )
                .expect_cost(0) // TODO: Test db read/write costs
                .expect_no_logs()
                .execute_returns(EvmDataWriter::new().write(U256::from(500u64)).build());
        });
}

#[test]
fn check_allowance_not_existing() {
    ExtBuilder::default()
        .with_balances(vec![(Account::Alice, 1000)])
        .build()
        .execute_with(|| {
            assert_ok!(Assets::force_create(
                RuntimeOrigin::root(),
                0u128,
                Account::Alice.into(),
                true,
                1
            ));

            precompiles()
                .prepare_test(
                    Account::Alice,
                    Account::AssetId(0u128),
                    EvmDataWriter::new_with_selector(Action::Allowance)
                        .write(Address(Account::Alice.into()))
                        .write(Address(Account::Bob.into()))
                        .build(),
                )
                .expect_cost(0) // TODO: Test db read/write costs
                .expect_no_logs()
                .execute_returns(EvmDataWriter::new().write(U256::from(0u64)).build());
        });
}

#[test]
fn transfer() {
    ExtBuilder::default()
        .with_balances(vec![(Account::Alice, 1000)])
        .build()
        .execute_with(|| {
            assert_ok!(Assets::force_create(
                RuntimeOrigin::root(),
                0u128,
                Account::Alice.into(),
                true,
                1
            ));
            assert_ok!(Assets::mint(
                RuntimeOrigin::signed(Account::Alice),
                0u128,
                Account::Alice.into(),
                1000
            ));

            precompiles()
                .prepare_test(
                    Account::Alice,
                    Account::AssetId(0u128),
                    EvmDataWriter::new_with_selector(Action::Transfer)
                        .write(Address(Account::Bob.into()))
                        .write(U256::from(400))
                        .build(),
                )
                .expect_log(LogsBuilder::new(Account::AssetId(0u128).into()).log3(
                    SELECTOR_LOG_TRANSFER,
                    Account::Alice,
                    Account::Bob,
                    EvmDataWriter::new().write(U256::from(400)).build(),
                ))
                .execute_returns(EvmDataWriter::new().write(true).build());

            precompiles()
                .prepare_test(
                    Account::Bob,
                    Account::AssetId(0u128),
                    EvmDataWriter::new_with_selector(Action::BalanceOf)
                        .write(Address(Account::Bob.into()))
                        .build(),
                )
                .expect_cost(0) // TODO: Test db read/write costs
                .expect_no_logs()
                .execute_returns(EvmDataWriter::new().write(U256::from(400)).build());

            precompiles()
                .prepare_test(
                    Account::Alice,
                    Account::AssetId(0u128),
                    EvmDataWriter::new_with_selector(Action::BalanceOf)
                        .write(Address(Account::Alice.into()))
                        .build(),
                )
                .expect_cost(0) // TODO: Test db read/write costs
                .expect_no_logs()
                .execute_returns(EvmDataWriter::new().write(U256::from(600)).build());
        });
}

#[test]
fn transfer_not_enough_founds() {
    ExtBuilder::default()
        .with_balances(vec![(Account::Alice, 1000)])
        .build()
        .execute_with(|| {
            assert_ok!(Assets::force_create(
                RuntimeOrigin::root(),
                0u128,
                Account::Alice.into(),
                true,
                1
            ));
            assert_ok!(Assets::mint(
                RuntimeOrigin::signed(Account::Alice),
                0u128,
                Account::Alice.into(),
                1
            ));

            precompiles()
                .prepare_test(
                    Account::Alice,
                    Account::AssetId(0u128),
                    EvmDataWriter::new_with_selector(Action::Transfer)
                        .write(Address(Account::Charlie.into()))
                        .write(U256::from(50))
                        .build(),
                )
                .execute_reverts(|output| {
                    from_utf8(&output)
                        .unwrap()
                        .contains("Dispatched call failed with error: DispatchErrorWithPostInfo")
                        && from_utf8(&output).unwrap().contains("BalanceLow")
                });
        });
}

#[test]
fn transfer_from() {
    ExtBuilder::default()
        .with_balances(vec![(Account::Alice, 1000)])
        .build()
        .execute_with(|| {
            assert_ok!(Assets::force_create(
                RuntimeOrigin::root(),
                0u128,
                Account::Alice.into(),
                true,
                1
            ));
            assert_ok!(Assets::mint(
                RuntimeOrigin::signed(Account::Alice),
                0u128,
                Account::Alice.into(),
                1000
            ));

            precompiles()
                .prepare_test(
                    Account::Alice,
                    Account::AssetId(0u128),
                    EvmDataWriter::new_with_selector(Action::Approve)
                        .write(Address(Account::Bob.into()))
                        .write(U256::from(500))
                        .build(),
                )
                .execute_some();

            precompiles()
                .prepare_test(
                    Account::Alice,
                    Account::AssetId(0u128),
                    EvmDataWriter::new_with_selector(Action::Approve)
                        .write(Address(Account::Bob.into()))
                        .write(U256::from(500))
                        .build(),
                )
                .execute_some();

            precompiles()
                .prepare_test(
                    Account::Bob, // Bob is the one sending transferFrom!
                    Account::AssetId(0u128),
                    EvmDataWriter::new_with_selector(Action::TransferFrom)
                        .write(Address(Account::Alice.into()))
                        .write(Address(Account::Charlie.into()))
                        .write(U256::from(400))
                        .build(),
                )
                .expect_log(LogsBuilder::new(Account::AssetId(0u128).into()).log3(
                    SELECTOR_LOG_TRANSFER,
                    Account::Alice,
                    Account::Charlie,
                    EvmDataWriter::new().write(U256::from(400)).build(),
                ))
                .execute_returns(EvmDataWriter::new().write(true).build());

            precompiles()
                .prepare_test(
                    Account::Alice,
                    Account::AssetId(0u128),
                    EvmDataWriter::new_with_selector(Action::BalanceOf)
                        .write(Address(Account::Alice.into()))
                        .build(),
                )
                .expect_cost(0) // TODO: Test db read/write costs
                .expect_no_logs()
                .execute_returns(EvmDataWriter::new().write(U256::from(600)).build());

            precompiles()
                .prepare_test(
                    Account::Bob,
                    Account::AssetId(0u128),
                    EvmDataWriter::new_with_selector(Action::BalanceOf)
                        .write(Address(Account::Bob.into()))
                        .build(),
                )
                .expect_cost(0) // TODO: Test db read/write costs
                .expect_no_logs()
                .execute_returns(EvmDataWriter::new().write(U256::from(0)).build());

            precompiles()
                .prepare_test(
                    Account::Charlie,
                    Account::AssetId(0u128),
                    EvmDataWriter::new_with_selector(Action::BalanceOf)
                        .write(Address(Account::Charlie.into()))
                        .build(),
                )
                .expect_cost(0) // TODO: Test db read/write costs
                .expect_no_logs()
                .execute_returns(EvmDataWriter::new().write(U256::from(400)).build());
        });
}

#[test]
fn transfer_from_non_incremental_approval() {
    ExtBuilder::default()
        .with_balances(vec![(Account::Alice, 1000)])
        .build()
        .execute_with(|| {
            assert_ok!(Assets::force_create(
                RuntimeOrigin::root(),
                0u128,
                Account::Alice.into(),
                true,
                1
            ));
            assert_ok!(Assets::mint(
                RuntimeOrigin::signed(Account::Alice),
                0u128,
                Account::Alice.into(),
                1000
            ));

            // We first approve 500
            precompiles()
                .prepare_test(
                    Account::Alice,
                    Account::AssetId(0u128),
                    EvmDataWriter::new_with_selector(Action::Approve)
                        .write(Address(Account::Bob.into()))
                        .write(U256::from(500))
                        .build(),
                )
                .expect_log(LogsBuilder::new(Account::AssetId(0u128).into()).log3(
                    SELECTOR_LOG_APPROVAL,
                    Account::Alice,
                    Account::Bob,
                    EvmDataWriter::new().write(U256::from(500)).build(),
                ))
                .execute_returns(EvmDataWriter::new().write(true).build());

            // We then approve 300. Non-incremental, so this is
            // the approved new value
            // Additionally, the gas used in this approval is higher because we
            // need to clear the previous one
            precompiles()
                .prepare_test(
                    Account::Alice,
                    Account::AssetId(0u128),
                    EvmDataWriter::new_with_selector(Action::Approve)
                        .write(Address(Account::Bob.into()))
                        .write(U256::from(300))
                        .build(),
                )
                .expect_log(LogsBuilder::new(Account::AssetId(0u128).into()).log3(
                    SELECTOR_LOG_APPROVAL,
                    Account::Alice,
                    Account::Bob,
                    EvmDataWriter::new().write(U256::from(300)).build(),
                ))
                .execute_returns(EvmDataWriter::new().write(true).build());

            // This should fail, as now the new approved quantity is 300
            precompiles()
                .prepare_test(
                    Account::Bob, // Bob is the one sending transferFrom!
                    Account::AssetId(0u128),
                    EvmDataWriter::new_with_selector(Action::TransferFrom)
                        .write(Address(Account::Alice.into()))
                        .write(Address(Account::Bob.into()))
                        .write(U256::from(500))
                        .build(),
                )
                .execute_reverts(|output| {
                    output
                        == b"Dispatched call failed with error: DispatchErrorWithPostInfo { \
					post_info: PostDispatchInfo { actual_weight: None, pays_fee: Pays::Yes }, \
					error: Module(ModuleError { index: 2, error: [10, 0, 0, 0], \
					message: Some(\"Unapproved\") }) }"
                });
        });
}

#[test]
fn transfer_from_above_allowance() {
    ExtBuilder::default()
        .with_balances(vec![(Account::Alice, 1000)])
        .build()
        .execute_with(|| {
            assert_ok!(Assets::force_create(
                RuntimeOrigin::root(),
                0u128,
                Account::Alice.into(),
                true,
                1
            ));
            assert_ok!(Assets::mint(
                RuntimeOrigin::signed(Account::Alice),
                0u128,
                Account::Alice.into(),
                1000
            ));

            precompiles()
                .prepare_test(
                    Account::Alice,
                    Account::AssetId(0u128),
                    EvmDataWriter::new_with_selector(Action::Approve)
                        .write(Address(Account::Bob.into()))
                        .write(U256::from(300))
                        .build(),
                )
                .execute_some();

            precompiles()
                .prepare_test(
                    Account::Bob, // Bob is the one sending transferFrom!
                    Account::AssetId(0u128),
                    EvmDataWriter::new_with_selector(Action::TransferFrom)
                        .write(Address(Account::Alice.into()))
                        .write(Address(Account::Bob.into()))
                        .write(U256::from(400))
                        .build(),
                )
                .execute_reverts(|output| {
                    output
                        == b"Dispatched call failed with error: DispatchErrorWithPostInfo { \
					post_info: PostDispatchInfo { actual_weight: None, pays_fee: Pays::Yes }, \
					error: Module(ModuleError { index: 2, error: [10, 0, 0, 0], \
					message: Some(\"Unapproved\") }) }"
                });
        });
}

#[test]
fn transfer_from_self() {
    ExtBuilder::default()
        .with_balances(vec![(Account::Alice, 1000)])
        .build()
        .execute_with(|| {
            assert_ok!(Assets::force_create(
                RuntimeOrigin::root(),
                0u128,
                Account::Alice.into(),
                true,
                1
            ));
            assert_ok!(Assets::mint(
                RuntimeOrigin::signed(Account::Alice),
                0u128,
                Account::Alice.into(),
                1000
            ));

            precompiles()
                .prepare_test(
                    Account::Alice, // Alice sending transferFrom herself, no need for allowance.
                    Account::AssetId(0u128),
                    EvmDataWriter::new_with_selector(Action::TransferFrom)
                        .write(Address(Account::Alice.into()))
                        .write(Address(Account::Bob.into()))
                        .write(U256::from(400))
                        .build(),
                )
                .expect_log(LogsBuilder::new(Account::AssetId(0u128).into()).log3(
                    SELECTOR_LOG_TRANSFER,
                    Account::Alice,
                    Account::Bob,
                    EvmDataWriter::new().write(U256::from(400)).build(),
                ))
                .execute_returns(EvmDataWriter::new().write(true).build());

            precompiles()
                .prepare_test(
                    Account::Alice,
                    Account::AssetId(0u128),
                    EvmDataWriter::new_with_selector(Action::BalanceOf)
                        .write(Address(Account::Alice.into()))
                        .build(),
                )
                .expect_cost(0) // TODO: Test db read/write costs
                .expect_no_logs()
                .execute_returns(EvmDataWriter::new().write(U256::from(600)).build());

            precompiles()
                .prepare_test(
                    Account::Alice,
                    Account::AssetId(0u128),
                    EvmDataWriter::new_with_selector(Action::BalanceOf)
                        .write(Address(Account::Bob.into()))
                        .build(),
                )
                .expect_cost(0) // TODO: Test db read/write costs
                .expect_no_logs()
                .execute_returns(EvmDataWriter::new().write(U256::from(400)).build());
        });
}

#[test]
fn get_metadata() {
    ExtBuilder::default()
        .with_balances(vec![(Account::Alice, 1000), (Account::Bob, 2500)])
        .build()
        .execute_with(|| {
            assert_ok!(Assets::force_create(
                RuntimeOrigin::root(),
                0u128,
                Account::Alice.into(),
                true,
                1
            ));
            assert_ok!(Assets::force_set_metadata(
                RuntimeOrigin::root(),
                0u128,
                b"TestToken".to_vec(),
                b"Test".to_vec(),
                12,
                false
            ));

            precompiles()
                .prepare_test(
                    Account::Alice,
                    Account::AssetId(0u128),
                    EvmDataWriter::new_with_selector(Action::Name).build(),
                )
                .expect_cost(0) // TODO: Test db read/write costs
                .expect_no_logs()
                .execute_returns(
                    EvmDataWriter::new()
                        .write::<Bytes>("TestToken".into())
                        .build(),
                );

            precompiles()
                .prepare_test(
                    Account::Alice,
                    Account::AssetId(0u128),
                    EvmDataWriter::new_with_selector(Action::Symbol).build(),
                )
                .expect_cost(0) // TODO: Test db read/write costs
                .expect_no_logs()
                .execute_returns(EvmDataWriter::new().write::<Bytes>("Test".into()).build());

            precompiles()
                .prepare_test(
                    Account::Alice,
                    Account::AssetId(0u128),
                    EvmDataWriter::new_with_selector(Action::Decimals).build(),
                )
                .expect_cost(0) // TODO: Test db read/write costs
                .expect_no_logs()
                .execute_returns(EvmDataWriter::new().write(12u8).build());
        });
}

#[test]
fn minimum_balance_is_right() {
    ExtBuilder::default().build().execute_with(|| {
        let expected_min_balance = 19;
        assert_ok!(Assets::force_create(
            RuntimeOrigin::root(),
            0u128,
            Account::Alice.into(),
            true,
            expected_min_balance,
        ));

        precompiles()
            .prepare_test(
                Account::Alice,
                Account::AssetId(0u128),
                EvmDataWriter::new_with_selector(Action::MinimumBalance).build(),
            )
            .expect_cost(0) // TODO: Test db read/write costs
            .expect_no_logs()
            .execute_returns(EvmDataWriter::new().write(expected_min_balance).build());
    });
}

#[test]
fn mint_is_ok() {
    ExtBuilder::default().build().execute_with(|| {
        let asset_id = 0;
        assert_ok!(Assets::force_create(
            RuntimeOrigin::root(),
            asset_id,
            Account::Alice.into(),
            true,
            1,
        ));

        // Sanity check, Bob should be without assets
        assert!(Assets::balance(asset_id, &Account::Bob.into()).is_zero());

        // Mint some assets for Bob
        let mint_amount = 7 * 11 * 19;
        precompiles()
            .prepare_test(
                Account::Alice,
                Account::AssetId(asset_id),
                EvmDataWriter::new_with_selector(Action::Mint)
                    .write(Address(Account::Bob.into()))
                    .write(U256::from(mint_amount))
                    .build(),
            )
            .expect_no_logs()
            .execute_returns(EvmDataWriter::new().write(true).build());

        // Ensure Bob's asset balance was increased
        assert_eq!(Assets::balance(asset_id, &Account::Bob.into()), mint_amount);
    });
}

#[test]
fn mint_non_admin_is_not_ok() {
    ExtBuilder::default().build().execute_with(|| {
        let asset_id = 0;
        assert_ok!(Assets::force_create(
            RuntimeOrigin::root(),
            asset_id,
            Account::Alice.into(),
            true,
            1,
        ));

        precompiles()
            .prepare_test(
                Account::Bob,
                Account::AssetId(asset_id),
                EvmDataWriter::new_with_selector(Action::Mint)
                    .write(Address(Account::Bob.into()))
                    .write(U256::from(42))
                    .build(),
            )
            .expect_no_logs()
            .execute_reverts(|output| from_utf8(&output).unwrap().contains("NoPermission"));
    });
}

#[test]
fn burn_is_ok() {
    ExtBuilder::default().build().execute_with(|| {
        let asset_id = 0;
        assert_ok!(Assets::force_create(
            RuntimeOrigin::root(),
            asset_id,
            Account::Alice.into(),
            true,
            1,
        ));

        // Issue some initial assets for Bob
        let init_amount = 123;
        assert_ok!(Assets::mint(
            RuntimeOrigin::signed(Account::Alice),
            asset_id,
            Account::Bob.into(),
            init_amount,
        ));
        assert_eq!(Assets::balance(asset_id, &Account::Bob.into()), init_amount);

        // Burn some assets from Bob
        let burn_amount = 19;
        precompiles()
            .prepare_test(
                Account::Alice,
                Account::AssetId(asset_id),
                EvmDataWriter::new_with_selector(Action::Burn)
                    .write(Address(Account::Bob.into()))
                    .write(U256::from(burn_amount))
                    .build(),
            )
            .expect_no_logs()
            .execute_returns(EvmDataWriter::new().write(true).build());

        // Ensure Bob's asset balance was decreased
        assert_eq!(
            Assets::balance(asset_id, &Account::Bob.into()),
            init_amount - burn_amount
        );
    });
}

#[test]
fn burn_non_admin_is_not_ok() {
    ExtBuilder::default().build().execute_with(|| {
        let asset_id = 0;
        assert_ok!(Assets::force_create(
            RuntimeOrigin::root(),
            asset_id,
            Account::Alice.into(),
            true,
            1,
        ));
        assert_ok!(Assets::mint(
            RuntimeOrigin::signed(Account::Alice),
            asset_id,
            Account::Bob.into(),
            1000000,
        ));

        precompiles()
            .prepare_test(
                Account::Bob,
                Account::AssetId(asset_id),
                EvmDataWriter::new_with_selector(Action::Burn)
                    .write(Address(Account::Bob.into()))
                    .write(U256::from(42))
                    .build(),
            )
            .expect_no_logs()
            .execute_reverts(|output| from_utf8(&output).unwrap().contains("NoPermission"));
    });
}
