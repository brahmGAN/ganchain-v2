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

use super::*;
use pallet_evm_test_vector_support::{
	test_precompile_failure_test_vectors, test_precompile_test_vectors,
};

#[test]
fn process_consensus_tests() -> Result<(), String> {
	test_precompile_test_vectors::<Bw6761G1Add>("../testdata/bw6761G1Add.json")?;
	test_precompile_test_vectors::<Bw6761G1Mul>("../testdata/bw6761G1Mul.json")?;
	test_precompile_test_vectors::<Bw6761G1MultiExp>("../testdata/bw6761G1MultiExp.json")?;
	test_precompile_test_vectors::<Bw6761G2Add>("../testdata/bw6761G2Add.json")?;
	test_precompile_test_vectors::<Bw6761G2Mul>("../testdata/bw6761G2Mul.json")?;
	test_precompile_test_vectors::<Bw6761G2MultiExp>("../testdata/bw6761G2MultiExp.json")?;
	test_precompile_test_vectors::<Bw6761Pairing>("../testdata/bw6761Pairing.json")?;
	Ok(())
}

#[test]
fn process_consensus_failure_tests() -> Result<(), String> {
	test_precompile_failure_test_vectors::<Bw6761G1Add>("../testdata/fail-bw6761G1Add.json")?;
	test_precompile_failure_test_vectors::<Bw6761G1Mul>("../testdata/fail-bw6761G1Mul.json")?;
	test_precompile_failure_test_vectors::<Bw6761G1MultiExp>(
		"../testdata/fail-bw6761G1MultiExp.json",
	)?;
	test_precompile_failure_test_vectors::<Bw6761G2Add>("../testdata/fail-bw6761G2Add.json")?;
	test_precompile_failure_test_vectors::<Bw6761G2Mul>("../testdata/fail-bw6761G2Mul.json")?;
	test_precompile_failure_test_vectors::<Bw6761G2MultiExp>(
		"../testdata/fail-bw6761G2MultiExp.json",
	)?;
	test_precompile_failure_test_vectors::<Bw6761Pairing>("../testdata/fail-bw6761Pairing.json")?;
	Ok(())
}
