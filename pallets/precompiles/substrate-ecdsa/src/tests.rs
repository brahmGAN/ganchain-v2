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

use hex_literal::hex;

use crate::mock::*;
use crate::*;

use precompile_utils::testing::*;
use sp_core::{ecdsa, Pair};

fn precompiles() -> TestPrecompileSet<Runtime> {
    PrecompilesValue::get()
}

#[test]
fn wrong_signature_length_returns_false() {
    ExtBuilder::default().build().execute_with(|| {
        let pair = ecdsa::Pair::from_seed(b"12345678901234567890123456789012");
        let public = pair.public();
        let signature = hex!["0042"];
        let message = hex!["00"];

        precompiles()
            .prepare_test(
                TestAccount::Alice,
                PRECOMPILE_ADDRESS,
                EvmDataWriter::new_with_selector(Action::Verify)
                    .write(Bytes::from(<ecdsa::Public as AsRef<[u8]>>::as_ref(&public)))
                    .write(Bytes::from(&signature[..]))
                    .write(Bytes::from(&message[..]))
                    .build(),
            )
            .expect_no_logs()
            .execute_returns(EvmDataWriter::new().write(false).build());
    });
}

#[test]
fn bad_signature_returns_false() {
    ExtBuilder::default().build().execute_with(|| {
        let pair = ecdsa::Pair::from_seed(b"12345678901234567890123456789012");
        let public = pair.public();
        let message = hex!("2f8c6129d816cf51c374bc7f08c3e63ed156cf78aefb4a6550d97b87997977ee00000000000000000200d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a4500000000000000");
        let signature = pair.sign(&message[..]);
        assert!(ecdsa::Pair::verify(&signature, &message[..], &public));

        let bad_message = hex!["00"];

        precompiles()
            .prepare_test(
                TestAccount::Alice,
                PRECOMPILE_ADDRESS,
                EvmDataWriter::new_with_selector(Action::Verify)
                .write(Bytes::from(<ecdsa::Public as AsRef<[u8]>>::as_ref(&public)))
                .write(Bytes::from(<ecdsa::Signature as AsRef<[u8]>>::as_ref(&signature)))
                .write(Bytes::from(&bad_message[..]))
                    .build(),
            )
            .expect_no_logs()
            .execute_returns(EvmDataWriter::new().write(false).build());
    });
}

#[test]
fn substrate_test_vector_works() {
    ExtBuilder::default().build().execute_with(|| {
        let pair = ecdsa::Pair::from_seed(&hex!(
            "1d2187216832d1ee14be2e677f9e3ebceca715510ba1460a20d6fce07ba36b1e"
        ));
        let public = pair.public();
        assert_eq!(
            public,
            ecdsa::Public::from_raw(hex!(
                "02071bca0b0da3cfa98d3089db224999a827fc1df1a3d6221194382872f0d1a82a"
            ))
        );
        let message = hex!("2f8c6129d816cf51c374bc7f08c3e63ed156cf78aefb4a6550d97b87997977ee00000000000000000200d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a4500000000000000");
        let signature = pair.sign(&message[..]);
        assert!(ecdsa::Pair::verify(&signature, &message[..], &public));

        precompiles()
            .prepare_test(
                TestAccount::Alice,
                PRECOMPILE_ADDRESS,
                EvmDataWriter::new_with_selector(Action::Verify)
                .write(Bytes::from(<ecdsa::Public as AsRef<[u8]>>::as_ref(&public)))
                .write(Bytes::from(<ecdsa::Signature as AsRef<[u8]>>::as_ref(&signature)))
                .write(Bytes::from(&message[..]))
                    .build(),
            )
            .expect_no_logs()
            .execute_returns(EvmDataWriter::new().write(true).build());
    });
}
