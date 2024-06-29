import { assert, expect } from "chai";
import { step } from "mocha-steps";
import { ETH_BLOCK_GAS_LIMIT, GENESIS_ACCOUNT, GENESIS_ACCOUNT_PRIVATE_KEY } from "./config";

import { describeWithFrontier, customRequest, createAndFinalizeBlock } from "./util";
import { AbiItem } from "web3-utils";

import Test from "../build/contracts/Test.json";
import Storage from "../build/contracts/Storage.json";
import ForceGasLimit from "../build/contracts/ForceGasLimit.json";

const TEST_CONTRACT_BYTECODE = Test.bytecode;
const TEST_CONTRACT_DEPLOYED_BYTECODE = Test.deployedBytecode;

const FORCE_GAS_CONTRACT_BYTECODE = ForceGasLimit.bytecode;
const FORCE_GAS_CONTRACT_ABI = ForceGasLimit.abi as AbiItem[];

describeWithFrontier("Frontier RPC (estimate gas historically)", (context) => {
	const TEST_CONTRACT_BYTECODE = Storage.bytecode;
	const TEST_CONTRACT_ABI = Storage.abi as AbiItem[];

	it("estimate gas historically should work", async function () {
		const contract = new context.web3.eth.Contract(TEST_CONTRACT_ABI);

		this.timeout(15000);
		const tx = await context.web3.eth.accounts.signTransaction(
			{
				from: GENESIS_ACCOUNT,
				data: TEST_CONTRACT_BYTECODE,
				value: "0x00",
				gasPrice: "0x3B9ACA00",
				gas: "0x100000",
			},
			GENESIS_ACCOUNT_PRIVATE_KEY
		);

		expect(await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction])).to.include({
			id: 1,
			jsonrpc: "2.0",
		});

		await createAndFinalizeBlock(context.web3);
		let receipt0 = await context.web3.eth.getTransactionReceipt(tx.transactionHash);
		let contractAddress = receipt0.contractAddress;

		// Estimate what a sstore set costs at block number 1
		const SSTORE_SET_DATA = contract.methods
			.setStorage(
				"0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc",
				"0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
			)
			.encodeABI();

		const ESTIMATE_AT_1 = context.web3.utils.hexToNumber(
			(
				await customRequest(context.web3, "eth_estimateGas", [
					{
						to: contractAddress,
						data: SSTORE_SET_DATA,
					},
				])
			).result
		);

		// Set the storage and create a block
		const tx1 = await context.web3.eth.accounts.signTransaction(
			{
				from: GENESIS_ACCOUNT,
				to: contractAddress,
				data: SSTORE_SET_DATA,
				value: "0x00",
				gasPrice: "0x3B9ACA00",
				gas: "0x500000",
			},
			GENESIS_ACCOUNT_PRIVATE_KEY
		);
		await customRequest(context.web3, "eth_sendRawTransaction", [tx1.rawTransaction]);
		await createAndFinalizeBlock(context.web3);

		// Estimate what a sstore reset costs at block number 2
		const ESTIMATE_AT_2 = context.web3.utils.hexToNumber(
			(
				await customRequest(context.web3, "eth_estimateGas", [
					{
						to: contractAddress,
						data: SSTORE_SET_DATA,
					},
				])
			).result
		);

		// SSTORE over an existing storage is cheaper
		expect(ESTIMATE_AT_2).to.be.lt(ESTIMATE_AT_1 as number);

		// Estimate what a sstore reset costed at block number 1, queried historically
		const ESTIMATE_AT_1_QUERY = context.web3.utils.hexToNumber(
			(
				await customRequest(context.web3, "eth_estimateGas", [
					{
						to: contractAddress,
						data: SSTORE_SET_DATA,
					},
					1,
				])
			).result
		);

		// Expect to get the original estimated gas at block 1
		expect(ESTIMATE_AT_1_QUERY).to.be.eq(ESTIMATE_AT_1);
	});
});

describeWithFrontier("Frontier RPC (RPC execution)", (context) => {
	step("should call with gas limit under block gas limit", async function () {
		const result = await customRequest(context.web3, "eth_call", [
			{
				from: GENESIS_ACCOUNT,
				gas: `0x${(ETH_BLOCK_GAS_LIMIT - 1).toString(16)}`,
				data: TEST_CONTRACT_BYTECODE,
			},
		]);

		expect(result.result).to.be.equal(TEST_CONTRACT_DEPLOYED_BYTECODE);
	});

	step("should call with gas limit up to 10x block gas limit", async function () {
		const result = await customRequest(context.web3, "eth_call", [
			{
				from: GENESIS_ACCOUNT,
				gas: `0x${(ETH_BLOCK_GAS_LIMIT * 10).toString(16)}`,
				data: TEST_CONTRACT_BYTECODE,
			},
		]);

		expect(result.result).to.be.equal(TEST_CONTRACT_DEPLOYED_BYTECODE);
	});

	step("shouldn't call with gas limit up higher than 10x block gas limit", async function () {
		const result = await customRequest(context.web3, "eth_call", [
			{
				from: GENESIS_ACCOUNT,
				gas: `0x${(ETH_BLOCK_GAS_LIMIT * 10 + 1).toString(16)}`,
				data: TEST_CONTRACT_BYTECODE,
			},
		]);

		expect((result as any).error.message).to.be.equal(
			"provided gas limit is too high (can be up to 10x the block gas limit)"
		);
	});

	step("should estimateGas with gas limit under block gas limit", async function () {
		const result = await customRequest(context.web3, "eth_estimateGas", [
			{
				from: GENESIS_ACCOUNT,
				gas: `0x${ETH_BLOCK_GAS_LIMIT.toString(16)}`,
				data: TEST_CONTRACT_BYTECODE,
			},
		]);

		expect(result.result).to.be.equal("0x30464");
	});

	step("should estimateGas with gas limit up to 10x block gas limit", async function () {
		const result = await customRequest(context.web3, "eth_estimateGas", [
			{
				from: GENESIS_ACCOUNT,
				gas: `0x${(ETH_BLOCK_GAS_LIMIT * 10).toString(16)}`,
				data: TEST_CONTRACT_BYTECODE,
			},
		]);

		expect(result.result).to.be.equal("0x30464");
	});

	step("shouldn't estimateGas with gas limit up higher than 10x block gas limit", async function () {
		const result = await customRequest(context.web3, "eth_estimateGas", [
			{
				from: GENESIS_ACCOUNT,
				gas: `0x${(ETH_BLOCK_GAS_LIMIT * 20 + 1).toString(16)}`,
				data: TEST_CONTRACT_BYTECODE,
			},
		]);

		expect(result.result).to.not.exist;
		expect((result as any).error.message).to.be.equal(
			"provided gas limit is too high (can be up to 10x the block gas limit)"
		);
	});

	step("should use the gas limit multiplier fallback", async function () {
		const contract = new context.web3.eth.Contract(FORCE_GAS_CONTRACT_ABI);

		const tx = await context.web3.eth.accounts.signTransaction(
			{
				from: GENESIS_ACCOUNT,
				data: FORCE_GAS_CONTRACT_BYTECODE,
				value: "0x00",
				gasPrice: "0x3B9ACA00",
				gas: "0x100000",
			},
			GENESIS_ACCOUNT_PRIVATE_KEY
		);

		await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction]);
		await createAndFinalizeBlock(context.web3);

		const block = await context.web3.eth.getBlock("latest");

		let receipt = await context.web3.eth.getTransactionReceipt(tx.transactionHash);
		let contractAddress = receipt.contractAddress;

		// When not specifying gas we expect the gas limit to default to a 10x block gas limit
		// non-transactional call. The contract's method used requires close to block gas limit * 10.
		const result = await customRequest(context.web3, "eth_call", [
			{
				to: contractAddress,
				// require something close to the block gas limit * 10
				data: contract.methods.force_gas(block.gasLimit * 10 - 500_000).encodeABI(),
			},
		]);

		expect(result).to.include({
			jsonrpc: "2.0",
			result: "0x0000000000000000000000000000000000000000000000000000000000000001",
			id: 1,
		});
	});

	step("`input` field alias is properly deserialized", async function () {
		const result = await customRequest(context.web3, "eth_call", [
			{
				from: GENESIS_ACCOUNT,
				gas: `0x${(ETH_BLOCK_GAS_LIMIT - 1).toString(16)}`,
				input: TEST_CONTRACT_BYTECODE,
			},
		]);

		expect(result.result).to.be.equal(TEST_CONTRACT_DEPLOYED_BYTECODE);
	});

	step("Deserializes correctly when data and input are equal", async function () {
		const result = await customRequest(context.web3, "eth_call", [
			{
				from: GENESIS_ACCOUNT,
				gas: `0x${(ETH_BLOCK_GAS_LIMIT - 1).toString(16)}`,
				data: TEST_CONTRACT_BYTECODE,
				input: TEST_CONTRACT_BYTECODE,
			},
		]);

		expect(result.result).to.be.equal(TEST_CONTRACT_DEPLOYED_BYTECODE);
	});

	step("Throws error when data and input are both present and not equal", async function () {
		const result = await customRequest(context.web3, "eth_call", [
			{
				from: GENESIS_ACCOUNT,
				gas: `0x${(ETH_BLOCK_GAS_LIMIT - 1).toString(16)}`,
				data: TEST_CONTRACT_BYTECODE,
				input: "0x12345678",
			},
		]);
		expect((result as any).error.message).to.match(/^Ambiguous value for `data` and `input`/);
	});
});
