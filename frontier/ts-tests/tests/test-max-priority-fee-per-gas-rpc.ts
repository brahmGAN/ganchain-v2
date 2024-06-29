import { ethers } from "ethers";
import { expect } from "chai";
import { step } from "mocha-steps";

import { GENESIS_ACCOUNT, GENESIS_ACCOUNT_PRIVATE_KEY, CHAIN_ID } from "./config";
import { createAndFinalizeBlock, describeWithFrontier, customRequest } from "./util";

// We use ethers library in this test as apparently web3js's types are not fully EIP-1559 compliant yet.
describeWithFrontier("Frontier RPC (Max Priority Fee Per Gas)", (context) => {
	async function sendTransaction(context, payload: any) {
		let signer = new ethers.Wallet(GENESIS_ACCOUNT_PRIVATE_KEY, context.ethersjs);
		// Ethers internally matches the locally calculated transaction hash against the one returned as a response.
		// Test would fail in case of mismatch.
		const tx = await signer.sendTransaction(payload);
		return tx;
	}

	let nonce = 0;

	async function createBlocks(block_count, priority_fees) {
		for (var b = 0; b < block_count; b++) {
			for (var p = 0; p < priority_fees.length; p++) {
				await sendTransaction(context, {
					from: GENESIS_ACCOUNT,
					to: "0x0000000000000000000000000000000000000000",
					data: "0x",
					value: "0x00",
					maxFeePerGas: "0x3B9ACA00",
					maxPriorityFeePerGas: context.web3.utils.numberToHex(priority_fees[p]),
					accessList: [],
					nonce: nonce,
					gasLimit: "0x5208",
					chainId: CHAIN_ID,
				});
				nonce++;
			}
			await createAndFinalizeBlock(context.web3);
		}
	}

	step("should default to zero on genesis", async function () {
		let result = await customRequest(context.web3, "eth_maxPriorityFeePerGas", []);
		expect(result.result).to.be.eq("0x0");
	});

	step("should default to zero on empty blocks", async function () {
		await createAndFinalizeBlock(context.web3);
		let result = await customRequest(context.web3, "eth_maxPriorityFeePerGas", []);
		expect(result.result).to.be.eq("0x0");
	});

	// - Create 20 blocks, each with 10 txns.
	// - Every txn includes a monotonically increasing tip.
	// - The oracle returns the minimum fee in the percentile 60 for the last 20 blocks.
	// - In this case, and being the first tip 0, that minimum fee is 5.
	step("maxPriorityFeePerGas should suggest the percentile 60 tip", async function () {
		this.timeout(100000);

		let block_count = 20;
		let txns_per_block = 10;

		let priority_fee = 0;

		for (let i = 0; i < block_count; i++) {
			let priority_fees = [];
			for (let j = 0; j < txns_per_block; j++) {
				priority_fees.push(priority_fee);
				priority_fee++;
			}
			await createBlocks(1, priority_fees);
		}

		let result = (await customRequest(context.web3, "eth_maxPriorityFeePerGas", [])).result;
		expect(result).to.be.eq("0x5");
	});

	// If in the last 20 blocks at least one is empty (or only contains zero-tip txns), the
	// suggested tip will be zero.
	// That's the expected behaviour in this simplified oracle version: there is a decent chance of
	// being able to include a zero-tip txn in a low congested network.
	step("maxPriorityFeePerGas should suggest zero if there are recent empty blocks", async function () {
		this.timeout(100000);

		for (let i = 0; i < 10; i++) {
			await createBlocks(1, [0, 1, 2, 3, 4, 5]);
		}
		await createAndFinalizeBlock(context.web3);
		for (let i = 0; i < 9; i++) {
			await createBlocks(1, [0, 1, 2, 3, 4, 5]);
		}

		let result = (await customRequest(context.web3, "eth_maxPriorityFeePerGas", [])).result;
		expect(result).to.be.eq("0x0");
	});
});
