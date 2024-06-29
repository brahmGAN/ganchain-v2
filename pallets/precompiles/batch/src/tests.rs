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
// Copyright 2022 Stake Technologies
// This file is part of pallet-evm-precompile-batch package, originally developed by Purestake Inc.
// pallet-evm-precompile-batch package used in Astar Network in terms of GPLv3.
//
// pallet-evm-precompile-batch is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// pallet-evm-precompile-batch is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with pallet-evm-precompile-batch.  If not, see <http://www.gnu.org/licenses/>.

use crate::mock::{precompile_address, BatchPrecompileMock, ExtBuilder, PrecompilesValue, Runtime};
use crate::{log_subcall_failed, log_subcall_succeeded, Mode, *};
use fp_evm::ExitError;
use precompile_utils::{call_cost, testing::*, LogsBuilder};
use sp_core::{H256, U256};

fn precompiles() -> BatchPrecompileMock<Runtime> {
    PrecompilesValue::get()
}

fn costs() -> (u64, u64) {
    let return_log_cost = log_subcall_failed(precompile_address(), 0)
        .compute_cost()
        .unwrap();
    let call_cost =
        return_log_cost + call_cost(U256::one(), <Runtime as pallet_evm::Config>::config());
    (return_log_cost, call_cost)
}

#[test]
fn batch_some_empty() {
    ExtBuilder::default().build().execute_with(|| {
        precompiles()
            .prepare_test(
                Alice,
                precompile_address(),
                EvmDataWriter::new_with_selector(Action::BatchSome)
                    .write::<std::vec::Vec<Address>>(vec![])
                    .write::<std::vec::Vec<U256>>(vec![])
                    .write::<std::vec::Vec<Bytes>>(vec![])
                    .write::<std::vec::Vec<U256>>(vec![])
                    .build(),
            )
            .execute_returns(EvmDataWriter::new().write(true).build());
    })
}

#[test]
fn batch_some_until_failure_empty() {
    ExtBuilder::default().build().execute_with(|| {
        precompiles()
            .prepare_test(
                Alice,
                precompile_address(),
                EvmDataWriter::new_with_selector(Action::BatchSomeUntilFailure)
                    .write::<std::vec::Vec<Address>>(vec![])
                    .write::<std::vec::Vec<U256>>(vec![])
                    .write::<std::vec::Vec<Bytes>>(vec![])
                    .write::<std::vec::Vec<U256>>(vec![])
                    .build(),
            )
            .execute_returns(EvmDataWriter::new().write(true).build());
    })
}

#[test]
fn batch_all_empty() {
    ExtBuilder::default().build().execute_with(|| {
        precompiles()
            .prepare_test(
                Alice,
                precompile_address(),
                EvmDataWriter::new_with_selector(Action::BatchAll)
                    .write::<std::vec::Vec<Address>>(vec![])
                    .write::<std::vec::Vec<U256>>(vec![])
                    .write::<std::vec::Vec<Bytes>>(vec![])
                    .write::<std::vec::Vec<U256>>(vec![])
                    .build(),
            )
            .execute_returns(EvmDataWriter::new().write(true).build());
    })
}

fn check_mode(mode: Mode) -> Action {
    match mode {
        Mode::BatchAll => Action::BatchAll,
        Mode::BatchSome => Action::BatchSome,
        Mode::BatchSomeUntilFailure => Action::BatchSomeUntilFailure,
    }
}

fn batch_returns(
    precompiles: &BatchPrecompileMock<Runtime>,
    mode: Mode,
) -> PrecompilesTester<BatchPrecompileMock<Runtime>> {
    let mut counter = 0;
    let one = b"one";
    let two = b"two";
    let (_, total_call_cost) = costs();

    precompiles
        .prepare_test(
            Alice,
            precompile_address(),
            EvmDataWriter::new_with_selector(check_mode(mode))
                .write(vec![Address(Bob.into()), Address(Charlie.into())])
                .write(vec![U256::from(1u8), U256::from(2u8)])
                .write(vec![Bytes::from(&one[..]), Bytes::from(&two[..])])
                .write::<std::vec::Vec<U256>>(vec![])
                .build(),
        )
        .with_target_gas(Some(100_000))
        .with_subcall_handle(move |subcall| {
            let Subcall {
                address,
                transfer,
                input,
                target_gas,
                is_static,
                context,
            } = subcall;

            // Called from the precompile caller.
            assert_eq!(context.caller, Alice.into());
            assert_eq!(is_static, false);

            match address {
                a if a == Bob.into() => {
                    assert_eq!(counter, 0, "this is the first call");
                    counter += 1;

                    assert_eq!(
                        target_gas,
                        Some(100_000 - total_call_cost),
                        "batch forward all gas"
                    );
                    let transfer = transfer.expect("there is a transfer");
                    assert_eq!(transfer.source, Alice.into());
                    assert_eq!(transfer.target, Bob.into());
                    assert_eq!(transfer.value, 1u8.into());

                    assert_eq!(context.address, Bob.into());
                    assert_eq!(context.apparent_value, 1u8.into());

                    assert_eq!(&input, b"one");

                    SubcallOutput {
                        cost: 13,
                        logs: vec![
                            LogsBuilder::new(Bob.into()).log1(H256::repeat_byte(0x11), vec![])
                        ],
                        ..SubcallOutput::succeed()
                    }
                }
                a if a == Charlie.into() => {
                    assert_eq!(counter, 1, "this is the second call");
                    counter += 1;

                    assert_eq!(
                        target_gas,
                        Some(100_000 - 13 - total_call_cost * 2),
                        "batch forward all gas"
                    );
                    let transfer = transfer.expect("there is a transfer");
                    assert_eq!(transfer.source, Alice.into());
                    assert_eq!(transfer.target, Charlie.into());
                    assert_eq!(transfer.value, 2u8.into());

                    assert_eq!(context.address, Charlie.into());
                    assert_eq!(context.apparent_value, 2u8.into());

                    assert_eq!(&input, b"two");

                    SubcallOutput {
                        cost: 17,
                        logs: vec![
                            LogsBuilder::new(Charlie.into()).log1(H256::repeat_byte(0x22), vec![])
                        ],
                        ..SubcallOutput::succeed()
                    }
                }
                _ => panic!("unexpected subcall"),
            }
        })
        .expect_cost(13 + 17 + total_call_cost * 2)
}

#[test]
fn batch_some_returns() {
    ExtBuilder::default().build().execute_with(|| {
        batch_returns(&precompiles(), Mode::BatchSome)
            .expect_log(LogsBuilder::new(Bob.into()).log1(H256::repeat_byte(0x11), vec![]))
            .expect_log(log_subcall_succeeded(precompile_address(), 0))
            .expect_log(LogsBuilder::new(Charlie.into()).log1(H256::repeat_byte(0x22), vec![]))
            .expect_log(log_subcall_succeeded(precompile_address(), 1))
            .execute_returns(EvmDataWriter::new().write(true).build())
    })
}

#[test]
fn batch_some_until_failure_returns() {
    ExtBuilder::default().build().execute_with(|| {
        batch_returns(&precompiles(), Mode::BatchSomeUntilFailure)
            .expect_log(LogsBuilder::new(Bob.into()).log1(H256::repeat_byte(0x11), vec![]))
            .expect_log(log_subcall_succeeded(precompile_address(), 0))
            .expect_log(LogsBuilder::new(Charlie.into()).log1(H256::repeat_byte(0x22), vec![]))
            .expect_log(log_subcall_succeeded(precompile_address(), 1))
            .execute_returns(EvmDataWriter::new().write(true).build())
    })
}

#[test]
fn batch_all_returns() {
    ExtBuilder::default().build().execute_with(|| {
        batch_returns(&precompiles(), Mode::BatchAll)
            .expect_log(LogsBuilder::new(Bob.into()).log1(H256::repeat_byte(0x11), vec![]))
            .expect_log(log_subcall_succeeded(precompile_address(), 0))
            .expect_log(LogsBuilder::new(Charlie.into()).log1(H256::repeat_byte(0x22), vec![]))
            .expect_log(log_subcall_succeeded(precompile_address(), 1))
            .execute_returns(EvmDataWriter::new().write(true).build())
    })
}

fn batch_out_of_gas(
    precompiles: &BatchPrecompileMock<Runtime>,
    mode: Mode,
) -> PrecompilesTester<BatchPrecompileMock<Runtime>> {
    let one = b"one";
    let (_, total_call_cost) = costs();
    precompiles
        .prepare_test(
            Alice,
            precompile_address(),
            EvmDataWriter::new_with_selector(check_mode(mode))
                .write(vec![Address(Bob.into())])
                .write(vec![U256::from(1u8)])
                .write(vec![Bytes::from(&one[..])])
                .write::<std::vec::Vec<U256>>(vec![])
                .build(),
        )
        .with_target_gas(Some(50_000))
        .with_subcall_handle(move |subcall| {
            let Subcall {
                address,
                transfer,
                input,
                target_gas,
                is_static,
                context,
            } = subcall;

            // Called from the precompile caller.
            assert_eq!(context.caller, Alice.into());
            assert_eq!(is_static, false);

            match address {
                a if a == Bob.into() => {
                    assert_eq!(
                        target_gas,
                        Some(50_000 - total_call_cost),
                        "batch forward all gas"
                    );
                    let transfer = transfer.expect("there is a transfer");
                    assert_eq!(transfer.source, Alice.into());
                    assert_eq!(transfer.target, Bob.into());
                    assert_eq!(transfer.value, 1u8.into());

                    assert_eq!(context.address, Bob.into());
                    assert_eq!(context.apparent_value, 1u8.into());

                    assert_eq!(&input, b"one");

                    SubcallOutput {
                        cost: 11_000,
                        ..SubcallOutput::out_of_gas()
                    }
                }
                _ => panic!("unexpected subcall"),
            }
        })
}

#[test]
fn batch_some_out_of_gas() {
    ExtBuilder::default().build().execute_with(|| {
        batch_out_of_gas(&precompiles(), Mode::BatchSome)
            .expect_log(log_subcall_failed(precompile_address(), 0))
            .execute_returns(EvmDataWriter::new().write(true).build())
    })
}

#[test]
fn batch_some_until_failure_out_of_gas() {
    ExtBuilder::default().build().execute_with(|| {
        batch_out_of_gas(&precompiles(), Mode::BatchSomeUntilFailure)
            .expect_log(log_subcall_failed(precompile_address(), 0))
            .execute_returns(EvmDataWriter::new().write(true).build())
    })
}

#[test]
fn batch_all_out_of_gas() {
    ExtBuilder::default().build().execute_with(|| {
        batch_out_of_gas(&precompiles(), Mode::BatchAll).execute_error(ExitError::OutOfGas)
    })
}

fn batch_incomplete(
    precompiles: &BatchPrecompileMock<Runtime>,
    mode: Mode,
) -> PrecompilesTester<BatchPrecompileMock<Runtime>> {
    let mut counter = 0;
    let one = b"one";

    let (_, total_call_cost) = costs();

    precompiles
        .prepare_test(
            Alice,
            precompile_address(),
            EvmDataWriter::new_with_selector(check_mode(mode))
                .write(vec![
                    Address(Bob.into()),
                    Address(Charlie.into()),
                    Address(Alice.into()),
                ])
                .write(vec![U256::from(1u8), U256::from(2u8), U256::from(3u8)])
                .write(vec![Bytes::from(&one[..])])
                .write::<std::vec::Vec<U256>>(vec![])
                .build(),
        )
        .with_target_gas(Some(300_000))
        .with_subcall_handle(move |subcall| {
            let Subcall {
                address,
                transfer,
                input,
                target_gas,
                is_static,
                context,
            } = subcall;

            // Called from the precompile caller.
            assert_eq!(context.caller, Alice.into());
            assert_eq!(is_static, false);

            match address {
                a if a == Bob.into() => {
                    assert_eq!(counter, 0, "this is the first call");
                    counter += 1;

                    assert_eq!(
                        target_gas,
                        Some(300_000 - total_call_cost),
                        "batch forward all gas"
                    );
                    let transfer = transfer.expect("there is a transfer");
                    assert_eq!(transfer.source, Alice.into());
                    assert_eq!(transfer.target, Bob.into());
                    assert_eq!(transfer.value, 1u8.into());

                    assert_eq!(context.address, Bob.into());
                    assert_eq!(context.apparent_value, 1u8.into());

                    assert_eq!(&input, b"one");

                    SubcallOutput {
                        cost: 13,
                        logs: vec![
                            LogsBuilder::new(Bob.into()).log1(H256::repeat_byte(0x11), vec![])
                        ],
                        ..SubcallOutput::succeed()
                    }
                }
                a if a == Charlie.into() => {
                    assert_eq!(counter, 1, "this is the second call");
                    counter += 1;

                    assert_eq!(
                        target_gas,
                        Some(300_000 - 13 - total_call_cost * 2),
                        "batch forward all gas"
                    );
                    let transfer = transfer.expect("there is a transfer");
                    assert_eq!(transfer.source, Alice.into());
                    assert_eq!(transfer.target, Charlie.into());
                    assert_eq!(transfer.value, 2u8.into());

                    assert_eq!(context.address, Charlie.into());
                    assert_eq!(context.apparent_value, 2u8.into());

                    assert_eq!(&input, b"");

                    SubcallOutput {
                        output: String::from("Revert message").as_bytes().to_vec(),
                        cost: 17,
                        ..SubcallOutput::revert()
                    }
                }
                a if a == Alice.into() => {
                    assert_eq!(counter, 2, "this is the third call");
                    counter += 1;

                    assert_eq!(
                        target_gas,
                        Some(300_000 - 13 - 17 - total_call_cost * 3),
                        "batch forward all gas"
                    );
                    let transfer = transfer.expect("there is a transfer");
                    assert_eq!(transfer.source, Alice.into());
                    assert_eq!(transfer.target, Alice.into());
                    assert_eq!(transfer.value, 3u8.into());

                    assert_eq!(context.address, Alice.into());
                    assert_eq!(context.apparent_value, 3u8.into());

                    assert_eq!(&input, b"");

                    SubcallOutput {
                        cost: 19,
                        logs: vec![
                            LogsBuilder::new(Alice.into()).log1(H256::repeat_byte(0x33), vec![])
                        ],
                        ..SubcallOutput::succeed()
                    }
                }
                _ => panic!("unexpected subcall"),
            }
        })
}

#[test]
fn batch_some_incomplete() {
    ExtBuilder::default().build().execute_with(|| {
        let (_, total_call_cost) = costs();

        batch_incomplete(&precompiles(), Mode::BatchSome)
            .expect_log(LogsBuilder::new(Bob.into()).log1(H256::repeat_byte(0x11), vec![]))
            .expect_log(log_subcall_succeeded(precompile_address(), 0))
            .expect_log(log_subcall_failed(precompile_address(), 1))
            .expect_log(LogsBuilder::new(Alice.into()).log1(H256::repeat_byte(0x33), vec![]))
            .expect_log(log_subcall_succeeded(precompile_address(), 2))
            .expect_cost(13 + 17 + 19 + total_call_cost * 3)
            .execute_returns(EvmDataWriter::new().write(true).build())
    })
}

#[test]
fn batch_some_until_failure_incomplete() {
    ExtBuilder::default().build().execute_with(|| {
        let (_, total_call_cost) = costs();

        batch_incomplete(&precompiles(), Mode::BatchSomeUntilFailure)
            .expect_log(LogsBuilder::new(Bob.into()).log1(H256::repeat_byte(0x11), vec![]))
            .expect_log(log_subcall_succeeded(precompile_address(), 0))
            .expect_log(log_subcall_failed(precompile_address(), 1))
            .expect_cost(13 + 17 + total_call_cost * 2)
            .execute_returns(EvmDataWriter::new().write(true).build())
    })
}

#[test]
fn batch_all_incomplete() {
    ExtBuilder::default().build().execute_with(|| {
        batch_incomplete(&precompiles(), Mode::BatchAll)
            .execute_reverts(|output| output == b"Revert message")
    })
}

fn batch_log_out_of_gas(
    precompiles: &BatchPrecompileMock<Runtime>,
    mode: Mode,
) -> PrecompilesTester<BatchPrecompileMock<Runtime>> {
    let (log_cost, _) = costs();
    let one = b"one";

    precompiles
        .prepare_test(
            Alice,
            precompile_address(),
            EvmDataWriter::new_with_selector(check_mode(mode))
                .write(vec![Address(Bob.into())])
                .write(vec![U256::from(1u8)])
                .write(vec![Bytes::from(&one[..])])
                .write::<std::vec::Vec<U256>>(vec![])
                .build(),
        )
        .with_target_gas(Some(log_cost - 1))
        .with_subcall_handle(move |_subcall| panic!("there shouldn't be any subcalls"))
}

#[test]
fn batch_all_log_out_of_gas() {
    ExtBuilder::default().build().execute_with(|| {
        batch_log_out_of_gas(&precompiles(), Mode::BatchAll).execute_error(ExitError::OutOfGas);
    })
}

#[test]
fn batch_some_log_out_of_gas() {
    ExtBuilder::default().build().execute_with(|| {
        batch_log_out_of_gas(&precompiles(), Mode::BatchSome)
            .expect_no_logs()
            .execute_returns(EvmDataWriter::new().write(true).build());
    })
}

#[test]
fn batch_some_until_failure_log_out_of_gas() {
    ExtBuilder::default().build().execute_with(|| {
        batch_log_out_of_gas(&precompiles(), Mode::BatchSomeUntilFailure)
            .expect_no_logs()
            .execute_returns(EvmDataWriter::new().write(true).build());
    })
}

fn batch_call_out_of_gas(
    precompiles: &BatchPrecompileMock<Runtime>,
    mode: Mode,
) -> PrecompilesTester<BatchPrecompileMock<Runtime>> {
    let (_, total_call_cost) = costs();
    let one = b"one";

    precompiles
        .prepare_test(
            Alice,
            precompile_address(),
            EvmDataWriter::new_with_selector(check_mode(mode))
                .write(vec![Address(Bob.into())])
                .write(vec![U256::from(1u8)])
                .write(vec![Bytes::from(&one[..])])
                .write::<std::vec::Vec<U256>>(vec![])
                .build(),
        )
        .with_target_gas(Some(total_call_cost - 1))
        .with_subcall_handle(move |_subcall| panic!("there shouldn't be any subcalls"))
}

#[test]
fn batch_all_call_out_of_gas() {
    ExtBuilder::default().build().execute_with(|| {
        batch_call_out_of_gas(&precompiles(), Mode::BatchAll).execute_error(ExitError::OutOfGas);
    })
}

#[test]
fn batch_some_call_out_of_gas() {
    ExtBuilder::default().build().execute_with(|| {
        batch_call_out_of_gas(&precompiles(), Mode::BatchSome)
            .expect_log(log_subcall_failed(precompile_address(), 0))
            .execute_returns(EvmDataWriter::new().write(true).build());
    })
}

#[test]
fn batch_some_until_failure_call_out_of_gas() {
    ExtBuilder::default().build().execute_with(|| {
        batch_call_out_of_gas(&precompiles(), Mode::BatchSomeUntilFailure)
            .expect_log(log_subcall_failed(precompile_address(), 0))
            .execute_returns(EvmDataWriter::new().write(true).build());
    })
}

fn batch_gas_limit(
    precompiles: &BatchPrecompileMock<Runtime>,
    mode: Mode,
) -> PrecompilesTester<BatchPrecompileMock<Runtime>> {
    let (_, total_call_cost) = costs();
    let one = b"one";

    precompiles
        .prepare_test(
            Alice,
            precompile_address(),
            EvmDataWriter::new_with_selector(check_mode(mode))
                .write(vec![Address(Bob.into())])
                .write(vec![U256::from(1u8)])
                .write(vec![Bytes::from(&one[..])])
                .write::<std::vec::Vec<U256>>(vec![U256::from(50_000 - total_call_cost + 1)])
                .build(),
        )
        .with_target_gas(Some(50_000))
        .with_subcall_handle(move |_subcall| panic!("there shouldn't be any subcalls"))
}

#[test]
fn batch_all_gas_limit() {
    ExtBuilder::default().build().execute_with(|| {
        batch_gas_limit(&precompiles(), Mode::BatchAll).execute_error(ExitError::OutOfGas);
    })
}

#[test]
fn batch_some_gas_limit() {
    ExtBuilder::default().build().execute_with(|| {
        let (return_log_cost, _) = costs();

        batch_gas_limit(&precompiles(), Mode::BatchSome)
            .expect_log(log_subcall_failed(precompile_address(), 0))
            .expect_cost(return_log_cost)
            .execute_returns(EvmDataWriter::new().write(true).build());
    })
}

#[test]
fn batch_some_until_failure_gas_limit() {
    ExtBuilder::default().build().execute_with(|| {
        batch_gas_limit(&precompiles(), Mode::BatchSomeUntilFailure)
            .expect_log(log_subcall_failed(precompile_address(), 0))
            .execute_returns(EvmDataWriter::new().write(true).build());
    })
}
